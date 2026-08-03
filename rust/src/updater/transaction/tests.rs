use super::*;
use crate::updater::staging;
use std::fs;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

const LEGACY_MARKER_V1_JSON: &str = r#"{
    "version": 1,
    "transaction_id": "00112233445566778899aabbccddeeff",
    "binary_name": "flistwalker.exe",
    "parent_pid": 42,
    "helper_pid": null,
    "helper_start_token": null,
    "helper_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "phase": "prepared_parent_owned",
    "targets": [
        {"role":"readme","originally_present":true,"old_hash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","new_hash":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","state":"prepared"},
        {"role":"license","originally_present":true,"old_hash":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","new_hash":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","state":"prepared"},
        {"role":"notices","originally_present":false,"old_hash":null,"new_hash":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","state":"prepared"},
        {"role":"binary","originally_present":true,"old_hash":"1111111111111111111111111111111111111111111111111111111111111111","new_hash":"2222222222222222222222222222222222222222222222222222222222222222","state":"prepared"}
    ]
}"#;

#[test]
fn tc159_legacy_marker_v1_json_contract_survives_internal_refactors() {
    let marker: TransactionMarker =
        serde_json::from_str(LEGACY_MARKER_V1_JSON).expect("decode legacy marker v1");

    assert_eq!(marker.version, 1);
    assert_eq!(marker.phase, Phase::PreparedParentOwned);
    assert_eq!(marker.helper_pid, None);
    assert_eq!(marker.helper_start_token, None);
    assert_eq!(
        marker
            .targets
            .iter()
            .map(|target| target.role)
            .collect::<Vec<_>>(),
        TargetRole::ORDER
    );
    assert_eq!(marker.targets[0].state, TargetState::Prepared);
    assert_eq!(marker.targets[2].old_hash, None);

    let expected: serde_json::Value =
        serde_json::from_str(LEGACY_MARKER_V1_JSON).expect("parse fixture value");
    let serialized = serde_json::to_value(&marker).expect("serialize marker v1");
    assert_eq!(
        serialized, expected,
        "JSON object key order is not a contract"
    );
}

#[cfg(target_os = "windows")]
struct PathEnvGuard {
    original: Option<std::ffi::OsString>,
}

#[cfg(target_os = "windows")]
impl PathEnvGuard {
    fn isolate_to(path: &Path) -> Self {
        let original = std::env::var_os("PATH");
        unsafe {
            std::env::set_var("PATH", path);
        }
        Self { original }
    }
}

#[cfg(target_os = "windows")]
impl Drop for PathEnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(original) = self.original.take() {
                std::env::set_var("PATH", original);
            } else {
                std::env::remove_var("PATH");
            }
        }
    }
}

struct TestProcessControl {
    parent_exited: bool,
    restart_results: Vec<Result<()>>,
    restart_calls: usize,
    restart_modes: Vec<UpdateRestartMode>,
}

impl TestProcessControl {
    fn parent_exited_and_restart_ok() -> Self {
        Self {
            parent_exited: true,
            restart_results: vec![Ok(())],
            restart_calls: 0,
            restart_modes: Vec::new(),
        }
    }

    fn parent_wait_timeout() -> Self {
        Self {
            parent_exited: false,
            restart_results: Vec::new(),
            restart_calls: 0,
            restart_modes: Vec::new(),
        }
    }

    fn restart_fails_then_old_succeeds() -> Self {
        Self {
            parent_exited: true,
            restart_results: vec![Err(anyhow::anyhow!("injected restart failure")), Ok(())],
            restart_calls: 0,
            restart_modes: Vec::new(),
        }
    }

    fn restart_calls(&self) -> usize {
        self.restart_calls
    }

    fn restart_modes(&self) -> &[UpdateRestartMode] {
        &self.restart_modes
    }
}

impl ProcessControl for TestProcessControl {
    fn wait_for_exit(&mut self, _pid: u32, _timeout: Duration) -> Result<bool> {
        Ok(self.parent_exited)
    }

