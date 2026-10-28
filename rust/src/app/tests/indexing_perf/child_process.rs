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
    if name == NATIVE_DIAG_LEAF {
        relay_native_writer_events(&outcome.output, outcome.passed);
    }
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

const NATIVE_DIAG_LEAF: &str = "app::tests::indexing_perf::harness::extensions::tc_229_all_tab_profile_controls_preserve_own_expected_state";
const NATIVE_DIAG_PREFIX: &str = "NATIVE_WRITER_OBSERVATION ";
const NATIVE_PROFILES: [&str; 8] = [
    "T1-active-warm",
    "T1-promotion",
    "T1-A-B-C-A",
    "T1-S1-selective",
    "T1-S1-dense",
    "T1-S2",
    "T1-natural-reclaim",
    "P1-preview",
];

fn scalar_sample_valid(value: &serde_json::Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    if map.len() != 5
        || ![
            "platform",
            "cpu_100ns",
            "io_pending",
            "cpu_error",
            "io_error",
        ]
        .iter()
        .all(|key| map.contains_key(*key))
    {
        return false;
    }
    ["windows", "unsupported", "missing-handle"]
        .contains(&value["platform"].as_str().unwrap_or_default())
        && (value["cpu_100ns"].is_null() || value["cpu_100ns"].as_u64().is_some())
        && (value["io_pending"].is_null() || value["io_pending"].as_bool().is_some())
        && ["cpu_error", "io_error"].iter().all(|key| {
            value[*key].is_null() || value[*key].as_u64().is_some_and(|n| n <= u32::MAX.into())
        })
}
fn finite_native_event(value: &serde_json::Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    let keys = [
        "schema",
        "kind",
        "wait_decision",
        "wait_sequence",
        "wall_before_us",
        "wall_after_us",
        "worker_path_fingerprint",
        "worker_rust_thread_fingerprint",
        "phase",
        "last_io_checkpoint",
        "sample_before",
        "sample_after",
        "classification",
    ];
    let checkpoint = value["last_io_checkpoint"].as_str();
    let checkpoints = [
        "before-sidecar-parent-create",
        "before-sidecar-open",
        "before-sidecar-try-lock",
        "before-sidecar-retry-delay",
        "before-document-read",
        "before-document-decode",
        "before-last-root-canonicalize",
        "before-default-root-canonicalize",
        "before-document-serialize",
        "before-atomic-ancestor-probes",
        "before-atomic-parent-create",
        "before-atomic-temp-open",
        "before-atomic-file-write",
        "before-atomic-permissions",
        "before-atomic-file-times",
        "before-atomic-file-sync",
        "before-atomic-replace-hook",
        "before-atomic-file-close",
        "before-atomic-replace",
        "before-atomic-parent-sync",
        "before-result-publication",
    ];
    map.len() == keys.len()
        && keys.iter().all(|key| map.contains_key(*key))
        && value["schema"] == 1
        && value["kind"] == "owned-thread-samples"
        && ["timeout", "finished"].contains(&value["wait_decision"].as_str().unwrap_or_default())
        && value["wait_sequence"].as_u64().is_some_and(|n| n > 0)
        && value["wall_before_us"]
            .as_u64()
            .zip(value["wall_after_us"].as_u64())
            .is_some_and(|(a, b)| b >= a)
        && value["worker_path_fingerprint"].as_str().is_some_and(|s| {
            s.len() == 16
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        && value["worker_rust_thread_fingerprint"]
            .as_u64()
            .is_some_and(|n| n > 0)
        && [
            "starting",
            "waiting-command",
            "pre-io-gate",
            "write-result",
            "result-published",
            "returning",
            "worker-body-returned",
        ]
        .contains(&value["phase"].as_str().unwrap_or_default())
        && (value["last_io_checkpoint"].is_null()
            || checkpoint.is_some_and(|c| checkpoints.contains(&c)))
        && scalar_sample_valid(&value["sample_before"])
        && scalar_sample_valid(&value["sample_after"])
        && [
            "unsupported",
            "query-incomplete",
            "counter-inconsistent",
            "io-pending-observed",
            "cpu-progress-no-positive-io-sample",
            "no-io-sample-no-cpu-progress",
        ]
        .contains(&value["classification"].as_str().unwrap_or_default())
}
fn native_relay_rows(output: &str, passed: bool) -> (Vec<serde_json::Value>, serde_json::Value) {
    let mut context = None;
    let mut profiles = std::collections::HashSet::new();
    let mut owners = std::collections::HashMap::<(String, String), (String, u64)>::new();
    let mut rows = Vec::new();
    let mut owner_contexts = std::collections::HashMap::new();
    let mut issues = std::collections::BTreeSet::new();
    for line in output.lines() {
        if let Some(marker) = line.strip_prefix("EXTENSION_CONTROL_SMOKE ") {
            let parts = marker.split_whitespace().collect::<Vec<_>>();
            if parts.len() == 2
                && NATIVE_PROFILES.contains(&parts[0])
                && ["FileList", "Walker"].contains(&parts[1])
            {
                let key = (parts[0].to_owned(), parts[1].to_owned());
                if !profiles.insert(key.clone()) {
                    issues.insert("duplicate-profile");
                }
                context = Some(key);
            } else {
                context = None;
                issues.insert("invalid-profile");
            }
        }
        if let Some(raw) = line.strip_prefix(NATIVE_DIAG_PREFIX) {
            if rows.len() >= 32 {
                issues.insert("overflow");
                continue;
            }
            if raw.len() > 2048 {
                issues.insert("oversize");
                continue;
            }
            let Some(key) = context.as_ref() else {
                issues.insert("missing-context");
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
                issues.insert("invalid-json");
                continue;
            };
            if !finite_native_event(&value) {
                issues.insert("invalid-schema");
                continue;
            }
            let owner = value["worker_path_fingerprint"]
                .as_str()
                .unwrap()
                .to_owned();
            let sequence = value["wait_sequence"].as_u64().unwrap();
            match owners.get(key) {
                None if sequence == 1 => {}
                Some((old, seq)) if old == &owner && seq.checked_add(1) == Some(sequence) => {}
                _ => {
                    issues.insert("owner-or-sequence");
                    continue;
                }
            }
            if owner_contexts.get(&owner).is_some_and(|old| old != key) {
                issues.insert("cross-context-owner");
                continue;
            }
            owner_contexts.insert(owner.clone(), key.clone());
            if sequence > 2 {
                issues.insert("profile-event-count");
            }
            owners.insert(key.clone(), (owner, sequence));
            rows.push(serde_json::json!({"schema":1,"leaf":NATIVE_DIAG_LEAF,"role":"normal","profile":key.0,"source":key.1,"observation":value}));
        }
    }
    let expected = NATIVE_PROFILES.len() * 2;
    if passed && (profiles.len() != expected || owners.len() != expected || rows.len() < expected) {
        issues.insert("missing-success-events");
    }
    let summary = serde_json::json!({"schema":1,"leaf":NATIVE_DIAG_LEAF,"role":"normal","expected_profiles":expected,"profiles":profiles.len(),"events":rows.len(),"transport_complete":passed && issues.is_empty(),"issues":issues});
    (rows, summary)
}
fn relay_native_writer_events(output: &str, passed: bool) {
    use std::io::Write;
    let (rows, summary) = native_relay_rows(output, passed);
    // The child has physically exited and its fixture cleanup is complete.
    // Direct Write intentionally exports bounded events even on successful tests.
    let mut stderr = std::io::stderr().lock();
    for row in rows {
        let _ = writeln!(stderr, "NATIVE_WRITER_RELAY_EVENT {row}");
    }
    let _ = writeln!(stderr, "NATIVE_WRITER_RELAY_SUMMARY {summary}");
}
#[cfg(test)]
mod native_relay_tests {
    use super::*;
    fn event(sequence: u64) -> String {
        let sample = serde_json::json!({"platform":"unsupported","cpu_100ns":null,"io_pending":null,"cpu_error":null,"io_error":null});
        format!(
            "{NATIVE_DIAG_PREFIX}{}",
            serde_json::json!({"schema":1,"kind":"owned-thread-samples","wait_decision":"finished","wait_sequence":sequence,"wall_before_us":0,"wall_after_us":1,"worker_path_fingerprint":"0123456789abcdef","worker_rust_thread_fingerprint":1,"phase":"returning","last_io_checkpoint":null,"sample_before":sample,"sample_after":sample,"classification":"unsupported"})
        )
    }
    #[test]
    fn native_diag_relay_qualifies_all_successful_profiles() {
        let input = NATIVE_PROFILES
            .iter()
            .flat_map(|p| ["FileList", "Walker"].map(|s| (*p, s)))
            .enumerate()
            .map(|(i, (p, s))| {
                format!(
                    "EXTENSION_CONTROL_SMOKE {p} {s}\n{}",
                    event(1).replace("0123456789abcdef", &format!("{i:016x}"))
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (rows, summary) = native_relay_rows(&input, true);
        assert_eq!(rows.len(), NATIVE_PROFILES.len() * 2);
        assert_eq!(summary["transport_complete"], true);
    }
    #[test]
    fn native_diag_relay_preserves_existing_preview_profile() {
        // The first failing Windows log stopped after fourteen tab cases, but
        // the real ALL/tabs inventory continues with Preview in both sources.
        let input = format!("EXTENSION_CONTROL_SMOKE P1-preview FileList\n{}\nEXTENSION_CONTROL_SMOKE P1-preview Walker\n{}", event(1), event(1).replace("0123456789abcdef", "fedcba9876543210"));
        let (rows, summary) = native_relay_rows(&input, false);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row["profile"] == "P1-preview"));
        assert!(summary["issues"].as_array().unwrap().is_empty());
    }
    #[test]
    fn native_diag_relay_rejects_private_unknown_and_missing_fields() {
        let input = format!(
            "EXTENSION_CONTROL_SMOKE T1-promotion Walker\n{}\n{}",
            event(1),
            event(1)
        );
        let (_, summary) = native_relay_rows(&input, true);
        assert_eq!(summary["transport_complete"], false);
        assert!(summary["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "owner-or-sequence"));
        let mut value: serde_json::Value =
            serde_json::from_str(event(1).strip_prefix(NATIVE_DIAG_PREFIX).unwrap()).unwrap();
        value["raw_secret"] = "DO_NOT_FORWARD".into();
        let (rows, _) = native_relay_rows(
            &format!("EXTENSION_CONTROL_SMOKE T1-promotion Walker\n{NATIVE_DIAG_PREFIX}{value}"),
            false,
        );
        assert!(rows.is_empty());
    }
    #[test]
    fn native_diag_relay_caps_rows_and_marks_overflow() {
        let mut lines = vec!["EXTENSION_CONTROL_SMOKE T1-promotion Walker".into()];
        lines.extend((1..=33).map(event));
        let (rows, summary) = native_relay_rows(&lines.join("\n"), false);
        assert_eq!(rows.len(), 32);
        assert!(summary["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "overflow"));
    }
}
