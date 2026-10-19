#include <signal.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

__attribute__((noinline)) void diagnostic_fault(const char *private_value) {
    volatile int *address = 0;
    *address = 1;
}

__attribute__((noinline)) void diagnostic_abort(void) {
    abort();
}

static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t ready = PTHREAD_COND_INITIALIZER;
static int leader_waiting;

__attribute__((noinline)) void *diagnostic_worker(void *private_value) {
    pthread_mutex_lock(&lock);
    while (!leader_waiting) pthread_cond_wait(&ready, &lock);
    pthread_mutex_unlock(&lock);
    diagnostic_fault(private_value);
    return 0;
}

__attribute__((noinline)) void diagnostic_leader_wait(pthread_t worker) {
    pthread_mutex_lock(&lock);
    leader_waiting = 1;
    pthread_cond_signal(&ready);
    pthread_mutex_unlock(&lock);
    pthread_join(worker, 0);
}

int main(int argc, char **argv) {
    if (argc < 2) return 9;
    if (!strcmp(argv[1], "segv")) diagnostic_fault(argv[2]);
    if (!strcmp(argv[1], "abort")) diagnostic_abort();
    if (!strcmp(argv[1], "fail")) return 7;
    if (!strcmp(argv[1], "worker-segv")) {
        pthread_t worker;
        if (pthread_create(&worker, 0, diagnostic_worker, argv[2])) return 9;
        diagnostic_leader_wait(worker);
    }
    if (!strcmp(argv[1], "child-segv") || !strcmp(argv[1], "child-abort")) {
        pid_t child = fork();
        if (child < 0) return 9;
        if (!child) {
            char *args[] = {argv[0], !strcmp(argv[1], "child-segv") ? "segv" : "abort", argv[2], 0};
            execv(argv[0], args);
            _exit(9);
        }
        int status = 0;
        if (waitpid(child, &status, 0) != child) return 9;
        if (!WIFSIGNALED(status)) return 9;
        return !strcmp(argv[1], "child-segv") ? 7 : 0;
    }
    return 0;
}
