use super::*;
use std::fs;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

mod support;
use support::*;

mod app_core;
mod index_pipeline;
mod pipeline_tests;
mod query_history;
mod render_tests;
mod session_tabs;
mod shortcuts;
mod window_ime;
