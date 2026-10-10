//! B0 collector leaf only. Serial fresh-process driver lives in retained evidence.
use super::*;
use crate::app::activation_observer::{admit, Probe};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn frame(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    let _ = ctx.run_ui(
        egui::RawInput {
            focused: true,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            assert!(app.run_update_cycle(ui));
        },
    );
    app.finish_activation_headless_frame();
}

fn debt(app: &FlistWalkerApp) -> bool {
    let i = &app.shell.indexing;
    let load = i.tx.load();
    i.pending_request_id.is_some()
        || i.in_progress
        || !i.pending_queue.is_empty()
        || !i.inflight_requests.is_empty()
        || !i.build.pending_entries.is_empty()
        || i.pending_entries_request_id.is_some()
        || i.pending_finish.is_some()
        || i.refresh_after_pending_finish.is_some()
        || i.root_after_pending_finish.is_some()
        || i.build_reclaim_pending
        || i.pending_stale_build_reclaim.is_some()
        || i.pending_replace_all.is_some()
        || i.background_finalizations.keys().next().is_some()
        || !i.background_states.is_empty()
        || i.build.active_filter.is_some()
        || i.kind_resolution_in_progress
        || !i.build.pending_kind_paths.is_empty()
        || !i.build.in_flight_kind_paths.is_empty()
        || app.shell.tabs.reclaimer_pending() != 0
        || app.shell.search.in_progress()
        || load.queued != 0
        || load.inflight != 0
}

fn settle(app: &mut FlistWalkerApp, ctx: &egui::Context, deadline: Instant) -> u64 {
    let mut frames = 0;
    loop {
        assert!(Instant::now() < deadline, "B0 measurement deadline");
        frame(app, ctx);
        frames += 1;
        if !debt(app) {
            return frames;
        }
        thread::yield_now();
    }
}

fn final_oracle(app: &FlistWalkerApp, root: &Path, expected: &[PathBuf]) -> bool {
    let r = &app.shell.runtime;
    let paths = r
        .results
        .iter()
        .map(|r| r.0.clone())
        .collect::<HashSet<_>>();
    r.root == root
        && r.query_state.query.is_empty()
        && r.result_sort_mode == ResultSortMode::Score
        && r.result_sort_scope == ResultSortScope::ShownResults
        && r.results
            .iter()
            .all(|(_, score)| score.is_finite() && *score == 0.0)
        && match &app.shell.indexing.build.index.source {
            IndexSource::FileList(_) => root.join("FileList.txt").exists(),
            IndexSource::Walker => !root.join("FileList.txt").exists(),
            IndexSource::None => false,
        }
        && (!root.join("FileList.txt").exists()
            || r.results.iter().map(|r| &r.0).eq(expected.iter()))
        && r.current_row == Some(0)
        && r.pinned_paths.is_empty()
        && r.evicted_selected_path.is_none()
        && paths == expected.iter().cloned().collect()
        && r.results.len() == expected.len()
        && r.all_entries.len() == expected.len()
        && !debt(app)
}

fn ns(origin: Instant, at: Instant) -> Option<u64> {
    at.checked_duration_since(origin)
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
}

fn request_records(app: &FlistWalkerApp, origin: Instant) -> Vec<Value> {
    app.shell.indexing.perf_allocations.iter().map(|a| {
        let o = a.observation.lock().unwrap();
        json!({"id":a.id,"generation":a.id,"tab":a.tab,"allocation_ns":ns(origin,a.at),
            "send_windows":o.activation_send_windows.iter().map(|(b,e,r)|json!({"begin_ns":ns(origin,*b),"return_ns":ns(origin,*e),"outcome":r})).collect::<Vec<_>>(),
            "admission_return_ns":o.admitted_at.and_then(|v|ns(origin,v)),"root":o.admitted_root,
            "dequeue_ns":o.activation_dequeued.and_then(|v|ns(origin,v)),
            "worker_start_ns":o.activation_worker_started.and_then(|v|ns(origin,v)),
            "started_ns":o.started_published.and_then(|v|ns(origin,v)),"started_source":o.started_source,
            "started_root":o.started_root,"entries_emitted":o.entries_emitted,"batches":o.batches,
            "terminal_ns":o.terminal_published.and_then(|v|ns(origin,v)),"terminal_kind":o.terminal_kind,
            "terminal_source":o.terminal_source,"returned_ns":o.request_processing_returned.and_then(|v|ns(origin,v)),
            "skipped_closed":o.skipped_closed_before_start})
    }).collect()
}