    fn restart(&mut self, _target: &Path, mode: UpdateRestartMode) -> Result<()> {
        self.restart_calls += 1;
        self.restart_modes.push(mode);
        if self.restart_results.is_empty() {
            return Ok(());
        }
        self.restart_results.remove(0)
    }
}

struct FailAfterAppliedRole {
    role: TargetRole,
}

impl FailAfterAppliedRole {
    fn new(role: TargetRole) -> Self {
        Self { role }
    }
}

impl FailureInjector for FailAfterAppliedRole {
    fn after_applied(&mut self, role: TargetRole) -> Result<()> {
        if role == self.role {
            bail!("injected failure after {}", role.label());
        }
        Ok(())
    }
}

struct TestProcessProbe {
    live_pid: Option<u32>,
    executable_matches: bool,
}

impl TestProcessProbe {
    fn none_alive() -> Self {
        Self {
            live_pid: None,
            executable_matches: false,
        }
    }

    fn matching_live(pid: u32) -> Self {
        Self {
            live_pid: Some(pid),
            executable_matches: true,
        }
    }
}

impl ProcessProbe for TestProcessProbe {
    fn is_alive(&self, pid: u32) -> bool {
        self.live_pid == Some(pid)
    }

    fn executable_matches(&self, pid: u32, _expected: &Path) -> bool {
        self.live_pid == Some(pid) && self.executable_matches
    }
}

struct Fixture {
    root: PathBuf,
    sources_dir: PathBuf,
    binary: PathBuf,
    readme: PathBuf,
    license: PathBuf,
    notices: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = staging::test_unique_update_temp_dir().expect("root");
        let sources_dir = staging::test_unique_update_temp_dir().expect("sources");
        let binary = root.join(if cfg!(target_os = "windows") {
            "flistwalker.exe"
        } else {
            "flistwalker"
        });
        fs::write(&binary, b"old-binary").expect("old binary");
        fs::write(root.join("README.txt"), b"old-readme").expect("old readme");
        fs::write(root.join("THIRD_PARTY_NOTICES.txt"), b"old-notices").expect("old notices");
        let readme = sources_dir.join("readme");
        let license = sources_dir.join("license");
        let notices = sources_dir.join("notices");
        let new_binary = sources_dir.join("binary");
        fs::write(&new_binary, b"new-binary").expect("new binary");
        fs::write(&readme, b"new-readme").expect("new readme");
        fs::write(&license, b"new-license").expect("new license");
        fs::write(&notices, b"new-notices").expect("new notices");
        Self {
            root,
            sources_dir,
            binary: new_binary,
            readme,
            license,
            notices,
        }
    }

    fn current_exe(&self) -> PathBuf {
        self.root.join(if cfg!(target_os = "windows") {
            "flistwalker.exe"
        } else {
            "flistwalker"
        })
    }

    fn sources(&self) -> TransactionSources<'_> {
        TransactionSources {
            binary: &self.binary,
            readme: &self.readme,
            license: &self.license,
            notices: &self.notices,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.sources_dir);
    }
}

fn assert_old_bundle(root: &Path, current_exe: &Path) {
    assert_eq!(fs::read(current_exe).expect("binary"), b"old-binary");
    assert_eq!(
        fs::read(root.join("README.txt")).expect("readme"),
        b"old-readme"
    );
    assert!(!root.join("LICENSE.txt").exists());
    assert_eq!(
        fs::read(root.join("THIRD_PARTY_NOTICES.txt")).expect("notices"),
        b"old-notices"
    );
}

