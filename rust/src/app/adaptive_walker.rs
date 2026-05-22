use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const CONTROL_SAMPLE_SIZE: usize = 64;
const CONTROL_SAMPLE_STABILITY_PCT: u64 = 5;

pub(super) struct AdaptiveWalkerEntry {
    pub(super) path: PathBuf,
    pub(super) file_type: fs::FileType,
}

#[derive(Clone, Debug, Default)]
pub(super) struct AdaptiveWalkerMetrics {
    pub(super) dirs_read: usize,
    pub(super) read_dir_errors: usize,
    pub(super) max_inflight_read_dirs: usize,
    pub(super) throttle_events: usize,
    pub(super) adaptive_limit_min: usize,
    pub(super) adaptive_limit_max: usize,
    pub(super) adaptive_limit_final: usize,
    pub(super) adaptive_limit_change_count: usize,
    pub(super) adaptive_limit_avg: f64,
    pub(super) read_dir_total_us: u128,
    pub(super) read_dir_max_us: u128,
}

struct SharedState {
    queue: VecDeque<PathBuf>,
    active: usize,
}

struct Shared {
    state: Mutex<SharedState>,
    cv: Condvar,
    stop: AtomicBool,
    limit: AtomicUsize,
    max_workers: usize,
    sample_size: usize,
    control: Mutex<LimitControlState>,
    metrics: AdaptiveAtomicMetrics,
}

#[derive(Clone, Copy, Debug)]
struct LimitThroughputSample {
    completed: usize,
    elapsed_us: u128,
}

#[derive(Clone, Copy, Debug)]
struct AdaptiveControlSnapshot {
    change_count: usize,
    average_limit: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LimitDirection {
    Increase,
    Decrease,
}

#[derive(Debug)]
struct LimitControlState {
    window_started_at: Instant,
    previous_sample: Option<LimitThroughputSample>,
    last_direction: Option<LimitDirection>,
    weighted_limit_us: u128,
    elapsed_us: u128,
    change_count: usize,
}

#[derive(Default)]
struct AdaptiveAtomicMetrics {
    dirs_read: AtomicUsize,
    read_dir_errors: AtomicUsize,
    max_inflight_read_dirs: AtomicUsize,
    throttle_events: AtomicUsize,
    adaptive_limit_min: AtomicUsize,
    adaptive_limit_max: AtomicUsize,
    limit_sample_count: AtomicUsize,
    read_dir_total_us: AtomicU64,
    read_dir_max_us: AtomicU64,
}

impl AdaptiveAtomicMetrics {
    fn new(initial_limit: usize) -> Self {
        Self {
            adaptive_limit_min: AtomicUsize::new(initial_limit),
            adaptive_limit_max: AtomicUsize::new(initial_limit),
            ..Self::default()
        }
    }

    fn record_limit(&self, limit: usize) {
        fetch_min(&self.adaptive_limit_min, limit);
        fetch_max(&self.adaptive_limit_max, limit);
    }

    fn record_limit_sample(&self, sample_size: usize) -> bool {
        let sample_index = self.limit_sample_count.fetch_add(1, Ordering::Relaxed) + 1;
        sample_index.is_multiple_of(sample_size)
    }

    fn record_read_dir(&self, elapsed: Duration) {
        self.dirs_read.fetch_add(1, Ordering::Relaxed);
        let elapsed_us = elapsed.as_micros().min(u64::MAX as u128) as u64;
        self.read_dir_total_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
        fetch_max_u64(&self.read_dir_max_us, elapsed_us);
    }

