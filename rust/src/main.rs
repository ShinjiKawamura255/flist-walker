#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;
use tracing_subscriber::EnvFilter;

use flist_walker::actions::{
    execute_authorized_action_request, execute_or_open, AuthorizedActionBackend,
    AuthorizedActionGuard, AuthorizedActionMode, AuthorizedActionOutcome, AuthorizedActionReport,
    AuthorizedActionRequest,
};
use flist_walker::app::{configure_egui_fonts, request_process_shutdown, FlistWalkerApp};
use flist_walker::cli_tui::{run_cli_tui, CliTuiOptions, CliTuiOutcome};
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
use flist_walker::runtime_config::initialize_runtime_config;
use flist_walker::search::{
    rank_search_results, SearchPrefixCache, SearchSortMode, SearchSortScope,
};
use flist_walker::updater::{
    recover_interrupted_update_on_startup, run_internal_update_helper_if_requested,
};
use resvg::{tiny_skia, usvg};

const APP_TITLE: &str = "FlistWalker";
const APP_ID: &str = "flistwalker";
const DEFAULT_WINDOW_SIZE: eframe::egui::Vec2 = eframe::egui::vec2(1400.0, 900.0);
const MIN_WINDOW_SIZE: eframe::egui::Vec2 = eframe::egui::vec2(640.0, 400.0);

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum CliEntryType {
    #[default]
    All,
    File,
    Folder,
}

impl CliEntryType {
    fn include_flags(self) -> (bool, bool) {
        match self {
            Self::All => (true, true),
            Self::File => (true, false),
            Self::Folder => (false, true),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum CliIndexSource {
    #[default]
    Auto,
    Filelist,
    Walker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum CliSortMode {
    #[default]
    Score,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
    CreatedDesc,
    CreatedAsc,
    SizeDesc,
    SizeAsc,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum CliAction {
    #[default]
    Print,
    Open,
    Reveal,
}

impl CliAction {
    fn authorized_mode(self) -> Option<AuthorizedActionMode> {
        match self {
            Self::Print => None,
            Self::Open => Some(AuthorizedActionMode::ExecuteOrOpen),
            Self::Reveal => Some(AuthorizedActionMode::Reveal),
        }
    }
}

impl From<CliSortMode> for SearchSortMode {
    fn from(value: CliSortMode) -> Self {
        match value {
            CliSortMode::Score => Self::Score,
            CliSortMode::NameAsc => Self::NameAsc,
            CliSortMode::NameDesc => Self::NameDesc,
            CliSortMode::ModifiedDesc => Self::ModifiedDesc,
            CliSortMode::ModifiedAsc => Self::ModifiedAsc,
            CliSortMode::CreatedDesc => Self::CreatedDesc,
            CliSortMode::CreatedAsc => Self::CreatedAsc,
            CliSortMode::SizeDesc => Self::SizeDesc,
            CliSortMode::SizeAsc => Self::SizeAsc,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "flistwalker")]
#[command(about = "Find files and folders with fuzzy search")]
#[command(version)]
struct Args {
    /// Query using fuzzy matching and the supported fzf-style operators.
    #[arg(default_value = "", value_name = "QUERY")]
    query: String,

    /// Root directory to search (defaults to the current directory).
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with_all = ["use_default_root", "saved_root", "list_saved_roots"]
    )]
    root: Option<PathBuf>,

    /// Search using the persisted default root.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["root", "saved_root", "list_saved_roots"]
    )]
    use_default_root: bool,

    /// Search using a one-based index from the persisted saved roots.
    #[arg(
        long,
        value_name = "INDEX",
        requires = "cli",
        conflicts_with_all = ["root", "use_default_root", "list_saved_roots"]
    )]
    saved_root: Option<usize>,

    /// Maximum number of paths to return.
    #[arg(long, default_value_t = 1000)]
    limit: usize,

    /// Print paths without opening the GUI.
    #[arg(long, default_value_t = false)]
    cli: bool,