fn intent_records(
    app: &FlistWalkerApp,
    requests: &[Value],
    settled: &[u64],
    final_guards: &[bool],
    cleanup: bool,
) -> Vec<Value> {
    let probe = app.activation_observer.as_ref().unwrap();
    probe.intents.iter().enumerate().map(|(n, intent)| {
        let ceiling = probe.intents.get(n+1).map_or(app.shell.indexing.next_request_id, |i|i.request_floor);
        let target = requests.iter().filter(|r|r["tab"].as_u64()==intent.target && r["id"].as_u64().is_some_and(|id|id>=intent.request_floor&&id<ceiling)).collect::<Vec<_>>();
        let retained = intent.committed && intent.lifecycle == "Ready";
        let expected_source = if intent.root.join("FileList.txt").exists() {"FileList"} else {"Walker"};
        let expected_count = usize::from(!retained);
        let source_work = target.iter().all(|r|r["started_source"]==expected_source
            && r["terminal_source"]==expected_source && r["terminal_kind"]=="finished"
            && r["entries_emitted"].as_u64()==Some(128) && r["root"]==json!(intent.root)
            && r["started_root"]==json!(intent.root) && r["skipped_closed"]==false);
        let generation = intent.first_model.as_ref().and_then(|r|r.generation);
        let identity = generation.and_then(|id| {
            let binding = requests.iter().find(|r| r["id"].as_u64() == Some(id))?;
            let tab = binding["tab"].as_u64()?;
            Some(Some(tab) == intent.target && (retained || target.iter().any(|r| r["id"].as_u64() == Some(id))))
        });
        let mut first_model = probe.receipt_guard(intent.first_model.as_ref(),intent,false);
        let first_frame = probe.receipt_guard(intent.first_frame.as_ref(),intent,true);
        if identity == Some(false) || !final_guards[n] || intent.first_later_failure.is_some() {first_model="FAIL";}
        if first_model != "FAIL" && (identity.is_none() || intent.first_later_unknown.is_some()) { first_model="UNKNOWN"; }
        let mut clock = probe.clock_valid && intent.t0_ns.is_some() && !intent.canceled;
        let t0=intent.t0_ns.unwrap_or(u64::MAX);
        if intent.kind == "tab-switch" { clock &= intent.ingress_metadata_done_ns.is_some_and(|done| t0 <= done && done <= settled[n]); }
        let model=intent.first_model.as_ref().and_then(|r|r.at_ns);
        let display=intent.first_frame.as_ref().and_then(|r|r.at_ns);
        clock &= model.is_some_and(|m|m>=t0)&&display.is_some_and(|d|model.is_some_and(|m|d>=m)&&d<=settled[n]);
        for r in &target {
            let times=[r["allocation_ns"].as_u64(),r["dequeue_ns"].as_u64(),r["worker_start_ns"].as_u64(),r["started_ns"].as_u64(),r["terminal_ns"].as_u64(),r["returned_ns"].as_u64()];
            clock &= times.iter().all(Option::is_some);
            if let [Some(a),Some(d),Some(w),Some(s),Some(t),Some(e)]=times {
                clock &= t0<=a&&a<=d&&d<=w&&w<=s&&s<=t&&t<=e&&e<=settled[n]&&model.is_some_and(|m|m>=s);
                let windows=r["send_windows"].as_array().unwrap();
                let accepted=windows.iter().filter(|w|w["outcome"]=="accepted").collect::<Vec<_>>();
                clock &= accepted.len()==1&&windows.iter().all(|w| w["begin_ns"].as_u64().zip(w["return_ns"].as_u64()).is_some_and(|(b,e)|a<=b&&b<=e));
                clock &= accepted.first().is_some_and(|v|v["begin_ns"].as_u64().is_some_and(|b|b<=d));
            }
        }
        let guards=json!({"first_model":first_model,"first_frame":first_frame,
            "work":if target.len()==expected_count&&source_work {"PASS"}else{"FAIL"},
            "cleanup":if cleanup {"PASS"}else{"FAIL"}});
        let admission=json!({"guards":guards,"clock_valid":clock,"normal_exit":true,"timed_out":false});
        let status=admit(&admission);
        let metric=|value:Option<u64>| if status=="OBSERVATION_VALID" {value.and_then(|v|v.checked_sub(t0))}else{None};
        let worker=target.first().and_then(|r|r["worker_start_ns"].as_u64());
        json!({"intent":intent,"retained":retained,"expected_source":expected_source,
            "target_request_ids":target.iter().map(|r|r["id"].clone()).collect::<Vec<_>>(),"expected_new_requests":expected_count,
            "guards":guards,"clock_valid":clock,"normal_exit":true,"timed_out":false,
            "status":status,"numeric_status":"NOT_EVALUATED","policy":null,"settled_ns":settled[n],
            "metrics_ns":{"intent_to_worker":metric(worker),"intent_to_first_model":metric(model),
                "intent_to_headless_frame":metric(display),"native_display":null},
            "worker_metric_reason":if retained {"retained; no new indexing allowed"}else{"actual worker before root resolution"}})
    }).collect()
}

