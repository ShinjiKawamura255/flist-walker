#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;
use tracing_subscriber::EnvFilter;

use flist_walker::app::{configure_egui_fonts, request_process_shutdown, FlistWalkerApp};
use flist_walker::cli_tui::{run_cli_tui, CliTuiOptions, CliTuiOutcome};
use flist_walker::ignore_list::{
    ensure_ignore_list_sample, load_ignore_terms_from_current_exe, parse_ignore_terms,
};
use flist_walker::indexer::{
    build_index_cancellable, find_filelist_in_first_level, is_index_build_cancelled,
};
use flist_walker::path_utils::output_path_bytes;
use flist_walker::query::{CompiledIgnoreTerms, QueryScope};
use flist_walker::runtime_config::initialize_runtime_config;
use flist_walker::search::try_search_entries_with_scope;
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

#[derive(Parser, Debug)]
#[command(name = "flistwalker")]
#[command(about = "Find files and folders with fuzzy search")]
#[command(version)]
struct Args {
    /// Query using fuzzy matching and the supported fzf-style operators.
    #[arg(default_value = "", value_name = "QUERY")]
    query: String,

    /// Root directory to search (defaults to the current directory).
    #[arg(long, value_name = "PATH")]
    root: Option<PathBuf>,

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

    /// Exit with status 1 when no path matches.
    #[arg(long, default_value_t = false, requires = "cli")]
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

    /// Write indexing progress to standard error.
    #[arg(long, default_value_t = false, requires = "cli")]
    progress: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatchOutcome {
    Matches,
    NoMatch,
    Cancelled,
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

fn load_cli_ignore_terms(args: &Args) -> Result<Vec<String>> {
    if args.no_ignore {
        Ok(Vec::new())
    } else if let Some(path) = args.ignore_file.as_deref() {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read ignore file: {}", path.display()))?;
        Ok(parse_ignore_terms(&text))
    } else {
        Ok(load_ignore_terms_from_current_exe())
    }
}

fn run_cli(args: &Args, cancelled: &AtomicBool) -> Result<BatchOutcome> {
    let root = resolve_root(args.root.as_deref().unwrap_or(Path::new(".")))?;
    let (include_files, include_dirs) = args.entry_type.include_flags();
    let use_filelist = match args.source {
        CliIndexSource::Auto | CliIndexSource::Filelist => true,
        CliIndexSource::Walker => false,
    };
    if matches!(args.source, CliIndexSource::Filelist)
        && find_filelist_in_first_level(&root).is_none()
    {
        anyhow::bail!(
            "FileList was required but none was found in {}",
            root.display()
        );
    }

    let ignore_terms = load_cli_ignore_terms(args)?;
    let ignore_case = !args.case_sensitive;
    let compiled_ignore_terms = CompiledIgnoreTerms::compile(&ignore_terms, ignore_case);

    if args.progress {
        eprintln!("Indexing {}...", root.display());
    }
    let indexed_entries =
        match build_index_cancellable(&root, use_filelist, include_files, include_dirs, || {
            cancelled.load(Ordering::Relaxed)
        }) {
            Ok(entries) => entries,
            Err(error) if is_index_build_cancelled(&error) => return Ok(BatchOutcome::Cancelled),
            Err(error) => return Err(error),
        };
    if cancelled.load(Ordering::Relaxed) {
        return Ok(BatchOutcome::Cancelled);
    }
    let mut entries = Vec::with_capacity(indexed_entries.len());
    for path in indexed_entries {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(BatchOutcome::Cancelled);
        }
        if !compiled_ignore_terms.matches_path(
            &path,
            QueryScope {
                root: Some(&root),
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
    let query = args.query.trim();
    let paths = if query.is_empty() {
        entries.iter().take(args.limit).cloned().collect::<Vec<_>>()
    } else {
        let results = try_search_entries_with_scope(
            query,
            &entries,
            args.limit,
            args.regex,
            ignore_case,
            Some(&root),
            true,
        )
        .map_err(anyhow::Error::msg)?;
        if cancelled.load(Ordering::Relaxed) {
            return Ok(BatchOutcome::Cancelled);
        }
        results
            .into_iter()
            .map(|(path, _score)| path)
            .collect::<Vec<_>>()
    };

    let mut framed_output = Vec::new();
    for path in &paths {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(BatchOutcome::Cancelled);
        }
        framed_output.extend_from_slice(&output_path_bytes(
            path,
            &root,
            !args.absolute,
            args.print0,
        ));
        framed_output.extend_from_slice(if args.print0 { b"\0" } else { b"\n" });
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(BatchOutcome::Cancelled);
    }
    let stdout = io::stdout();
    let mut output = stdout.lock();
    output.write_all(&framed_output)?;
    output.flush()?;

    Ok(if paths.is_empty() {
        BatchOutcome::NoMatch
    } else {
        BatchOutcome::Matches
    })
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

fn main() -> Result<ExitCode> {
    init_tracing();
    if run_internal_update_helper_if_requested()? {
        return Ok(ExitCode::SUCCESS);
    }

    let args = Args::parse();
    let _runtime_config = initialize_runtime_config();
    if let Err(err) = ensure_ignore_list_sample() {
        warn!("failed to materialize ignore list sample: {}", err);
    }
    if args.cli {
        if args.interactive {
            let root = resolve_root(args.root.as_deref().unwrap_or(Path::new(".")))?;
            let (include_files, include_dirs) = args.entry_type.include_flags();
            let options = CliTuiOptions {
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
                ignore_terms: load_cli_ignore_terms(&args)?,
            };
            Ok(match run_cli_tui(&root, &options)? {
                CliTuiOutcome::Selected => ExitCode::SUCCESS,
                CliTuiOutcome::Cancelled => ExitCode::from(130),
            })
        } else {
            let cancelled = Arc::new(AtomicBool::new(false));
            let signal_cancelled = Arc::clone(&cancelled);
            ctrlc::set_handler(move || signal_cancelled.store(true, Ordering::Relaxed))
                .context("failed to install CLI signal handler")?;
            Ok(match run_cli(&args, &cancelled)? {
                BatchOutcome::Cancelled => ExitCode::from(130),
                BatchOutcome::NoMatch if args.fail_no_match => ExitCode::from(1),
                BatchOutcome::Matches | BatchOutcome::NoMatch => ExitCode::SUCCESS,
            })
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

    #[test]
    fn default_gui_args_do_not_trigger_cli_option_requirements() {
        let args = Args::try_parse_from(["flistwalker"]).expect("parse default GUI arguments");

        assert!(!args.cli);
        assert!(!args.interactive);
        assert!(matches!(args.entry_type, CliEntryType::All));
        assert!(matches!(args.source, CliIndexSource::Auto));
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
        let cancelled = AtomicBool::new(true);

        assert_eq!(
            run_cli(&args, &cancelled).expect("cancelled CLI outcome"),
            BatchOutcome::Cancelled
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