    /// Run the interactive terminal selector.
    #[arg(long, default_value_t = false, requires = "cli")]
    interactive: bool,

    /// Print absolute paths instead of paths relative to the root.
    #[arg(long, default_value_t = false, requires = "cli")]
    absolute: bool,

    /// Terminate each output path with NUL instead of a newline.
    #[arg(long, default_value_t = false, requires = "cli")]
    print0: bool,

    /// Exit with status 1 when no path matches (batch CLI only).
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with = "interactive"
    )]
    fail_no_match: bool,

    /// Select files, folders, or both.
    #[arg(
        long = "type",
        value_enum,
        default_value_t = CliEntryType::All,
        requires = "cli"
    )]
    entry_type: CliEntryType,

    /// Interpret QUERY as a regular expression.
    #[arg(long, default_value_t = false, requires = "cli")]
    regex: bool,

    /// Match QUERY and ignore terms case-sensitively.
    #[arg(long, default_value_t = false, requires = "cli")]
    case_sensitive: bool,

    /// Choose automatic FileList preference, FileList only, or walker only.
    #[arg(long, value_enum, default_value_t = CliIndexSource::Auto, requires = "cli")]
    source: CliIndexSource,

    /// Read ignore terms from PATH instead of the executable-side ignore file.
    #[arg(
        long,
        value_name = "PATH",
        requires = "cli",
        conflicts_with = "no_ignore"
    )]
    ignore_file: Option<PathBuf>,

    /// Disable ignore-list filtering.
    #[arg(long, default_value_t = false, requires = "cli")]
    no_ignore: bool,

    /// Write indexing progress to standard error (batch CLI only).
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with = "interactive"
    )]
    progress: bool,

    /// Sort the complete match set before applying --limit.
    #[arg(
        long,
        value_enum,
        default_value_t = CliSortMode::Score,
        requires = "cli"
    )]
    sort: CliSortMode,

    /// Print matches, open a match, or reveal its containing folder.
    #[arg(
        long,
        value_enum,
        default_value_t = CliAction::Print,
        requires = "cli",
        conflicts_with_all = ["interactive", "list_saved_roots"]
    )]
    action: CliAction,

    /// Allow an open or reveal action to target every post-limit match.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["interactive", "list_saved_roots"]
    )]
    action_all: bool,

    /// Create the root FileList from a fresh walker index without prompting.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = ["interactive", "list_saved_roots", "action", "action_all"]
    )]
    create_filelist: bool,

    /// Permit replacing an existing root FileList during --create-filelist.
    #[arg(long, default_value_t = false, requires = "create_filelist")]
    overwrite_filelist: bool,

    /// Update pre-existing ancestor FileLists during --create-filelist.
    #[arg(long, default_value_t = false, requires = "create_filelist")]
    propagate_ancestors: bool,

    /// List persisted saved roots without indexing or selecting paths.
    #[arg(
        long,
        default_value_t = false,
        requires = "cli",
        conflicts_with_all = [
            "root",
            "use_default_root",
            "saved_root",
            "interactive",
            "action",
            "action_all"
        ]
    )]
    list_saved_roots: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BatchOutcome {
    Matches,
    NoMatch,
    Cancelled,
    ActionRejected,
    Action(AuthorizedActionReport),
}

