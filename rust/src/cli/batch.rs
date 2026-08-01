use anyhow::{Context, Result};
use std::io::{self, BufWriter, IsTerminal, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;

use flist_walker::actions::{
    execute_authorized_action_request, execute_or_open, AuthorizedActionBackend,
    AuthorizedActionGuard, AuthorizedActionOutcome, AuthorizedActionReport,
    AuthorizedActionRequest,
};
use flist_walker::cli_tui::{run_cli_tui, CliTuiOptions, CliTuiOutcome};
use flist_walker::command_exec::{execute_external_command, ExecOptions, ExecOutcome, ExecReport};
use flist_walker::entry::Entry;
use flist_walker::ignore_list::{
    ensure_ignore_list_sample, load_ignore_terms_from_current_exe, parse_ignore_terms,
};
use flist_walker::indexer::{
    build_index_cancellable, execute_filelist_write_plan, find_filelist_in_first_level,
    is_index_build_cancelled, plan_filelist_write_cancellable, FileListWriteOptions,
    FileListWriteReport,
};
use flist_walker::path_utils::{normalize_path_for_display, output_path_bytes};
use flist_walker::persistence::load_persisted_roots_and_history;
use flist_walker::query::{CompiledIgnoreTerms, QueryScope};
use flist_walker::search::{rank_search_results, SearchPrefixCache, SearchSortScope};

use super::args::{
    parse_exec_template, validate_list_saved_roots_args, Args, CliAction, CliColorMode,
    CliIndexSource,
};
use crate::launch_path::resolve_root;

#[derive(Clone, Debug, PartialEq, Eq)]
enum BatchOutcome {
    Matches,
    NoMatch,
    Cancelled,
    ActionRejected,
    Action(AuthorizedActionReport),
    Exec(ExecReport),
}

pub(super) fn run_cli_mode(args: &Args) -> Result<ExitCode> {
    if !args.interactive && args.list_saved_roots {
        list_saved_roots(args)?;
        return Ok(ExitCode::SUCCESS);
    }
    if !args.interactive && args.create_filelist {
        let root = match resolve_cli_root(args) {
            Ok(root) => root,
            Err(error) => {
                eprintln!("error: {error}");
                return Ok(ExitCode::from(2));
            }
        };
        let cancelled = install_cli_signal_handler()?;
        return Ok(cli_filelist_exit_code(run_cli_filelist(
            &root,
            args,
            cancelled.as_ref(),
        )));
    }
    if let Err(err) = ensure_ignore_list_sample() {
        warn!("failed to materialize ignore list sample: {}", err);
    }
    if args.interactive {
        run_interactive(args)
    } else {
        let root = match resolve_cli_root(args) {
            Ok(root) => root,
            Err(error) => {
                eprintln!("error: {error}");
                return Ok(ExitCode::from(2));
            }
        };
        let cancelled = install_cli_signal_handler()?;
        Ok(batch_exit_code(
            run_cli(args, &root, &cancelled)?,
            args.fail_no_match,
        ))
    }
}

fn run_interactive(args: &Args) -> Result<ExitCode> {
    let root = match resolve_cli_root(args) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("error: {error}");
            return Ok(ExitCode::from(2));
        }
    };
    let options = cli_tui_options(args, load_cli_tui_ignore_terms(args)?);
    Ok(match run_cli_tui(&root, &options)? {
        CliTuiOutcome::Selected { paths, root } => {
            if let Some(template) = parse_exec_template(args).expect("validated --exec") {
                let cancelled = install_cli_signal_handler()?;
                let report = execute_external_command(
                    &root,
                    &paths,
                    &template,
                    ExecOptions {
                        max_paths_per_batch: args.exec_max_args.map(NonZeroUsize::get),
                        dry_run: args.dry_run,
                    },
                    cancelled.as_ref(),
                );
                write_cli_exec_report(&report);
                batch_exit_code(BatchOutcome::Exec(report), false)
            } else {
                write_cli_paths(
                    &paths,
                    &root,
                    args.absolute,
                    args.print0,
                    false,
                    &AtomicBool::new(false),
                )?;
                ExitCode::SUCCESS
            }
        }
        CliTuiOutcome::Cancelled => ExitCode::from(130),
    })
}

