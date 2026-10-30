//! Only normal actual-worker tests use owned leaf processes. Perf runners stay
//! in their explicit serial measurement process; production behavior is intact.
use super::*;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD_TEST: &str = "FW_INDEX_PERF_NORMAL_CHILD";
const CHILD_ROLE: &str = "FW_INDEX_PERF_NORMAL_CHILD_ROLE";
const NORMAL_WATCHDOG: Duration = Duration::from_secs(180);
static NONCE: AtomicU64 = AtomicU64::new(0);

fn canonical(module: &str, name: &str) -> String {
    format!("{}::{name}", module.split_once("::").unwrap().1)
}
fn command(name: &str, role: &str, root: &Path) -> Command {
    // No process-global config or environment setter belongs in this function.
    let mut cmd = Command::new(std::env::current_exe().expect("current test executable"));
    cmd.args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(CHILD_TEST, name)
        .env(CHILD_ROLE, role)
        .env("TMPDIR", root)
        .env("TMP", root)
        .env("TEMP", root)
        .stdin(Stdio::null());
    cmd
}
struct OwnedRoot {
    path: PathBuf,
    keep: bool,
}
impl OwnedRoot {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "fipc-{}-{}",
                std::process::id(),
                NONCE.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path, keep: false },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create owned child root: {e}"),
            }
        }
    }
}
impl Drop for OwnedRoot {
    fn drop(&mut self) {
        if !self.keep {
            if let Err(e) = fs::remove_dir_all(&self.path) {
                eprintln!(
                    "owned child fixture cleanup failed {}: {e}",
                    self.path.display()
                );
            }
        }
    }
}
struct OwnedChild {
    child: Child,
    root: OwnedRoot,
    exited: bool,
}
impl OwnedChild {
    fn stop(&mut self) -> bool {
        if !self.exited {
            let _ = self.child.kill();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match self.child.try_wait() {
                    Ok(Some(_)) => {
                        self.exited = true;
                        break;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        }
        self.exited
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.stop() {
            // Never remove fixtures that a physically live owned child may use.
            self.root.keep = true;
            eprintln!(
                "owned child {} did not exit; retaining {}",
                self.child.id(),
                self.root.path.display()
            );
        }
    }
}
struct Outcome {
    passed: bool,
    reaped: bool,
    cleaned: bool,
    ready: bool,
    timed_out: bool,
    output: String,
    output_path: PathBuf,
}
fn successful_single_test(success: bool, output: &str) -> bool {
    let summaries = output
        .lines()
        .filter(|line| line.starts_with("test result:"))
        .collect::<Vec<_>>();
    success
        && summaries.len() == 1
        && summaries[0].starts_with("test result: ok. 1 passed; 0 failed;")
}
fn spawn(name: &str, role: &str, inherited_cap: Option<usize>, timeout: Duration) -> Outcome {
    assert!(
        std::env::var_os(CHILD_TEST).is_none(),
        "leaf cannot spawn a descendant"
    );
    let root = OwnedRoot::new();
    let root_path = root.path.clone();
    let fixtures = root.path.join("fixtures");
    fs::create_dir(&fixtures).unwrap();
    let logs =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/indexing-perf-study/normal-child");
    fs::create_dir_all(&logs).unwrap();
    let (output_path, file) = loop {
        let path = logs.join(format!(
            "{}-{}.log",
            std::process::id(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("create owned child output: {e}"),
        }
    };
    let mut cmd = command(name, role, &fixtures);
    if let Some(cap) = inherited_cap {
        cmd.env("FLISTWALKER_WALKER_MAX_ENTRIES", cap.to_string());
    }
    if role == "wrong-identity" {
        cmd.env(CHILD_TEST, "wrong-owned-identity");
    }
    // File redirection drains both streams without filling a pipe while waiting.
    cmd.stdout(Stdio::from(file.try_clone().unwrap()))
        .stderr(Stdio::from(file));
    let child = cmd.spawn().expect("spawn owned leaf test");
    let mut owned = OwnedChild {
        child,
        root,
        exited: false,
    };
    let begin = Instant::now();
    let mut ready_at = (role != "timeout").then_some(begin);
    let mut successful = false;
    let mut timed_out = false;
    loop {
        match owned.child.try_wait().expect("poll owned child") {
            Some(status) => {
                owned.exited = true;
                successful = status.success();
                break;
            }
            None => {
                if ready_at.is_none() && fixtures.join("watchdog-ready").exists() {
                    ready_at = Some(Instant::now());
                }
                if ready_at.is_some_and(|at| at.elapsed() >= timeout)
                    || ready_at.is_none() && begin.elapsed() >= Duration::from_secs(30)
                {
                    timed_out = true;
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    let reaped = owned.stop();
    let output = fs::read_to_string(&output_path).unwrap_or_default();
    let identity = format!("INDEX_PERF_CHILD_BODY {name}");
    let passed = successful_single_test(successful, &output)
        && output
            .lines()
            .filter(|line| line.ends_with(&identity))
            .count()
            == 1;
    drop(owned); // physical exit precedes owned fixture removal, including panic.
    let cleaned = !root_path.exists();
    if let Some(mut observation) = normal_writer_observation(name, role, &output, reaped) {
        observation["passed"] = passed.into();
        observation["reaped"] = reaped.into();
        observation["owned_fixture_cleaned"] = cleaned.into();
        // Publish only bounded, allowlisted observations after physical child exit.
        // Direct stderr survives libtest's successful-test capture; no worker timer
        // or original outcome is changed. Missing output is missing evidence.
        use std::io::Write;
        if let Err(error) = writeln!(
            std::io::stderr().lock(),
            "INDEX_PERF_CHILD_WRITER_OBSERVATION {observation}"
        ) {
            eprintln!("writer observation export failed: {error}");
        }
    }
    eprintln!(
        "INDEX_PERF_CHILD {}",
        serde_json::json!({"test":name,"role":role,"passed":passed,"reaped":reaped,"owned_fixture_cleaned":cleaned,"ready":ready_at.is_some(),"watchdog_expired":timed_out,"output_path":output_path})
    );
    Outcome {
        passed,
        reaped,
        cleaned,
        ready: ready_at.is_some(),
        timed_out,
        output,
        output_path,
    }
}
fn child_identity_valid(name: &str, marker: &str, args: &[String]) -> bool {
    marker == name
        && args.windows(2).any(|a| a[0] == "--exact" && a[1] == name)
        && args.iter().any(|a| a == "--test-threads=1")
}
pub(in crate::app::tests::indexing_perf) fn isolate(module: &str, name: &str) -> bool {
    let name = canonical(module, name);
    if let Some(child_name) = std::env::var_os(CHILD_TEST) {
        assert!(
            child_identity_valid(
                &name,
                child_name.to_str().unwrap_or_default(),
                &std::env::args().skip(1).collect::<Vec<_>>()
            ),
            "wrong leaf identity or exact arguments"
        );
        // Only the matched leaf branch mutates its own process settings.
        crate::runtime_config::RuntimeConfig::default().apply_to_process_env();
        eprintln!("INDEX_PERF_CHILD_BODY {name}");
        eprintln!(
            "INDEX_PERF_CHILD_SETTINGS {}",
            serde_json::json!({"test":name,"leaf_pid":std::process::id(),"walker_cap":crate::runtime_config::current_runtime_config().walker_max_entries})
        );
        return false;
    }
    let outcome = spawn(&name, "normal", None, NORMAL_WATCHDOG);
    assert!(
        outcome.passed && outcome.reaped && outcome.cleaned,
        "owned normal test {name} failed ({}):\n{}",
        outcome.output_path.display(),
        outcome.output
    );
    true
}
#[test]
fn tc_229_leaf_process_inherited_cap_isolation() {
    let name = canonical(
        module_path!(),
        "tc_229_leaf_process_inherited_cap_isolation",
    );
    if std::env::var_os(CHILD_TEST).is_some() {
        let role = std::env::var(CHILD_ROLE).unwrap();
        if role == "normal" {
            // Controlled contamination stays inside this leaf and replays a
            // global-config application before the known-settings entrance.
            crate::runtime_config::RuntimeConfig::from_current_env().apply_to_process_env();
        }
        assert!(!isolate(
            module_path!(),
            "tc_229_leaf_process_inherited_cap_isolation"
        ));
        match role.as_str() {
            "panic" => {
                let _fixture = Fixture::new(32);
                panic!("intentional leaf failure");
            }
            "timeout" => {
                fs::write(
                    std::env::temp_dir().join("watchdog-ready"),
                    "owned fixture readiness",
                )
                .unwrap();
                loop {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            "normal" => {}
            _ => panic!("unsupported leaf probe role"),
        }
        let fixture = Fixture::new(16_384);
        let sample = run_sample(&fixture, Case::B0, Source::Walker);
        assert_eq!(sample.observation.entries_emitted, 16_384);
        assert_eq!(
            crate::runtime_config::current_runtime_config().walker_max_entries,
            500_000
        );
        // This simulation is isolated inside a leaf and creates a Command only;
        // it does not spawn or claim a real mutable parent's roundtrip state.
        let sentinel = crate::runtime_config::RuntimeConfig {
            walker_max_entries: 12_345,
            ..Default::default()
        };
        crate::runtime_config::set_process_runtime_config(sentinel);
        let simulated = command(&name, "normal", &std::env::temp_dir());
        assert_eq!(
            crate::runtime_config::current_runtime_config().walker_max_entries,
            12_345
        );
        assert!(simulated
            .get_envs()
            .any(|(key, value)| key == CHILD_TEST && value == Some(std::ffi::OsStr::new(&name))));
        assert!(simulated.get_args().any(|arg| arg == "--exact"));
        return;
    }
    let outcome = spawn(&name, "normal", Some(12_345), NORMAL_WATCHDOG);
    assert!(
        outcome.passed && outcome.reaped && outcome.cleaned,
        "inherited cap must not alter owned leaf workload:\n{}",
        outcome.output
    );
    let zero = spawn(
        "app::tests::indexing_perf::nonexistent_owned_test",
        "normal",
        None,
        NORMAL_WATCHDOG,
    );
    assert!(!zero.passed && zero.reaped && zero.cleaned);
    assert!(zero.output.contains("0 passed; 0 failed;"));
    for role in ["panic", "wrong-identity"] {
        let failed = spawn(&name, role, None, NORMAL_WATCHDOG);
        assert!(!failed.passed && failed.reaped && failed.cleaned);
        assert!(failed.output.contains(if role == "panic" {
            "intentional leaf failure"
        } else {
            "wrong leaf identity"
        }));
    }
    let timeout = spawn(&name, "timeout", None, Duration::from_millis(50));
    assert!(
        !timeout.passed && timeout.ready && timeout.timed_out && timeout.reaped && timeout.cleaned
    );
}
#[test]
fn tc_229_child_contract_rejects_zero_failure_and_wrong_recursion_identity() {
    assert!(successful_single_test(
        true,
        "test result: ok. 1 passed; 0 failed;"
    ));
    assert!(!successful_single_test(
        true,
        "test result: ok. 0 passed; 0 failed;"
    ));
    assert!(!successful_single_test(
        true,
        "test result: ok. 1 passed; 0 failed;\ntest result: ok. 0 passed; 0 failed;"
    ));
    assert!(!successful_single_test(
        false,
        "test result: ok. 1 passed; 0 failed;"
    ));
    let args = vec!["--exact".into(), "owned".into(), "--test-threads=1".into()];
    assert!(child_identity_valid("owned", "owned", &args));
    assert!(!child_identity_valid("owned", "other", &args));
    assert!(!child_identity_valid("owned", "owned", &[]));
}

fn normal_writer_observation(
    name: &str,
    role: &str,
    output: &str,
    reaped: bool,
) -> Option<serde_json::Value> {
    if role != "normal" || !reaped {
        return None;
    }
    let selected =
        name.ends_with("::tc_229_real_worker_extended_profiles_smoke_flat_files_warm_reclaim");
    let mut waits = Vec::new();
    let mut phases = Vec::new();
    let mut settings = None;
    let mut omitted = 0usize;
    let subset = |value: &serde_json::Value, fields: &[&str]| {
        serde_json::Value::Object(
            fields
                .iter()
                .filter_map(|key| value.get(*key).map(|v| ((*key).to_owned(), v.clone())))
                .collect(),
        )
    };
    for line in output.lines() {
        if let Some(raw) = line.strip_prefix("TEST_WRITER_WAIT_FAILURE ") {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
                if waits.len() == 64 {
                    omitted += 1;
                    continue;
                }
                let mut wait = subset(
                    &value,
                    &[
                        "stage",
                        "phase",
                        "worker_age_ms",
                        "elapsed_ms",
                        "registry_acquisition_ms",
                        "ownership_acquisition_ms",
                        "shutdown_sent",
                        "physical_finished",
                        "status_available",
                        "accepted_generation",
                        "persisted_generation",
                        "outstanding",
                        "last_error_present",
                    ],
                );
                if let Some(io) = value.get("io") {
                    wait["io"] = subset(
                        io,
                        &[
                            "worker_id",
                            "process_id",
                            "native_thread_id",
                            "stage",
                            "parent",
                            "stage_elapsed_ms",
                            "observer_injected",
                        ],
                    );
                }
                waits.push(wait);
            } else {
                omitted += 1;
            }
        } else if selected {
            if let Some(raw) = line.strip_prefix("INDEX_PERF_NORMAL_PHASE ") {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
                    if phases.len() < 32 {
                        phases.push(subset(
                            &value,
                            &[
                                "shape",
                                "partition",
                                "phase",
                                "elapsed_ms",
                                "profile",
                                "source",
                                "entries",
                            ],
                        ));
                    } else {
                        omitted += 1;
                    }
                } else {
                    omitted += 1;
                }
            } else if let Some(raw) = line.strip_prefix("INDEX_PERF_CHILD_SETTINGS ") {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
                    settings = Some(subset(&value, &["test", "leaf_pid", "walker_cap"]));
                } else {
                    omitted += 1;
                }
            }
        }
    }
    if !selected && waits.is_empty() && omitted == 0 {
        return None;
    }
    Some(
        serde_json::json!({"test":name,"waits":waits,"normal_phases":phases,"leaf_settings":settings,"omitted_or_invalid_records":omitted,"scope":"post-exit observation;not an OS stack or causal repair"}),
    )
}

#[test]
fn tc_229_writer_observation_exports_healthy_selected_leaf_and_preserves_identity() {
    let name = "app::tests::indexing_perf::harness::extensions::tc_229_real_worker_extended_profiles_smoke_flat_files_warm_reclaim";
    let report = normal_writer_observation(name, "normal",
        "private unrelated stdout\nINDEX_PERF_CHILD_SETTINGS {\"test\":\"leaf\",\"leaf_pid\":123,\"walker_cap\":500000,\"private\":\"omit\"}\nINDEX_PERF_NORMAL_PHASE {\"phase\":\"profile-completed\",\"elapsed_ms\":42,\"secret\":\"omit\"}\n", true).unwrap();
    assert_eq!(report["test"], name);
    assert_eq!(report["waits"].as_array().unwrap().len(), 0);
    assert_eq!(report["normal_phases"][0]["phase"], "profile-completed");
    assert_eq!(report["leaf_settings"]["leaf_pid"], 123);
    assert!(!report.to_string().contains("private") && !report.to_string().contains("secret"));
    assert!(normal_writer_observation(name, "normal", "", false).is_none());
    assert!(normal_writer_observation(name, "panic", "", true).is_none());
    assert!(normal_writer_observation("unselected", "normal", "", true).is_none());
}

#[test]
fn tc_229_writer_observation_limits_records_and_omits_unapproved_payload() {
    let line = "TEST_WRITER_WAIT_FAILURE {\"phase\":\"write-result\",\"elapsed_ms\":5000,\"private_content\":\"omit\",\"io\":{\"stage\":\"sync-temp\",\"native_thread_id\":42,\"private_path\":\"omit\"}}\n";
    let report = normal_writer_observation("unselected", "normal", &line.repeat(70), true).unwrap();
    assert_eq!(report["waits"].as_array().unwrap().len(), 64);
    assert_eq!(report["omitted_or_invalid_records"], 6);
    assert_eq!(report["waits"][0]["io"]["stage"], "sync-temp");
    assert_eq!(report["waits"][0]["io"]["native_thread_id"], 42);
    assert!(!report.to_string().contains("private"));
}
