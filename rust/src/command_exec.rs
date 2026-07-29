use crate::actions::{authorize_action_targets, reauthorize_action_target};
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

const PLACEHOLDER: &str = "{}";
#[cfg(windows)]
const WINDOWS_COMMAND_LINE_LIMIT: usize = 32_767;
#[cfg(unix)]
const POSIX_EXEC_SAFETY_MARGIN: usize = 2_048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandTemplate {
    program: OsString,
    before_paths: Vec<OsString>,
    after_paths: Vec<OsString>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandTemplateError(&'static str);

impl fmt::Display for CommandTemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for CommandTemplateError {}

impl CommandTemplate {
    pub fn parse(parts: &[OsString]) -> Result<Self, CommandTemplateError> {
        let Some(program) = parts.first() else {
            return Err(CommandTemplateError("--exec requires a command template"));
        };
        if program == OsStr::new(PLACEHOLDER) {
            return Err(CommandTemplateError(
                "--exec requires a fixed command before the {} placeholder",
            ));
        }
        #[cfg(windows)]
        if is_windows_batch_program(program) {
            return Err(CommandTemplateError(
                "--exec rejects direct .bat/.cmd programs; use cmd.exe /C explicitly",
            ));
        }
        let placeholder_indices = parts
            .iter()
            .enumerate()
            .filter_map(|(index, part)| (part == OsStr::new(PLACEHOLDER)).then_some(index))
            .collect::<Vec<_>>();
        if placeholder_indices.len() != 1 {
            return Err(CommandTemplateError(
                "--exec requires exactly one standalone {} placeholder",
            ));
        }
        let placeholder = placeholder_indices[0];
        if parts.iter().any(|part| {
            part != OsStr::new(PLACEHOLDER)
                && part.to_str().is_some_and(|text| text.contains(PLACEHOLDER))
        }) {
            return Err(CommandTemplateError(
                "the {} placeholder must be a standalone argument",
            ));
        }
        Ok(Self {
            program: program.clone(),
            before_paths: parts[1..placeholder].to_vec(),
            after_paths: parts[placeholder + 1..].to_vec(),
        })
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn args_for_paths(&self, paths: &[PathBuf]) -> Vec<OsString> {
        let mut args =
            Vec::with_capacity(self.before_paths.len() + paths.len() + self.after_paths.len());
        args.extend(self.before_paths.iter().cloned());
        args.extend(paths.iter().map(|path| path.as_os_str().to_os_string()));
        args.extend(self.after_paths.iter().cloned());
        args
    }

    fn fixed_args(&self) -> impl Iterator<Item = &OsStr> {
        std::iter::once(self.program.as_os_str())
            .chain(self.before_paths.iter().map(OsString::as_os_str))
            .chain(self.after_paths.iter().map(OsString::as_os_str))
    }
}

#[cfg(windows)]
fn is_windows_batch_program(program: &OsStr) -> bool {
    Path::new(program)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecOptions {
    pub max_paths_per_batch: Option<usize>,
    pub dry_run: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecOutcome {
    NoTargets,
    DryRun,
    Completed,
    Cancelled,
    Blocked,
    SpawnFailed,
    ChildFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecReport {
    pub outcome: ExecOutcome,
    pub completed_paths: usize,
    pub total_paths: usize,
    pub launched_batches: usize,
    pub planned_batches: usize,
    pub exit_code: Option<i32>,
    pub diagnostic: Option<String>,
}

impl ExecReport {
    fn new(outcome: ExecOutcome, completed_paths: usize, total_paths: usize) -> Self {
        Self {
            outcome,
            completed_paths,
            total_paths,
            launched_batches: 0,
            planned_batches: 0,
            exit_code: None,
            diagnostic: None,
        }
    }

    fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = Some(diagnostic.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildExit {
    Success,
    Failed(Option<i32>),
}

pub trait ExternalCommandBackend {
    fn run(&self, program: &OsStr, args: &[OsString]) -> std::io::Result<ChildExit>;
}

pub struct ProcessCommandBackend;

impl ExternalCommandBackend for ProcessCommandBackend {
    fn run(&self, program: &OsStr, args: &[OsString]) -> std::io::Result<ChildExit> {
        let status = Command::new(program).args(args).status()?;
        Ok(if status.success() {
            ChildExit::Success
        } else {
            ChildExit::Failed(status.code())
        })
    }
}

fn greedy_batch_ranges(
    path_costs: &[usize],
    fixed_cost: usize,
    budget: usize,
    max_paths_per_batch: Option<usize>,
) -> Result<Vec<Range<usize>>, &'static str> {
    if fixed_cost > budget {
        return Err("the fixed command arguments exceed the platform command-line limit");
    }
    if max_paths_per_batch == Some(0) {
        return Err("--exec-max-args must be greater than zero");
    }
    let max_paths = max_paths_per_batch.unwrap_or(usize::MAX);
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < path_costs.len() {
        let mut used = fixed_cost;
        let mut end = start;
        while end < path_costs.len() && end - start < max_paths {
            let Some(next) = used.checked_add(path_costs[end]) else {
                break;
            };
            if next > budget {
                break;
            }
            used = next;
            end += 1;
        }
        if end == start {
            return Err("one selected path exceeds the platform command-line limit");
        }
        ranges.push(start..end);
        start = end;
    }
    Ok(ranges)
}

#[cfg(unix)]
fn argument_cost(value: &OsStr) -> usize {
    use std::os::unix::ffi::OsStrExt;
    value
        .as_bytes()
        .len()
        .saturating_add(1)
        .saturating_add(std::mem::size_of::<usize>())
}

#[cfg(windows)]
fn argument_cost(value: &OsStr) -> usize {
    use std::os::windows::ffi::OsStrExt;
    let units = value.encode_wide().collect::<Vec<_>>();
    let requires_quotes = units.is_empty() || units.iter().any(|unit| matches!(*unit, 0x20 | 0x09));
    let mut encoded = usize::from(requires_quotes).saturating_mul(2);
    let mut backslashes = 0usize;
    for unit in units {
        if unit == b'\\' as u16 {
            backslashes = backslashes.saturating_add(1);
        } else if unit == b'"' as u16 {
            encoded = encoded
                .saturating_add(backslashes.saturating_mul(2))
                .saturating_add(2);
            backslashes = 0;
        } else {
            encoded = encoded.saturating_add(backslashes).saturating_add(1);
            backslashes = 0;
        }
    }
    encoded
        .saturating_add(if requires_quotes {
            backslashes.saturating_mul(2)
        } else {
            backslashes
        })
        .saturating_add(1)
}

#[cfg(not(any(unix, windows)))]
fn argument_cost(value: &OsStr) -> usize {
    value.to_string_lossy().len().saturating_add(1)
}

fn fixed_command_cost(template: &CommandTemplate) -> usize {
    template
        .fixed_args()
        .map(argument_cost)
        .fold(command_fixed_overhead(), usize::saturating_add)
}

#[cfg(unix)]
fn command_fixed_overhead() -> usize {
    std::mem::size_of::<usize>()
}

#[cfg(not(unix))]
fn command_fixed_overhead() -> usize {
    1
}

#[cfg(unix)]
fn system_command_budget() -> Result<usize, String> {
    use std::os::unix::ffi::OsStrExt;

    let raw_limit = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
    if raw_limit <= 0 {
        return Err("failed to query the platform ARG_MAX limit".to_string());
    }
    let pointer_size = std::mem::size_of::<usize>();
    let environment_cost = std::env::vars_os().fold(pointer_size, |used, (key, value)| {
        used.saturating_add(key.as_os_str().as_bytes().len())
            .saturating_add(1)
            .saturating_add(value.as_os_str().as_bytes().len())
            .saturating_add(1)
            .saturating_add(pointer_size)
    });
    (raw_limit as usize)
        .checked_sub(environment_cost)
        .and_then(|limit| limit.checked_sub(POSIX_EXEC_SAFETY_MARGIN))
        .ok_or_else(|| "the inherited environment leaves no command-line argument budget".into())
}

#[cfg(windows)]
fn system_command_budget() -> Result<usize, String> {
    Ok(WINDOWS_COMMAND_LINE_LIMIT)
}

#[cfg(not(any(unix, windows)))]
fn system_command_budget() -> Result<usize, String> {
    Ok(128 * 1024)
}

#[cfg(unix)]
fn is_argument_list_too_long(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::E2BIG)
}

#[cfg(windows)]
fn is_argument_list_too_long(error: &std::io::Error) -> bool {
    const ERROR_FILENAME_EXCED_RANGE: i32 = 206;
    error.raw_os_error() == Some(ERROR_FILENAME_EXCED_RANGE)
}

#[cfg(not(any(unix, windows)))]
fn is_argument_list_too_long(_error: &std::io::Error) -> bool {
    false
}

pub fn execute_external_command(
    root: &Path,
    paths: &[PathBuf],
    template: &CommandTemplate,
    options: ExecOptions,
    cancelled: &AtomicBool,
) -> ExecReport {
    if paths.is_empty() {
        return ExecReport::new(ExecOutcome::NoTargets, 0, 0);
    }
    let budget = match system_command_budget() {
        Ok(budget) => budget,
        Err(error) => {
            return ExecReport::new(ExecOutcome::Blocked, 0, paths.len()).with_diagnostic(error)
        }
    };
    execute_external_command_with_budget(
        root,
        paths,
        template,
        options,
        cancelled,
        &ProcessCommandBackend,
        budget,
    )
}

fn execute_external_command_with_budget(
    root: &Path,
    paths: &[PathBuf],
    template: &CommandTemplate,
    options: ExecOptions,
    cancelled: &AtomicBool,
    backend: &dyn ExternalCommandBackend,
    budget: usize,
) -> ExecReport {
    if paths.is_empty() {
        return ExecReport::new(ExecOutcome::NoTargets, 0, 0);
    }
    if cancelled.load(Ordering::Acquire) {
        return ExecReport::new(ExecOutcome::Cancelled, 0, paths.len());
    }
    let authorized = match authorize_action_targets(root, paths, false) {
        Ok(authorized) => authorized,
        Err(error) => {
            return ExecReport::new(ExecOutcome::Blocked, 0, paths.len())
                .with_diagnostic(error.to_string())
        }
    };
    let path_costs = authorized
        .targets
        .iter()
        .map(|target| argument_cost(target.execution_path.as_os_str()))
        .collect::<Vec<_>>();
    let ranges = match greedy_batch_ranges(
        &path_costs,
        fixed_command_cost(template),
        budget,
        options.max_paths_per_batch,
    ) {
        Ok(ranges) => ranges,
        Err(error) => {
            return ExecReport::new(ExecOutcome::Blocked, 0, authorized.targets.len())
                .with_diagnostic(error)
        }
    };
    let mut planned_batches = ranges.len();
    if options.dry_run {
        let mut report = ExecReport::new(ExecOutcome::DryRun, 0, authorized.targets.len());
        report.planned_batches = planned_batches;
        return report;
    }

    let mut completed_paths = 0usize;
    let mut launched_batches = 0usize;
    let mut pending_ranges = VecDeque::from(ranges);
    while let Some(range) = pending_ranges.pop_front() {
        if cancelled.load(Ordering::Acquire) {
            let mut report = ExecReport::new(
                ExecOutcome::Cancelled,
                completed_paths,
                authorized.targets.len(),
            );
            report.launched_batches = launched_batches;
            report.planned_batches = planned_batches;
            return report;
        }
        let mut batch_paths = Vec::with_capacity(range.len());
        for target in &authorized.targets[range.clone()] {
            match reauthorize_action_target(&authorized.canonical_root, target) {
                Ok(path) => batch_paths.push(path),
                Err(error) => {
                    let mut report = ExecReport::new(
                        ExecOutcome::Blocked,
                        completed_paths,
                        authorized.targets.len(),
                    )
                    .with_diagnostic(error.to_string());
                    report.launched_batches = launched_batches;
                    report.planned_batches = planned_batches;
                    return report;
                }
            }
        }
        if cancelled.load(Ordering::Acquire) {
            let mut report = ExecReport::new(
                ExecOutcome::Cancelled,
                completed_paths,
                authorized.targets.len(),
            );
            report.launched_batches = launched_batches;
            report.planned_batches = planned_batches;
            return report;
        }
        let args = template.args_for_paths(&batch_paths);
        match backend.run(template.program(), &args) {
            Ok(ChildExit::Success) => {
                launched_batches += 1;
                completed_paths += batch_paths.len();
            }
            Ok(ChildExit::Failed(exit_code)) => {
                if cancelled.load(Ordering::Acquire) {
                    let mut report = ExecReport::new(
                        ExecOutcome::Cancelled,
                        completed_paths,
                        authorized.targets.len(),
                    );
                    report.launched_batches = launched_batches + 1;
                    report.planned_batches = planned_batches;
                    return report;
                }
                let mut report = ExecReport::new(
                    ExecOutcome::ChildFailed,
                    completed_paths,
                    authorized.targets.len(),
                );
                report.launched_batches = launched_batches + 1;
                report.planned_batches = planned_batches;
                report.exit_code = exit_code;
                return report;
            }
            Err(error) => {
                if is_argument_list_too_long(&error) && range.len() > 1 {
                    let midpoint = range.start + range.len() / 2;
                    pending_ranges.push_front(midpoint..range.end);
                    pending_ranges.push_front(range.start..midpoint);
                    planned_batches += 1;
                    continue;
                }
                if cancelled.load(Ordering::Acquire) {
                    let mut report = ExecReport::new(
                        ExecOutcome::Cancelled,
                        completed_paths,
                        authorized.targets.len(),
                    );
                    report.launched_batches = launched_batches;
                    report.planned_batches = planned_batches;
                    return report;
                }
                let mut report = ExecReport::new(
                    ExecOutcome::SpawnFailed,
                    completed_paths,
                    authorized.targets.len(),
                )
                .with_diagnostic(error.to_string());
                report.launched_batches = launched_batches;
                report.planned_batches = planned_batches;
                return report;
            }
        }
    }
    let outcome = if cancelled.load(Ordering::Acquire) {
        ExecOutcome::Cancelled
    } else {
        ExecOutcome::Completed
    };
    let mut report = ExecReport::new(outcome, completed_paths, authorized.targets.len());
    report.launched_batches = launched_batches;
    report.planned_batches = planned_batches;
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct RecordingBackend {
        calls: Mutex<Vec<(OsString, Vec<OsString>)>>,
        fail_call: Option<usize>,
        cancel_after_first: Option<std::sync::Arc<AtomicBool>>,
    }

    #[derive(Default)]
    struct RuntimeLimitBackend {
        calls: Mutex<Vec<Vec<OsString>>>,
    }

    impl ExternalCommandBackend for RuntimeLimitBackend {
        fn run(&self, _program: &OsStr, args: &[OsString]) -> std::io::Result<ChildExit> {
            self.calls.lock().expect("calls lock").push(args.to_vec());
            if args.len() > 2 {
                #[cfg(unix)]
                let code = libc::E2BIG;
                #[cfg(windows)]
                let code = 206;
                #[cfg(not(any(unix, windows)))]
                let code = 7;
                return Err(std::io::Error::from_raw_os_error(code));
            }
            Ok(ChildExit::Success)
        }
    }

    impl ExternalCommandBackend for RecordingBackend {
        fn run(&self, program: &OsStr, args: &[OsString]) -> std::io::Result<ChildExit> {
            let call_index = {
                let mut calls = self.calls.lock().expect("calls lock");
                let index = calls.len();
                calls.push((program.to_os_string(), args.to_vec()));
                index
            };
            if call_index == 0 {
                if let Some(cancel) = &self.cancel_after_first {
                    cancel.store(true, Ordering::Release);
                }
            }
            Ok(if self.fail_call == Some(call_index) {
                ChildExit::Failed(Some(7))
            } else {
                ChildExit::Success
            })
        }
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("flistwalker-command-exec-{name}-{nonce}"))
    }

    fn template(parts: &[&str]) -> CommandTemplate {
        CommandTemplate::parse(&parts.iter().map(OsString::from).collect::<Vec<_>>())
            .expect("valid template")
    }

    #[test]
    fn tc_170_template_requires_one_standalone_placeholder_after_program() {
        for invalid in [
            vec!["tool"],
            vec!["{}"],
            vec!["tool", "{}", "{}"],
            vec!["tool", "prefix-{}"],
        ] {
            assert!(CommandTemplate::parse(
                &invalid.into_iter().map(OsString::from).collect::<Vec<_>>()
            )
            .is_err());
        }

        assert!(CommandTemplate::parse(&[
            OsString::from("tool"),
            OsString::from("--fixed"),
            OsString::from("{}"),
        ])
        .is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn tc_170_windows_template_rejects_implicit_batch_shell_programs() {
        for program in ["script.bat", "SCRIPT.CMD", r"C:\tools\run.cmd"] {
            let error = CommandTemplate::parse(&[OsString::from(program), OsString::from("{}")])
                .expect_err("batch program must require explicit cmd.exe opt-in");
            assert!(error.to_string().contains("cmd.exe /C"));
        }

        assert!(CommandTemplate::parse(&[
            OsString::from("cmd.exe"),
            OsString::from("/C"),
            OsString::from("script.cmd"),
            OsString::from("{}"),
        ])
        .is_ok());
    }

    #[test]
    fn tc_170_template_expands_paths_as_distinct_argv_values() {
        let template = template(&["tool", "before", "{}", "after"]);
        let paths = [
            PathBuf::from("one path.txt"),
            PathBuf::from("--not-an-option"),
        ];

        assert_eq!(
            template.args_for_paths(&paths),
            vec![
                OsString::from("before"),
                OsString::from("one path.txt"),
                OsString::from("--not-an-option"),
                OsString::from("after"),
            ]
        );
    }

    #[test]
    fn tc_170_greedy_batches_fill_budget_and_respect_path_cap() {
        assert_eq!(
            greedy_batch_ranges(&[4, 4, 7], 2, 10, None).expect("pack"),
            vec![0..2, 2..3]
        );
        assert_eq!(
            greedy_batch_ranges(&[1, 1, 1], 1, 100, Some(2)).expect("pack"),
            vec![0..2, 2..3]
        );
        assert!(greedy_batch_ranges(&[9], 2, 10, None).is_err());
    }

    #[test]
    fn tc_170_zero_targets_never_spawn_a_command() {
        let root = test_root("zero");
        fs::create_dir_all(&root).expect("create root");
        let backend = RecordingBackend::default();
        let cancelled = AtomicBool::new(false);

        let report = execute_external_command_with_budget(
            &root,
            &[],
            &template(&["tool", "{}"]),
            ExecOptions::default(),
            &cancelled,
            &backend,
            1024,
        );

        assert_eq!(report.outcome, ExecOutcome::NoTargets);
        assert!(backend.calls.lock().expect("calls lock").is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tc_170_all_targets_are_authorized_and_run_in_stable_batches() {
        let root = test_root("all");
        fs::create_dir_all(&root).expect("create root");
        let paths = (0..5)
            .map(|index| {
                let name = if index == 0 {
                    "--dangerous-option.txt".to_string()
                } else {
                    format!("item {index}.txt")
                };
                let path = root.join(name);
                fs::write(&path, "x").expect("write item");
                path
            })
            .collect::<Vec<_>>();
        let backend = RecordingBackend::default();
        let cancelled = AtomicBool::new(false);

        let report = execute_external_command_with_budget(
            &root,
            &paths,
            &template(&["tool", "--", "{}"]),
            ExecOptions {
                max_paths_per_batch: Some(2),
                dry_run: false,
            },
            &cancelled,
            &backend,
            usize::MAX / 2,
        );

        assert_eq!(report.outcome, ExecOutcome::Completed);
        assert_eq!(report.completed_paths, 5);
        assert_eq!(report.planned_batches, 3);
        let calls = backend.calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].1[0], OsString::from("--"));
        assert_eq!(calls[0].1[1], paths[0].canonicalize().expect("canonical 0"));
        assert_eq!(calls[0].1[2], paths[1].canonicalize().expect("canonical 1"));
        assert_eq!(calls[2].1[1], paths[4].canonicalize().expect("canonical 4"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tc_170_runtime_argument_limit_subdivides_without_skipping_targets() {
        let root = test_root("runtime-limit");
        fs::create_dir_all(&root).expect("create root");
        let paths = (0..4)
            .map(|index| {
                let path = root.join(format!("item-{index}.txt"));
                fs::write(&path, "x").expect("write item");
                path
            })
            .collect::<Vec<_>>();
        let backend = RuntimeLimitBackend::default();

        let report = execute_external_command_with_budget(
            &root,
            &paths,
            &template(&["tool", "{}"]),
            ExecOptions::default(),
            &AtomicBool::new(false),
            &backend,
            usize::MAX / 2,
        );

        assert_eq!(report.outcome, ExecOutcome::Completed);
        assert_eq!(report.completed_paths, 4);
        assert_eq!(report.launched_batches, 2);
        assert_eq!(report.planned_batches, 2);
        let calls = backend.calls.lock().expect("calls lock");
        assert_eq!(
            calls.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 2, 2]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tc_170_failure_and_cancel_stop_later_batches() {
        let root = test_root("stop");
        fs::create_dir_all(&root).expect("create root");
        let paths = (0..3)
            .map(|index| {
                let path = root.join(format!("item-{index}.txt"));
                fs::write(&path, "x").expect("write item");
                path
            })
            .collect::<Vec<_>>();
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let backend = RecordingBackend {
            fail_call: None,
            cancel_after_first: Some(std::sync::Arc::clone(&cancelled)),
            ..RecordingBackend::default()
        };

        let report = execute_external_command_with_budget(
            &root,
            &paths,
            &template(&["tool", "{}"]),
            ExecOptions {
                max_paths_per_batch: Some(1),
                dry_run: false,
            },
            cancelled.as_ref(),
            &backend,
            usize::MAX / 2,
        );

        assert_eq!(report.outcome, ExecOutcome::Cancelled);
        assert_eq!(report.completed_paths, 1);
        assert_eq!(backend.calls.lock().expect("calls lock").len(), 1);

        let failing_backend = RecordingBackend {
            fail_call: Some(1),
            ..RecordingBackend::default()
        };
        let failed = execute_external_command_with_budget(
            &root,
            &paths,
            &template(&["tool", "{}"]),
            ExecOptions {
                max_paths_per_batch: Some(1),
                dry_run: false,
            },
            &AtomicBool::new(false),
            &failing_backend,
            usize::MAX / 2,
        );
        assert_eq!(failed.outcome, ExecOutcome::ChildFailed);
        assert_eq!(failed.completed_paths, 1);
        assert_eq!(failed.exit_code, Some(7));
        assert_eq!(failing_backend.calls.lock().expect("calls lock").len(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn tc_170_windows_argument_cost_accounts_for_quotes_and_backslashes() {
        assert_eq!(argument_cost(OsStr::new("abc")), 4);
        assert_eq!(argument_cost(OsStr::new("a b")), 6);
        assert_eq!(argument_cost(OsStr::new("a\"b")), 5);
        assert_eq!(argument_cost(OsStr::new(r"a\b")), 4);
    }

    #[test]
    fn tc_170_root_escape_is_blocked_before_spawn() {
        let root = test_root("root");
        let outside = test_root("outside");
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&outside).expect("create outside");
        let outside_path = outside.join("outside.txt");
        fs::write(&outside_path, "x").expect("write outside");
        let backend = RecordingBackend::default();

        let report = execute_external_command_with_budget(
            &root,
            &[outside_path],
            &template(&["tool", "{}"]),
            ExecOptions::default(),
            &AtomicBool::new(false),
            &backend,
            1024,
        );

        assert_eq!(report.outcome, ExecOutcome::Blocked);
        assert!(backend.calls.lock().expect("calls lock").is_empty());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn tc_170_dry_run_plans_every_batch_without_spawning() {
        let root = test_root("dry-run");
        fs::create_dir_all(&root).expect("create root");
        let paths = (0..3)
            .map(|index| {
                let path = root.join(format!("item-{index}.txt"));
                fs::write(&path, "x").expect("write item");
                path
            })
            .collect::<Vec<_>>();
        let backend = RecordingBackend::default();

        let report = execute_external_command_with_budget(
            &root,
            &paths,
            &template(&["tool", "{}"]),
            ExecOptions {
                max_paths_per_batch: Some(2),
                dry_run: true,
            },
            &AtomicBool::new(false),
            &backend,
            usize::MAX / 2,
        );

        assert_eq!(report.outcome, ExecOutcome::DryRun);
        assert_eq!(report.planned_batches, 2);
        assert!(backend.calls.lock().expect("calls lock").is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
