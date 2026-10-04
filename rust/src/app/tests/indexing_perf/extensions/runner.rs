use super::cases::Profile;
use super::fixture::{ExtendedFixture, Shape};
use super::*;
fn selected(name: &str, default: Vec<String>) -> Vec<String> {
    let values = std::env::var(name)
        .ok()
        .map_or(default, |s| s.split(',').map(str::to_owned).collect());
    assert!(
        super::super::super::unique_selection(
            &values.iter().map(String::as_str).collect::<Vec<_>>()
        ),
        "empty or duplicate {name}"
    );
    values
}
fn setting(name: &str, default: usize) -> usize {
    super::super::super::setting(name, default)
}
#[test]
#[ignore = "release actual-worker extension measurements; serial CPU/storage observations"]
fn perf_indexing_extended_paired() {
    let count = setting("FW_INDEX_PERF_EXTRA_ENTRIES", 100000);
    let pairs = setting("FW_INDEX_PERF_EXTRA_PAIRS", 7);
    let profiles = selected(
        "FW_INDEX_PERF_EXTRA_CASES",
        Profile::ALL
            .into_iter()
            .filter(|p| *p != Profile::Truncated)
            .map(|p| p.name().to_owned())
            .collect(),
    )
    .iter()
    .map(|s| Profile::parse(s))
    .collect::<Vec<_>>();
    assert!(
        !profiles.contains(&Profile::Truncated),
        "actual cap has its separate serial runner"
    );
    let sources = selected(
        "FW_INDEX_PERF_EXTRA_SOURCES",
        vec!["FileList".into(), "Walker".into()],
    )
    .iter()
    .map(|s| match s.as_str() {
        "FileList" => Source::FileList,
        "Walker" => Source::Walker,
        _ => panic!("unknown source {s}"),
    })
    .collect::<Vec<_>>();
    let mut fixtures = Vec::new();
    let mut supported = Vec::new();
    let cap = crate::runtime_config::current_runtime_config().walker_max_entries;
    let selected_profiles = profiles.clone();
    let mut unsupported = Vec::new();
    for profile in profiles {
        for source in profile
            .sources()
            .into_iter()
            .filter(|s| sources.contains(s))
        {
            if source == Source::Walker
                && profile == Profile::Links
                && count.saturating_mul(2).saturating_sub(2) > cap
            {
                unsupported.push(serde_json::json!({"case":profile.name(),"source":source.name(),"reason":"complete links emitted count exceeds actual cap"}));
                eprintln!(
                    "INDEX_PERF_UNSUPPORTED {}",
                    serde_json::json!({"case":profile.name(),"source":source.name(),"input_entries":count,"emitted_entries":count*2-2,"actual_limit":cap,"reason":"complete internal-links fixture exceeds Walker cap"})
                );
                continue;
            }
            if source == Source::Walker {
                assert!(count <= cap, "selected workload exceeds actual Walker cap");
            }
            if !fixtures.iter().any(|(shape, _)| *shape == profile.shape()) {
                fixtures.push((
                    profile.shape(),
                    ExtendedFixture::new(count, profile.shape()),
                ));
            }
            supported.push((profile, source));
        }
    }
    assert!(
        !supported.is_empty(),
        "no supported profile/source cells selected"
    );
    let b = ExtendedFixture::new(count, Shape::FlatFiles);
    let c = ExtendedFixture::new(count, Shape::FlatFiles);
    let mut rows = 0;
    eprintln!(
        "INDEX_PERF_META {}",
        serde_json::json!({"schema_version":1,"runner":"extended","entries":count,"pairs":pairs,"selected_source_cells":supported.len(),"selected_cases":selected_profiles.iter().map(|p|p.name()).collect::<Vec<_>>(),"selected_sources":sources.iter().map(|s|s.name()).collect::<Vec<_>>(),"supported_cells":supported.iter().map(|(p,s)|serde_json::json!({"case":p.name(),"source":s.name()})).collect::<Vec<_>>(),"unsupported_cells":unsupported,"coverage_kind":if supported.len()==44{"all-extended-nontruncated"}else{"selected-subset"},"runtime_settings":crate::runtime_config::current_runtime_config(),"environment_identity":{"arch":std::env::consts::ARCH,"os":std::env::consts::OS,"crate_version":env!("CARGO_PKG_VERSION"),"pinned_rust_toolchain":include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/rust-toolchain.toml")),"logical_cpus":thread::available_parallelism().unwrap().get(),"optimized":!cfg!(debug_assertions),"frame_period_ms":16},"expected_rows":supported.len()*pairs*2,"default_scale_reason":"100k exercises concurrency; 500k only explicit optional observation","native":false})
    );
    for (profile, source) in supported.iter().copied() {
        let fixture = &fixtures
            .iter()
            .find(|(shape, _)| *shape == profile.shape())
            .unwrap()
            .1;
        let companions = if profile == Profile::TabChain {
            vec![&b, &c]
        } else if profile.tabs() {
            vec![&b]
        } else {
            vec![]
        };
        let run = |condition| {
            if profile.parser() {
                supplementary::parser_run(fixture, profile)
            } else {
                driver::run(
                    fixture,
                    &companions,
                    profile,
                    source,
                    condition,
                    count >= 100000,
                )
            }
        };
        for condition in [false, true] {
            eprintln!(
                "INDEX_PERF_RUN_START {}",
                serde_json::json!({"profile":profile.name(),"source":source.name(),"condition":condition,"role":"untimed-warmup","entries":count})
            );
            let _ = run(condition);
        }
        for pair in 0..pairs {
            for (position, condition) in (if pair % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            })
            .into_iter()
            .enumerate()
            {
                eprintln!(
                    "INDEX_PERF_RUN_START {}",
                    serde_json::json!({"profile":profile.name(),"source":source.name(),"condition":condition,"role":"sample","entries":count,"pair":pair,"position":position})
                );
                let mut row = run(condition);
                let object = row.as_object_mut().unwrap();
                object.insert("pair".into(), pair.into());
                object.insert("position".into(), position.into());
                object.insert(
                    "order".into(),
                    if pair % 2 == 0 { "AB" } else { "BA" }.into(),
                );
                object.insert(
                    "case".into(),
                    if condition { profile.name() } else { "B0" }.into(),
                );
                object.insert(
                    "full_scale_cell".into(),
                    (count == 100000 && pairs >= 7).into(),
                );
                eprintln!("INDEX_PERF_SAMPLE {row}");
                rows += 1;
            }
        }
    }
    assert_eq!(rows, supported.len() * pairs * 2, "missing raw rows");
}
#[test]
#[ignore = "actual default cap+1 fixture; serial owned RuntimeConfig restoration"]
fn perf_indexing_truncated_serial() {
    let args = std::env::args().collect::<Vec<_>>();
    assert!(
        args.iter().any(|a| a == "--test-threads=1")
            || args
                .windows(2)
                .any(|w| w[0] == "--test-threads" && w[1] == "1"),
        "global config cap profile requires --test-threads=1"
    );
    let pairs = setting("FW_INDEX_PERF_EXTRA_PAIRS", 7);
    eprintln!(
        "INDEX_PERF_META {}",
        serde_json::json!({"schema_version":1,"runner":"truncated-serial","selected_cases":["W1-truncated"],"selected_sources":["Walker"],"pairs":pairs,"input_entries":500001,"actual_cap":500000,"expected_rows":pairs*2,"comparison_kind":"AA-variability","owned_global_config_restoration":true})
    );
    let fixture = ExtendedFixture::new(500001, Shape::FlatFiles);
    for _ in 0..2 {
        let _ = supplementary::truncated_run(&fixture, 500000);
    }
    let mut rows = 0;
    for pair in 0..pairs {
        for (position, condition) in (if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        })
        .into_iter()
        .enumerate()
        {
            let mut row = supplementary::truncated_run(&fixture, 500000);
            let o = row.as_object_mut().unwrap();
            o.insert("pair".into(), pair.into());
            o.insert("position".into(), position.into());
            o.insert(
                "order".into(),
                if pair % 2 == 0 { "AB" } else { "BA" }.into(),
            );
            o.insert(
                "case".into(),
                if condition { "W1-truncated" } else { "B0" }.into(),
            );
            eprintln!("INDEX_PERF_SAMPLE {row}");
            rows += 1;
        }
    }
    assert_eq!(rows, pairs * 2);
}
