use super::*;
use crate::entry::Entry;
use std::fs;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

mod support;
use support::*;

fn unknown_entry(path: PathBuf) -> Entry {
    Entry::unknown(path)
}

fn file_entry(path: PathBuf) -> Entry {
    Entry::file(path)
}

fn dir_entry(path: PathBuf) -> Entry {
    Entry::dir(path)
}

mod app_core;
mod index_pipeline;
mod pipeline_tests;
mod query_history;
mod render_tests;
mod session_tabs;
mod shortcuts;
mod window_ime;
