// expect: 0
// expect-stdout: written by write\nok
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main() {
    if (write(STDOUT_FILENO, "written by write\n", 17) != 17) return 1;
    char cwd[260];
    if (getcwd(cwd, 260) == 0 || strlen(cwd) == 0) return 2;
    if (getpid() <= 0) return 3;
    if (access(".", F_OK) != 0) return 4;
    if (access("vbrcc_no_such_file", F_OK) == 0) return 5;
    char path[300];
    sprintf(path, "%s/vbrcc_unistd_test.txt", getenv("TEMP"));
    FILE *f = fopen(path, "w");
    if (!f) return 6;
    fputs("abc", f);
    fclose(f);
    if (access(path, R_OK) != 0) return 7;
    if (unlink(path) != 0) return 8;
    if (access(path, F_OK) == 0) return 9;
    int fd = dup(STDOUT_FILENO);
    if (fd < 0) return 10;
    if (close(fd) != 0) return 11;
    sleep(0);
    printf("ok");
    return 0;
}
