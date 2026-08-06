mod cli;
mod gui_launch;
mod launch_path;
mod windows_console;

use anyhow::Result;
use std::process::ExitCode;
use tracing::warn;
use tracing_subscriber::EnvFilter;

use flist_walker::ignore_list::ensure_ignore_list_sample;
use flist_walker::runtime_config::initialize_runtime_config;
use flist_walker::updater::run_internal_updater_command_if_requested;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .without_time()
        .compact()
        .try_init();
}

fn main() -> Result<ExitCode> {
    init_tracing();
    if run_internal_updater_command_if_requested()? {
        return Ok(ExitCode::SUCCESS);
    }

    let args = cli::parse_args();
    if args.requests_update_command() {
        return cli::run_update_command(args.update_requested());
    }
    if let Err(error) = cli::validate_args(&args) {
        eprintln!("error: {error}");
        return Ok(ExitCode::from(2));
    }

    if args.is_cli() {
        let _runtime_config = initialize_runtime_config();
        return cli::run(&args);
    }

    windows_console::detach_from_console_for_gui();
    let _runtime_config = initialize_runtime_config();
    if let Err(err) = ensure_ignore_list_sample() {
        warn!("failed to materialize ignore list sample: {}", err);
    }
    gui_launch::initialize()?;
    gui_launch::run(args.root(), args.query(), args.limit())?;
    Ok(ExitCode::SUCCESS)
}
