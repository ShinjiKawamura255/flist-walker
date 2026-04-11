use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::OnceLock;
use std::{env, thread};

const SEARCH_PARALLEL_THRESHOLD_DEFAULT: usize = 25_000;
const SEARCH_PARALLEL_CHUNK_MIN: usize = 1_024;
const SEARCH_PARALLEL_CHUNK_MAX: usize = 16_384;
const SEARCH_THREADS_MAX: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchExecutionMode {
    Auto,
    Sequential,
    Parallel,
}

pub(super) fn search_parallel_threshold() -> usize {
    env::var("FLISTWALKER_SEARCH_PARALLEL_THRESHOLD")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(SEARCH_PARALLEL_THRESHOLD_DEFAULT)
}

pub(super) fn search_threads() -> usize {
    env::var("FLISTWALKER_SEARCH_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(|value| value.get())
                .unwrap_or(1)
        })
        .min(SEARCH_THREADS_MAX)
}

pub(super) fn search_parallel_chunk_size(candidate_count: usize) -> usize {
    let threads = search_threads().max(1);
    let target = candidate_count / threads.saturating_mul(8).max(1);
    target.clamp(SEARCH_PARALLEL_CHUNK_MIN, SEARCH_PARALLEL_CHUNK_MAX)
}

fn search_thread_pool() -> &'static Option<ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = search_threads();
        if threads <= 1 {
            None
        } else {
            ThreadPoolBuilder::new().num_threads(threads).build().ok()
        }
    })
}

pub(super) fn with_search_thread_pool<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match search_thread_pool() {
        Some(pool) => pool.install(f),
        None => f(),
    }
}

pub(super) fn resolve_execution_mode(
    mode: SearchExecutionMode,
    candidate_count: usize,
) -> SearchExecutionMode {
    match mode {
        SearchExecutionMode::Auto => {
            if candidate_count >= search_parallel_threshold() && search_threads() > 1 {
                SearchExecutionMode::Parallel
            } else {
                SearchExecutionMode::Sequential
            }
        }
        other => other,
    }
}