#[test]
#[ignore = "B0 owned leaf: invoke serial exact test only; no numeric gate"]
fn tc_234_activation_b0_owned_child() {
    let scenario = std::env::var("FW_ACTIVATION_B0_CASE").expect("owned B0 child only");
    assert!(matches!(scenario.as_str(), "filelist" | "restore"));
    let observer = std::env::var("FW_ACTIVATION_B0_OBSERVER").as_deref() == Ok("on");
    let settings = test_settings_scope("activation-b0").with_strict_cleanup();
    let base = settings
        .runtime_config_path()
        .parent()
        .unwrap()
        .to_path_buf();
    let count = if scenario == "restore" { 3 } else { 1 };
    let roots = (0..count)
        .map(|i| base.join(format!("root-{i}")))
        .collect::<Vec<_>>();
    let mut oracle = BTreeMap::new();
    for root in &roots {
        fs::create_dir(root).unwrap();
        let paths = (0..128)
            .map(|i| root.join(format!("entry-{i:03}.txt")))
            .collect::<Vec<_>>();
        for path in &paths {
            fs::write(path, "fixture\n").unwrap();
        }
        if scenario == "filelist" {
            fs::write(
                root.join("FileList.txt"),
                paths
                    .iter()
                    .map(|p| format!("{}\n", p.file_name().unwrap().to_string_lossy()))
                    .collect::<String>(),
            )
            .unwrap();
        }
        oracle.insert(root.clone(), paths);
    }
    let config = crate::runtime_config::RuntimeConfig {
        restore_tabs_enabled: scenario == "restore",
        filelist_auto_check_enabled: false,
        disable_self_update: true,
        ..Default::default()
    };
    crate::runtime_config::save_runtime_config_to_path(&settings.runtime_config_path(), &config)
        .unwrap();
    let state = UiState {
        last_root: Some(roots[0].to_string_lossy().into()),
        show_preview: Some(false),
        ignore_list_enabled: false,
        tabs: if scenario == "restore" {
            roots
                .iter()
                .map(|r| SavedTabState {
                    root: r.to_string_lossy().into(),
                    use_filelist: false,
                    ignore_case: true,
                    include_files: true,
                    include_dirs: false,
                    ..Default::default()
                })
                .collect()
        } else {
            Vec::new()
        },
        active_tab: Some(0),
        ..Default::default()
    };
    fs::write(
        FlistWalkerApp::ui_state_file_path_in(&base),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let ctx = egui::Context::default();
    let rss_before = memory_stats::memory_stats().map(|m| m.physical_mem);
    let mut probe = observer.then(|| Probe::new(Instant::now(), roots[0].clone(), oracle.clone()));
    let origin = Instant::now();
    if let Some(probe) = probe.as_mut() {
        probe.origin = origin;
    }
    let mut app = FlistWalkerApp::build_activation_probe(roots[0].clone(), &base, probe);
    let deadline = origin + Duration::from_secs(15);
    let mut settled = Vec::new();
    let mut final_guards = Vec::new();
    let mut frames = Vec::new();
    frames.push(settle(&mut app, &ctx, deadline));
    settled.push(ns(origin, Instant::now()).unwrap());
    final_guards.push(final_oracle(&app, &roots[0], &oracle[&roots[0]]));
    let ready_results = app.shell.runtime.results.clone();
    let mut dormant_precondition = true;
    let mut ready_precondition = true;
    if scenario == "restore" {
        dormant_precondition =
            app.shell.tabs.get(1).unwrap().index_state.lifecycle() == TabResourceLifecycle::Dormant;
        app.switch_to_tab_index(1);
        frames.push(settle(&mut app, &ctx, deadline));
        settled.push(ns(origin, Instant::now()).unwrap());
        final_guards.push(final_oracle(&app, &roots[1], &oracle[&roots[1]]));
        ready_precondition =
            app.shell.tabs.get(0).unwrap().index_state.lifecycle() == TabResourceLifecycle::Ready;
        app.switch_to_tab_index(0);
        frames.push(settle(&mut app, &ctx, deadline));
        settled.push(ns(origin, Instant::now()).unwrap());
        final_guards.push(
            final_oracle(&app, &roots[0], &oracle[&roots[0]])
                && app.shell.runtime.results == ready_results,
        );
    }
    let final_next_request_id = app.shell.indexing.next_request_id;
    let actual_tab_count = app.shell.tabs.len();
    let settled_debt = debt(&app);
    let index_load = app.shell.indexing.tx.load();
    let all_work = app.shell.indexing.next_request_id == if scenario == "restore" { 3 } else { 2 };
    let requests = request_records(&app, origin);
    let rss_settled = memory_stats::memory_stats().map(|m| m.physical_mem);
    let summary = app
        .shutdown_workers_with_timeout(Duration::from_secs(5), "activation B0 owned child")
        .unwrap();
    let clean_join = summary.joined == summary.total
        && summary.pending.is_empty()
        && summary.panicked.is_empty();
    let intents = if observer {
        intent_records(&app, &requests, &settled, &final_guards, clean_join)
    } else {
        Vec::new()
    };
    let observed_bytes = serde_json::to_vec(&json!({"requests":requests,"intents":intents}))
        .unwrap()
        .len();
    let cleanup_ns = ns(origin, Instant::now());
    drop(app);
    drop(settings);
    let rss_after_drop = memory_stats::memory_stats().map(|m| m.physical_mem);
    let control = all_work
        && final_guards.iter().all(|g| *g)
        && dormant_precondition
        && ready_precondition
        && clean_join
        && !base.exists();
    let sample = json!({"format":"activation-b0-v3","ingress_clock_contract":"switch entry before observer metadata","process_id":std::process::id(),"scenario":scenario,"observer":observer,
        "planned_intents":if scenario=="restore"{3}else{1},"observed_intents":intents.len(),"planned_roots":roots,"actual_tab_count":actual_tab_count,"next_request_id":final_next_request_id,
        "settled_debt":settled_debt,"index_load":{"queued":index_load.queued,"inflight":index_load.inflight},"requests":requests,"intents":intents,
        "clock":"process-local std::time::Instant","numeric_policy":null,"numeric_status":"NOT_EVALUATED",
        "control_status":if control{"PASS"}else{"FAIL"},"final_guards":final_guards,
        "work_count":if all_work {if scenario=="restore" {2}else{1}}else{0},
        "preconditions":{"dormant":dormant_precondition,"ready":ready_precondition},
        "driver_settled_ns":settled,"driver_frames":frames,"cleanup":{"joined":summary.joined,"total":summary.total,
            "pending":summary.pending,"panicked":summary.panicked,"at_ns":cleanup_ns,"settings_removed":!base.exists()},
        "observer_serialized_bytes":observed_bytes,"rss_bytes":{"before":rss_before,"settled":rss_settled,"after_drop":rss_after_drop},
        "observer_off_reason":if observer{Value::Null}else{json!("worker/model/actual-row telemetry disabled; only common final-oracle/settled control checkpoint collected")}});
    println!(
        "ACTIVATION_B0_SAMPLE {}",
        serde_json::to_string(&sample).unwrap()
    );
    assert!(control, "B0 real work/final-oracle/cleanup contract failed");
    assert!(
        observed_bytes <= 256 * 1024,
        "observer footprint exceeds declared serialized bound"
    );
    assert!(
        sample["intents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["status"] == "OBSERVATION_VALID"),
        "B0 observation invalid"
    );
}

#[test]
fn tc_234_b0_later_invalid_receipt_cannot_be_erased_by_recovery() {
    let _settings = test_settings_scope("b0-later-receipt");
    let root = test_root("b0-later-receipt");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("entry-000.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab = app.current_tab_id().unwrap();
    for defect in ["score", "scope", "unknown-generation"] {
        let mut probe = Probe::new(
            Instant::now(),
            root.clone(),
            BTreeMap::from([(root.clone(), vec![path.clone()])]),
        );
        probe.intents[0].target = Some(tab);
        probe.generations.insert(tab, 1);
        probe.sources.insert(tab, "Walker".into());
        app.activation_observer = Some(probe);
        app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
        app.shell.runtime.committed_for_test_mut().current_row = Some(0);
        app.shell.runtime.result_sort_mode = ResultSortMode::Score;
        app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;
        app.observe_activation_model();
        app.observe_activation_drawn_row(0, &path);
        app.finish_activation_headless_frame();
        match defect {
            "score" => app.shell.runtime.committed_for_test_mut().results[0].1 = 123.0,
            "scope" => app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches,
            _ => {
                app.activation_observer
                    .as_mut()
                    .unwrap()
                    .generations
                    .remove(&tab);
            }
        }
        app.observe_activation_model();
        app.observe_activation_drawn_row(0, &path);
        app.finish_activation_headless_frame();
        app.shell.runtime.committed_for_test_mut().results[0].1 = 0.0;
        app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;
        app.activation_observer
            .as_mut()
            .unwrap()
            .generations
            .insert(tab, 1);
        app.observe_activation_model();
        app.observe_activation_drawn_row(0, &path);
        app.finish_activation_headless_frame();
        let intent = &app.activation_observer.as_ref().unwrap().intents[0];
        assert_eq!(intent.first_model.as_ref().unwrap().results[0].1, 0.0);
        assert_eq!(
            intent.first_frame.as_ref().unwrap().sort_scope,
            "ShownResults"
        );
        let raw = serde_json::to_value(intent).unwrap();
        let key = if defect == "unknown-generation" {
            "first_later_unknown"
        } else {
            "first_later_failure"
        };
        assert!(
            raw[key].is_object(),
            "{defect}: recovery erased mandatory evidence"
        );
        if defect == "unknown-generation" {
            app.shell.runtime.committed_for_test_mut().results[0].1 = 123.0;
            app.observe_activation_model();
            let intent = &app.activation_observer.as_ref().unwrap().intents[0];
            assert!(
                intent.first_later_unknown.is_some() && intent.first_later_failure.is_some(),
                "UNKNOWN must not mask later FAIL"
            );
        }
    }
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tc_234_existing_perf_observer_has_no_b0_retry_cap_or_worker_clocks() {
    let _settings = test_settings_scope("b0-opt-in-boundary");
    let root = test_root("b0-opt-in-boundary");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("entry-000.txt"), "fixture").unwrap();
    fs::write(root.join("FileList.txt"), "entry-000.txt\n").unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.shell.indexing.pending_request_id.is_some() {
        assert!(Instant::now() < deadline);
        app.poll_index_response_with_budget_for_test(Duration::from_millis(10));
        thread::yield_now();
    }
    app.shell.indexing.perf_observe_history = true;
    assert!(!app.shell.indexing.activation_observe_requests);
    app.request_index_refresh();
    while app.shell.indexing.pending_request_id.is_some() {
        assert!(Instant::now() < deadline);
        app.poll_index_response_with_budget_for_test(Duration::from_millis(10));
        thread::yield_now();
    }
    let o = app
        .shell
        .indexing
        .perf_allocations
        .last()
        .unwrap()
        .observation
        .lock()
        .unwrap()
        .clone();
    assert!(!o.activation_enabled);
    assert!(o.activation_send_windows.is_empty());
    assert!(o.activation_dequeued.is_none() && o.activation_worker_started.is_none());
    reset_index_request_state_for_test(&mut app);
    let tab = app.current_tab_id().unwrap();
    let (tx, rx) = bounded_request_channel::<IndexRequest>(1);
    let request = |id| IndexRequest {
        request_id: id,
        tab_id: tab,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: false,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    };
    assert!(tx.try_send(request(999_999)).is_ok());
    app.shell.indexing.tx = tx;
    let id = app.shell.indexing.allocate_request_id(Some(tab));
    app.shell.indexing.pending_queue.push_back(request(id));
    for _ in 0..65 {
        app.dispatch_index_queue();
        assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    }
    let o = app
        .shell
        .indexing
        .perf_allocations
        .last()
        .unwrap()
        .observation
        .lock()
        .unwrap()
        .clone();
    assert!(!o.activation_enabled && o.activation_send_windows.is_empty());
    assert!(rx.try_recv().is_ok());
    app.dispatch_index_queue();
    assert!(app.shell.indexing.pending_queue.is_empty());
    assert!(rx.try_recv().is_ok());
    drop(rx);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tc_234_b0_real_emitter_keeps_missing_first_evidence_indeterminate() {
    let _settings = test_settings_scope("b0-emitter-missing-first");
    let root = test_root("b0-emitter-missing-first");
    fs::create_dir_all(&root).unwrap();
    let paths = (0..128)
        .map(|n| root.join(format!("entry-{n:03}.txt")))
        .collect::<Vec<_>>();
    for path in &paths {
        fs::write(path, "fixture").unwrap();
    }
    fs::write(
        root.join("FileList.txt"),
        paths
            .iter()
            .map(|p| format!("{}\n", p.file_name().unwrap().to_string_lossy()))
            .collect::<String>(),
    )
    .unwrap();
    let ctx = egui::Context::default();
    let mut app = FlistWalkerApp::new(root.clone(), 500, String::new());
    settle(&mut app, &ctx, Instant::now() + Duration::from_secs(5));
    assert!(final_oracle(&app, &root, &paths));
    let tab = app.current_tab_id().unwrap();
    let generation = app.shell.indexing.next_request_id - 1;
    let mut probe = Probe::new(
        Instant::now(),
        root.clone(),
        BTreeMap::from([(root.clone(), paths)]),
    );
    let intent = &mut probe.intents[0];
    intent.kind = "tab-switch";
    intent.target = Some(tab);
    intent.lifecycle = "Ready".into();
    intent.committed = true;
    intent.request_floor = app.shell.indexing.next_request_id;
    intent.ingress_metadata_done_ns = Some(0);
    probe.generations.insert(tab, generation);
    probe
        .sources
        .insert(tab, format!("{:?}", app.shell.indexing.build.index.source));
    app.activation_observer = Some(probe);
    app.observe_activation_model();
    frame(&mut app, &ctx);
    let settled = [ns(
        app.activation_observer.as_ref().unwrap().origin,
        Instant::now(),
    )
    .unwrap()];
    let joined = app
        .shutdown_workers_with_timeout(Duration::from_secs(5), "B0 emitter control")
        .unwrap();
    let cleanup =
        joined.joined == joined.total && joined.pending.is_empty() && joined.panicked.is_empty();
    assert!(cleanup);
    let requests = [json!({"id":generation,"tab":tab})];
    let healthy = app.activation_observer.clone();
    assert_eq!(
        intent_records(&app, &requests, &settled, &[true], cleanup)[0]["status"],
        "OBSERVATION_VALID"
    );
    for missing in ["model", "generation", "frame", "request-binding"] {
        app.activation_observer = healthy.clone();
        let intent = &mut app.activation_observer.as_mut().unwrap().intents[0];
        match missing {
            "model" => intent.first_model = None,
            "generation" => intent.first_model.as_mut().unwrap().generation = None,
            "frame" => intent.first_frame = None,
            _ => {}
        }
        let binding = if missing == "request-binding" {
            &[][..]
        } else {
            &requests[..]
        };
        let row = &intent_records(&app, binding, &settled, &[true], cleanup)[0];
        assert_eq!(
            row["status"], "INDETERMINATE",
            "{missing}: missing evidence is not a product FAIL"
        );
        assert!(row["metrics_ns"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_null));
        // Observed wrong frame still dominates missing model/generation.
        if missing != "frame" {
            app.activation_observer.as_mut().unwrap().intents[0]
                .first_frame
                .as_mut()
                .unwrap()
                .results[0]
                .1 = 123.0;
            assert_eq!(
                intent_records(&app, binding, &settled, &[true], cleanup)[0]["status"],
                "FAIL"
            );
        }
        assert_eq!(
            intent_records(&app, binding, &settled, &[false], cleanup)[0]["status"],
            "FAIL",
            "known final result failure must dominate"
        );
    }
    drop(app);
    fs::remove_dir_all(root).unwrap();
}
