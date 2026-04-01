#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

use flist_walker::app::{configure_egui_fonts, request_process_shutdown, FlistWalkerApp};
use flist_walker::indexer::build_index;
use flist_walker::search::search_entries_with_scope;
use resvg::{tiny_skia, usvg};

#[derive(Parser, Debug)]
#[command(name = "flistwalker")]
#[command(about = "FlistWalker Rust implementation")]
#[command(version)]
struct Args {
    #[arg(default_value = "")]
    query: String,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long, default_value_t = 1000)]
    limit: usize,
    #[arg(long, default_value_t = false)]
    cli: bool,
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

fn run_cli(args: &Args) -> Result<()> {
    let root = resolve_root(args.root.as_deref().unwrap_or(Path::new(".")))?;
    let entries = build_index(&root, true, true, true)?;
    let query = args.query.trim();
    if query.is_empty() {
        for path in entries.iter().take(args.limit) {
            println!("{}", path.display());
        }
        return Ok(());
    }

    let results = search_entries_with_scope(
        query,
        &entries,
        args.limit,
        false,
        true,
        Some(&root),
        true,
    );
    for (path, score) in results {
        println!("[{score:6.1}] {}", path.display());
    }
    Ok(())
}

fn run_gui(args: &Args) -> Result<()> {
    configure_windows_dpi_mode();
    let root_explicit = args.root.is_some();
    let root = resolve_root(args.root.as_deref().unwrap_or(Path::new(".")))?;
    let mut native_options = eframe::NativeOptions::default();
    let mut viewport =
        eframe::egui::ViewportBuilder::default().with_inner_size(eframe::egui::vec2(1400.0, 900.0));
    FlistWalkerApp::trace_window_event(
        "run_gui_start",
        &format!("root={} limit={}", root.display(), args.limit),
    );
    if let Some((pos, size)) = FlistWalkerApp::startup_window_geometry() {
        viewport = viewport.with_position(pos);
        viewport = viewport.with_inner_size(size);
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
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }
    native_options.viewport = viewport;
    let query = args.query.clone();
    let limit = args.limit;

    eframe::run_native(
        "FlistWalker",
        native_options,
        Box::new(move |cc| {
            configure_egui_fonts(&cc.egui_ctx);
            Ok(Box::new(FlistWalkerApp::from_launch(
                root,
                limit,
                query,
                root_explicit,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
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

fn main() -> Result<()> {
    init_tracing();
    ctrlc::set_handler(|| {
        request_process_shutdown();
    })
    .context("failed to install signal handler")?;

    let args = Args::parse();
    if args.cli {
        run_cli(&args)
    } else {
        run_gui(&args)
    }
}
