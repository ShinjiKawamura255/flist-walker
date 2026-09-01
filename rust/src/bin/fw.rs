use anyhow::Result;
use std::process::ExitCode;

use flist_walker::process_entry::initialize_process_entry;
use flist_walker::updater::InternalUpdaterAction;

fn main() -> Result<ExitCode> {
    match initialize_process_entry()? {
        InternalUpdaterAction::Exit => return Ok(ExitCode::SUCCESS),
        InternalUpdaterAction::ContinueGuiAfterUpdate => {
            anyhow::bail!("GUI update restart is only valid for the universal binary")
        }
        InternalUpdaterAction::Continue => {}
    }
    flist_walker::cli::run_dedicated()
}
