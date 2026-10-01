//! Keep shared storage and result decisions independent from GUI orchestration.
use std::path::{Path, PathBuf};

fn source_root() -> PathBuf {
    Path::new(option_env!("CARGO_MANIFEST_DIR").unwrap_or("rust")).join("src")
}

fn check_leaf_sources(path: &Path, forbidden: &[&str]) {
    for item in std::fs::read_dir(path).expect("module directory") {
        let path = item.expect("module entry").path();
        if path.file_stem().is_some_and(|stem| stem == "tests") {
            continue;
        }
        if path.is_dir() {
            check_leaf_sources(&path, forbidden);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).expect("Rust source");
            for token in forbidden {
                assert!(
                    !source.contains(token),
                    "{} depends on {token}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn common_persistence_does_not_depend_on_gui() {
    let root = source_root();
    let facade = std::fs::read_to_string(root.join("lib.rs")).unwrap();
    assert!(
        facade.contains("pub mod persistence;"),
        "persistence must own its implementation"
    );
    check_leaf_sources(
        &root.join("persistence"),
        &["crate::app", "FlistWalkerApp", "eframe", "egui"],
    );
}

#[test]
fn persistence_document_and_settings_do_not_control_worker_scheduling() {
    let root = source_root().join("persistence/worker");
    for owner in ["document.rs", "settings.rs"] {
        let source = std::fs::read_to_string(root.join(owner)).expect("persistence owner");
        for forbidden in [
            "std::sync",
            "std::thread",
            "PersistenceSender",
            "UiStatePersistenceCommand",
            "AdmissionState",
            "UI_STATE_PERSISTENCE",
        ] {
            assert!(!source.contains(forbidden), "{owner} controls {forbidden}");
        }
    }
    let actor = std::fs::read_to_string(source_root().join("persistence/worker.rs")).unwrap();
    for policy in [
        "fn build_ui_state_document",
        "fn merge_json_leaves",
        "fn restore_atomic_target",
    ] {
        assert!(!actor.contains(policy), "worker still owns {policy}");
    }
}

#[test]
fn result_policy_cannot_reach_application_or_workers() {
    let source = std::fs::read_to_string(source_root().join("app/result_policy.rs"))
        .expect("shared result policy module");
    for forbidden in [
        "FlistWalkerApp",
        "WorkerBus",
        "worker::",
        "eframe",
        "egui",
        ".shell",
    ] {
        assert!(
            !source.contains(forbidden),
            "result policy depends on {forbidden}"
        );
    }
}
