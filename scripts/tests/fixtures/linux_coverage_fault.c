#include <signal.h>
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

int main(int argc, char **argv) {
    if (argc < 2) return 9;
    if (!strcmp(argv[1], "segv")) diagnostic_fault(argv[2]);
    if (!strcmp(argv[1], "abort")) diagnostic_abort();
    if (!strcmp(argv[1], "fail")) return 7;
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
