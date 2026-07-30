mod args;
mod batch;

use anyhow::{Context, Result};
use std::process::ExitCode;

use flist_walker::updater::{
    check_for_update, current_version_string, prepare_and_start_update,
    recover_interrupted_update_on_startup, self_update_disabled, UpdateCandidate, UpdateSupport,
};

pub(crate) use args::{parse_args, validate_args, Args};
use batch::run_cli_mode;

fn update_available_message(candidate: &UpdateCandidate) -> String {
    format!(
        "Update available: v{} (current: v{})",
        candidate.target_version, candidate.current_version
    )
}

pub(crate) fn run_update_command(install: bool) -> Result<ExitCode> {
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
        println!("Run flistwalker --update to install it.");
        return Ok(ExitCode::SUCCESS);
    }

    match &candidate.support {
        UpdateSupport::Auto => {
            prepare_and_start_update(&candidate, &std::env::current_exe()?)
                .context("Update installation failed")?;
            println!("Update started. FlistWalker will restart when installation is complete.");
            Ok(ExitCode::SUCCESS)
        }
        UpdateSupport::ManualOnly { message } => {
            eprintln!("Automatic update is unavailable: {message}");
            eprintln!("Download the release manually: {}", candidate.release_url);
            Ok(ExitCode::from(1))
        }
    }
}

pub(crate) fn run(args: &Args) -> Result<ExitCode> {
    run_cli_mode(args)
}
