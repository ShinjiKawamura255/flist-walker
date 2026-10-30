//! Temporary test-only capability query. Never starts a trace or changes settings.
mod query;
#[cfg(windows)]
mod windows_host;

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const CHILD_MODE: &str = "FW_TC229_CAPABILITY_CHILD";
const CHILD_ROOT: &str = "FW_TC229_CAPABILITY_ROOT";
const CHILD_TEST: &str = "test_windows_capability::owned_helper_entry";
const MAX_OUTPUT: u64 = 65536;
static NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Event {
    id: u16,
    version: u8,
    field: String,
    filter_build_status: u32,
    filter_cleanup_status: u32,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Provider {
    provider: String,
    status: u32,
    event_count: usize,
    events: Vec<Event>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Error {
    kind: String,
    provider: String,
    stage: String,
    event_id: Option<u16>,
    version: Option<u8>,
    code: Option<u32>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Report {
    mode: String,
    query_status: String,
    complete: bool,
    trace_started: bool,
    routing_verified: bool,
    error: Option<Error>,
    providers: Vec<Provider>,
}
fn incomplete(stage: &str, code: Option<u32>) -> String {
    serde_json::json!({"mode":"metadata-only","query_status":"incomplete","complete":false,
        "trace_started":false,"routing_verified":false,"providers":[],
        "error":{"kind":"integration","provider":"none","stage":stage,
        "event_id":null,"version":null,"code":code}})
    .to_string()
}
fn validate_report(bytes: &[u8], exit: Option<i32>) -> Result<Report, &'static str> {
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("output-limit");
    }
    let r: Report = serde_json::from_slice(bytes).map_err(|_| "typed-output")?;
    if r.mode != "metadata-only" || r.trace_started || r.routing_verified || r.providers.len() > 2 {
        return Err("report-contract");
    }
    let labels = ["system-io", "system-scheduler"];
    for (i, p) in r.providers.iter().enumerate() {
        if p.provider != labels[i]
            || p.status != 0
            || p.event_count == 0
            || p.event_count > 256
            || p.events.is_empty()
            || p.events.len() > 128
        {
            return Err("provider-contract");
        }
        for e in &p.events {
            if ![
                "TTID",
                "IssuingThreadId",
                "ThreadId",
                "OldThreadId",
                "NewThreadId",
            ]
            .contains(&e.field.as_str())
                || e.filter_build_status != 0
                || e.filter_cleanup_status != 0
            {
                return Err("field-contract");
            }
        }
    }
    if let Some(e) = &r.error {
        if !["api-error", "insufficient", "limit", "integration"].contains(&e.kind.as_str())
            || !["none", "system-io", "system-scheduler"].contains(&e.provider.as_str())
            || ![
                "provider-count",
                "time",
                "enumerate",
                "enumeration-layout",
                "empty-events",
                "event-count-or-layout",
                "event-metadata",
                "metadata-layout",
                "filter-build",
                "filter-null",
                "filter-cleanup",
                "event-output",
                "no-scalar-tid-field",
                "output",
                "sdk-discovery",
                "sdk-compiler",
                "sdk-layout",
                "sdk-command-time",
                "sdk-command-reap",
                "child-gate",
                "child-time",
                "child-mode",
            ]
            .contains(&e.stage.as_str())
        {
            return Err("error-contract");
        }
    }
    let good = r.query_status == "complete"
        && r.complete
        && r.error.is_none()
        && r.providers.len() == 2
        && exit == Some(0);
    let insufficient =
        r.query_status == "incomplete" && !r.complete && r.error.is_some() && exit == Some(78);
    if !good && !insufficient {
        return Err("exit-contract");
    }
    Ok(r)
}

struct Root {
    path: PathBuf,
    keep: bool,
}
impl Root {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "fwcap-{}-{}",
                std::process::id(),
                NONCE.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path, keep: false },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => panic!("capability owned root creation failed"),
            }
        }
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
struct Owned {
    child: Child,
    root: Root,
    reaped: bool,
    stopped: Option<bool>,
    cleanup_error: Option<CleanupError>,
    #[cfg(windows)]
    job: Option<windows_host::Job>,
}
impl Owned {
    fn stop(&mut self) -> bool {
        if let Some(result) = self.stopped {
            return result;
        }
        #[cfg(windows)]
        if let Some(job) = &self.job {
            if !checked_job_state(
                job.empty(),
                &mut self.cleanup_error,
                "job-empty-before-stop",
            ) {
                if let Err(code) = job.terminate() {
                    self.cleanup_error.get_or_insert(CleanupError {
                        stage: "job-terminate",
                        code,
                    });
                }
            }
        }
        if !self.reaped {
            let _ = self.child.kill();
        }
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if !self.reaped {
                self.reaped = matches!(self.child.try_wait(), Ok(Some(_)));
            }
            #[cfg(windows)]
            let descendants_gone = self.job.as_ref().is_none_or(|j| {
                checked_job_state(j.empty(), &mut self.cleanup_error, "job-empty-after-stop")
            });
            #[cfg(not(windows))]
            let descendants_gone = true; // Inert routes do not spawn descendants here.
            if self.reaped && descendants_gone {
                self.stopped = Some(true);
                return true;
            }
            if Instant::now() >= end {
                self.stopped = Some(false);
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.stop() {
            self.root.keep = true;
        }
    }
}
#[derive(Serialize)]
struct Observation {
    mode: &'static str,
    owner_pid: u32,
    child_pid: u32,
    child_birth_100ns: Option<u64>,
    wall_ms: u128,
    timed_out: bool,
    reaped: bool,
    owned_tree_empty: bool,
    fixture_cleaned: bool,
    exit_code: Option<i32>,
    transport_error: Option<&'static str>,
    transport_code: Option<u32>,
    cleanup_error: Option<CleanupError>,
    report: Option<Report>,
}
fn run_owned(mode: &str, timeout: Duration) -> Observation {
    assert!(
        std::env::var_os(CHILD_MODE).is_none(),
        "owned helper cannot spawn a controller"
    );
    let begin = Instant::now();
    let root = Root::new();
    let path = root.path.clone();
    let mut cmd = Command::new(std::env::current_exe().expect("owned test executable"));
    cmd.args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
        .env_clear()
        .env(CHILD_MODE, mode)
        .env(CHILD_ROOT, &path)
        .env("TEMP", &path)
        .env("TMP", &path)
        .env("TMPDIR", &path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for key in ["SystemRoot", "WINDIR", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    let child = cmd.spawn().expect("spawn capability owned helper");
    let pid = child.id();
    let mut owned = Owned {
        child,
        root,
        reaped: false,
        stopped: None,
        cleanup_error: None,
        #[cfg(windows)]
        job: None,
    };
    let mut error = None;
    let mut transport_code: Option<u32> = None;
    #[cfg(windows)]
    let birth = match windows_host::Job::attach(&owned.child) {
        Ok((job, birth)) => {
            owned.job = Some(job);
            Some(birth)
        }
        Err(code) => {
            error = Some("owned-job-or-birth");
            transport_code = Some(code);
            None
        }
    };
    #[cfg(not(windows))]
    let birth = None;
    if error.is_none() && fs::write(path.join("go"), b"owned").is_err() {
        error = Some("child-gate");
    }
    let mut timed_out = false;
    let mut exit = None;
    while error.is_none() {
        match owned.child.try_wait() {
            Ok(Some(status)) => {
                owned.reaped = true;
                exit = status.code();
                break;
            }
            Err(_) => {
                error = Some("child-poll");
                break;
            }
            Ok(None) => {}
        }
        if begin.elapsed() >= timeout {
            timed_out = true;
            error = Some("parent-wall-time");
            break;
        }
        if fs::metadata(path.join("report.json")).is_ok_and(|m| m.len() > MAX_OUTPUT) {
            error = Some("output-limit");
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let clean = owned.stop();
    let reaped = owned.reaped;
    let cleanup_error = owned.cleanup_error;
    if error.is_none() {
        if let Some(issue) = cleanup_error {
            error = Some(issue.stage);
            transport_code = Some(issue.code);
        }
    }
    if !clean {
        owned.root.keep = true;
        error = Some("owned-tree-reap");
    }
    let mut report = None;
    if clean && error.is_none() {
        let mut bytes = Vec::new();
        match fs::File::open(path.join("report.json"))
            .and_then(|f| f.take(MAX_OUTPUT + 1).read_to_end(&mut bytes))
        {
            Ok(_) => match validate_report(&bytes, exit) {
                Ok(r) => report = Some(r),
                Err(e) => error = Some(e),
            },
            Err(_) => error = Some("missing-report"),
        }
    }
    // Cleanup the same owned job/root before exporting; no raw child logs escape.
    drop(owned);
    Observation {
        mode: "metadata-only",
        owner_pid: std::process::id(),
        child_pid: pid,
        child_birth_100ns: birth,
        wall_ms: begin.elapsed().as_millis(),
        timed_out,
        reaped,
        owned_tree_empty: clean,
        fixture_cleaned: !path.exists(),
        exit_code: exit,
        transport_error: error,
        transport_code,
        cleanup_error,
        report,
    }
}
fn save_report(root: &Path, report: &str, code: u8) -> ! {
    if report.len() as u64 > MAX_OUTPUT {
        std::process::exit(78)
    }
    if fs::write(root.join("report.json"), report).is_err() {
        std::process::exit(78)
    }
    std::process::exit(i32::from(code))
}
#[test]
fn owned_helper_entry() {
    let Some(mode) = std::env::var_os(CHILD_MODE) else {
        return;
    };
    let start = Instant::now();
    let Some(root) = std::env::var_os(CHILD_ROOT).map(PathBuf::from) else {
        std::process::exit(78)
    };
    while !root.join("go").is_file() {
        if start.elapsed() >= Duration::from_secs(10) {
            save_report(&root, &incomplete("child-gate", None), 78)
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    match mode.to_str() {
        Some("inert-incomplete") => save_report(&root, &incomplete("child-mode", Some(5)), 78),
        Some("inert-hang") => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        Some("inert-overflow") => {
            let mut f = fs::File::create(root.join("report.json")).unwrap();
            f.write_all(&vec![b'x'; MAX_OUTPUT as usize + 1]).unwrap();
            std::process::exit(78)
        }
        #[cfg(all(windows, target_env = "msvc"))]
        Some("query") => {
            if let Err((stage, code)) = windows_host::compile_sdk(&root, start) {
                save_report(&root, &incomplete(stage, code), 78)
            }
            if start.elapsed() >= Duration::from_secs(10) {
                save_report(&root, &incomplete("child-time", None), 78)
            }
            let (report, code) = query::native::run(start);
            save_report(&root, &report, code)
        }
        _ => save_report(&root, &incomplete("child-mode", None), 78),
    }
}
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn windows_metadata_only_once() {
    let observation = run_owned("query", Duration::from_secs(20));
    let good = observation.transport_error.is_none()
        && observation.transport_code.is_none()
        && observation.cleanup_error.is_none()
        && observation.owned_tree_empty
        && observation.fixture_cleaned
        && observation.reaped
        && observation.report.as_ref().is_some_and(|r| r.complete);
    writeln!(
        std::io::stderr().lock(),
        "TC229_WINDOWS_CAPABILITY_OBSERVATION {}",
        serde_json::to_string(&observation).expect("typed observation")
    )
    .expect("capability export");
    assert!(
        good,
        "capability query incomplete; see typed observation; TC229 cause remains unknown"
    );
}
#[test]
fn incomplete_exit_is_preserved_and_owned_child_is_reaped() {
    let o = run_owned("inert-incomplete", Duration::from_secs(20));
    assert!(o.transport_error.is_none() && o.reaped && o.owned_tree_empty && o.fixture_cleaned);
    assert_eq!(o.exit_code, Some(78));
    assert!(!o.report.unwrap().complete);
}
#[test]
fn watchdog_kills_only_owned_inert_helper_and_reaps() {
    let o = run_owned("inert-hang", Duration::from_millis(100));
    assert!(o.timed_out && o.reaped && o.owned_tree_empty && o.fixture_cleaned);
    assert_eq!(o.transport_error, Some("parent-wall-time"));
}
#[test]
fn oversized_child_output_is_never_exported() {
    let o = run_owned("inert-overflow", Duration::from_secs(20));
    assert!(o.reaped && o.fixture_cleaned && o.report.is_none());
    assert_eq!(o.transport_error, Some("output-limit"));
}
#[test]
fn typed_output_rejects_foreign_data_and_inconsistent_success() {
    let mut v: serde_json::Value = serde_json::from_str(&incomplete("child-mode", None)).unwrap();
    v["private_path"] = "must-not-escape".into();
    assert!(validate_report(v.to_string().as_bytes(), Some(78)).is_err());
    let b = incomplete("child-mode", Some(5));
    assert!(validate_report(b.as_bytes(), Some(0)).is_err());
    let mut v: serde_json::Value = serde_json::from_str(&b).unwrap();
    v["error"]["stage"] = "sensitive-text".into();
    assert!(validate_report(v.to_string().as_bytes(), Some(78)).is_err());
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct CleanupError {
    stage: &'static str,
    code: u32,
}
fn checked_job_state(
    result: Result<bool, u32>,
    error: &mut Option<CleanupError>,
    stage: &'static str,
) -> bool {
    match result {
        Ok(empty) => empty,
        Err(code) => {
            error.get_or_insert(CleanupError { stage, code });
            false
        }
    }
}
#[test]
fn review_cleanup_error_is_not_erased_by_later_empty() {
    let mut error = None;
    assert!(!checked_job_state(
        Err(5),
        &mut error,
        "job-empty-before-stop"
    ));
    assert!(checked_job_state(
        Ok(true),
        &mut error,
        "job-empty-after-stop"
    ));
    assert_eq!(
        error,
        Some(CleanupError {
            stage: "job-empty-before-stop",
            code: 5
        })
    );
}
#[test]
fn review_output_limit_preserves_its_typed_error() {
    let (out, exit) = query::render_report(&["x".repeat(70000), "{}".into()], None);
    let r = validate_report(out.as_bytes(), Some(i32::from(exit)))
        .expect("output fallback has same typed schema");
    assert_eq!(r.error.unwrap().stage, "output");
}