#[test]
fn tc158_prepare_is_confined_exclusive_and_binary_last() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");

    assert_eq!(prepared.install_dir(), fixture.root.canonicalize().unwrap());
    assert!(prepared.lock_path().exists());
    assert!(prepared.marker_path().exists());
    assert_eq!(
        prepared.target_roles(),
        [
            TargetRole::Readme,
            TargetRole::License,
            TargetRole::Notices,
            TargetRole::Binary
        ]
    );
    for path in prepared.new_paths() {
        assert_eq!(path.parent(), Some(prepared.install_dir()));
        assert!(path.exists());
    }
    assert_old_bundle(&fixture.root, &current_exe);

    let second = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "ffeeddccbbaa99887766554433221100",
        42,
    );
    assert!(second.is_err(), "fixed transaction lock must be exclusive");
}

#[test]
fn tc158_prepare_rejects_existing_or_non_file_derived_paths_without_cleanup() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let transaction_id = "00112233445566778899aabbccddeeff";
    let collision = new_path(
        &fixture.root.canonicalize().unwrap(),
        transaction_id,
        TargetRole::Readme,
    );
    fs::create_dir(&collision).expect("create collision directory");

    let err = prepare_transaction_with_id(&current_exe, fixture.sources(), transaction_id, 42)
        .err()
        .expect("derived path collision must fail");

    assert!(err.to_string().contains("prepared update file"));
    assert!(
        collision.is_dir(),
        "pre-existing collision must be preserved"
    );
    assert!(!fixture.root.join(LOCK_FILE_NAME).exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc158_operation_revalidation_rejects_changed_target_or_prepared_content() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let marker = read_marker(prepared.marker_path()).expect("marker");
    let target = prepared.install_dir().join("README.txt");
    let prepared_readme = new_path(
        prepared.install_dir(),
        &marker.transaction_id,
        TargetRole::Readme,
    );

    fs::write(&target, b"concurrent-target-change").expect("change target");
    let target_err = apply_one_target(prepared.install_dir(), &marker, 0)
        .expect_err("changed target must fail before replacement");
    assert!(target_err.to_string().contains("old hash"));
    assert_eq!(
        fs::read(&target).expect("target"),
        b"concurrent-target-change"
    );
    assert!(prepared_readme.exists());

    fs::write(&target, b"old-readme").expect("restore target fixture");
    fs::write(&prepared_readme, b"tampered-new-content").expect("change prepared");
    let prepared_err = apply_one_target(prepared.install_dir(), &marker, 0)
        .expect_err("changed prepared file must fail before replacement");
    assert!(prepared_err.to_string().contains("new hash"));
    assert_eq!(fs::read(&target).expect("target"), b"old-readme");
}

#[test]
fn tc158_absent_target_promotion_never_overwrites_a_racing_destination() {
    let root = staging::test_unique_update_temp_dir().expect("root");
    let source = root.join("license.new");
    let target = root.join("LICENSE.txt");
    fs::write(&source, b"new-license").expect("source");
    fs::write(&target, b"racing-destination").expect("target");

    let err = promote_absent_no_overwrite(&source, &target, &root)
        .expect_err("no-overwrite promotion must reject an existing destination");

    assert!(err.to_string().contains("without overwrite"));
    assert_eq!(fs::read(&target).expect("target"), b"racing-destination");
    assert_eq!(fs::read(&source).expect("source"), b"new-license");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn tc159_helper_cannot_ack_or_mutate_before_matching_registration() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");

    let err = acknowledge_registered_helper(
        prepared.marker_path(),
        77,
        "wrong-start-token",
        prepared.helper_path(),
    )
    .expect_err("unregistered helper must fail");

    assert!(err.to_string().contains("registration"));
    assert!(!prepared.ack_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_precommit_failure_rolls_back_applied_sidecars() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "matching-start-token")
        .expect("register");
    acknowledge_registered_helper(
        prepared.marker_path(),
        77,
        "matching-start-token",
        prepared.helper_path(),
    )
    .expect("ack");
    let mut process = TestProcessControl::parent_exited_and_restart_ok();
    let mut failures = FailAfterAppliedRole::new(TargetRole::License);

    let result = execute_registered_transaction(
        prepared.marker_path(),
        "matching-start-token",
        &mut process,
        &mut failures,
    );

    assert!(result.is_err(), "injected precommit failure must surface");
    assert_old_bundle(&fixture.root, &current_exe);
    assert_eq!(
        read_marker(prepared.marker_path()).unwrap().phase,
        Phase::RolledBack
    );
    assert_eq!(process.restart_calls(), 0);
}

