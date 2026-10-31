// The non-test expansion evaluates only the original expression, exactly once.
macro_rules! observe_persistence_io {
    ($stage:ident, $operation:expr) => {{
        #[cfg(test)]
        let _io_operation = crate::test_io::enter(crate::test_io::Stage::$stage);
        $operation
    }};
}

#[cfg(test)]
pub(crate) mod test_io;

pub mod actions;
pub mod app;
pub mod cli;
pub mod cli_tui;
pub mod command_exec;
pub mod entry;
pub mod fs_atomic;
pub mod ignore_list;
pub mod indexer;
pub mod launch_path;
pub mod path_utils;
pub mod persistence;
pub mod process_entry;
pub mod query;
pub(crate) mod query_history;
pub mod runtime_config;
pub mod search;
pub mod search_catalog;
pub(crate) mod text_editing;
pub mod ui_model;
pub mod update_security;
pub mod updater;
pub(crate) mod walker_runtime;

#[cfg(test)]
pub(crate) fn env_var_test_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
