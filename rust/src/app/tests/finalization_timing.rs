//! Test-only observation. CPU/off-CPU bounds never change the wall-clock gate.
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(super) struct FrameTiming {
    pub(super) wall: Duration,
    pub(super) cpu: Option<Duration>,
    pub(super) measurement_overhead: Duration,
    pub(super) monotonic_window_ns: Option<[u128; 2]>,
}

impl FrameTiming {
    pub(super) fn off_cpu_lower_bound(&self) -> Option<Duration> {
        self.cpu.map(|cpu| self.wall.saturating_sub(cpu))
    }

    pub(super) fn cpu_lower_bound(&self) -> Option<Duration> {
        self.cpu
            .map(|cpu| cpu.saturating_sub(self.measurement_overhead))
    }
}

#[cfg(unix)]
fn clock_time(clock: libc::clockid_t) -> Option<Duration> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the OS writes one initialized timespec for the calling thread.
    let result = unsafe { libc::clock_gettime(clock, &mut time) };
    (result == 0 && time.tv_sec >= 0 && (0..1_000_000_000).contains(&time.tv_nsec))
        .then(|| Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
}

#[cfg(unix)]
fn thread_cpu_time() -> Option<Duration> {
    clock_time(libc::CLOCK_THREAD_CPUTIME_ID)
}

#[cfg(not(unix))]
fn thread_cpu_time() -> Option<Duration> {
    None
}

#[cfg(unix)]
fn monotonic_time_ns() -> Option<u128> {
    clock_time(libc::CLOCK_MONOTONIC).map(|time| time.as_nanos())
}

#[cfg(not(unix))]
fn monotonic_time_ns() -> Option<u128> {
    None
}

pub(super) fn native_thread_id() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut id = 0;
        // SAFETY: zero requests the calling thread; id points to initialized u64.
        (unsafe { libc::pthread_threadid_np(0, &mut id) } == 0).then_some(id)
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: gettid takes no arguments and returns the calling kernel TID.
        let id = unsafe { libc::syscall(libc::SYS_gettid) };
        (id > 0).then_some(id as u64)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

pub(super) fn measure(work: impl FnOnce()) -> FrameTiming {
    // CPU samples enclose the strict wall interval. Thus cpu is an upper bound
    // on CPU spent within it; subtracting the outer wall overhead gives a lower
    // bound. Clock error/quantization remains visible rather than a verdict.
    let outer = Instant::now();
    let cpu_started = thread_cpu_time();
    let monotonic_started = monotonic_time_ns();
    let started = Instant::now();
    work();
    let wall = started.elapsed();
    let monotonic_finished = monotonic_time_ns();
    let cpu = thread_cpu_time()
        .zip(cpu_started)
        .and_then(|(end, start)| end.checked_sub(start));
    let measurement_overhead = outer.elapsed().saturating_sub(wall);
    FrameTiming {
        wall,
        cpu,
        measurement_overhead,
        monotonic_window_ns: monotonic_started
            .zip(monotonic_finished)
            .and_then(|(start, end)| (start <= end).then_some([start, end])),
    }
}

/// Explicit negative experiments only: callers must retain their nonzero exit.
/// No production call site, normal test run, retry, or timing exemption uses it.
pub(super) fn diagnostic_control(mode: &str) {
    match mode {
        "cpu" => {
            let start = thread_cpu_time().expect("CPU control requires a thread CPU clock");
            let deadline = Instant::now() + Duration::from_secs(5);
            while thread_cpu_time().expect("CPU clock disappeared") - start
                < Duration::from_millis(120)
            {
                assert!(
                    Instant::now() < deadline,
                    "CPU control exceeded safety deadline"
                );
                std::hint::black_box(
                    (0..256).fold(1u64, |n, i| n.wrapping_mul(31).wrapping_add(i)),
                );
            }
        }
        "sleep" => std::thread::sleep(Duration::from_millis(150)),
        #[cfg(unix)]
        "stop" => {
            // The collecting parent must observe WIFSTOPPED, retain its stop
            // interval, and always SIGCONT/kill+reap on timeout or exception.
            eprintln!("TC207_CONTROL stop pid={}", std::process::id());
            // SAFETY: signal only this disposable test process, never the host.
            assert_eq!(unsafe { libc::raise(libc::SIGSTOP) }, 0);
        }
        _ => panic!("unknown TC-207 diagnostic control: {mode}"),
    }
}

#[test]
fn tc_207_cpu_and_off_cpu_bounds_preserve_measurement_uncertainty() {
    let work = FrameTiming {
        wall: Duration::from_millis(125),
        cpu: Some(Duration::from_millis(121)),
        measurement_overhead: Duration::from_millis(2),
        monotonic_window_ns: None,
    };
    assert_eq!(work.cpu_lower_bound(), Some(Duration::from_millis(119)));
    assert_eq!(work.off_cpu_lower_bound(), Some(Duration::from_millis(4)));
    let off_cpu = FrameTiming {
        wall: Duration::from_millis(150),
        cpu: Some(Duration::from_millis(1)),
        measurement_overhead: Duration::from_micros(5),
        monotonic_window_ns: None,
    };
    assert_eq!(
        off_cpu.off_cpu_lower_bound(),
        Some(Duration::from_millis(149))
    );
    let bracket = FrameTiming {
        wall: Duration::from_micros(1),
        cpu: Some(Duration::from_micros(2)),
        measurement_overhead: Duration::from_micros(3),
        monotonic_window_ns: None,
    };
    assert_eq!(bracket.cpu_lower_bound(), Some(Duration::ZERO));
    assert_eq!(bracket.off_cpu_lower_bound(), Some(Duration::ZERO));
    let missing = FrameTiming { cpu: None, ..work };
    assert_eq!(missing.cpu_lower_bound(), None);
    assert_eq!(missing.off_cpu_lower_bound(), None);
}

#[test]
fn tc_207_measure_observes_executed_work_and_retains_wall_interval() {
    let mut called = false;
    let timing = measure(|| called = true);
    assert!(called);
    #[cfg(unix)]
    assert!(timing.cpu.is_some(), "native thread CPU clock unavailable");
    #[cfg(unix)]
    assert!(timing.monotonic_window_ns.is_some());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    assert!(native_thread_id().is_some_and(|id| id > 0));
    #[cfg(not(unix))]
    assert!(timing.cpu.is_none());
}