#[cfg(target_os = "windows")]
fn configure_windows_dpi_mode() {
    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDPIAware() -> i32;
    }
    // SAFETY: process-wide DPI mode is switched before native window creation.
    // Keep this always-on for Windows to reduce monitor-crossing auto-resize jitter.
    let _ = unsafe { SetProcessDPIAware() };
    FlistWalkerApp::trace_window_event("windows_dpi_mode", "mode=system(always)");
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_dpi_mode() {}

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
    }
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

    if args.action == CliAction::Print {
        write_cli_paths(&paths, root, args.absolute, args.print0, cancelled.as_ref())?;
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
    cancelled: &AtomicBool,
) -> Result<()> {
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    for path in paths {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        output.write_all(&output_path_bytes(path, root, !absolute, print0))?;
        output.write_all(if print0 { b"\0" } else { b"\n" })?;
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }
    output.flush()?;
    Ok(())
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

fn validate_list_saved_roots_args(args: &Args) -> std::result::Result<(), &'static str> {
    if !args.query.is_empty()
        || args.limit != 1000
        || args.absolute
        || args.fail_no_match
        || !matches!(args.entry_type, CliEntryType::All)
        || args.regex
        || args.case_sensitive
        || !matches!(args.source, CliIndexSource::Auto)
        || args.ignore_file.is_some()
        || args.no_ignore
        || args.progress
        || !matches!(args.sort, CliSortMode::Score)
        || !matches!(args.action, CliAction::Print)
        || args.action_all
    {
        return Err("--list-saved-roots cannot be combined with search options");
    }
    Ok(())
}

fn validate_batch_action_args(args: &Args) -> std::result::Result<(), &'static str> {
    if args.action_all && args.action == CliAction::Print {
        return Err("--action-all requires --action open or --action reveal");
    }
    if args.action != CliAction::Print && (args.absolute || args.print0) {
        return Err("--absolute and --print0 are only valid with --action print");
    }
    Ok(())
}

fn validate_create_filelist_args(args: &Args) -> std::result::Result<(), &'static str> {
    if !args.create_filelist {
        return Ok(());
    }
    if !args.query.is_empty()
        || args.limit != 1000
        || args.absolute
        || args.print0
        || args.fail_no_match
        || !matches!(args.entry_type, CliEntryType::All)
        || args.regex
        || args.case_sensitive
        || !matches!(args.source, CliIndexSource::Auto)
        || args.ignore_file.is_some()
        || args.no_ignore
        || !matches!(args.sort, CliSortMode::Score)
        || !matches!(args.action, CliAction::Print)
        || args.action_all
    {
        return Err("--create-filelist cannot be combined with search, output, or action options");
    }
    Ok(())
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

