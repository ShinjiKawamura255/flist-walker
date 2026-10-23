use super::*;
use crate::app::index_mailbox::IndexResponseMailbox;
use crate::indexer::MaxDepth;

fn request(id: u64) -> IndexRequest {
    IndexRequest {
        request_id: id,
        tab_id: id,
        root: std::env::temp_dir(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    }
}
fn sink(req: &IndexRequest, shutdown: Arc<AtomicBool>) -> MailboxResponseSink {
    MailboxResponseSink {
        request_id: req.request_id,
        root: req.root.clone(),
        tab_id: req.tab_id,
        mailbox: Arc::new(IndexResponseMailbox::with_data_capacity(1)),
        shutdown,
        latest_request_ids: Arc::new(Mutex::new(HashMap::from([(req.tab_id, req.request_id)]))),
    }
}
#[test]
fn tc_235_decoder_bounds_fragmentation_and_native_roundtrip() {
    let mut d = Decoder::default();
    d.push(&[0xff, 0xff, 0xff, 0x7f]).unwrap();
    assert!(d.next().is_err());
    let frame = Frame {
        version: 1,
        request_id: 1,
        tab_id: 1,
        sequence: 0,
        event: Event::Phase {
            phase: "Resolving root".into(),
        },
    };
    let bytes = serde_json::to_vec(&frame).unwrap();
    let mut wire = (bytes.len() as u32).to_le_bytes().to_vec();
    wire.extend(bytes);
    let mut d = Decoder::default();
    for byte in wire {
        d.push(&[byte]).unwrap();
    }
    assert_eq!(d.next().unwrap().unwrap().sequence, 0);
    assert!(d.is_empty());
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(b"native-\xff".to_vec()))
    };
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[65, 0xd800]))
    };
    let encoded = wire::NativePath::encode(&path).unwrap();
    assert_eq!(encoded.decode().unwrap(), path);
}
#[test]
fn tc_235_protocol_rejects_wrong_identity_order_and_invalid_kind() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let req = request(1);
    let sink = sink(&req, shutdown);
    let frame = |id, sequence, event| Frame {
        version: 1,
        request_id: id,
        tab_id: 1,
        sequence,
        event,
    };
    let mut p = Protocol::new();
    assert!(p
        .accept(
            frame(
                2,
                0,
                Event::Phase {
                    phase: "wrong".into()
                }
            ),
            &req,
            &sink
        )
        .is_err());
    let mut p = Protocol::new();
    assert!(p
        .accept(frame(1, 0, Event::Batch { entries: vec![] }), &req, &sink)
        .is_err());
    let mut p = Protocol::new();
    let _ = sink.mailbox.try_recv();
    p.accept(
        frame(
            1,
            0,
            Event::Started {
                source: Source::Walker,
            },
        ),
        &req,
        &sink,
    )
    .unwrap();
    assert!(p
        .accept(
            frame(
                1,
                1,
                Event::Started {
                    source: Source::Walker
                }
            ),
            &req,
            &sink
        )
        .is_err());
    let mut p = Protocol::new();
    let _ = sink.mailbox.try_recv();
    p.accept(
        frame(
            1,
            0,
            Event::Started {
                source: Source::Walker,
            },
        ),
        &req,
        &sink,
    )
    .unwrap();
    assert!(p
        .accept(
            frame(
                1,
                1,
                Event::Finished {
                    source: Source::Walker
                }
            ),
            &req,
            &sink
        )
        .is_err());
    let entry: wire::Entry =
        serde_json::from_value(serde_json::json!({"path":[65],"kind":99,"kind_known":true}))
            .unwrap();
    assert!(p
        .accept(
            frame(
                1,
                2,
                Event::Batch {
                    entries: vec![entry]
                }
            ),
            &req,
            &sink
        )
        .is_err());
}

