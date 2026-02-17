#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use flist_walker::app::{configure_egui_fonts, FlistWalkerApp};
use flist_walker::indexer::build_index;
use flist_walker::search::search_entries;

#[derive(Parser, Debug)]
#[command(name = "flistwalker")]
#[command(about = "FlistWalker Rust implementation")]
struct Args {
    #[arg(default_value = "")]
    query: String,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value_t = 1000)]
    limit: usize,
    #[arg(long, default_value_t = false)]
    cli: bool,
}

fn run_cli(args: &Args) -> Result<()> {
    let root = args
        .root
        .canonicalize()
        .unwrap_or_else(|_| args.root.clone());
    let entries = build_index(&root, true, true, true)?;
    let query = args.query.trim();
    if query.is_empty() {
        for path in entries.iter().take(args.limit.min(1000)) {
            println!("{}", path.display());
        }
        return Ok(());
    }

    let results = search_entries(query, &entries, args.limit.min(1000), false);
    for (path, score) in results {
        println!("[{score:6.1}] {}", path.display());
    }
    Ok(())
}

fn run_gui(args: &Args) -> Result<()> {
    let root = args
        .root
        .canonicalize()
        .unwrap_or_else(|_| args.root.clone());
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport =
        eframe::egui::ViewportBuilder::default().with_inner_size(eframe::egui::vec2(1400.0, 900.0));
    let query = args.query.clone();
    let limit = args.limit;

    eframe::run_native(
        "FlistWalker",
        native_options,
        Box::new(move |cc| {
            configure_egui_fonts(&cc.egui_ctx);
            Box::new(FlistWalkerApp::new(root, limit, query))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.cli {
        run_cli(&args)
    } else {
        run_gui(&args)
    }
}
