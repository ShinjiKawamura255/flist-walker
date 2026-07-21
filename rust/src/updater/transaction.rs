#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::staging;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct TestProcessControl {
        parent_exited: bool,
        restart_results: Vec<Result<()>>,
        restart_calls: usize,
    }

    impl TestProcessControl {
        fn parent_exited_and_restart_ok() -> Self {
            Self {
                parent_exited: true,
                restart_results: vec![Ok(())],
                restart_calls: 0,
            }
        }

        fn parent_wait_timeout() -> Self {
            Self {
                parent_exited: false,
                restart_results: Vec::new(),
                restart_calls: 0,
            }
        }

        fn restart_fails_then_old_succeeds() -> Self {
            Self {
                parent_exited: true,
                restart_results: vec![Err(anyhow::anyhow!("injected restart failure")), Ok(())],
                restart_calls: 0,
            }
        }

        fn restart_calls(&self) -> usize {
            self.restart_calls
        }
    }

    impl ProcessControl for TestProcessControl {
        fn wait_for_exit(&mut self, _pid: u32, _timeout: Duration) -> Result<bool> {
            Ok(self.parent_exited)
        }

        fn restart(&mut self, _target: &Path) -> Result<()> {
            self.restart_calls += 1;
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
            42,
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
}
use anyhow::{bail, Context, Result};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const MARKER_FILE_NAME: &str = ".flistwalker-update.marker.json";
const LOCK_FILE_NAME: &str = ".flistwalker-update.lock";
const MARKER_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TargetRole {
    Readme,
    License,
    Notices,
    Binary,
}

impl TargetRole {
    const ORDER: [Self; 4] = [Self::Readme, Self::License, Self::Notices, Self::Binary];

    fn label(self) -> &'static str {
        match self {
            Self::Readme => "readme",
            Self::License => "license",
            Self::Notices => "notices",
            Self::Binary => "binary",
        }
    }

    fn target_name(self, binary_name: &str) -> &str {
        match self {
            Self::Readme => "README.txt",
            Self::License => "LICENSE.txt",
            Self::Notices => "THIRD_PARTY_NOTICES.txt",
            Self::Binary => binary_name,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    PreparedParentOwned,
    HelperRegistered,
    ApplyingSidecars,
    BinaryIntent,
    BinaryCommitted,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetState {
    Prepared,
    Intent,
    Applied,
    RolledBack,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TargetRecord {
    role: TargetRole,
    originally_present: bool,
    old_hash: Option<String>,
    new_hash: String,
    state: TargetState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TransactionMarker {
    version: u32,
    transaction_id: String,
    binary_name: String,
    parent_pid: u32,
    helper_pid: Option<u32>,
    helper_start_token: Option<String>,
    helper_hash: String,
    phase: Phase,
    targets: Vec<TargetRecord>,
}

pub(super) struct TransactionSources<'a> {
    pub(super) binary: &'a Path,
    pub(super) readme: &'a Path,
    pub(super) license: &'a Path,
    pub(super) notices: &'a Path,
}

impl TransactionSources<'_> {
    fn for_role(&self, role: TargetRole) -> &Path {
        match role {
            TargetRole::Readme => self.readme,
            TargetRole::License => self.license,
            TargetRole::Notices => self.notices,
            TargetRole::Binary => self.binary,
        }
    }
}

pub(super) struct PreparedTransaction {
    install_dir: PathBuf,
    marker_path: PathBuf,
    #[cfg(test)]
    lock_path: PathBuf,
    ack_path: PathBuf,
    helper_path: PathBuf,
    transaction_id: String,
    armed: bool,
}

impl PreparedTransaction {
    pub(super) fn install_dir(&self) -> &Path {
        &self.install_dir
    }
    pub(super) fn marker_path(&self) -> &Path {
        &self.marker_path
    }
    #[cfg(test)]
    pub(super) fn lock_path(&self) -> &Path {
        &self.lock_path
    }
    #[cfg(test)]
    pub(super) fn ack_path(&self) -> &Path {
        &self.ack_path
    }
    pub(super) fn helper_path(&self) -> &Path {
        &self.helper_path
    }
    #[cfg(test)]
    pub(super) fn target_roles(&self) -> [TargetRole; 4] {
        TargetRole::ORDER
    }
    #[cfg(test)]
    pub(super) fn new_paths(&self) -> Vec<PathBuf> {
        TargetRole::ORDER
            .into_iter()
            .map(|role| new_path(&self.install_dir, &self.transaction_id, role))
            .collect()
    }
    pub(super) fn register_helper(&mut self, helper_pid: u32, start_token: &str) -> Result<()> {
        validate_start_token(start_token)?;
        let mut marker = read_marker(&self.marker_path)?;
        if marker.phase != Phase::PreparedParentOwned || marker.helper_pid.is_some() {
            bail!("helper registration requires prepared parent-owned transaction");
        }
        marker.helper_pid = Some(helper_pid);
        marker.helper_start_token = Some(start_token.to_string());
        marker.phase = Phase::HelperRegistered;
        write_marker_atomic(&self.marker_path, &marker)
    }
    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PreparedTransaction {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(marker) = read_marker(&self.marker_path) {
                let _ = cleanup_transaction_artifacts(&self.install_dir, &marker, true);
            }
        }
    }
}

pub(super) fn prepare_transaction_with_id(
    current_exe: &Path,
    sources: TransactionSources<'_>,
    transaction_id: &str,
    parent_pid: u32,
) -> Result<PreparedTransaction> {
    validate_transaction_id(transaction_id)?;
    validate_regular_file(current_exe, "current executable")?;
    let canonical_exe = current_exe
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", current_exe.display()))?;
    let install_dir = canonical_exe
        .parent()
        .context("current executable has no parent")?
        .canonicalize()
        .context("failed to canonicalize executable directory")?;
    validate_directory(&install_dir, "executable directory")?;
    let binary_name = canonical_exe
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| is_safe_basename(value))
        .context("current executable filename is not a safe basename")?
        .to_string();
    let marker_path = install_dir.join(MARKER_FILE_NAME);
    let lock_path = install_dir.join(LOCK_FILE_NAME);
    reject_existing(&marker_path, "transaction marker")?;
    reject_existing(&lock_path, "transaction lock")?;

    let mut owned = OwnedPreparation::default();
    create_new_synced(
        &lock_path,
        format!("{transaction_id}\n{parent_pid}\n").as_bytes(),
    )
    .context("failed to acquire updater transaction lock")?;
    owned.paths.push(lock_path.clone());

    let helper_path = helper_path(&install_dir, transaction_id);
    reject_existing(&helper_path, "helper executable")?;
    copy_new_synced(&canonical_exe, &helper_path).context("failed to prepare updater helper")?;
    owned.paths.push(helper_path.clone());
    let helper_hash = sha256_file(&helper_path)?;

    let mut targets = Vec::with_capacity(TargetRole::ORDER.len());
    for role in TargetRole::ORDER {
        let source = sources.for_role(role);
        validate_regular_file(source, "verified update source")?;
        let target = install_dir.join(role.target_name(&binary_name));
        validate_target_if_present(&target, "installation target")?;
        let prepared_path = new_path(&install_dir, transaction_id, role);
        let backup = backup_path(&install_dir, transaction_id, role);
        reject_existing(&prepared_path, "prepared update file")?;
        reject_existing(&backup, "update backup")?;
        let source_hash = sha256_file(source)?;
        let copied_hash = copy_new_synced(source, &prepared_path)?;
        if source_hash != copied_hash {
            bail!("prepared update hash mismatch for {}", role.label());
        }
        owned.paths.push(prepared_path);
        let originally_present = target.try_exists().unwrap_or(false);
        let old_hash = if originally_present {
            Some(sha256_file(&target)?)
        } else {
            None
        };
        targets.push(TargetRecord {
            role,
            originally_present,
            old_hash,
            new_hash: copied_hash,
            state: TargetState::Prepared,
        });
    }
    let marker = TransactionMarker {
        version: MARKER_VERSION,
        transaction_id: transaction_id.to_string(),
        binary_name: binary_name.clone(),
        parent_pid,
        helper_pid: None,
        helper_start_token: None,
        helper_hash,
        phase: Phase::PreparedParentOwned,
        targets,
    };
    write_marker_new(&marker_path, &marker)?;
    owned.paths.push(marker_path.clone());
    sync_parent(&install_dir)?;
    let prepared = PreparedTransaction {
        install_dir: install_dir.clone(),
        marker_path,
        #[cfg(test)]
        lock_path,
        ack_path: ack_path(&install_dir, transaction_id),
        helper_path,
        transaction_id: transaction_id.to_string(),
        armed: true,
    };
    owned.disarm();
    Ok(prepared)
}

#[derive(Default)]
struct OwnedPreparation {
    paths: Vec<PathBuf>,
}
impl OwnedPreparation {
    fn disarm(&mut self) {
        self.paths.clear();
    }
}
impl Drop for OwnedPreparation {
    fn drop(&mut self) {
        for path in self.paths.iter().rev() {
            let _ = fs::remove_file(path);
        }
    }
}

pub(super) fn acknowledge_registered_helper(
    marker_path: &Path,
    helper_pid: u32,
    start_token: &str,
    actual_helper_path: &Path,
) -> Result<PathBuf> {
    let marker = read_marker(marker_path)?;
    let install_dir = validated_marker_parent(marker_path, &marker)?;
    if marker.phase != Phase::HelperRegistered
        || marker.helper_pid != Some(helper_pid)
        || marker.helper_start_token.as_deref() != Some(start_token)
    {
        bail!("helper registration does not match durable marker");
    }
    let expected_helper = helper_path(&install_dir, &marker.transaction_id);
    if actual_helper_path.canonicalize().ok() != expected_helper.canonicalize().ok()
        || sha256_file(actual_helper_path)? != marker.helper_hash
    {
        bail!("helper registration executable identity mismatch");
    }
    let path = ack_path(&install_dir, &marker.transaction_id);
    create_new_synced(
        &path,
        format!("{}\n{}\n", marker.transaction_id, start_token).as_bytes(),
    )?;
    sync_parent(&install_dir)?;
    Ok(path)
}

pub(super) trait ProcessControl {
    fn wait_for_exit(&mut self, pid: u32, timeout: Duration) -> Result<bool>;
    fn restart(&mut self, target: &Path) -> Result<()>;
}
pub(super) trait FailureInjector {
    fn after_applied(&mut self, _role: TargetRole) -> Result<()> {
        Ok(())
    }
}

pub(super) fn execute_registered_transaction(
    marker_path: &Path,
    start_token: &str,
    process: &mut impl ProcessControl,
    failures: &mut impl FailureInjector,
) -> Result<()> {
    let mut marker = read_marker(marker_path)?;
    let install_dir = validated_marker_parent(marker_path, &marker)?;
    recover_marker_update_artifacts(&install_dir, &marker)?;
    if marker.phase != Phase::HelperRegistered
        || marker.helper_start_token.as_deref() != Some(start_token)
    {
        bail!("transaction helper registration is not valid");
    }
    validate_ack(&install_dir, &marker, start_token)?;
    if !process.wait_for_exit(marker.parent_pid, Duration::from_secs(30))? {
        rollback_transaction(&install_dir, marker_path, &mut marker)?;
        cleanup_rolled_back(&install_dir, &marker)?;
        bail!("parent process did not exit within 30 seconds");
    }
    marker.phase = Phase::ApplyingSidecars;
    write_marker_atomic(marker_path, &marker)?;
    let apply_result = (|| {
        for index in 0..marker.targets.len() {
            let role = marker.targets[index].role;
            if role == TargetRole::Binary {
                marker.phase = Phase::BinaryIntent;
                write_marker_atomic(marker_path, &marker)?;
            }
            marker.targets[index].state = TargetState::Intent;
            write_marker_atomic(marker_path, &marker)?;
            apply_one_target(&install_dir, &marker, index)?;
            marker.targets[index].state = TargetState::Applied;
            write_marker_atomic(marker_path, &marker)?;
            failures.after_applied(role)?;
        }
        verify_bundle_hashes(&install_dir, &marker, true)?;
        marker.phase = Phase::BinaryCommitted;
        write_marker_atomic(marker_path, &marker)
    })();
    if let Err(err) = apply_result {
        rollback_transaction(&install_dir, marker_path, &mut marker)?;
        return Err(err).context("update activation failed and was rolled back");
    }
    let binary = target_path(&install_dir, &marker, TargetRole::Binary);
    if let Err(err) = process.restart(&binary) {
        rollback_transaction(&install_dir, marker_path, &mut marker)?;
        let _ = process.restart(&binary);
        return Err(err).context("failed to restart updated application; old bundle restored");
    }
    Ok(())
}

fn recover_marker_update_artifacts(install_dir: &Path, marker: &TransactionMarker) -> Result<()> {
    let temp = marker_temp_path(install_dir, &marker.transaction_id);
    let previous = temp.with_extension("previous");
    for artifact in [temp, previous] {
        match fs::symlink_metadata(&artifact) {
            Ok(_) => {
                validate_regular_file(&artifact, "interrupted marker artifact")?;
                let bytes = fs::read(&artifact)
                    .with_context(|| format!("failed to read {}", artifact.display()))?;
                let artifact_marker: TransactionMarker = serde_json::from_slice(&bytes)
                    .context("failed to parse interrupted marker artifact")?;
                validate_marker(&artifact_marker)?;
                if artifact_marker.transaction_id != marker.transaction_id {
                    bail!("interrupted marker artifact belongs to another transaction");
                }
                fs::remove_file(&artifact).with_context(|| {
                    format!("failed to remove marker artifact {}", artifact.display())
                })?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to inspect {}", artifact.display()));
            }
        }
    }
    sync_parent(install_dir)
}

fn apply_one_target(install_dir: &Path, marker: &TransactionMarker, index: usize) -> Result<()> {
    let record = &marker.targets[index];
    let target = target_path(install_dir, marker, record.role);
    let prepared = new_path(install_dir, &marker.transaction_id, record.role);
    let backup = backup_path(install_dir, &marker.transaction_id, record.role);
    revalidate_operation_paths(install_dir, &target, &prepared, &backup, record)?;
    if record.originally_present {
        replace_existing(&prepared, &target, &backup)?;
    } else {
        promote_absent_no_overwrite(&prepared, &target, install_dir)?;
    }
    if sha256_file(&target)? != record.new_hash {
        bail!("installed target hash mismatch for {}", record.role.label());
    }
    Ok(())
}

fn promote_absent_no_overwrite(source: &Path, target: &Path, install_dir: &Path) -> Result<()> {
    fs::hard_link(source, target).with_context(|| {
        format!(
            "failed to promote absent target without overwrite {}",
            target.display()
        )
    })?;
    sync_parent(install_dir)?;
    fs::remove_file(source)
        .with_context(|| format!("failed to remove promoted source {}", source.display()))?;
    sync_parent(install_dir)
}

fn rollback_transaction(
    install_dir: &Path,
    marker_path: &Path,
    marker: &mut TransactionMarker,
) -> Result<()> {
    if marker.phase != Phase::RollingBack {
        marker.phase = Phase::RollingBack;
        write_marker_atomic(marker_path, marker)?;
    }
    for index in (0..marker.targets.len()).rev() {
        let record = marker.targets[index].clone();
        let target = target_path(install_dir, marker, record.role);
        let backup = backup_path(install_dir, &marker.transaction_id, record.role);
        revalidate_rollback_paths(install_dir, marker, &target, &backup, &record)?;
        let target_hash = hash_if_regular(&target)?;
        if record.originally_present {
            let old_hash = record
                .old_hash
                .as_deref()
                .context("missing old target hash")?;
            if target_hash.as_deref() == Some(old_hash) {
                match hash_if_regular(&backup)? {
                    None => {}
                    Some(hash) if hash == old_hash => {
                        revalidate_rollback_hashes(
                            install_dir,
                            marker,
                            &target,
                            &backup,
                            &record,
                            Some(old_hash),
                            Some(old_hash),
                        )?;
                        fs::remove_file(&backup).with_context(|| {
                            format!("failed to remove verified backup {}", backup.display())
                        })?;
                        sync_parent(install_dir)?;
                    }
                    Some(_) => {
                        bail!("ambiguous rollback backup for {}", record.role.label());
                    }
                }
            } else if target_hash.as_deref() == Some(record.new_hash.as_str())
                && hash_if_regular(&backup)?.as_deref() == Some(old_hash)
            {
                revalidate_rollback_hashes(
                    install_dir,
                    marker,
                    &target,
                    &backup,
                    &record,
                    Some(&record.new_hash),
                    Some(old_hash),
                )?;
                restore_existing(&backup, &target, install_dir, record.role)?;
            } else if record.state != TargetState::Prepared {
                bail!("ambiguous rollback state for {}", record.role.label());
            }
        } else if target_hash.as_deref() == Some(record.new_hash.as_str()) {
            revalidate_rollback_hashes(
                install_dir,
                marker,
                &target,
                &backup,
                &record,
                Some(&record.new_hash),
                None,
            )?;
            fs::remove_file(&target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
            sync_parent(install_dir)?;
        } else if target_hash.is_some() {
            bail!("ambiguous rollback state for {}", record.role.label());
        }
        marker.targets[index].state = TargetState::RolledBack;
        write_marker_atomic(marker_path, marker)?;
    }
    verify_bundle_hashes(install_dir, marker, false)?;
    marker.phase = Phase::RolledBack;
    write_marker_atomic(marker_path, marker)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryOutcome {
    Deferred,
    RolledBack,
    Committed,
    Ambiguous,
}
pub(super) trait ProcessProbe {
    fn is_alive(&self, pid: u32) -> bool;
    fn executable_matches(&self, pid: u32, expected: &Path) -> bool;
    fn is_current_process(&self, _pid: u32) -> bool {
        false
    }
}

pub(super) fn recover_transaction(
    marker_path: &Path,
    process_probe: &impl ProcessProbe,
) -> Result<RecoveryOutcome> {
    let mut marker = match read_marker(marker_path) {
        Ok(marker) => marker,
        Err(_) => return Ok(RecoveryOutcome::Ambiguous),
    };
    let install_dir = validated_marker_parent(marker_path, &marker)?;
    if process_probe.is_alive(marker.parent_pid)
        && !process_probe.is_current_process(marker.parent_pid)
    {
        return Ok(RecoveryOutcome::Deferred);
    }
    if let Some(pid) = marker.helper_pid {
        if process_probe.is_alive(pid) && !process_probe.is_current_process(pid) {
            let helper = helper_path(&install_dir, &marker.transaction_id);
            let helper_file_matches = hash_if_regular(&helper).ok().flatten().as_deref()
                == Some(marker.helper_hash.as_str());
            if !helper_file_matches || !process_probe.executable_matches(pid, &helper) {
                return Ok(RecoveryOutcome::Ambiguous);
            }
            let acknowledgement = ack_path(&install_dir, &marker.transaction_id);
            match fs::symlink_metadata(&acknowledgement) {
                Ok(_) => {
                    if validate_ack(
                        &install_dir,
                        &marker,
                        marker.helper_start_token.as_deref().unwrap_or_default(),
                    )
                    .is_err()
                    {
                        return Ok(RecoveryOutcome::Ambiguous);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    if marker.phase != Phase::HelperRegistered {
                        return Ok(RecoveryOutcome::Ambiguous);
                    }
                }
                Err(_) => return Ok(RecoveryOutcome::Ambiguous),
            }
            return Ok(RecoveryOutcome::Deferred);
        }
    }
    recover_marker_update_artifacts(&install_dir, &marker)?;
    let classifications = marker
        .targets
        .iter()
        .map(|record| classify_target(&install_dir, &marker, record))
        .collect::<Result<Vec<_>>>()?;
    if classifications.contains(&TargetClassification::Unknown) {
        return Ok(RecoveryOutcome::Ambiguous);
    }
    if marker.phase == Phase::BinaryCommitted
        || (marker.phase == Phase::BinaryIntent
            && classifications
                .iter()
                .all(|value| *value == TargetClassification::New))
    {
        if classifications
            .iter()
            .all(|value| *value == TargetClassification::New)
        {
            marker.phase = Phase::BinaryCommitted;
            write_marker_atomic(marker_path, &marker)?;
            if cleanup_committed(&install_dir, &marker).is_err() {
                return Ok(RecoveryOutcome::Ambiguous);
            }
            return Ok(RecoveryOutcome::Committed);
        }
        return Ok(RecoveryOutcome::Ambiguous);
    }
    if rollback_transaction(&install_dir, marker_path, &mut marker).is_err() {
        return Ok(RecoveryOutcome::Ambiguous);
    }
    if cleanup_rolled_back(&install_dir, &marker).is_err() {
        return Ok(RecoveryOutcome::Ambiguous);
    }
    Ok(RecoveryOutcome::RolledBack)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetClassification {
    Old,
    New,
    Unknown,
}

fn classify_target(
    install_dir: &Path,
    marker: &TransactionMarker,
    record: &TargetRecord,
) -> Result<TargetClassification> {
    let hash = hash_if_regular(&target_path(install_dir, marker, record.role))?;
    if hash.as_deref() == Some(record.new_hash.as_str()) {
        return Ok(TargetClassification::New);
    }
    if record.originally_present && hash == record.old_hash {
        return Ok(TargetClassification::Old);
    }
    if !record.originally_present && hash.is_none() {
        return Ok(TargetClassification::Old);
    }
    Ok(TargetClassification::Unknown)
}

fn verify_bundle_hashes(
    install_dir: &Path,
    marker: &TransactionMarker,
    expect_new: bool,
) -> Result<()> {
    for record in &marker.targets {
        let actual = hash_if_regular(&target_path(install_dir, marker, record.role))?;
        let valid = if expect_new {
            actual.as_deref() == Some(record.new_hash.as_str())
        } else if record.originally_present {
            actual == record.old_hash
        } else {
            actual.is_none()
        };
        if !valid {
            bail!(
                "bundle hash verification failed for {}",
                record.role.label()
            );
        }
    }
    Ok(())
}

fn cleanup_committed(install_dir: &Path, marker: &TransactionMarker) -> Result<()> {
    verify_bundle_hashes(install_dir, marker, true)?;
    cleanup_transaction_artifacts(install_dir, marker, true)
}
fn cleanup_rolled_back(install_dir: &Path, marker: &TransactionMarker) -> Result<()> {
    verify_bundle_hashes(install_dir, marker, false)?;
    cleanup_transaction_artifacts(install_dir, marker, true)
}
fn cleanup_transaction_artifacts(
    install_dir: &Path,
    marker: &TransactionMarker,
    include_marker_and_lock: bool,
) -> Result<()> {
    validate_cleanup_artifacts(install_dir, marker, include_marker_and_lock)?;
    let transaction_id = &marker.transaction_id;
    for role in TargetRole::ORDER {
        remove_file_if_present(&new_path(install_dir, transaction_id, role))?;
        remove_file_if_present(&backup_path(install_dir, transaction_id, role))?;
        remove_file_if_present(&failed_path(install_dir, transaction_id, role))?;
    }
    remove_file_if_present(&ack_path(install_dir, transaction_id))?;
    remove_file_if_present(&helper_path(install_dir, transaction_id))?;
    remove_file_if_present(&marker_temp_path(install_dir, transaction_id))?;
    remove_file_if_present(
        &marker_temp_path(install_dir, transaction_id).with_extension("previous"),
    )?;
    if include_marker_and_lock {
        remove_file_if_present(&install_dir.join(MARKER_FILE_NAME))?;
        remove_file_if_present(&install_dir.join(LOCK_FILE_NAME))?;
    }
    sync_parent(install_dir)
}
fn validate_cleanup_artifacts(
    install_dir: &Path,
    marker: &TransactionMarker,
    include_marker_and_lock: bool,
) -> Result<()> {
    validate_directory(install_dir, "transaction directory")?;
    for record in &marker.targets {
        validate_optional_hash(
            &new_path(install_dir, &marker.transaction_id, record.role),
            Some(&record.new_hash),
            "prepared update cleanup artifact",
        )?;
        validate_optional_hash(
            &backup_path(install_dir, &marker.transaction_id, record.role),
            record.old_hash.as_ref(),
            "backup cleanup artifact",
        )?;
        validate_optional_hash(
            &failed_path(install_dir, &marker.transaction_id, record.role),
            Some(&record.new_hash),
            "failed replacement cleanup artifact",
        )?;
    }
    let acknowledgement = ack_path(install_dir, &marker.transaction_id);
    if acknowledgement.try_exists().unwrap_or(false) {
        let token = marker
            .helper_start_token
            .as_deref()
            .context("acknowledgement exists without a helper token")?;
        validate_ack(install_dir, marker, token)?;
    }
    validate_optional_hash(
        &helper_path(install_dir, &marker.transaction_id),
        Some(&marker.helper_hash),
        "helper cleanup artifact",
    )?;
    for artifact in [
        marker_temp_path(install_dir, &marker.transaction_id),
        marker_temp_path(install_dir, &marker.transaction_id).with_extension("previous"),
    ] {
        if artifact.try_exists().unwrap_or(false) {
            validate_regular_file(&artifact, "marker cleanup artifact")?;
            let artifact_marker: TransactionMarker = serde_json::from_slice(
                &fs::read(&artifact)
                    .with_context(|| format!("failed to read {}", artifact.display()))?,
            )
            .context("failed to parse marker cleanup artifact")?;
            validate_marker(&artifact_marker)?;
            if artifact_marker.transaction_id != marker.transaction_id {
                bail!("marker cleanup artifact belongs to another transaction");
            }
        }
    }
    if include_marker_and_lock {
        validate_regular_file(
            &install_dir.join(MARKER_FILE_NAME),
            "transaction marker cleanup artifact",
        )?;
        let (lock_transaction_id, lock_parent_pid) = read_lock_record(install_dir)?;
        if lock_transaction_id != marker.transaction_id || lock_parent_pid != marker.parent_pid {
            bail!("transaction lock identity does not match marker");
        }
    }
    Ok(())
}
fn validate_optional_hash(path: &Path, expected: Option<&String>, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            let expected = expected.with_context(|| format!("unexpected {label}"))?;
            validate_regular_file(path, label)?;
            if sha256_file(path)? != *expected {
                bail!("{label} hash mismatch: {}", path.display());
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
fn remove_file_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

pub(super) fn read_marker(path: &Path) -> Result<TransactionMarker> {
    validate_regular_file(path, "transaction marker")?;
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let marker: TransactionMarker =
        serde_json::from_slice(&bytes).context("failed to parse update transaction marker")?;
    validate_marker(&marker)?;
    Ok(marker)
}
fn write_marker_new(path: &Path, marker: &TransactionMarker) -> Result<()> {
    validate_marker(marker)?;
    create_new_synced(
        path,
        &serde_json::to_vec(marker).context("failed to serialize marker")?,
    )
}
fn write_marker_atomic(path: &Path, marker: &TransactionMarker) -> Result<()> {
    validate_marker(marker)?;
    let install_dir = path.parent().context("transaction marker has no parent")?;
    revalidate_transaction_directory(install_dir)?;
    if path.file_name().and_then(|value| value.to_str()) != Some(MARKER_FILE_NAME) {
        bail!("transaction marker path is not fixed");
    }
    validate_regular_file(path, "transaction marker replacement target")?;
    let temp = marker_temp_path(install_dir, &marker.transaction_id);
    create_new_synced(
        &temp,
        &serde_json::to_vec(marker).context("failed to serialize marker")?,
    )?;
    let result = replace_marker_file(&temp, path);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    sync_parent(install_dir)
}

#[cfg(target_os = "windows")]
fn replace_marker_file(source: &Path, destination: &Path) -> Result<()> {
    let backup = source.with_extension("previous");
    reject_existing(&backup, "marker replacement backup")?;
    powershell_file_replace(source, destination, Some(&backup))?;
    fs::remove_file(&backup)
        .with_context(|| format!("failed to remove marker backup {}", backup.display()))
}
#[cfg(not(target_os = "windows"))]
fn replace_marker_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).with_context(|| {
        format!(
            "failed to replace marker {} with {}",
            destination.display(),
            source.display()
        )
    })
}
#[cfg(target_os = "windows")]
fn replace_existing(source: &Path, target: &Path, backup: &Path) -> Result<()> {
    powershell_file_replace(source, target, Some(backup))
}
#[cfg(not(target_os = "windows"))]
fn replace_existing(source: &Path, target: &Path, backup: &Path) -> Result<()> {
    copy_new_synced(target, backup)?;
    sync_parent(target.parent().context("target has no parent")?)?;
    fs::rename(source, target)
        .with_context(|| format!("failed to replace {}", target.display()))?;
    sync_parent(target.parent().context("target has no parent")?)
}
#[cfg(target_os = "windows")]
fn restore_existing(
    backup: &Path,
    target: &Path,
    install_dir: &Path,
    role: TargetRole,
) -> Result<()> {
    let marker = read_marker(&install_dir.join(MARKER_FILE_NAME))?;
    let failed = failed_path(install_dir, &marker.transaction_id, role);
    powershell_file_replace(backup, target, Some(&failed))?;
    fs::remove_file(&failed).with_context(|| format!("failed to remove {}", failed.display()))
}
#[cfg(not(target_os = "windows"))]
fn restore_existing(
    backup: &Path,
    target: &Path,
    install_dir: &Path,
    _role: TargetRole,
) -> Result<()> {
    fs::rename(backup, target)
        .with_context(|| format!("failed to restore {}", target.display()))?;
    sync_parent(install_dir)
}
#[cfg(target_os = "windows")]
fn powershell_file_replace(source: &Path, target: &Path, backup: Option<&Path>) -> Result<()> {
    let command = "$backup=$env:FLISTWALKER_REPLACE_BACKUP;if([string]::IsNullOrEmpty($backup)){$backup=$null};[System.IO.File]::Replace($env:FLISTWALKER_REPLACE_SOURCE,$env:FLISTWALKER_REPLACE_TARGET,$backup,$false)";
    let mut process = Command::new("powershell.exe");
    process
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .env("FLISTWALKER_REPLACE_SOURCE", powershell_path(source))
        .env("FLISTWALKER_REPLACE_TARGET", powershell_path(target))
        .env(
            "FLISTWALKER_REPLACE_BACKUP",
            backup.map(powershell_path).unwrap_or_default(),
        );
    let status = process
        .status()
        .context("failed to launch PowerShell File.Replace")?;
    if !status.success() {
        bail!("PowerShell File.Replace failed with {status}");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn powershell_path(path: &Path) -> std::ffi::OsString {
    let value = path.as_os_str().to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}").into();
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).into()
}

fn revalidate_operation_paths(
    install_dir: &Path,
    target: &Path,
    prepared: &Path,
    backup: &Path,
    record: &TargetRecord,
) -> Result<()> {
    revalidate_transaction_directory(install_dir)?;
    for path in [target, prepared, backup] {
        if path.parent() != Some(install_dir) {
            bail!("transaction path escaped executable directory");
        }
    }
    validate_regular_file(prepared, "prepared update file")?;
    validate_target_if_present(target, "installation target")?;
    if backup.try_exists().unwrap_or(false) {
        bail!("update backup already exists");
    }
    if target.try_exists().unwrap_or(false) != record.originally_present {
        bail!("installation target presence changed during update");
    }
    if sha256_file(prepared)? != record.new_hash {
        bail!(
            "prepared update new hash changed for {}",
            record.role.label()
        );
    }
    if record.originally_present {
        let expected = record
            .old_hash
            .as_deref()
            .context("existing target is missing its old hash")?;
        if sha256_file(target)? != expected {
            bail!(
                "installation target old hash changed for {}",
                record.role.label()
            );
        }
    }
    Ok(())
}

fn revalidate_rollback_paths(
    install_dir: &Path,
    marker: &TransactionMarker,
    target: &Path,
    backup: &Path,
    record: &TargetRecord,
) -> Result<()> {
    revalidate_transaction_directory(install_dir)?;
    let failed = failed_path(install_dir, &marker.transaction_id, record.role);
    for path in [target, backup, &failed] {
        if path.parent() != Some(install_dir) {
            bail!("rollback path escaped executable directory");
        }
    }
    validate_target_if_present(target, "rollback target")?;
    if backup.try_exists().unwrap_or(false) {
        validate_regular_file(backup, "rollback backup")?;
    }
    if failed.try_exists().unwrap_or(false) {
        validate_regular_file(&failed, "rollback failed-target evidence")?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn revalidate_rollback_hashes(
    install_dir: &Path,
    marker: &TransactionMarker,
    target: &Path,
    backup: &Path,
    record: &TargetRecord,
    expected_target: Option<&str>,
    expected_backup: Option<&str>,
) -> Result<()> {
    revalidate_rollback_paths(install_dir, marker, target, backup, record)?;
    if hash_if_regular(target)?.as_deref() != expected_target {
        bail!(
            "rollback target hash changed immediately before mutation for {}",
            record.role.label()
        );
    }
    if hash_if_regular(backup)?.as_deref() != expected_backup {
        bail!(
            "rollback backup hash changed immediately before mutation for {}",
            record.role.label()
        );
    }
    Ok(())
}

fn revalidate_transaction_directory(install_dir: &Path) -> Result<()> {
    validate_directory(install_dir, "transaction directory")?;
    if install_dir
        .canonicalize()
        .context("failed to revalidate transaction directory")?
        != install_dir
    {
        bail!("transaction directory identity changed");
    }
    Ok(())
}
fn validate_ack(install_dir: &Path, marker: &TransactionMarker, token: &str) -> Result<()> {
    validate_regular_file(
        &ack_path(install_dir, &marker.transaction_id),
        "helper acknowledgement",
    )?;
    let expected = format!("{}\n{}\n", marker.transaction_id, token);
    let actual = fs::read_to_string(ack_path(install_dir, &marker.transaction_id))
        .context("helper acknowledgement is missing")?;
    if actual != expected {
        bail!("helper acknowledgement does not match transaction");
    }
    Ok(())
}
fn validated_marker_parent(path: &Path, marker: &TransactionMarker) -> Result<PathBuf> {
    validate_marker(marker)?;
    if path.file_name().and_then(|value| value.to_str()) != Some(MARKER_FILE_NAME) {
        bail!("transaction marker path is not fixed");
    }
    let canonical = path
        .parent()
        .context("transaction marker has no parent")?
        .canonicalize()
        .context("failed to canonicalize transaction directory")?;
    validate_directory(&canonical, "transaction directory")?;
    Ok(canonical)
}
fn validate_marker(marker: &TransactionMarker) -> Result<()> {
    if marker.version != MARKER_VERSION {
        bail!("unsupported transaction marker version");
    }
    validate_transaction_id(&marker.transaction_id)?;
    if !is_safe_basename(&marker.binary_name) {
        bail!("invalid marker binary name");
    }
    if marker.targets.len() != TargetRole::ORDER.len()
        || marker
            .targets
            .iter()
            .map(|record| record.role)
            .ne(TargetRole::ORDER)
    {
        bail!("invalid marker target role order");
    }
    if marker.parent_pid == 0 || !is_sha256(&marker.helper_hash) {
        bail!("invalid marker process or helper identity");
    }
    for record in &marker.targets {
        if !is_sha256(&record.new_hash)
            || record.originally_present != record.old_hash.is_some()
            || record
                .old_hash
                .as_deref()
                .is_some_and(|hash| !is_sha256(hash))
        {
            bail!("invalid marker target hash contract");
        }
    }
    match marker.phase {
        Phase::PreparedParentOwned => {
            if marker.helper_pid.is_some()
                || marker.helper_start_token.is_some()
                || !all_states(&marker.targets, TargetState::Prepared)
            {
                bail!("invalid parent-owned transaction state");
            }
        }
        Phase::HelperRegistered => {
            validate_registered_helper(marker)?;
            if !all_states(&marker.targets, TargetState::Prepared) {
                bail!("registered helper cannot have mutated targets");
            }
        }
        Phase::ApplyingSidecars => {
            validate_registered_helper(marker)?;
            if marker.targets[3].state != TargetState::Prepared
                || !is_forward_prefix(&marker.targets[..3])
            {
                bail!("invalid sidecar application state");
            }
        }
        Phase::BinaryIntent => {
            validate_registered_helper(marker)?;
            if !marker.targets[..3]
                .iter()
                .all(|record| record.state == TargetState::Applied)
                || !matches!(
                    marker.targets[3].state,
                    TargetState::Prepared | TargetState::Intent | TargetState::Applied
                )
            {
                bail!("invalid binary commit-intent state");
            }
        }
        Phase::BinaryCommitted => {
            validate_registered_helper(marker)?;
            if !all_states(&marker.targets, TargetState::Applied) {
                bail!("committed transaction must contain only applied targets");
            }
        }
        Phase::RollingBack => {
            validate_optional_helper(marker)?;
            if !is_rollback_suffix(&marker.targets) {
                bail!("invalid rollback transition");
            }
        }
        Phase::RolledBack => {
            validate_optional_helper(marker)?;
            if !all_states(&marker.targets, TargetState::RolledBack) {
                bail!("rolled-back transaction has incomplete target state");
            }
        }
    }
    Ok(())
}
fn validate_registered_helper(marker: &TransactionMarker) -> Result<()> {
    if marker.helper_pid.is_none()
        || marker.helper_pid == Some(0)
        || marker
            .helper_start_token
            .as_deref()
            .is_none_or(|token| validate_start_token(token).is_err())
    {
        bail!("invalid registered helper identity");
    }
    Ok(())
}
fn validate_optional_helper(marker: &TransactionMarker) -> Result<()> {
    match (&marker.helper_pid, &marker.helper_start_token) {
        (None, None) => Ok(()),
        (Some(_), Some(_)) => validate_registered_helper(marker),
        _ => bail!("incomplete helper identity"),
    }
}
fn all_states(targets: &[TargetRecord], expected: TargetState) -> bool {
    targets.iter().all(|record| record.state == expected)
}
fn is_forward_prefix(targets: &[TargetRecord]) -> bool {
    let mut stage = 0u8;
    for record in targets {
        stage = match (stage, record.state) {
            (0, TargetState::Applied) => 0,
            (0, TargetState::Intent) => 1,
            (0 | 1, TargetState::Prepared) => 2,
            (2, TargetState::Prepared) => 2,
            _ => return false,
        };
    }
    true
}
fn is_rollback_suffix(targets: &[TargetRecord]) -> bool {
    let split = targets
        .iter()
        .position(|record| record.state == TargetState::RolledBack)
        .unwrap_or(targets.len());
    is_forward_prefix(&targets[..split])
        && targets[split..]
            .iter()
            .all(|record| record.state == TargetState::RolledBack)
}
fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn validate_transaction_id(value: &str) -> Result<()> {
    if value.len() != 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("transaction ID must be 32 lowercase hexadecimal characters");
    }
    Ok(())
}
fn validate_start_token(value: &str) -> Result<()> {
    if value.len() < 16
        || value.len() > 128
        || !value.is_ascii()
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        bail!("invalid helper start token");
    }
    Ok(())
}
fn is_safe_basename(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.bytes().any(|byte| byte.is_ascii_control())
}
fn validate_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        bail!("{label} must be a non-link directory");
    }
    Ok(())
}
fn validate_regular_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        bail!("{label} must be a non-link regular file");
    }
    Ok(())
}
fn validate_target_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && !is_reparse_point(&metadata) =>
        {
            Ok(())
        }
        Ok(_) => bail!("{label} must be a non-link regular file when present"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
fn reject_existing(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!("{label} already exists"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
#[cfg(target_os = "windows")]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(target_os = "windows"))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn create_new_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
}
fn copy_new_synced(source: &Path, destination: &Path) -> Result<String> {
    let mut input =
        File::open(source).with_context(|| format!("failed to open {}", source.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", source.display()))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .with_context(|| format!("failed to write {}", destination.display()))?;
        hasher.update(&buffer[..count]);
    }
    let permissions = fs::metadata(source)
        .with_context(|| format!("failed to read permissions {}", source.display()))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .with_context(|| format!("failed to set permissions {}", destination.display()))?;
    output
        .sync_all()
        .with_context(|| format!("failed to sync {}", destination.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}
fn sha256_file(path: &Path) -> Result<String> {
    let mut input =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn hash_if_regular(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && !is_reparse_point(&metadata) =>
        {
            Ok(Some(sha256_file(path)?))
        }
        Ok(_) => bail!(
            "transaction target is not a regular file: {}",
            path.display()
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
#[cfg(all(unix, not(target_os = "macos")))]
fn sync_parent(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn sync_parent(_path: &Path) -> Result<()> {
    Ok(())
}

fn new_path(dir: &Path, transaction_id: &str, role: TargetRole) -> PathBuf {
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-{}.new",
        role.label()
    ))
}
fn backup_path(dir: &Path, transaction_id: &str, role: TargetRole) -> PathBuf {
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-{}.bak",
        role.label()
    ))
}
fn failed_path(dir: &Path, transaction_id: &str, role: TargetRole) -> PathBuf {
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-{}.failed",
        role.label()
    ))
}
fn helper_path(dir: &Path, transaction_id: &str) -> PathBuf {
    let extension = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-helper{extension}"
    ))
}
fn ack_path(dir: &Path, transaction_id: &str) -> PathBuf {
    dir.join(format!(".flistwalker-update-{transaction_id}.ack"))
}
fn marker_temp_path(dir: &Path, transaction_id: &str) -> PathBuf {
    dir.join(format!(".flistwalker-update-{transaction_id}.marker.tmp"))
}
fn target_path(dir: &Path, marker: &TransactionMarker, role: TargetRole) -> PathBuf {
    dir.join(role.target_name(&marker.binary_name))
}

pub(super) fn prepare_transaction(
    current_exe: &Path,
    sources: TransactionSources<'_>,
) -> Result<PreparedTransaction> {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    prepare_transaction_with_id(current_exe, sources, &hex_bytes(&bytes), std::process::id())
}

pub(super) fn new_start_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex_bytes(&bytes)
}

impl PreparedTransaction {
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(super) fn acknowledgement_matches(&self, token: &str) -> bool {
        fs::read_to_string(&self.ack_path)
            .ok()
            .is_some_and(|value| value == format!("{}\n{}\n", self.transaction_id, token))
    }
}

pub(super) fn run_internal_helper(
    marker_path: &Path,
    transaction_id: &str,
    start_token: &str,
) -> Result<()> {
    validate_transaction_id(transaction_id)?;
    validate_start_token(start_token)?;
    let actual_helper = std::env::current_exe().context("failed to resolve helper executable")?;
    let deadline = std::time::Instant::now()
        .checked_add(Duration::from_secs(10))
        .context("helper registration deadline overflow")?;
    let probe = RealProcessControl;
    loop {
        let marker = read_marker(marker_path)?;
        if marker.transaction_id != transaction_id {
            bail!("helper transaction ID does not match marker");
        }
        match marker.phase {
            Phase::PreparedParentOwned => {
                if !probe.is_alive(marker.parent_pid) {
                    bail!("parent exited before durable helper registration");
                }
            }
            Phase::HelperRegistered => {
                if marker.helper_pid != Some(std::process::id())
                    || marker.helper_start_token.as_deref() != Some(start_token)
                {
                    bail!("durable helper registration identity mismatch");
                }
                acknowledge_registered_helper(
                    marker_path,
                    std::process::id(),
                    start_token,
                    &actual_helper,
                )?;
                let mut process = RealProcessControl;
                let mut failures = NoFailure;
                return execute_registered_transaction(
                    marker_path,
                    start_token,
                    &mut process,
                    &mut failures,
                );
            }
            _ => bail!("helper observed an invalid pre-ack transaction phase"),
        }
        if std::time::Instant::now() >= deadline {
            bail!("timed out waiting for durable helper registration");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn recover_current_installation(current_exe: &Path) -> Result<Option<RecoveryOutcome>> {
    validate_regular_file(current_exe, "current executable")?;
    let canonical = current_exe
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", current_exe.display()))?;
    let install_dir = canonical
        .parent()
        .context("current executable has no parent")?;
    let marker_path = install_dir.join(MARKER_FILE_NAME);
    let probe = RealProcessControl;
    if !marker_path.try_exists().unwrap_or(false) {
        if install_dir
            .join(LOCK_FILE_NAME)
            .try_exists()
            .unwrap_or(false)
        {
            return recover_orphan_preparation(install_dir, &probe).map(Some);
        }
        return Ok(None);
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let outcome = recover_transaction(&marker_path, &probe)?;
        if outcome == RecoveryOutcome::Ambiguous {
            bail!(
                "ambiguous updater transaction preserved for operator recovery: marker={}, lock={}",
                marker_path.display(),
                install_dir.join(LOCK_FILE_NAME).display()
            );
        }
        if outcome != RecoveryOutcome::Deferred {
            return Ok(Some(outcome));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(Some(RecoveryOutcome::Deferred));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn recover_orphan_preparation(
    install_dir: &Path,
    process_probe: &impl ProcessProbe,
) -> Result<RecoveryOutcome> {
    validate_directory(install_dir, "transaction directory")?;
    let (transaction_id, parent_pid) = read_lock_record(install_dir)?;
    if process_probe.is_alive(parent_pid) && !process_probe.is_current_process(parent_pid) {
        return Ok(RecoveryOutcome::Deferred);
    }
    let derived_artifacts = TargetRole::ORDER
        .into_iter()
        .flat_map(|role| {
            [
                new_path(install_dir, &transaction_id, role),
                backup_path(install_dir, &transaction_id, role),
                failed_path(install_dir, &transaction_id, role),
            ]
        })
        .chain([
            ack_path(install_dir, &transaction_id),
            helper_path(install_dir, &transaction_id),
            marker_temp_path(install_dir, &transaction_id),
            marker_temp_path(install_dir, &transaction_id).with_extension("previous"),
        ])
        .collect::<Vec<_>>();
    if derived_artifacts
        .iter()
        .any(|path| fs::symlink_metadata(path).is_ok())
    {
        bail!(
            "orphan updater preparation artifacts preserved for operator recovery: lock={}, directory={}",
            install_dir.join(LOCK_FILE_NAME).display(),
            install_dir.display()
        );
    }
    remove_file_if_present(&install_dir.join(LOCK_FILE_NAME))?;
    sync_parent(install_dir)?;
    Ok(RecoveryOutcome::RolledBack)
}

fn read_lock_record(install_dir: &Path) -> Result<(String, u32)> {
    let lock = install_dir.join(LOCK_FILE_NAME);
    validate_regular_file(&lock, "transaction lock")?;
    let contents = fs::read_to_string(&lock)
        .with_context(|| format!("failed to read transaction lock {}", lock.display()))?;
    let mut lines = contents.lines();
    let transaction_id = lines.next().context("orphan transaction ID is missing")?;
    validate_transaction_id(transaction_id)?;
    let parent_pid = lines
        .next()
        .context("orphan transaction owner PID is missing")?
        .parse::<u32>()
        .context("orphan transaction owner PID is invalid")?;
    if parent_pid == 0 || lines.next().is_some() {
        bail!("orphan transaction lock format is invalid");
    }
    Ok((transaction_id.to_string(), parent_pid))
}

struct NoFailure;
impl FailureInjector for NoFailure {}

struct RealProcessControl;

impl ProcessProbe for RealProcessControl {
    fn is_alive(&self, pid: u32) -> bool {
        process_is_alive(pid)
    }

    fn is_current_process(&self, pid: u32) -> bool {
        pid == std::process::id()
    }

    fn executable_matches(&self, pid: u32, expected: &Path) -> bool {
        process_executable_matches(pid, expected)
    }
}

impl ProcessControl for RealProcessControl {
    fn wait_for_exit(&mut self, pid: u32, timeout: Duration) -> Result<bool> {
        wait_for_process_exit(pid, timeout)
    }

    fn restart(&mut self, target: &Path) -> Result<()> {
        restart_target(target)
    }
}

#[cfg(target_os = "windows")]
fn process_is_alive(pid: u32) -> bool {
    wait_for_process_exit(pid, Duration::ZERO)
        .map(|exited| !exited)
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<bool> {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
    }
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        const ERROR_INVALID_PARAMETER: u32 = 87;
        let error = unsafe { GetLastError() };
        if error == ERROR_INVALID_PARAMETER {
            return Ok(true);
        }
        bail!("OpenProcess failed while waiting for PID {pid}: Windows error {error}");
    }
    let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
    let wait = unsafe { WaitForSingleObject(handle, milliseconds) };
    let _ = unsafe { CloseHandle(handle) };
    match wait {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        other => bail!("WaitForSingleObject failed with code {other}"),
    }
}

#[cfg(target_os = "windows")]
fn process_executable_matches(pid: u32, expected: &Path) -> bool {
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::OsStringExt;
    type Handle = *mut c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            path: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut buffer = vec![0u16; 32_768];
    let mut size = buffer.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    let _ = unsafe { CloseHandle(handle) };
    if success == 0 {
        return false;
    }
    let actual = PathBuf::from(OsString::from_wide(&buffer[..size as usize]));
    actual.canonicalize().ok() == expected.canonicalize().ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_is_alive(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_executable_matches(pid: u32, expected: &Path) -> bool {
    fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .and_then(|path| path.canonicalize().ok())
        == expected.canonicalize().ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<bool> {
    let deadline = std::time::Instant::now() + timeout;
    while process_is_alive(pid) {
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(true)
}

#[cfg(target_os = "macos")]
fn process_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn process_executable_matches(_pid: u32, _expected: &Path) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn wait_for_process_exit(_pid: u32, _timeout: Duration) -> Result<bool> {
    Ok(true)
}

#[cfg(target_os = "windows")]
fn restart_target(target: &Path) -> Result<()> {
    Command::new(target)
        .spawn()
        .with_context(|| format!("failed to restart {}", target.display()))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn restart_target(target: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let error = Command::new(target).exec();
    Err(error).with_context(|| format!("failed to exec {}", target.display()))
}

#[cfg(target_os = "macos")]
fn restart_target(_target: &Path) -> Result<()> {
    bail!("macOS auto-update is unsupported")
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}
