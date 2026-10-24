// Test-process-only clock bridge. Constructor/atexit run outside measured frames.
#include <os/signpost.h>
#include <mach/mach_time.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static os_log_t clock_log;
static pid_t owner_pid;
static char nonce[33];
static unsigned long long monotonic_ns(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t) != 0) abort();
    return (unsigned long long)t.tv_sec * 1000000000ULL + t.tv_nsec;
}
static void anchor(const char *label) {
    uint64_t tid = 0;
    if (pthread_threadid_np(NULL, &tid) != 0) abort();
    mach_timebase_info_data_t tb;
    if (mach_timebase_info(&tb) != KERN_SUCCESS) abort();
    unsigned long long m0 = monotonic_ns(), a0 = mach_absolute_time();
    os_signpost_event_emit(clock_log, OS_SIGNPOST_ID_EXCLUSIVE, "TC207Clock",
        "%{public}s nonce=%{public}s mono_before_ns=%{public}llu", label, nonce, m0);
    unsigned long long a1 = mach_absolute_time(), m1 = monotonic_ns();
    printf("TC207_CLOCK {\"label\":\"%s\",\"nonce\":\"%s\",\"pid\":%d,\"native_thread_id\":%llu,"
           "\"mono_before_ns\":%llu,\"mono_after_ns\":%llu,\"mach_before\":%llu,"
           "\"mach_after\":%llu,\"timebase_numer\":%u,\"timebase_denom\":%u,"
           "\"signpost_enabled\":%s}\n", label, nonce, getpid(), (unsigned long long)tid,
           m0, m1, a0, a1, tb.numer, tb.denom,
           os_signpost_enabled(clock_log) ? "true" : "false");
    fflush(stdout);
}
static void finish(void) { if (getpid() == owner_pid) anchor("process-exit"); }
__attribute__((constructor)) static void start(void) {
    const char *parent = getenv("TC207_DIAGNOSTIC_PARENT_PID");
    const char *value = getenv("TC207_DIAGNOSTIC_NONCE");
    if (!parent || !value || getppid() != (pid_t)strtol(parent, NULL, 10)) return;
    if (strlen(value) != 32 || strspn(value, "0123456789abcdef") != 32) abort();
    memcpy(nonce, value, 33);
    owner_pid = getpid();
    // No library injection or clock hooks in subsequently exec'd test helpers.
    unsetenv("DYLD_INSERT_LIBRARIES");
    unsetenv("TC207_DIAGNOSTIC_PARENT_PID");
    unsetenv("TC207_DIAGNOSTIC_NONCE");
    // The collector observes WIFSTOPPED, attaches and resumes on the actual
    // xctrace-start notification. This pause precedes every test frame/anchor.
    if (raise(SIGSTOP) != 0) abort();
    clock_log = os_log_create("org.flistwalker.tc207.diagnostic", OS_LOG_CATEGORY_POINTS_OF_INTEREST);
    anchor("process-start");
    if (atexit(finish) != 0) abort();
}
