use anyhow::Result;
use tracing_subscriber::EnvFilter;

use crate::updater::run_internal_updater_command_if_requested;

fn init_tracing_if_requested() {
    let Ok(filter) = EnvFilter::try_from_default_env() else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .without_time()
        .compact()
        .try_init();
}

/// Initializes optional tracing, then handles hidden updater commands before
/// either public entrypoint gives its arguments to clap.
pub fn initialize_process_entry() -> Result<bool> {
    init_tracing_if_requested();
    run_internal_updater_command_if_requested()
}
