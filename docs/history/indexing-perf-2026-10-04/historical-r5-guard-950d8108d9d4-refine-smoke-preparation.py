from pathlib import Path
B=Path(__file__).resolve().parent
for root in [B/t for t in ['v0.27.0','v0.28.0','v0.29.0','v0.30.0','first-red-v0.27.0']]:
 p=root/'rust/src/app/tests/indexing_perf/extensions/runner.rs';s=p.read_text();i=s.index('\n#[test]\nfn historical_actual_worker_small_membership_and_root_proof()');s=s[:i]+'''
fn assert_historical_small_actual_proof(row: &serde_json::Value, fixture: &ExtendedFixture, source: Source) {
    // driver::run has already checked the independent active oracle, owned latest
    // generation, actual body completion and both debts before returning.
    assert_eq!(row["correct"].as_bool(), Some(true));
    let primary = row["request_id"].as_u64().unwrap();
    let request = row["index_requests"].as_array().unwrap().iter()
        .find(|r| r["request_id"].as_u64() == Some(primary)).unwrap();
    assert_eq!(request["actual_started_root"], serde_json::to_value(&fixture.root).unwrap());
    assert_eq!(request["started_source"].as_str(), Some(source.name()));
    assert_eq!(request["terminal_role"].as_str(), Some("required-latest-success"));
    assert_eq!(request["terminal_kind"].as_str(), Some("finished"));
    assert!(request["request_processing_returned_ms"].is_number());
    assert_eq!(request["mailbox_closed"].as_bool(), Some(true));
    assert_eq!(row["final_index_sender_load"]["queued"].as_u64(), Some(0));
    assert_eq!(row["final_index_sender_load"]["inflight"].as_u64(), Some(0));
}

#[test]
fn historical_actual_worker_small_membership_and_root_proof() {
    let cleanup = crate::app::historical_perf::CellGuard::begin();
    let fixture = ExtendedFixture::new(4096, Shape::FlatMixed);
    for (profile, source, condition) in [
        (Profile::Files, Source::Walker, false),
        (Profile::Files, Source::Walker, true),
        (Profile::Folders, Source::Walker, false),
        (Profile::Folders, Source::Walker, true),
        (Profile::Ignore, Source::Walker, false),
        (Profile::Ignore, Source::FileList, false),
    ] {
        let row = driver::run(&fixture, &[], profile, source, condition, false);
        let filter = profile.filter(condition);
        let expected = fixture.expected.iter().filter(|r| if r.is_dir { filter.dirs } else { filter.files }).count();
        assert_eq!(row["expected_final_logical_entries"].as_u64(), Some(expected as u64));
        assert_historical_small_actual_proof(&row, &fixture, source);
        assert!(cleanup.safe(), "actual runtime/parser/writer joins must be positive before another run");
        assert!(restored_fixture(&fixture, source), "actual owned fixture restored");
    }
}

#[test]
fn historical_actual_worker_nested_callback_and_final_hierarchy() {
    let cleanup = crate::app::historical_perf::CellGuard::begin();
    for (profile, shape, reused) in [
        (Profile::NestedEarly, Shape::NestedEarly, true),
        (Profile::NestedLate, Shape::NestedLate, false),
    ] {
        let fixture = ExtendedFixture::new(4096, shape);
        let row = driver::run(&fixture, &[], profile, Source::FileList, false, false);
        assert_eq!(row["nested_input_reused"].as_bool(), Some(reused));
        assert_eq!(row["expected_final_logical_entries"].as_u64(), Some(4096));
        assert_historical_small_actual_proof(&row, &fixture, Source::FileList);
        assert!(cleanup.safe(), "actual worker joins must precede owned root deletion");
        assert!(restored_fixture(&fixture, Source::FileList));
    }
}
''';p.write_text(s)
print('Smoke guards prepared with real original source/root/body/latest/debt/oracle and positive owned cleanup; no execution')