#[test]
fn tc158_success_commits_sidecars_before_binary_and_records_restart() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "matching-start-token")
        .expect("register");
    acknowledge_registered_helper(
        prepared.marker_path(),
        77,
        "matching-start-token",
        prepared.helper_path(),
    )
    .expect("ack");
    let mut process = TestProcessControl::parent_exited_and_restart_ok();
    let mut failures = NoFailure;

    execute_registered_transaction(
        prepared.marker_path(),
        "matching-start-token",
        &mut process,
        &mut failures,
    )
    .expect("commit");

    assert_eq!(fs::read(&current_exe).unwrap(), b"new-binary");
    assert_eq!(
        fs::read(fixture.root.join("README.txt")).unwrap(),
        b"new-readme"
    );
    assert_eq!(
        fs::read(fixture.root.join("LICENSE.txt")).unwrap(),
        b"new-license"
    );
    assert_eq!(
        fs::read(fixture.root.join("THIRD_PARTY_NOTICES.txt")).unwrap(),
        b"new-notices"
    );
    assert_eq!(
        read_marker(prepared.marker_path()).unwrap().phase,
        Phase::BinaryCommitted
    );
    assert_eq!(process.restart_calls(), 1);
    assert_eq!(process.restart_modes(), &[UpdateRestartMode::Headless]);
}

#[test]
fn tc159_restart_failure_restores_old_bundle_and_restarts_old_binary() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "matching-start-token")
        .expect("register");
    acknowledge_registered_helper(
        prepared.marker_path(),
        77,
        "matching-start-token",
        prepared.helper_path(),
    )
    .expect("ack");
    let mut process = TestProcessControl::restart_fails_then_old_succeeds();
    let mut failures = NoFailure;

    let result = execute_registered_transaction(
        prepared.marker_path(),
        "matching-start-token",
        &mut process,
        &mut failures,
    );

    assert!(result.is_err());
    assert_old_bundle(&fixture.root, &current_exe);
    assert_eq!(
        read_marker(prepared.marker_path()).unwrap().phase,
        Phase::RolledBack
    );
    assert_eq!(process.restart_calls(), 2);
    assert_eq!(
        process.restart_modes(),
        &[UpdateRestartMode::Headless, UpdateRestartMode::Gui]
    );
}

#[test]
fn tc159_recovery_resumes_an_interrupted_postcommit_rollback() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "registered-start-token")
        .expect("register helper");
    let mut marker = read_marker(prepared.marker_path()).expect("marker");
    marker.phase = Phase::ApplyingSidecars;
    write_marker_atomic(prepared.marker_path(), &marker).expect("applying sidecars");
    for index in 0..marker.targets.len() {
        if marker.targets[index].role == TargetRole::Binary {
            marker.phase = Phase::BinaryIntent;
            write_marker_atomic(prepared.marker_path(), &marker).expect("binary intent");
        }
        marker.targets[index].state = TargetState::Intent;
        write_marker_atomic(prepared.marker_path(), &marker).expect("intent");
        apply_one_target(prepared.install_dir(), &marker, index).expect("apply");
        marker.targets[index].state = TargetState::Applied;
        write_marker_atomic(prepared.marker_path(), &marker).expect("applied");
    }
    marker.phase = Phase::BinaryCommitted;
    write_marker_atomic(prepared.marker_path(), &marker).expect("committed");

    marker.phase = Phase::RollingBack;
    write_marker_atomic(prepared.marker_path(), &marker).expect("rolling back");
    let binary_index = marker.targets.len() - 1;
    restore_existing(
        &backup_path(
            prepared.install_dir(),
            &marker.transaction_id,
            TargetRole::Binary,
        ),
        &current_exe,
        prepared.install_dir(),
        TargetRole::Binary,
    )
    .expect("restore binary");
    marker.targets[binary_index].state = TargetState::RolledBack;
    write_marker_atomic(prepared.marker_path(), &marker).expect("partial rollback marker");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("resume recovery");

    assert_eq!(outcome, RecoveryOutcome::RolledBack);
    assert_old_bundle(&fixture.root, &current_exe);
    assert!(!prepared.marker_path().exists());
    assert!(!prepared.lock_path().exists());
}