#[cfg(unix)]
#[test]
fn tc_235_real_supervisor_recovers_stalls_crashes_partial_frames_and_backpressure() {
    // One matrix owns these global budgets to keep concurrent assertions exact.
    let shutdown = Arc::new(AtomicBool::new(false));
    struct ShutdownOnExit(Arc<AtomicBool>);
    impl Drop for ShutdownOnExit {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_secs(1);
            while PHYSICAL.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
                thread::sleep(POLL);
            }
        }
    }
    let _shutdown_on_exit = ShutdownOnExit(Arc::clone(&shutdown));
    let (pool, reaper) = ProcessPool::new(shutdown.clone());
    let wait_until = |f: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !f() {
            assert!(Instant::now() < deadline, "bounded supervisor wait expired");
            thread::sleep(POLL);
        }
    };
    let mut running = Vec::new();
    let mut cancels = Vec::new();
    let mut started = Vec::new();
    for id in [101, 102] {
        let req = request(id);
        let sink = sink(&req, shutdown.clone());
        cancels.push(sink.latest_request_ids.clone());
        started.push(Arc::clone(&sink.mailbox));
        let frame = Frame {
            version: 1,
            request_id: id,
            tab_id: id,
            sequence: 0,
            event: Event::Phase {
                phase: "Fixture waiting".into(),
            },
        };
        let bytes = serde_json::to_vec(&frame).unwrap();
        let mut wire = (bytes.len() as u32).to_le_bytes().to_vec();
        wire.extend(bytes);
        let script = format!("import os,time;os.write(1,bytes({wire:?}));time.sleep(60)");
        let pool = pool.clone();
        running.push(thread::spawn(move || {
            pool.run_command(&req, &sink, Command::new("python3").arg("-c").arg(script))
        }));
    }
    wait_until(&|| {
        started.iter().all(|mailbox| {
            mailbox
                .worker_phase()
                .is_some_and(|(phase, _)| phase == "Fixture waiting")
        })
    });
    assert_eq!(ACTIVE.load(Ordering::Acquire), 2);
    assert_eq!(PHYSICAL.load(Ordering::Acquire), 2);
    // Lack of output alone must not cancel a slow but healthy operation.
    thread::sleep(Duration::from_millis(80));
    assert_eq!(ACTIVE.load(Ordering::Acquire), 2);
    for cancel in cancels {
        cancel.lock().unwrap().clear();
    }
    for handle in running {
        assert_eq!(handle.join().unwrap().unwrap_err(), "superseded");
    }
    wait_until(&|| PHYSICAL.load(Ordering::Acquire) == 0);
    // Healthy response follows two stuck children through the same supervisor.
    let req = request(103);
    let s = sink(&req, shutdown.clone());
    let events = [
        Event::Started {
            source: Source::Walker,
        },
        Event::Snapshot {
            source: Source::Walker,
            baseline: FileObservation::Unavailable,
            current: FileObservation::Unavailable,
        },
        Event::Finished {
            source: Source::Walker,
        },
    ];
    let mut bytes = Vec::new();
    for (sequence, event) in events.into_iter().enumerate() {
        let b = serde_json::to_vec(&Frame {
            version: 1,
            request_id: 103,
            tab_id: 103,
            sequence: sequence as u64,
            event,
        })
        .unwrap();
        bytes.extend_from_slice(&(b.len() as u32).to_le_bytes());
        bytes.extend(b);
    }
    let script = format!("import os;os.write(1,bytes({bytes:?}))");
    let outcome = pool
        .run_command(&req, &s, Command::new("python3").arg("-c").arg(script))
        .unwrap();
    assert_eq!(outcome, ProcessOutcome::Completed);
    assert_eq!(outcome.label(), "completed");
    assert!(s.mailbox.has_terminal_response());
    let failed_frame = Frame {
        version: 1,
        request_id: 103,
        tab_id: 103,
        sequence: 0,
        event: Event::Failed {
            error: "unavailable root".into(),
        },
    };
    let failed_bytes = serde_json::to_vec(&failed_frame).unwrap();
    let mut failed_wire = (failed_bytes.len() as u32).to_le_bytes().to_vec();
    failed_wire.extend(failed_bytes);
    let failed_script = format!("import os;os.write(1,bytes({failed_wire:?}))");
    let failed_sink = sink(&req, shutdown.clone());
    let outcome = pool
        .run_command(
            &req,
            &failed_sink,
            Command::new("python3").arg("-c").arg(failed_script),
        )
        .unwrap();
    assert_eq!(outcome, ProcessOutcome::Failed);
    assert_eq!(outcome.label(), "failed");
    assert!(failed_sink.mailbox.has_terminal_response());
    // A terminal frame commits only after successful exit. A completed/closed
    // child has a finite exit deadline, unlike a quiet active traversal.
    for (tail, expected) in [
        ("import sys;sys.exit(7)", "exited unexpectedly"),
        ("import time;time.sleep(60)", "did not close output"),
    ] {
        let s = sink(&req, shutdown.clone());
        let script = format!("import os;os.write(1,bytes({bytes:?}));{tail}");
        let start = Instant::now();
        assert!(pool
            .run_command(&req, &s, Command::new("python3").arg("-c").arg(script))
            .unwrap_err()
            .contains(expected));
        assert!(!s.mailbox.has_terminal_response());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    let s = sink(&req, shutdown.clone());
    assert!(pool
        .run_command(
            &req,
            &s,
            Command::new("python3")
                .arg("-c")
                .arg("import os,time;os.close(1);time.sleep(60)")
        )
        .unwrap_err()
        .contains("did not close output"));
    for (script, expected) in [
        (
            "import os;os.write(1,b'\\x08\\x00\\x00\\x00{')",
            "partial IPC",
        ),
        ("import sys;sys.exit(7)", "exited unexpectedly"),
        (
            "import os;os.write(1,b'\\xff\\xff\\xff\\x7f')",
            "frame length",
        ),
    ] {
        let req = request(104);
        let s = sink(&req, shutdown.clone());
        assert!(pool
            .run_command(&req, &s, Command::new("python3").arg("-c").arg(script))
            .unwrap_err()
            .contains(expected));
    }
    // Stale generation aborts even while mailbox backpressure blocks publication.
    let req = request(105);
    let s = sink(&req, shutdown.clone());
    let cancel = s.latest_request_ids.clone();
    let mailbox = Arc::clone(&s.mailbox);
    let events = [
        Event::Started {
            source: Source::Walker,
        },
        Event::Batch { entries: vec![] },
        Event::Batch { entries: vec![] },
    ];
    let mut bytes = Vec::new();
    for (sequence, event) in events.into_iter().enumerate() {
        let b = serde_json::to_vec(&Frame {
            version: 1,
            request_id: 105,
            tab_id: 105,
            sequence: sequence as u64,
            event,
        })
        .unwrap();
        bytes.extend_from_slice(&(b.len() as u32).to_le_bytes());
        bytes.extend(b);
    }
    let script = format!("import os,time;os.write(1,bytes({bytes:?}));time.sleep(60)");
    let p = pool.clone();
    let h = thread::spawn(move || {
        p.run_command(&req, &s, Command::new("python3").arg("-c").arg(script))
    });
    wait_until(&|| mailbox.full_observed());
    cancel.lock().unwrap().clear();
    assert_eq!(h.join().unwrap().unwrap_err(), "superseded");
    wait_until(&|| PHYSICAL.load(Ordering::Acquire) == 0);
    // Inject four unreaped leases: admission must fail explicitly, without spawn.
    let held = (0..4)
        .map(|_| Lease::acquire(&PHYSICAL, 4).unwrap())
        .collect::<Vec<_>>();
    let req = request(106);
    let s = sink(&req, shutdown.clone());
    assert!(pool
        .run_command(&req, &s, Command::new("sleep").arg("60"))
        .unwrap_err()
        .contains("capacity exhausted"));
    assert_eq!(PHYSICAL.load(Ordering::Acquire), 4);
    drop(held);
    // Repeated Refresh cancellation retains no orphan/lease after reap.
    for id in 110..120 {
        let req = request(id);
        let s = sink(&req, shutdown.clone());
        let cancel = s.latest_request_ids.clone();
        let p = pool.clone();
        let h = thread::spawn(move || p.run_command(&req, &s, Command::new("sleep").arg("60")));
        wait_until(&|| PHYSICAL.load(Ordering::Acquire) == 1);
        cancel.lock().unwrap().clear();
        assert_eq!(h.join().unwrap().unwrap_err(), "superseded");
        wait_until(&|| PHYSICAL.load(Ordering::Acquire) == 0);
    }
    // Orderly shutdown reaps a real child, verified by its OS process id.
    let pid_path = std::env::temp_dir().join(format!("fw-index-child-pid-{}", std::process::id()));
    let script = format!(
        "import os,time;open({:?},'w').write(str(os.getpid())+chr(10));time.sleep(60)",
        pid_path.to_str().unwrap()
    );
    let req = request(130);
    let s = sink(&req, shutdown.clone());
    let p = pool.clone();
    let h = thread::spawn(move || {
        p.run_command(&req, &s, Command::new("python3").arg("-c").arg(script))
    });
    wait_until(&|| {
        std::fs::read_to_string(&pid_path)
            .ok()
            .is_some_and(|text| text.ends_with('\n') && text.trim().parse::<i32>().is_ok())
    });
    let pid = std::fs::read_to_string(&pid_path)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    shutdown.store(true, Ordering::Release);
    assert_eq!(h.join().unwrap().unwrap_err(), "superseded");
    reaper.join().unwrap();
    assert!(
        pool.reaper.retired.load(Ordering::Acquire),
        "every reaper exit must publish retirement before a late owner can enqueue"
    );
    let late_child = Command::new("sleep").arg("60").spawn().unwrap();
    let late_lease = Lease::acquire(&PHYSICAL, 4).unwrap();
    pool.reaper.retain_unconfirmed(ReapItem {
        child: late_child,
        _lease: late_lease,
    });
    assert!(pool.reaper.pending.lock().unwrap().is_empty());
    assert_eq!(retained_children().lock().unwrap().len(), 1);
    assert_eq!(PHYSICAL.load(Ordering::Acquire), 1);
    drop(pool);
    let (cleanup, cleanup_reaper) = ProcessPool::new(Arc::new(AtomicBool::new(true)));
    cleanup_reaper.join().unwrap();
    drop(cleanup);
    // SAFETY: signal zero only observes existence of this known test child.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    std::fs::remove_file(pid_path).unwrap();
    assert_eq!(PHYSICAL.load(Ordering::Acquire), 0);
    assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
}

#[test]
fn tc_235_large_replace_all_splits_by_bytes_without_reordering() {
    let prefix = "segment/".repeat(500);
    let entries = (0..1024)
        .map(|i| IndexEntry {
            path: PathBuf::from(format!("{prefix}{i}")),
            kind: crate::entry::EntryKind::file(),
            kind_known: true,
        })
        .collect();
    let mut count = 0;
    let mut frames = 0;
    emit_entries(entries, true, &mut |event| {
        let bytes = serde_json::to_vec(&event).unwrap();
        assert!(bytes.len() < 2 * 1024 * 1024, "bounded serialization chunk");
        let entries = match event {
            Event::ReplaceAll { entries } => {
                assert_eq!(frames, 0);
                entries
            }
            Event::Batch { entries } => {
                assert!(frames > 0);
                entries
            }
            _ => panic!("unexpected data frame"),
        };
        for entry in entries {
            assert_eq!(
                entry.path.decode().unwrap(),
                PathBuf::from(format!("{prefix}{count}"))
            );
            count += 1;
        }
        frames += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 1024);
    assert!(frames > 1);
}