fn run_gui(args: &Args) -> Result<()> {
    let startup_start = Instant::now();
    trace_startup_phase(startup_start, "run_gui_enter");
    configure_windows_dpi_mode();
    let root_explicit = args.root.is_some();
    let root = resolve_root(args.root.as_deref().unwrap_or(Path::new(".")))?;
    trace_startup_phase(startup_start, "root_resolved");
    let mut native_options = eframe::NativeOptions::default();
    let startup_geometry =
        FlistWalkerApp::startup_window_geometry_with_display_bounds(current_display_bounds());
    trace_startup_phase(startup_start, "startup_geometry_loaded");
    FlistWalkerApp::trace_window_event(
        "run_gui_start",
        &format!("root={} limit={}", root.display(), args.limit),
    );
    if let Some((pos, size)) = startup_geometry {
        FlistWalkerApp::trace_window_event(
            "run_gui_apply_startup_geometry",
            &format!(
                "x={:.1} y={:.1} width={:.1} height={:.1}",
                pos.x, pos.y, size.x, size.y
            ),
        );
    } else {
        FlistWalkerApp::trace_window_event("run_gui_no_startup_size", "using_default_size");
    }
    let icon = load_app_icon();
    trace_startup_phase(startup_start, "icon_prepared");
    native_options.viewport = build_root_viewport(startup_geometry, icon);
    let query = args.query.clone();
    let limit = args.limit;

    trace_startup_phase(startup_start, "run_native_before");
    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(move |cc| {
            configure_egui_fonts(&cc.egui_ctx);
            trace_startup_phase(startup_start, "fonts_configured");
            let app = FlistWalkerApp::from_launch(root, limit, query, root_explicit);
            trace_startup_phase(startup_start, "app_created");
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn current_display_bounds() -> Option<eframe::egui::Rect> {
    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(nIndex: i32) -> i32;
    }
    // SAFETY: GetSystemMetrics is read-only and does not require initialized window state.
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if width <= 0 || height <= 0 {
        FlistWalkerApp::trace_window_event(
            "current_display_bounds_unavailable",
            &format!("x={x} y={y} width={width} height={height}"),
        );
        return None;
    }
    let bounds = eframe::egui::Rect::from_min_size(
        eframe::egui::pos2(x as f32, y as f32),
        eframe::egui::vec2(width as f32, height as f32),
    );
    FlistWalkerApp::trace_window_event(
        "current_display_bounds",
        &format!(
            "x={:.1} y={:.1} width={:.1} height={:.1}",
            bounds.min.x,
            bounds.min.y,
            bounds.width(),
            bounds.height()
        ),
    );
    Some(bounds)
}

#[cfg(not(target_os = "windows"))]
fn current_display_bounds() -> Option<eframe::egui::Rect> {
    None
}

fn build_root_viewport(
    startup_geometry: Option<(eframe::egui::Pos2, eframe::egui::Vec2)>,
    icon: Option<eframe::egui::IconData>,
) -> eframe::egui::ViewportBuilder {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_app_id(APP_ID)
        .with_inner_size(DEFAULT_WINDOW_SIZE)
        .with_min_inner_size(MIN_WINDOW_SIZE);
    if let Some((pos, size)) = startup_geometry {
        viewport = viewport.with_position(pos).with_inner_size(size);
    }
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    viewport
}

fn load_app_icon() -> Option<eframe::egui::IconData> {
    let svg = include_bytes!("../assets/flistwalker-icon.svg");
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let target_px = 256u32;
    let mut pixmap = tiny_skia::Pixmap::new(target_px, target_px)?;
    let size = tree.size().to_int_size();
    let sx = target_px as f32 / size.width() as f32;
    let sy = target_px as f32 / size.height() as f32;
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let rgba = premultiplied_to_unmultiplied_rgba(pixmap.data());

    Some(eframe::egui::IconData {
        rgba,
        width: target_px,
        height: target_px,
    })
}

fn trace_startup_phase(start: Instant, phase: &str) {
    FlistWalkerApp::trace_window_event(
        "startup_phase",
        &format!(
            "phase={} elapsed_ms={:.3}",
            phase,
            start.elapsed().as_secs_f64() * 1000.0
        ),
    );
}

fn premultiplied_to_unmultiplied_rgba(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    for px in src.chunks_exact(4) {
        let r = px[0] as u32;
        let g = px[1] as u32;
        let b = px[2] as u32;
        let a = px[3] as u32;
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let unpremul = |c: u32| -> u8 {
            let v = ((c * 255 + a / 2) / a).min(255);
            v as u8
        };
        out.push(unpremul(r));
        out.push(unpremul(g));
        out.push(unpremul(b));
        out.push(a as u8);
    }
    out
}

fn resolve_root(root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize root: {}", root.display()))?;
    if !root.is_dir() {
        anyhow::bail!("root is not a directory: {}", root.display());
    }
    Ok(root)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .without_time()
        .compact()
        .try_init();
}

fn initialize_gui_mode() -> Result<()> {
    match recover_interrupted_update_on_startup() {
        Ok(Some(outcome)) => {
            warn!("startup updater recovery completed: {outcome}");
        }
        Ok(None) => {}
        Err(err) => {
            warn!("startup updater recovery requires operator attention: {err}");
        }
    }
    ctrlc::set_handler(|| {
        request_process_shutdown();
    })
    .context("failed to install signal handler")?;
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
    }
}

fn main() -> Result<ExitCode> {
    init_tracing();
    if run_internal_update_helper_if_requested()? {
        return Ok(ExitCode::SUCCESS);
    }

    let args = Args::parse();
    if args.cli && !args.interactive {
        if let Err(error) = validate_batch_action_args(&args) {
            eprintln!("error: {error}");
            return Ok(ExitCode::from(2));
        }
        if let Err(error) = validate_create_filelist_args(&args) {
            eprintln!("error: {error}");
            return Ok(ExitCode::from(2));
        }
    }
    let _runtime_config = initialize_runtime_config();
    if args.cli && !args.interactive && args.list_saved_roots {
        if let Err(error) = validate_list_saved_roots_args(&args) {
            eprintln!("error: {error}");
            return Ok(ExitCode::from(2));
        }
        list_saved_roots(&args)?;
        return Ok(ExitCode::SUCCESS);
    }
    if args.cli && !args.interactive && args.create_filelist {
        let root = match resolve_cli_root(&args) {
            Ok(root) => root,
            Err(error) => {
                eprintln!("error: {error}");
                return Ok(ExitCode::from(2));
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal_cancelled = Arc::clone(&cancelled);
        ctrlc::set_handler(move || signal_cancelled.store(true, Ordering::Relaxed))
            .context("failed to install CLI signal handler")?;
        return Ok(cli_filelist_exit_code(run_cli_filelist(
            &root,
            &args,
            cancelled.as_ref(),
        )));
    }
    if let Err(err) = ensure_ignore_list_sample() {
        warn!("failed to materialize ignore list sample: {}", err);
    }
    if args.cli {
        if args.interactive {
            let root = match resolve_cli_root(&args) {
                Ok(root) => root,
                Err(error) => {
                    eprintln!("error: {error}");
                    return Ok(ExitCode::from(2));
                }
            };
            let options = cli_tui_options(&args, load_cli_tui_ignore_terms(&args)?);
            Ok(match run_cli_tui(&root, &options)? {
                CliTuiOutcome::Selected => ExitCode::SUCCESS,
                CliTuiOutcome::Cancelled => ExitCode::from(130),
            })
        } else {
            let root = match resolve_cli_root(&args) {
                Ok(root) => root,
                Err(error) => {
                    eprintln!("error: {error}");
                    return Ok(ExitCode::from(2));
                }
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            let signal_cancelled = Arc::clone(&cancelled);
            ctrlc::set_handler(move || signal_cancelled.store(true, Ordering::Relaxed))
                .context("failed to install CLI signal handler")?;
            Ok(batch_exit_code(
                run_cli(&args, &root, &cancelled)?,
                args.fail_no_match,
            ))
        }
    } else {
        initialize_gui_mode()?;
        run_gui(&args)?;
        Ok(ExitCode::SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn default_gui_args_do_not_trigger_cli_option_requirements() {
        let args = Args::try_parse_from(["flistwalker"]).expect("parse default GUI arguments");

        assert!(!args.cli);
        assert!(!args.interactive);
        assert!(matches!(args.entry_type, CliEntryType::All));
        assert!(matches!(args.source, CliIndexSource::Auto));
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

    #[test]
    fn build_root_viewport_applies_defaults() {
        let viewport = build_root_viewport(None, None);

        assert_eq!(viewport.title.as_deref(), Some(APP_TITLE));
        assert_eq!(viewport.app_id.as_deref(), Some(APP_ID));
        assert_eq!(viewport.inner_size, Some(DEFAULT_WINDOW_SIZE));
        assert_eq!(viewport.min_inner_size, Some(MIN_WINDOW_SIZE));
        assert_eq!(viewport.position, None);
    }

    #[test]
    fn build_root_viewport_prefers_restored_geometry_and_icon() {
        let icon = eframe::egui::IconData {
            rgba: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        };
        let pos = eframe::egui::pos2(-1600.0, 120.0);
        let size = eframe::egui::vec2(900.0, 700.0);

        let viewport = build_root_viewport(Some((pos, size)), Some(icon));

        assert_eq!(viewport.position, Some(pos));
        assert_eq!(viewport.inner_size, Some(size));
        assert_eq!(viewport.min_inner_size, Some(MIN_WINDOW_SIZE));
        assert!(viewport.icon.is_some());
    }
}