#[test]
fn tc159_parent_wait_timeout_mutates_no_installation_target() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "matching-start-token")
        .expect("register");
    acknowledge_registered_helper(
        prepared.marker_path(),
        77,
        "matching-start-token",
        prepared.helper_path(),
    )
    .expect("ack");
    let mut process = TestProcessControl::parent_wait_timeout();
    let mut failures = NoFailure;

    let result = execute_registered_transaction(
        prepared.marker_path(),
        "matching-start-token",
        &mut process,
        &mut failures,
    );

    assert!(result.is_err());
    assert_old_bundle(&fixture.root, &current_exe);
    assert_eq!(process.restart_calls(), 0);
}

#[test]
fn tc159_recovery_defers_while_registered_helper_is_live() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "matching-start-token")
        .expect("register");
    let probe = TestProcessProbe::matching_live(77);

    let outcome = recover_transaction(prepared.marker_path(), &probe).expect("recover");

    assert_eq!(outcome, RecoveryOutcome::Deferred);
    assert!(!prepared.ack_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_recovery_defers_for_live_parent_before_any_artifact_mutation() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let marker = read_marker(prepared.marker_path()).expect("marker");
    let temp = marker_temp_path(prepared.install_dir(), &marker.transaction_id);
    fs::write(&temp, serde_json::to_vec(&marker).expect("serialize")).expect("marker temp");
    let probe = TestProcessProbe {
        live_pid: Some(42),
        executable_matches: false,
    };

    let outcome = recover_transaction(prepared.marker_path(), &probe).expect("recover");

    assert_eq!(outcome, RecoveryOutcome::Deferred);
    assert!(temp.exists(), "live parent artifacts must remain untouched");
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_live_helper_identity_mismatch_is_ambiguous_without_artifact_cleanup() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    prepared
        .register_helper(77, "matching-start-token")
        .expect("register");
    let marker = read_marker(prepared.marker_path()).expect("marker");
    let temp = marker_temp_path(prepared.install_dir(), &marker.transaction_id);
    fs::write(&temp, serde_json::to_vec(&marker).expect("serialize")).expect("marker temp");
    let probe = TestProcessProbe {
        live_pid: Some(77),
        executable_matches: false,
    };

    let outcome = recover_transaction(prepared.marker_path(), &probe).expect("recover");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert!(
        temp.exists(),
        "mismatched live process evidence must remain"
    );
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_binary_intent_with_complete_new_hashes_promotes_committed_bundle() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let mut prepared = prepared;
    prepared
        .register_helper(77, "registered-start-token")
        .expect("register helper");
    let mut marker = read_marker(prepared.marker_path()).expect("marker");
    marker.phase = Phase::ApplyingSidecars;
    write_marker_atomic(prepared.marker_path(), &marker).expect("applying sidecars");
    for index in 0..marker.targets.len() {
        if marker.targets[index].role == TargetRole::Binary {
            marker.phase = Phase::BinaryIntent;
            write_marker_atomic(prepared.marker_path(), &marker).expect("binary intent");
        }
        marker.targets[index].state = TargetState::Intent;
        write_marker_atomic(prepared.marker_path(), &marker).expect("intent");
        apply_one_target(prepared.install_dir(), &marker, index).expect("apply");
        marker.targets[index].state = TargetState::Applied;
        write_marker_atomic(prepared.marker_path(), &marker).expect("applied");
    }
    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("recover");

    assert_eq!(outcome, RecoveryOutcome::Committed);
    assert_eq!(fs::read(&current_exe).unwrap(), b"new-binary");
    assert!(!prepared.marker_path().exists());
    assert!(!prepared.lock_path().exists());
}

