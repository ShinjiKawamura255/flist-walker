#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use tracing::warn;
use tracing_subscriber::EnvFilter;

use flist_walker::app::{configure_egui_fonts, request_process_shutdown, FlistWalkerApp};
use flist_walker::ignore_list::{ensure_ignore_list_sample, load_ignore_terms_from_current_exe};
use flist_walker::indexer::build_index;
use flist_walker::query::path_matches_ignore_terms;
use flist_walker::runtime_config::initialize_runtime_config;
use flist_walker::search::search_entries_with_scope;
use resvg::{tiny_skia, usvg};

const APP_TITLE: &str = "FlistWalker";
const APP_ID: &str = "flistwalker";
const DEFAULT_WINDOW_SIZE: eframe::egui::Vec2 = eframe::egui::vec2(1400.0, 900.0);
const MIN_WINDOW_SIZE: eframe::egui::Vec2 = eframe::egui::vec2(640.0, 400.0);

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
    let ignore_terms = load_ignore_terms_from_current_exe();
    let entries = build_index(&root, true, true, true)?
        .into_iter()
        .filter(|path| !path_matches_ignore_terms(path, &root, &ignore_terms, true, true))
        .collect::<Vec<_>>();
    let query = args.query.trim();
    if query.is_empty() {
        for path in entries.iter().take(args.limit) {
            println!("{}", path.display());
        }
        return Ok(());
    }

    let results =
        search_entries_with_scope(query, &entries, args.limit, false, true, Some(&root), true);
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
    let startup_geometry = FlistWalkerApp::startup_window_geometry();
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
    native_options.viewport = build_root_viewport(startup_geometry, load_app_icon());
    let query = args.query.clone();
    let limit = args.limit;

    eframe::run_native(
        APP_TITLE,
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
    let _runtime_config = initialize_runtime_config();

    let args = Args::parse();
    if let Err(err) = ensure_ignore_list_sample() {
        warn!("failed to materialize ignore list sample: {}", err);
    }
    if args.cli {
        run_cli(&args)
    } else {
        run_gui(&args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
