#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod gui_launch;

use anyhow::Result;
use std::process::ExitCode;
use tracing::warn;

use flist_walker::cli;
use flist_walker::ignore_list::ensure_ignore_list_sample;
use flist_walker::process_entry::initialize_process_entry;
use flist_walker::runtime_config::initialize_runtime_config;
use flist_walker::updater::InternalUpdaterAction;

fn main() -> Result<ExitCode> {
    let process_entry = initialize_process_entry()?;
    if process_entry == InternalUpdaterAction::Exit {
        return Ok(ExitCode::SUCCESS);
    }

    let args = if process_entry == InternalUpdaterAction::ContinueGuiAfterUpdate {
        cli::parse_gui_restart_args()
    } else {
        cli::parse_args()
    };
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

    let _runtime_config = initialize_runtime_config();
    if let Err(err) = ensure_ignore_list_sample() {
        warn!("failed to materialize ignore list sample: {}", err);
    }
    let previous_update_failure = gui_launch::initialize()?;
    gui_launch::run(
        args.root(),
        args.query(),
        args.limit(),
        args.max_depth(),
        args.follow_links(),
        previous_update_failure,
    )?;
    Ok(ExitCode::SUCCESS)
}