#[test]
fn tc159_ambiguous_hash_state_preserves_recovery_evidence() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    fs::write(fixture.root.join("README.txt"), b"unknown-content").expect("tamper");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("recovery outcome");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
    assert!(prepared.new_paths().iter().all(|path| path.exists()));
}

#[test]
fn tc159_startup_recovery_reports_fixed_evidence_paths_for_ambiguous_state() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        u32::MAX,
    )
    .expect("prepare");
    fs::write(fixture.root.join("README.txt"), b"unknown-content").expect("tamper");

    let err = recover_current_installation(&current_exe)
        .expect_err("ambiguous startup recovery must require operator attention");
    let message = err.to_string();

    assert!(message.contains(MARKER_FILE_NAME));
    assert!(message.contains(LOCK_FILE_NAME));
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
}

#[test]
fn tc159_orphan_preparation_with_unverifiable_artifacts_preserves_evidence() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let owned_paths = prepared.new_paths();
    let helper = prepared.helper_path().to_path_buf();
    fs::remove_file(prepared.marker_path()).expect("simulate pre-marker crash");
    prepared.disarm();

    let err = recover_orphan_preparation(
        &fixture.root.canonicalize().expect("install dir"),
        &TestProcessProbe::none_alive(),
    )
    .expect_err("unverifiable orphan artifacts require operator recovery");

    assert!(err.to_string().contains("preserved"));
    assert_old_bundle(&fixture.root, &current_exe);
    assert!(owned_paths.iter().all(|path| path.exists()));
    assert!(helper.exists());
    assert!(fixture.root.join(LOCK_FILE_NAME).exists());
}

#[test]
fn tc159_orphan_lock_without_other_artifacts_is_removed_after_owner_exit() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let mut prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    for path in prepared.new_paths() {
        fs::remove_file(path).expect("remove prepared fixture");
    }
    fs::remove_file(prepared.helper_path()).expect("remove helper fixture");
    fs::remove_file(prepared.marker_path()).expect("remove marker fixture");
    prepared.disarm();

    let outcome = recover_orphan_preparation(
        &fixture.root.canonicalize().expect("install dir"),
        &TestProcessProbe::none_alive(),
    )
    .expect("recover lone orphan lock");

    assert_eq!(outcome, RecoveryOutcome::RolledBack);
    assert_old_bundle(&fixture.root, &current_exe);
    assert!(!fixture.root.join(LOCK_FILE_NAME).exists());
}

