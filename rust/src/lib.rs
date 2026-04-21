pub mod actions;
pub mod app;
pub mod entry;
pub mod fs_atomic;
pub mod ignore_list;
pub mod indexer;
pub mod path_utils;
pub mod query;
pub mod search;
pub mod ui_model;
pub mod update_security;
pub mod updater;

#[cfg(test)]
pub(crate) fn env_var_test_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