fn install_cli_signal_handler() -> Result<Arc<AtomicBool>> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal_cancelled = Arc::clone(&cancelled);
    ctrlc::set_handler(move || signal_cancelled.store(true, Ordering::Relaxed))
        .context("failed to install CLI signal handler")?;
    Ok(cancelled)
}

fn read_cli_ignore_terms(args: &Args) -> Result<Vec<String>> {
    if let Some(path) = args.ignore_file.as_deref() {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read ignore file: {}", path.display()))?;
        Ok(parse_ignore_terms(&text))
    } else {
        Ok(load_ignore_terms_from_current_exe())
    }
}

fn load_cli_ignore_terms(args: &Args) -> Result<Vec<String>> {
    if args.no_ignore {
        Ok(Vec::new())
    } else {
        read_cli_ignore_terms(args)
    }
}

fn load_cli_tui_ignore_terms(args: &Args) -> Result<Vec<String>> {
    read_cli_ignore_terms(args)
}

fn cli_tui_options(args: &Args, ignore_terms: Vec<String>) -> CliTuiOptions {
    let (include_files, include_dirs) = args.entry_type.include_flags();
    CliTuiOptions {
        initial_query: args.query.clone(),
        limit: args.limit,
        absolute: args.absolute,
        print0: args.print0,
        include_files,
        include_dirs,
        use_filelist: !matches!(args.source, CliIndexSource::Walker),
        require_filelist: matches!(args.source, CliIndexSource::Filelist),
        regex: args.regex,
        ignore_case: !args.case_sensitive,
        ignore_enabled: !args.no_ignore,
        ignore_terms,
        sort_mode: args.sort.into(),
        color_enabled: args.color_mode().enabled(no_color_is_set()),
    }
}

fn no_color_is_set() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

fn run_cli(args: &Args, root: &Path, cancelled: &Arc<AtomicBool>) -> Result<BatchOutcome> {
    let backend = CliActionBackend;
    run_cli_with_backend(args, root, cancelled, &backend)
}

fn run_cli_with_backend(
    args: &Args,
    root: &Path,
    cancelled: &Arc<AtomicBool>,
    backend: &dyn AuthorizedActionBackend,
) -> Result<BatchOutcome> {
    let (include_files, include_dirs) = args.entry_type.include_flags();
    let use_filelist = match args.source {
        CliIndexSource::Auto | CliIndexSource::Filelist => true,
        CliIndexSource::Walker => false,
    };
    if matches!(args.source, CliIndexSource::Filelist)
        && find_filelist_in_first_level(root).is_none()
    {
        anyhow::bail!(
            "FileList was required but none was found in {}",
            root.display()
        );
    }

    let ignore_terms = load_cli_ignore_terms(args)?;
    let ignore_case = !args.case_sensitive;
    let compiled_ignore_terms = CompiledIgnoreTerms::compile(&ignore_terms, ignore_case);

    let index_started = Instant::now();
    if args.progress {
        eprintln!("Indexing {}...", root.display());
    }
    let indexed_entries =
        match build_index_cancellable(root, use_filelist, include_files, include_dirs, || {
            cancelled.load(Ordering::Relaxed)
        }) {
            Ok(entries) => entries,
            Err(error) if is_index_build_cancelled(&error) => return Ok(BatchOutcome::Cancelled),
            Err(error) => return Err(error),
        };
    if cancelled.load(Ordering::Relaxed) {
        return Ok(BatchOutcome::Cancelled);
    }
    if args.progress {
        eprintln!(
            "Indexed {} candidate(s) in {} ms; filtering and searching...",
            indexed_entries.len(),
            index_started.elapsed().as_millis()
        );
    }
    let mut entries = Vec::with_capacity(indexed_entries.len());
    for path in indexed_entries {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(BatchOutcome::Cancelled);
        }
        if !compiled_ignore_terms.matches_path(
            &path,
            QueryScope {
                root: Some(root),
                prefer_relative: true,
                ignore_case,
            },
        ) {
            entries.push(path);
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(BatchOutcome::Cancelled);
    }
    let entries = Arc::new(entries.into_iter().map(Entry::from).collect());
    let mut prefix_cache = SearchPrefixCache::default();
    let search_started = Instant::now();
    let (search_results, search_error) = rank_search_results(
        &entries,
        args.query.trim(),
        root,
        args.limit,
        args.regex,
        ignore_case,
        true,
        &mut prefix_cache,
        args.sort.into(),
        SearchSortScope::AllMatches,
    );
    if let Some(error) = search_error {
        return Err(anyhow::Error::msg(error));
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(BatchOutcome::Cancelled);
    }
    if args.progress {
        eprintln!(
            "Matched {} path(s); returning {} in {} ms",
            search_results.total_match_count,
            search_results.results.len(),
            search_started.elapsed().as_millis()
        );
    }
    let paths = search_results
        .results
        .into_iter()
        .map(|(path, _score)| path)
        .collect::<Vec<_>>();

    if let Some(template) = parse_exec_template(args).expect("validated --exec template") {
        let report = execute_external_command(
            root,
            &paths,
            &template,
            ExecOptions {
                max_paths_per_batch: args.exec_max_args.map(NonZeroUsize::get),
                dry_run: args.dry_run,
            },
            cancelled.as_ref(),
        );
        write_cli_exec_report(&report);
        return Ok(if report.outcome == ExecOutcome::NoTargets {
            BatchOutcome::NoMatch
        } else {
            BatchOutcome::Exec(report)
        });
    }

    if args.action == CliAction::Print {
        write_cli_paths(
            &paths,
            root,
            args.absolute,
            args.print0,
            cli_output_color_enabled(
                args.color_mode(),
                io::stdout().is_terminal(),
                no_color_is_set(),
            ),
            cancelled.as_ref(),
        )?;
        if cancelled.load(Ordering::Relaxed) {
            return Ok(BatchOutcome::Cancelled);
        }
        return Ok(if paths.is_empty() {
            BatchOutcome::NoMatch
        } else {
            BatchOutcome::Matches
        });
    }
    if paths.is_empty() {
        return Ok(BatchOutcome::NoMatch);
    }
    Ok(dispatch_cli_action(args, root, paths, cancelled, backend))
}

