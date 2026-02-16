use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use fast_file_finder_rs::app::FastFileFinderApp;
use fast_file_finder_rs::indexer::build_index;
use fast_file_finder_rs::search::search_entries;

#[derive(Parser, Debug)]
#[command(name = "fast-file-finder-rs")]
#[command(about = "FastFileFinder Rust implementation")]
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
    let root = args.root.canonicalize().unwrap_or_else(|_| args.root.clone());
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
    let root = args.root.canonicalize().unwrap_or_else(|_| args.root.clone());
    let native_options = eframe::NativeOptions::default();
    let query = args.query.clone();
    let limit = args.limit;

    eframe::run_native(
        "FastFileFinder (Rust)",
        native_options,
        Box::new(move |_cc| Box::new(FastFileFinderApp::new(root, limit, query))),
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
