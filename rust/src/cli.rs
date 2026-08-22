mod args;
mod batch;

use anyhow::{Context, Result};
use std::process::ExitCode;

use crate::runtime_config::initialize_runtime_config;
use crate::updater::{
    check_for_update, current_version_string, prepare_and_start_update,
    recover_interrupted_update_on_startup, running_binary_command_name, self_update_disabled,
    set_running_binary_variant, BinaryVariant, UpdateCandidate, UpdateRestartMode, UpdateSupport,
};

pub use args::{parse_args, validate_args, Args};
use batch::run_cli_mode;

fn update_available_message(candidate: &UpdateCandidate) -> String {
    format!(
        "Update available: v{} (current: v{})",
        candidate.target_version, candidate.current_version
    )
}

pub fn run_update_command(install: bool) -> Result<ExitCode> {
    if self_update_disabled() {
        if install {
            eprintln!("Automatic updates are disabled.");
            return Ok(ExitCode::from(1));
        }
        println!("Update checks are disabled.");
        return Ok(ExitCode::SUCCESS);
    }

    if install {
        if let Some(notice) = recover_interrupted_update_on_startup()? {
            eprintln!("{notice}");
        }
    }

    let Some(candidate) = check_for_update().context("Update check failed")? else {
        println!("FlistWalker is up to date (v{}).", current_version_string());
        return Ok(ExitCode::SUCCESS);
    };

    println!("{}", update_available_message(&candidate));
    if !install {
        println!(
            "Run {} --update to install it.",
            running_binary_command_name()
        );
        return Ok(ExitCode::SUCCESS);
    }

    match &candidate.support {
        UpdateSupport::Auto => {
            prepare_and_start_update(
                &candidate,
                &std::env::current_exe()?,
                UpdateRestartMode::Headless,
            )
            .context("Update installation failed")?;
            println!("Update started. Installation will complete in the background.");
            Ok(ExitCode::SUCCESS)
        }
        UpdateSupport::ManualOnly { message } => {
            eprintln!("Automatic update is unavailable: {message}");
            eprintln!("Download the release manually: {}", candidate.release_url);
            Ok(ExitCode::from(1))
        }
    }
}

pub fn run(args: &Args) -> Result<ExitCode> {
    run_cli_mode(args)
}

pub fn run_dedicated() -> Result<ExitCode> {
    set_running_binary_variant(BinaryVariant::Cli);
    let args = args::parse_dedicated_args();
    if args.requests_update_command() {
        return run_update_command(args.update_requested());
    }
    if let Err(error) = validate_args(&args) {
        eprintln!("error: {error}");
        return Ok(ExitCode::from(2));
    }
    let _runtime_config = initialize_runtime_config();
    run(&args)
}