fn dispatch_cli_action(
    args: &Args,
    root: &Path,
    paths: Vec<PathBuf>,
    cancelled: &Arc<AtomicBool>,
    backend: &dyn AuthorizedActionBackend,
) -> BatchOutcome {
    if paths.len() > 1 && !args.action_all {
        eprintln!(
            "Action refused: {} matches require --action-all",
            paths.len()
        );
        return BatchOutcome::ActionRejected;
    }
    let request = AuthorizedActionRequest::new_with_cancellation(
        1,
        root.to_path_buf(),
        paths,
        args.action
            .authorized_mode()
            .expect("non-print CLI action has an authorized mode"),
        Arc::clone(cancelled),
    );
    let report = execute_authorized_action_request(&request, &CliActionGuard, backend);
    write_cli_action_report(&report);
    BatchOutcome::Action(report)
}

fn write_cli_paths(
    paths: &[PathBuf],
    root: &Path,
    absolute: bool,
    print0: bool,
    color_enabled: bool,
    cancelled: &AtomicBool,
) -> Result<()> {
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    for path in paths {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        write_cli_path_record(&mut output, path, root, absolute, print0, color_enabled)?;
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }
    output.flush()?;
    Ok(())
}

fn cli_output_color_enabled(
    mode: CliColorMode,
    stdout_is_terminal: bool,
    no_color_is_set: bool,
) -> bool {
    match mode {
        CliColorMode::Auto => stdout_is_terminal && !no_color_is_set,
        CliColorMode::Always => true,
        CliColorMode::Never => false,
    }
}

fn write_cli_path_record<W: Write>(
    output: &mut W,
    path: &Path,
    root: &Path,
    absolute: bool,
    print0: bool,
    color_enabled: bool,
) -> io::Result<()> {
    if color_enabled {
        output.write_all(b"\x1b[38;5;11m")?;
    }
    output.write_all(&output_path_bytes(path, root, !absolute, print0))?;
    if color_enabled {
        output.write_all(b"\x1b[0m")?;
    }
    output.write_all(if print0 { b"\0" } else { b"\n" })
}

struct CliActionGuard;

impl AuthorizedActionGuard for CliActionGuard {
    fn is_current(&self, _request_id: u64, _trusted_root: &Path) -> bool {
        true
    }
}

struct CliActionBackend;

