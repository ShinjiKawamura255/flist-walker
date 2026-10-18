#include <string.h>

__attribute__((noinline)) void diagnostic_crash(const char *private_value) {
    volatile int *address = 0;
    *address = 1;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "fault") == 0) diagnostic_crash(argv[2]);
    return argc > 1 && strcmp(argv[1], "fail") == 0 ? 7 : 0;
}