#[test]
fn tc159_invalid_marker_transition_is_ambiguous_and_preserves_evidence() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let mut marker = read_marker(prepared.marker_path()).expect("marker");
    marker.phase = Phase::BinaryCommitted;
    fs::write(
        prepared.marker_path(),
        serde_json::to_vec(&marker).expect("serialize invalid marker"),
    )
    .expect("tamper marker");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("invalid marker is classified without mutation");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
    assert!(prepared.new_paths().iter().all(|path| path.exists()));
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_non_file_marker_is_ambiguous_and_preserves_lock() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    fs::remove_file(prepared.marker_path()).expect("remove marker fixture");
    fs::create_dir(prepared.marker_path()).expect("replace marker with directory");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("non-file marker is classified without mutation");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert!(prepared.marker_path().is_dir());
    assert!(prepared.lock_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_marker_update_revalidates_destination_type_before_mutation() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let marker = read_marker(prepared.marker_path()).expect("marker");
    fs::remove_file(prepared.marker_path()).expect("remove marker fixture");
    fs::create_dir(prepared.marker_path()).expect("replace marker with directory");

    let err = write_marker_atomic(prepared.marker_path(), &marker)
        .expect_err("marker replacement must reject a non-file destination");

    assert!(err.to_string().contains("replacement target"));
    assert!(prepared.marker_path().is_dir());
    assert!(prepared.lock_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_cleanup_hash_mismatch_is_ambiguous_and_preserves_evidence() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    fs::write(prepared.helper_path(), b"tampered-helper").expect("tamper helper");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("cleanup mismatch is classified without deletion");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
    assert!(prepared.helper_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_rollback_rejects_tampered_backup_even_when_target_is_already_old() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let backup = backup_path(
        prepared.install_dir(),
        "00112233445566778899aabbccddeeff",
        TargetRole::Readme,
    );
    fs::write(&backup, b"tampered-backup").expect("tamper backup");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("tampered backup is classified without deletion");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert_eq!(fs::read(&backup).expect("backup"), b"tampered-backup");
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_rollback_revalidates_backup_type_before_reverse_mutation() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let backup = backup_path(
        prepared.install_dir(),
        "00112233445566778899aabbccddeeff",
        TargetRole::Readme,
    );
    fs::create_dir(&backup).expect("replace backup with directory");

    let outcome = recover_transaction(prepared.marker_path(), &TestProcessProbe::none_alive())
        .expect("non-file rollback backup is classified without deletion");

    assert_eq!(outcome, RecoveryOutcome::Ambiguous);
    assert!(backup.is_dir());
    assert!(prepared.marker_path().exists());
    assert!(prepared.lock_path().exists());
    assert_old_bundle(&fixture.root, &current_exe);
}

#[test]
fn tc159_rollback_rehashes_target_and_backup_immediately_before_mutation() {
    let fixture = Fixture::new();
    let current_exe = fixture.current_exe();
    let prepared = prepare_transaction_with_id(
        &current_exe,
        fixture.sources(),
        "00112233445566778899aabbccddeeff",
        42,
    )
    .expect("prepare");
    let marker = read_marker(prepared.marker_path()).expect("marker");
    let record = marker.targets[0].clone();
    let target = prepared.install_dir().join("README.txt");
    let backup = backup_path(
        prepared.install_dir(),
        &marker.transaction_id,
        TargetRole::Readme,
    );
    fs::write(&backup, b"old-readme").expect("backup fixture");

    let target_err = revalidate_rollback_hashes(
        prepared.install_dir(),
        &marker,
        &target,
        &backup,
        &record,
        Some(&record.new_hash),
        record.old_hash.as_deref(),
    )
    .expect_err("changed target hash must stop reverse mutation");
    assert!(
        target_err.to_string().contains("target hash changed"),
        "unexpected error: {target_err:#}"
    );

    fs::write(&backup, b"changed-after-branch-check").expect("change backup");
    let backup_err = revalidate_rollback_hashes(
        prepared.install_dir(),
        &marker,
        &target,
        &backup,
        &record,
        record.old_hash.as_deref(),
        record.old_hash.as_deref(),
    )
    .expect_err("changed backup hash must stop reverse mutation");
    assert!(
        backup_err.to_string().contains("backup hash changed"),
        "unexpected error: {backup_err:#}"
    );
    assert_eq!(fs::read(&target).expect("target"), b"old-readme");
    assert_eq!(
        fs::read(&backup).expect("backup"),
        b"changed-after-branch-check"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn tc159_process_executable_identity_matches_the_current_test_process() {
    let current = std::env::current_exe().expect("current test executable");
    assert!(process_executable_matches(std::process::id(), &current));
}

#[cfg(target_os = "windows")]
#[test]
fn tc160_windows_file_replace_preserves_the_old_dummy_file_as_backup() {
    let root = staging::test_unique_update_temp_dir().expect("root");
    let source = root.join("source.new");
    let target = root.join("target.bin");
    let backup = root.join("target.backup");
    fs::write(&source, b"new").expect("source");
    fs::write(&target, b"old").expect("target");

    replace_existing(&source, &target, &backup).expect("File.Replace");

    assert_eq!(fs::read(&target).expect("target"), b"new");
    assert_eq!(fs::read(&backup).expect("backup"), b"old");
    assert!(!source.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(target_os = "windows")]
#[test]
fn tc171_regression_windows_file_replace_does_not_require_powershell_on_path() {
    let _env_lock = crate::env_var_test_lock()
        .lock()
        .expect("env var test lock");
    let root = staging::test_unique_update_temp_dir().expect("root");
    let isolated_path = root.join("isolated-path");
    let source = root.join("source.new");
    let target = root.join("target.bin");
    let backup = root.join("target.backup");
    fs::create_dir(&isolated_path).expect("isolated PATH");
    fs::write(&source, b"new").expect("source");
    fs::write(&target, b"old").expect("target");

    let path_guard = PathEnvGuard::isolate_to(&isolated_path);
    let result = windows_file_replace(&source, &target, Some(&backup));
    drop(path_guard);

    result.expect("in-process File.Replace must not depend on powershell.exe");
    assert_eq!(fs::read(&target).expect("target"), b"new");
    assert_eq!(fs::read(&backup).expect("backup"), b"old");
    assert!(!source.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(target_os = "windows")]
#[test]
fn tc171_regression_windows_file_replace_supports_no_backup_boundary() {
    let root = staging::test_unique_update_temp_dir().expect("root");
    let source = root.join("source.new");
    let target = root.join("target.bin");
    fs::write(&source, b"new").expect("source");
    fs::write(&target, b"old").expect("target");

    windows_file_replace(&source, &target, None).expect("File.Replace without backup");

    assert_eq!(fs::read(&target).expect("target"), b"new");
    assert!(!source.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(target_os = "windows")]
#[test]
fn tc171_regression_windows_file_replace_failure_preserves_source() {
    let root = staging::test_unique_update_temp_dir().expect("root");
    let source = root.join("source.new");
    let missing_target = root.join("missing-target.bin");
    let backup = root.join("target.backup");
    fs::write(&source, b"new").expect("source");

    let error = windows_file_replace(&source, &missing_target, Some(&backup))
        .expect_err("File.Replace must fail when the target is missing");

    assert!(!error.to_string().is_empty());
    assert_eq!(fs::read(&source).expect("source remains"), b"new");
    assert!(!missing_target.exists());
    assert!(!backup.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(target_os = "windows")]
#[test]
fn tc171_regression_windows_file_replace_preserves_verbatim_path_prefix() {
    let path = Path::new(r"\\?\C:\very-long\update-target.bin");
    let encoded = windows_replace_wide_path(path).expect("encode verbatim path");
    let expected = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    assert_eq!(encoded, expected);
}

#[cfg(target_os = "windows")]
#[test]
fn tc171_regression_windows_file_replace_rejects_interior_nul() {
    let path = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0,
        b'x' as u16,
    ]));

    let error = windows_replace_wide_path(&path).expect_err("interior NUL must be rejected");

    assert!(error.to_string().contains("NUL"));
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn tc160_linux_synced_rename_preserves_the_old_dummy_file_as_backup() {
    let root = staging::test_unique_update_temp_dir().expect("root");
    let source = root.join("source.new");
    let target = root.join("target.bin");
    let backup = root.join("target.backup");
    fs::write(&source, b"new").expect("source");
    fs::write(&target, b"old").expect("target");

    replace_existing(&source, &target, &backup).expect("synced rename");

    assert_eq!(fs::read(&target).expect("target"), b"new");
    assert_eq!(fs::read(&backup).expect("backup"), b"old");
    assert!(!source.exists());
    fs::remove_dir_all(root).expect("cleanup");
}