impl AuthorizedActionBackend for CliActionBackend {
    fn execute_or_open(&self, path: &Path) -> Result<()> {
        execute_or_open(path)
    }

    fn reveal(&self, path: &Path) -> Result<()> {
        // The shared authorization lifecycle has already converted files to their parent
        // directories for reveal mode and reauthorized the resulting execution path.
        execute_or_open(path)
    }
}

fn write_cli_action_report(report: &AuthorizedActionReport) {
    eprintln!("{}", format_cli_action_report(report));
}

fn write_cli_exec_report(report: &ExecReport) {
    match report.outcome {
        ExecOutcome::NoTargets => {}
        ExecOutcome::DryRun => eprintln!(
            "Dry run: {} paths in {} batches",
            report.total_paths, report.planned_batches
        ),
        ExecOutcome::Completed => eprintln!(
            "Command completed: {} paths in {} batches",
            report.completed_paths, report.launched_batches
        ),
        ExecOutcome::Cancelled => eprintln!(
            "Command canceled after {} of {} paths",
            report.completed_paths, report.total_paths
        ),
        ExecOutcome::Blocked => eprintln!(
            "Command blocked after {} of {} paths: {}",
            report.completed_paths,
            report.total_paths,
            report
                .diagnostic
                .as_deref()
                .unwrap_or("authorization failed")
        ),
        ExecOutcome::SpawnFailed => eprintln!(
            "Command failed to start after {} of {} paths: {}",
            report.completed_paths,
            report.total_paths,
            report.diagnostic.as_deref().unwrap_or("unknown error")
        ),
        ExecOutcome::ChildFailed => eprintln!(
            "Command failed after {} of {} paths with exit code {}",
            report.completed_paths,
            report.total_paths,
            report
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    }
}

fn format_cli_action_report(report: &AuthorizedActionReport) -> String {
    let path = report
        .display_path
        .as_deref()
        .map(normalize_path_for_display)
        .unwrap_or_else(|| "selected target".to_string());
    let diagnostic = report.diagnostic.as_deref().unwrap_or("executor failed");
    let backend_error = report.backend_error.as_deref().unwrap_or("unknown error");
    match report.outcome {
        AuthorizedActionOutcome::Completed => {
            format!("Action completed: {}/{} targets", report.completed, report.total)
        }
        AuthorizedActionOutcome::Blocked => format!(
            "Action blocked: {path}: {}",
            report.diagnostic.as_deref().unwrap_or("authorization failed")
        ),
        AuthorizedActionOutcome::Canceled => "Action canceled".to_string(),
        AuthorizedActionOutcome::Superseded => "Action superseded".to_string(),
        AuthorizedActionOutcome::Failed => {
            format!("Action failed: {path}: {diagnostic}: {backend_error}")
        }
        AuthorizedActionOutcome::PartialFailure => format!(
            "Action partial failure: completed {}/{} targets at {path}: {diagnostic}: {backend_error}",
            report.completed, report.total
        ),
    }
}

fn resolve_cli_root(args: &Args) -> std::result::Result<PathBuf, String> {
    if args.use_default_root {
        let roots = load_persisted_roots_and_history();
        let root = roots
            .default_root
            .ok_or_else(|| "no persisted default root is configured".to_string())?;
        return resolve_root(&root).map_err(|error| error.to_string());
    }
    if let Some(index) = args.saved_root {
        let roots = load_persisted_roots_and_history();
        let root = roots
            .saved_roots
            .get(index.saturating_sub(1))
            .filter(|_| index != 0)
            .ok_or_else(|| format!("saved root index {index} is not configured"))?;
        return resolve_root(root).map_err(|error| error.to_string());
    }
    resolve_root(args.root.as_deref().unwrap_or(Path::new("."))).map_err(|error| error.to_string())
}

enum CliFileListOutcome {
    CanceledBeforePlan,
    FailedBeforePlan,
    Report(FileListWriteReport),
}

fn run_cli_filelist(root: &Path, args: &Args, cancelled: &AtomicBool) -> CliFileListOutcome {
    if args.progress {
        eprintln!("Indexing {} for FileList creation...", root.display());
    }
    let entries = match build_index_cancellable(root, false, true, true, || {
        cancelled.load(Ordering::Relaxed)
    }) {
        Ok(entries) => entries,
        Err(error) if is_index_build_cancelled(&error) => {
            return CliFileListOutcome::CanceledBeforePlan
        }
        Err(error) => {
            eprintln!("FileList failed: {error}");
            return CliFileListOutcome::FailedBeforePlan;
        }
    };
    let should_cancel = || cancelled.load(Ordering::Relaxed);
    let report = match plan_filelist_write_cancellable(
        root,
        &entries,
        FileListWriteOptions {
            allow_root_overwrite: args.overwrite_filelist,
            propagate_to_ancestors: args.propagate_ancestors,
        },
        &should_cancel,
    ) {
        Ok(plan) => execute_filelist_write_plan(&plan, &should_cancel),
        Err(report) => *report,
    };
    write_cli_filelist_report(&report);
    CliFileListOutcome::Report(report)
}

fn write_cli_filelist_report(report: &FileListWriteReport) {
    for path in &report.committed {
        eprintln!("FileList committed: {}", path.display());
    }
    for failure in &report.failed {
        eprintln!(
            "FileList failed: {}: {}",
            failure.path.display(),
            failure.error
        );
    }
    for path in &report.rolled_back {
        eprintln!("FileList rolled back: {}", path.display());
    }
    for failure in &report.rollback_failed {
        eprintln!(
            "FileList rollback failed: {}: {}",
            failure.path.display(),
            failure.error
        );
    }
    if report.committed.is_empty()
        && report.failed.is_empty()
        && report.rolled_back.is_empty()
        && report.rollback_failed.is_empty()
    {
        eprintln!("FileList canceled");
    }
}

fn cli_filelist_exit_code(outcome: CliFileListOutcome) -> ExitCode {
    match outcome {
        CliFileListOutcome::CanceledBeforePlan => ExitCode::from(130),
        CliFileListOutcome::FailedBeforePlan => ExitCode::from(1),
        CliFileListOutcome::Report(report) => ExitCode::from(report.exit_code() as u8),
    }
}

fn absolute_stored_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn list_saved_roots(args: &Args) -> Result<()> {
    validate_list_saved_roots_args(args).map_err(anyhow::Error::msg)?;
    let roots = load_persisted_roots_and_history();
    let mut framed_output = Vec::new();
    for (position, root) in roots.saved_roots.iter().enumerate() {
        let path = absolute_stored_path(root);
        if args.print0 {
            framed_output.extend_from_slice(&output_path_bytes(&path, Path::new("."), false, true));
            framed_output.push(0);
        } else {
            framed_output
                .extend_from_slice(format!("{}\t{}\n", position + 1, path.display()).as_bytes());
        }
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    output.write_all(&framed_output)?;
    output.flush()?;
    Ok(())
}

fn batch_exit_code(outcome: BatchOutcome, fail_no_match: bool) -> ExitCode {
    match outcome {
        BatchOutcome::Cancelled => ExitCode::from(130),
        BatchOutcome::NoMatch if fail_no_match => ExitCode::from(1),
        BatchOutcome::Matches | BatchOutcome::NoMatch => ExitCode::SUCCESS,
        BatchOutcome::ActionRejected => ExitCode::from(1),
        BatchOutcome::Action(report) => match report.outcome {
            AuthorizedActionOutcome::Completed => ExitCode::SUCCESS,
            AuthorizedActionOutcome::Canceled => ExitCode::from(130),
            AuthorizedActionOutcome::Blocked
            | AuthorizedActionOutcome::Superseded
            | AuthorizedActionOutcome::Failed
            | AuthorizedActionOutcome::PartialFailure => ExitCode::from(1),
        },
        BatchOutcome::Exec(report) => match report.outcome {
            ExecOutcome::Completed | ExecOutcome::DryRun | ExecOutcome::NoTargets => {
                ExitCode::SUCCESS
            }
            ExecOutcome::Cancelled => ExitCode::from(130),
            ExecOutcome::Blocked | ExecOutcome::SpawnFailed | ExecOutcome::ChildFailed => {
                ExitCode::from(1)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;
    use flist_walker::actions::{
        AuthorizedActionBackend, AuthorizedActionMode, AuthorizedActionOutcome,
        AuthorizedActionReport,
    };
    use flist_walker::path_utils::normalize_path_for_display;
    use flist_walker::search::SearchSortMode;

    use super::{
        batch_exit_code, cli_filelist_exit_code, cli_output_color_enabled, cli_tui_options,
        dispatch_cli_action, format_cli_action_report, load_cli_tui_ignore_terms, run_cli,
        write_cli_path_record, BatchOutcome, CliFileListOutcome,
    };
    use crate::cli::args::{Args, CliColorMode};

    struct RecordingCliActionBackend {
        calls: Mutex<Vec<(AuthorizedActionMode, PathBuf)>>,
        fail_at: Option<usize>,
        cancel_after_first_call: Option<Arc<AtomicBool>>,
    }

    impl RecordingCliActionBackend {
        fn successful() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_at: None,
                cancel_after_first_call: None,
            }
        }

        fn failing_at(call_index: usize) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_at: Some(call_index),
                cancel_after_first_call: None,
            }
        }

        fn canceling_after_first_call(cancellation: Arc<AtomicBool>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_at: None,
                cancel_after_first_call: Some(cancellation),
            }
        }

        fn calls(&self) -> Vec<(AuthorizedActionMode, PathBuf)> {
            self.calls.lock().expect("recording backend lock").clone()
        }

        fn dispatch(&self, mode: AuthorizedActionMode, path: &Path) -> Result<()> {
            let call_index = {
                let mut calls = self.calls.lock().expect("recording backend lock");
                let index = calls.len();
                calls.push((mode, path.to_path_buf()));
                index
            };
            if call_index == 0 {
                if let Some(cancellation) = &self.cancel_after_first_call {
                    cancellation.store(true, Ordering::Release);
                }
            }
            if self.fail_at == Some(call_index) {
                anyhow::bail!("recorded backend failure {call_index}");
            }
            Ok(())
        }
    }

    impl AuthorizedActionBackend for RecordingCliActionBackend {
        fn execute_or_open(&self, path: &Path) -> Result<()> {
            self.dispatch(AuthorizedActionMode::ExecuteOrOpen, path)
        }

        fn reveal(&self, path: &Path) -> Result<()> {
            self.dispatch(AuthorizedActionMode::Reveal, path)
        }
    }

    fn action_test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("flistwalker-cli-action-{name}-{nonce}"))
    }

    fn action_args(action: &str, action_all: bool) -> Args {
        let mut args = vec!["flistwalker", "--cli", "--action", action];
        if action_all {
            args.push("--action-all");
        }
        Args::try_parse_from(args).expect("parse action args")
    }

    fn action_report(outcome: BatchOutcome) -> AuthorizedActionReport {
        match outcome {
            BatchOutcome::Action(report) => report,
            other => panic!("expected action report, got {other:?}"),
        }
    }

    #[test]
    fn tc_163_interactive_startup_preserves_sort_ignore_and_saved_root_options() {
        let mut args = Args::try_parse_from([
            "flistwalker",
            "--cli",
            "--interactive",
            "--saved-root",
            "1",
            "--sort",
            "name-desc",
            "--no-ignore",
        ])
        .expect("interactive saved-root options should parse");
        let ignore_root = action_test_root("interactive-ignore-terms");
        fs::create_dir_all(&ignore_root).expect("create ignore root");
        let ignore_file = ignore_root.join("ignore-list.txt");
        fs::write(&ignore_file, "ignored\n").expect("write ignore fixture");
        args.ignore_file = Some(ignore_file);

        let options = cli_tui_options(
            &args,
            load_cli_tui_ignore_terms(&args).expect("load disabled TUI ignore terms"),
        );
        assert_eq!(options.sort_mode, SearchSortMode::NameDesc);
        assert!(!options.ignore_enabled);
        assert_eq!(options.ignore_terms, ["ignored"]);
        assert_eq!(args.saved_root, Some(1));

        let _ = fs::remove_dir_all(ignore_root);
    }

    #[test]
    fn tc_172_color_mode_applies_to_batch_and_interactive_output() {
        let always = Args::try_parse_from(["flistwalker", "--cli", "--color", "always"])
            .expect("parse forced color mode");
        assert!(cli_tui_options(&always, Vec::new()).color_enabled);

        let never = Args::try_parse_from(["flistwalker", "--cli", "--color", "never"])
            .expect("parse disabled color mode");
        assert!(!cli_tui_options(&never, Vec::new()).color_enabled);

        assert!(cli_output_color_enabled(CliColorMode::Auto, true, false));
        assert!(!cli_output_color_enabled(CliColorMode::Auto, false, false));
        assert!(!cli_output_color_enabled(CliColorMode::Auto, true, true));
        assert!(cli_output_color_enabled(CliColorMode::Always, false, true));
        assert!(!cli_output_color_enabled(CliColorMode::Never, true, false));

        let mut output = Vec::new();
        write_cli_path_record(
            &mut output,
            Path::new("root/match.txt"),
            Path::new("root"),
            false,
            false,
            true,
        )
        .expect("write colored path");
        assert_eq!(output, b"\x1b[38;5;11mmatch.txt\x1b[0m\n");

        let mut piped_output = Vec::new();
        write_cli_path_record(
            &mut piped_output,
            Path::new("root/match.txt"),
            Path::new("root"),
            false,
            false,
            false,
        )
        .expect("write plain path");
        assert_eq!(piped_output, b"match.txt\n");
    }

    #[test]
    fn tc_162_batch_cli_maps_a_preexisting_cancel_request_to_cancelled() {
        let root = std::env::current_dir().expect("current directory");
        let args = Args::try_parse_from([
            "flistwalker",
            "--cli",
            "--source",
            "walker",
            "--root",
            root.to_str().expect("UTF-8 test path"),
        ])
        .expect("parse CLI arguments");
        let cancelled = Arc::new(AtomicBool::new(true));

        assert_eq!(
            run_cli(&args, &root, &cancelled).expect("cancelled CLI outcome"),
            BatchOutcome::Cancelled
        );
    }

    #[test]
    fn tc_164_cli_action_rejects_implicit_multi_and_preflight_escape_without_calls() {
        let root = action_test_root("preflight");
        let outside = action_test_root("outside");
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&outside).expect("create outside");
        let first = root.join("first.txt");
        let second = root.join("second.txt");
        let escape = outside.join("escape.txt");
        fs::write(&first, "first").expect("write first");
        fs::write(&second, "second").expect("write second");
        fs::write(&escape, "escape").expect("write escape");
        let root = root.canonicalize().expect("canonical root");
        let first = first.canonicalize().expect("canonical first");
        let second = second.canonicalize().expect("canonical second");
        let escape = escape.canonicalize().expect("canonical escape");
        let cancellation = Arc::new(AtomicBool::new(false));

        let implicit_backend = RecordingCliActionBackend::successful();
        assert_eq!(
            dispatch_cli_action(
                &action_args("open", false),
                &root,
                vec![first.clone(), second],
                &cancellation,
                &implicit_backend,
            ),
            BatchOutcome::ActionRejected
        );
        assert!(implicit_backend.calls().is_empty());

        let escape_backend = RecordingCliActionBackend::successful();
        let report = action_report(dispatch_cli_action(
            &action_args("open", true),
            &root,
            vec![first, escape.clone()],
            &cancellation,
            &escape_backend,
        ));
        assert_eq!(report.outcome, AuthorizedActionOutcome::Blocked);
        assert_eq!(report.completed, 0);
        assert!(escape_backend.calls().is_empty());
        let blocked_diagnostic = format_cli_action_report(&report);
        assert!(blocked_diagnostic.contains("outside current root"));
        assert!(blocked_diagnostic.contains(&normalize_path_for_display(&escape)));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn tc_164_cli_action_uses_recording_backend_for_open_reveal_partial_and_cancel() {
        let root = action_test_root("dispatch");
        fs::create_dir_all(&root).expect("create root");
        let first = root.join("first.txt");
        let second = root.join("second.txt");
        fs::write(&first, "first").expect("write first");
        fs::write(&second, "second").expect("write second");
        let root = root.canonicalize().expect("canonical root");
        let first = first.canonicalize().expect("canonical first");
        let second = second.canonicalize().expect("canonical second");

        let open_backend = RecordingCliActionBackend::successful();
        let open = action_report(dispatch_cli_action(
            &action_args("open", false),
            &root,
            vec![first.clone()],
            &Arc::new(AtomicBool::new(false)),
            &open_backend,
        ));
        assert_eq!(open.outcome, AuthorizedActionOutcome::Completed);
        assert_eq!(
            open_backend.calls(),
            vec![(AuthorizedActionMode::ExecuteOrOpen, first.clone())]
        );

        let reveal_backend = RecordingCliActionBackend::successful();
        let reveal = action_report(dispatch_cli_action(
            &action_args("reveal", false),
            &root,
            vec![first.clone()],
            &Arc::new(AtomicBool::new(false)),
            &reveal_backend,
        ));
        assert_eq!(reveal.outcome, AuthorizedActionOutcome::Completed);
        assert_eq!(
            reveal_backend.calls(),
            vec![(AuthorizedActionMode::Reveal, root.clone())]
        );

        let partial_backend = RecordingCliActionBackend::failing_at(1);
        let partial = action_report(dispatch_cli_action(
            &action_args("open", true),
            &root,
            vec![first.clone(), second.clone()],
            &Arc::new(AtomicBool::new(false)),
            &partial_backend,
        ));
        assert_eq!(partial.outcome, AuthorizedActionOutcome::PartialFailure);
        assert_eq!((partial.completed, partial.total), (1, 2));
        let partial_diagnostic = format_cli_action_report(&partial);
        assert!(partial_diagnostic.contains("completed 1/2 targets"));
        assert!(partial_diagnostic.contains("recorded backend failure 1"));
        assert!(partial_diagnostic.contains(&normalize_path_for_display(&second)));
        assert_eq!(partial_backend.calls().len(), 2);
        assert_eq!(
            batch_exit_code(BatchOutcome::Action(partial.clone()), false),
            ExitCode::from(1)
        );

        let canceled = Arc::new(AtomicBool::new(true));
        let pre_canceled_backend = RecordingCliActionBackend::successful();
        let pre_canceled = action_report(dispatch_cli_action(
            &action_args("open", false),
            &root,
            vec![first.clone()],
            &canceled,
            &pre_canceled_backend,
        ));
        assert_eq!(pre_canceled.outcome, AuthorizedActionOutcome::Canceled);
        assert!(pre_canceled_backend.calls().is_empty());
        assert_eq!(
            batch_exit_code(BatchOutcome::Action(pre_canceled.clone()), false),
            ExitCode::from(130)
        );

        let between_canceled = Arc::new(AtomicBool::new(false));
        let between_backend =
            RecordingCliActionBackend::canceling_after_first_call(Arc::clone(&between_canceled));
        let between = action_report(dispatch_cli_action(
            &action_args("open", true),
            &root,
            vec![first, second],
            &between_canceled,
            &between_backend,
        ));
        assert_eq!(between.outcome, AuthorizedActionOutcome::Canceled);
        assert_eq!(between.completed, 1);
        assert_eq!(between_backend.calls().len(), 1);
        assert_eq!(
            batch_exit_code(BatchOutcome::Action(between), false),
            ExitCode::from(130)
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tc_165_cli_filelist_exit_mapping_preserves_clean_cancel_and_rollback_failure() {
        let root_target = PathBuf::from("FileList.txt");
        let clean_cancel = flist_walker::indexer::FileListWriteReport {
            status: flist_walker::indexer::FileListWriteStatus::Canceled,
            root_target: root_target.clone(),
            committed: vec![root_target.clone()],
            failed: Vec::new(),
            rolled_back: vec![root_target.clone()],
            rollback_failed: Vec::new(),
        };
        let rollback_failure = flist_walker::indexer::FileListWriteReport {
            status: flist_walker::indexer::FileListWriteStatus::Canceled,
            root_target: root_target.clone(),
            committed: vec![root_target.clone()],
            failed: Vec::new(),
            rolled_back: Vec::new(),
            rollback_failed: vec![flist_walker::indexer::FileListWriteFailure {
                path: root_target,
                error: "rollback injection".to_string(),
            }],
        };

        assert_eq!(
            cli_filelist_exit_code(CliFileListOutcome::Report(clean_cancel)),
            ExitCode::from(130)
        );
        assert_eq!(
            cli_filelist_exit_code(CliFileListOutcome::Report(rollback_failure)),
            ExitCode::from(1)
        );
        assert_eq!(
            cli_filelist_exit_code(CliFileListOutcome::FailedBeforePlan),
            ExitCode::from(1)
        );
    }
}