    fn snapshot(
        &self,
        final_limit: usize,
        control_snapshot: AdaptiveControlSnapshot,
    ) -> AdaptiveWalkerMetrics {
        AdaptiveWalkerMetrics {
            dirs_read: self.dirs_read.load(Ordering::Relaxed),
            read_dir_errors: self.read_dir_errors.load(Ordering::Relaxed),
            max_inflight_read_dirs: self.max_inflight_read_dirs.load(Ordering::Relaxed),
            throttle_events: self.throttle_events.load(Ordering::Relaxed),
            adaptive_limit_min: self.adaptive_limit_min.load(Ordering::Relaxed),
            adaptive_limit_max: self.adaptive_limit_max.load(Ordering::Relaxed),
            adaptive_limit_final: final_limit,
            adaptive_limit_change_count: control_snapshot.change_count,
            adaptive_limit_avg: control_snapshot.average_limit,
            read_dir_total_us: self.read_dir_total_us.load(Ordering::Relaxed) as u128,
            read_dir_max_us: self.read_dir_max_us.load(Ordering::Relaxed) as u128,
        }
    }
}

fn fetch_min(target: &AtomicUsize, value: usize) {
    let mut current = target.load(Ordering::Relaxed);
    while value < current {
        match target.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn fetch_max(target: &AtomicUsize, value: usize) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn fetch_max_u64(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn adjust_limit(shared: &Shared) {
    if !shared.metrics.record_limit_sample(shared.sample_size) {
        return;
    }

    let sample_count = shared.metrics.limit_sample_count.swap(0, Ordering::Relaxed);
    if sample_count == 0 {
        return;
    }

    let now = Instant::now();
    let mut control = match shared.control.lock() {
        Ok(control) => control,
        Err(_) => return,
    };
    let current_limit = shared.limit.load(Ordering::Relaxed).max(1);
    let elapsed_us = control.record_segment(current_limit, now);
    let current_sample = LimitThroughputSample {
        completed: sample_count,
        elapsed_us,
    };

    let Some(previous_sample) = control.previous_sample.replace(current_sample) else {
        return;
    };

    let current = shared.limit.load(Ordering::Relaxed);
    let (next, next_direction) = next_limit_from_throughput(
        current,
        shared.max_workers,
        control.last_direction,
        current_sample.completed,
        current_sample.elapsed_us,
        previous_sample.completed,
        previous_sample.elapsed_us,
    );
    control.last_direction = next_direction;
    if next != current {
        shared.limit.store(next, Ordering::Relaxed);
        shared.metrics.record_limit(next);
        control.change_count = control.change_count.saturating_add(1);
        shared.cv.notify_all();
    }
}

impl LimitControlState {
    fn record_segment(&mut self, current_limit: usize, now: Instant) -> u128 {
        let elapsed_us = self.window_started_at.elapsed().as_micros().max(1);
        self.weighted_limit_us = self
            .weighted_limit_us
            .saturating_add((current_limit as u128).saturating_mul(elapsed_us));
        self.elapsed_us = self.elapsed_us.saturating_add(elapsed_us);
        self.window_started_at = now;
        elapsed_us
    }

    fn snapshot(&mut self, current_limit: usize) -> AdaptiveControlSnapshot {
        self.record_segment(current_limit, Instant::now());
        let average_limit = if self.elapsed_us == 0 {
            current_limit.max(1) as f64
        } else {
            self.weighted_limit_us as f64 / self.elapsed_us as f64
        };
        AdaptiveControlSnapshot {
            change_count: self.change_count,
            average_limit,
        }
    }
}

impl Default for LimitControlState {
    fn default() -> Self {
        Self {
            window_started_at: Instant::now(),
            previous_sample: None,
            last_direction: None,
            weighted_limit_us: 0,
            elapsed_us: 0,
            change_count: 0,
        }
    }
}

pub(super) fn next_limit_from_throughput(
    current: usize,
    max_workers: usize,
    last_direction: Option<LimitDirection>,
    current_completed: usize,
    current_elapsed_us: u128,
    previous_completed: usize,
    previous_elapsed_us: u128,
) -> (usize, Option<LimitDirection>) {
    if current == 0 || max_workers == 0 {
        let next = current.max(1).min(max_workers.max(1));
        return (next, None);
    }

    let current_rate = (current_completed as u128).saturating_mul(previous_elapsed_us);
    let previous_rate = (previous_completed as u128).saturating_mul(current_elapsed_us);
    let improvement = previous_rate.saturating_mul(100 + CONTROL_SAMPLE_STABILITY_PCT as u128);
    let regression = previous_rate.saturating_mul(100 - CONTROL_SAMPLE_STABILITY_PCT as u128);
    let scaled_current = current_rate.saturating_mul(100);

    let direction = if scaled_current >= improvement {
        last_direction.unwrap_or(LimitDirection::Increase)
    } else if scaled_current <= regression {
        match last_direction {
            Some(LimitDirection::Increase) => LimitDirection::Decrease,
            Some(LimitDirection::Decrease) => LimitDirection::Increase,
            None => LimitDirection::Decrease,
        }
    } else if let Some(direction) = last_direction {
        direction
    } else {
        return (current, None);
    };

    let next = match direction {
        LimitDirection::Increase => current.saturating_add(1).min(max_workers),
        LimitDirection::Decrease => current.saturating_sub(1).max(1),
    };
    if next == current {
        (current, None)
    } else {
        (next, Some(direction))
    }
}

fn worker_loop(shared: Arc<Shared>, tx: SyncSender<AdaptiveWalkerEntry>) {
    loop {
        let dir = {
            let mut state = match shared.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                let limit = shared.limit.load(Ordering::Relaxed).max(1);
                if state.active < limit {
                    if let Some(dir) = state.queue.pop_front() {
                        state.active = state.active.saturating_add(1);
                        fetch_max(&shared.metrics.max_inflight_read_dirs, state.active);
                        break dir;
                    }
                } else if !state.queue.is_empty() {
                    shared
                        .metrics
                        .throttle_events
                        .fetch_add(1, Ordering::Relaxed);
                }
                if state.queue.is_empty() && state.active == 0 {
                    shared.cv.notify_all();
                    return;
                }
                let Ok((next_state, _)) = shared.cv.wait_timeout(state, CONTROL_POLL_INTERVAL)
                else {
                    return;
                };
                state = next_state;
            }
        };

        let started = Instant::now();
        let mut child_dirs = Vec::new();
        match fs::read_dir(&dir) {
            Ok(read_dir) => {
                for child in read_dir {
                    if shared.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(child) = child else {
                        shared
                            .metrics
                            .read_dir_errors
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    };
                    let Ok(file_type) = child.file_type() else {
                        shared
                            .metrics
                            .read_dir_errors
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    };
                    let policy = adaptive_entry_policy(&child, &file_type);
                    if policy.skip {
                        continue;
                    }
                    let path = child.path();
                    if file_type.is_dir() && policy.recurse {
                        child_dirs.push(path.clone());
                    }
                    if tx.send(AdaptiveWalkerEntry { path, file_type }).is_err() {
                        shared.stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            Err(_) => {
                shared
                    .metrics
                    .read_dir_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        let elapsed = started.elapsed();
        shared.metrics.record_read_dir(elapsed);
        adjust_limit(&shared);

        if let Ok(mut state) = shared.state.lock() {
            if !shared.stop.load(Ordering::Relaxed) {
                state.queue.extend(child_dirs);
            }
            state.active = state.active.saturating_sub(1);
            shared.cv.notify_all();
        } else {
            return;
        }
    }
}

struct AdaptiveEntryPolicy {
    skip: bool,
    recurse: bool,
}

fn adaptive_entry_policy(entry: &fs::DirEntry, file_type: &fs::FileType) -> AdaptiveEntryPolicy {
    if !file_type.is_symlink() {
        return AdaptiveEntryPolicy {
            skip: false,
            recurse: true,
        };
    }

    adaptive_entry_policy_from_attrs(windows_reparse_file_attributes(entry))
}

#[cfg(windows)]
fn windows_reparse_file_attributes(entry: &fs::DirEntry) -> Option<u32> {
    fs::symlink_metadata(entry.path())
        .ok()
        .map(|metadata| metadata.file_attributes())
}

#[cfg(not(windows))]
fn windows_reparse_file_attributes(_entry: &fs::DirEntry) -> Option<u32> {
    None
}

fn adaptive_entry_policy_from_attrs(windows_attrs: Option<u32>) -> AdaptiveEntryPolicy {
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0000_0004;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    let Some(attrs) = windows_attrs else {
        return AdaptiveEntryPolicy {
            skip: false,
            recurse: true,
        };
    };
    let is_reparse = attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    let is_hidden_system = attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM)
        == (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);

    AdaptiveEntryPolicy {
        skip: is_reparse && is_hidden_system,
        recurse: !is_reparse,
    }
}

pub(super) fn walk_adaptive(
    root: &Path,
    max_workers: usize,
    initial_limit: usize,
    mut on_entry: impl FnMut(AdaptiveWalkerEntry) -> bool,
    should_stop: impl Fn() -> bool,
) -> AdaptiveWalkerMetrics {
    let max_workers = max_workers.max(1);
    if max_workers == 1 {
        return walk_adaptive_serial(root, on_entry, should_stop);
    }
    let initial_limit = initial_limit.clamp(1, max_workers);
    let shared = Arc::new(Shared {
        state: Mutex::new(SharedState {
            queue: VecDeque::from([root.to_path_buf()]),
            active: 0,
        }),
        cv: Condvar::new(),
        stop: AtomicBool::new(false),
        limit: AtomicUsize::new(initial_limit),
        max_workers,
        sample_size: CONTROL_SAMPLE_SIZE,
        control: Mutex::new(LimitControlState {
            window_started_at: Instant::now(),
            previous_sample: None,
            last_direction: None,
            weighted_limit_us: 0,
            elapsed_us: 0,
            change_count: 0,
        }),
        metrics: AdaptiveAtomicMetrics::new(initial_limit),
    });
    let entry_queue_capacity = max_workers.saturating_mul(256).max(256);
    let (tx, rx) = mpsc::sync_channel(entry_queue_capacity);
    let mut handles = Vec::new();
    for _ in 0..max_workers {
        let worker_shared = Arc::clone(&shared);
        let worker_tx = tx.clone();
        handles.push(thread::spawn(move || worker_loop(worker_shared, worker_tx)));
    }
    drop(tx);

    loop {
        match rx.recv_timeout(CONTROL_POLL_INTERVAL) {
            Ok(entry) => {
                if should_stop() || !on_entry(entry) {
                    shared.stop.store(true, Ordering::Relaxed);
                    shared.cv.notify_all();
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if should_stop() {
                    shared.stop.store(true, Ordering::Relaxed);
                    shared.cv.notify_all();
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    shared.stop.store(true, Ordering::Relaxed);
    shared.cv.notify_all();
    drop(rx);
    for handle in handles {
        let _ = handle.join();
    }
    let final_limit = shared.limit.load(Ordering::Relaxed).max(1);
    let control_snapshot = match shared.control.lock() {
        Ok(mut control) => control.snapshot(final_limit),
        Err(_) => AdaptiveControlSnapshot {
            change_count: 0,
            average_limit: final_limit as f64,
        },
    };
    shared.metrics.snapshot(final_limit, control_snapshot)
}

fn walk_adaptive_serial(
    root: &Path,
    mut on_entry: impl FnMut(AdaptiveWalkerEntry) -> bool,
    should_stop: impl Fn() -> bool,
) -> AdaptiveWalkerMetrics {
    let mut metrics = AdaptiveWalkerMetrics {
        adaptive_limit_min: 1,
        adaptive_limit_max: 1,
        adaptive_limit_final: 1,
        adaptive_limit_change_count: 0,
        adaptive_limit_avg: 1.0,
        ..AdaptiveWalkerMetrics::default()
    };
    let mut queue = VecDeque::from([root.to_path_buf()]);

    while let Some(dir) = queue.pop_front() {
        if should_stop() {
            break;
        }
        metrics.max_inflight_read_dirs = metrics.max_inflight_read_dirs.max(1);
        let started = Instant::now();
        let mut stop = false;
        match fs::read_dir(&dir) {
            Ok(read_dir) => {
                for child in read_dir {
                    if should_stop() {
                        stop = true;
                        break;
                    }
                    let Ok(child) = child else {
                        metrics.read_dir_errors = metrics.read_dir_errors.saturating_add(1);
                        continue;
                    };
                    let Ok(file_type) = child.file_type() else {
                        metrics.read_dir_errors = metrics.read_dir_errors.saturating_add(1);
                        continue;
                    };
                    let policy = adaptive_entry_policy(&child, &file_type);
                    if policy.skip {
                        continue;
                    }
                    let path = child.path();
                    if file_type.is_dir() && policy.recurse {
                        queue.push_back(path.clone());
                    }
                    if !on_entry(AdaptiveWalkerEntry { path, file_type }) {
                        stop = true;
                        break;
                    }
                }
            }
            Err(_) => {
                metrics.read_dir_errors = metrics.read_dir_errors.saturating_add(1);
            }
        }
        let elapsed_us = started.elapsed().as_micros();
        metrics.dirs_read = metrics.dirs_read.saturating_add(1);
        metrics.read_dir_total_us = metrics.read_dir_total_us.saturating_add(elapsed_us);
        metrics.read_dir_max_us = metrics.read_dir_max_us.max(elapsed_us);
        if stop {
            break;
        }
    }

    metrics
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("flistwalker-adaptive-walker-{name}-{nonce}"))
    }

    #[test]
    fn windows_hidden_system_reparse_points_are_skipped() {
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0000_0004;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

        let policy = adaptive_entry_policy_from_attrs(Some(
            FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_REPARSE_POINT,
        ));

        assert!(policy.skip);
        assert!(!policy.recurse);
    }

    #[test]
    fn visible_reparse_points_are_emitted_but_not_recursed() {
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

        let policy = adaptive_entry_policy_from_attrs(Some(FILE_ATTRIBUTE_REPARSE_POINT));

        assert!(!policy.skip);
        assert!(!policy.recurse);
    }

    #[test]
    fn regular_entries_are_emitted_and_recursed() {
        let policy = adaptive_entry_policy_from_attrs(None);

        assert!(!policy.skip);
        assert!(policy.recurse);
    }

    #[test]
    fn single_worker_uses_serial_path_and_records_metrics() {
        let root = test_root("serial");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("dir")).expect("create dir");
        fs::write(root.join("dir").join("main.rs"), "fn main() {}").expect("write file");

        let mut paths = Vec::new();
        let metrics = walk_adaptive(
            &root,
            1,
            1,
            |entry| {
                paths.push(entry.path);
                true
            },
            || false,
        );

        assert!(paths.iter().any(|path| path.ends_with("dir")));
        assert!(paths.iter().any(|path| path.ends_with("main.rs")));
        assert!(metrics.dirs_read >= 1);
        assert_eq!(metrics.max_inflight_read_dirs, 1);
        assert_eq!(metrics.throttle_events, 0);
        assert_eq!(metrics.adaptive_limit_min, 1);
        assert_eq!(metrics.adaptive_limit_max, 1);
        assert_eq!(metrics.adaptive_limit_final, 1);
        assert_eq!(metrics.adaptive_limit_change_count, 0);
        assert_eq!(metrics.adaptive_limit_avg, 1.0);

        let _ = fs::remove_dir_all(&root);
    }
}
