use anyhow::Result;
use std::process::ExitCode;

use flist_walker::process_entry::initialize_process_entry;

fn main() -> Result<ExitCode> {
    if initialize_process_entry()? {
        return Ok(ExitCode::SUCCESS);
    }
    flist_walker::cli::run_dedicated()
}
