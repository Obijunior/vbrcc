// expect: 0
// expect-stdout: to stdout 7\nline two\nparsed 12 and ab
#include <stdio.h>
#include <stdlib.h>
int main() {
    fprintf(stdout, "to stdout %d\n", 7);
    fputs("line two\n", stdout);
    char path[300];
    sprintf(path, "%s/vbrcc_stdio_test.txt", getenv("TEMP"));
    FILE *f = fopen(path, "w");
    if (f == 0) return 1;
    fputs("12 ab\n", f);
    fputc('Z', f);
    fclose(f);
    f = fopen(path, "r");
    if (f == 0) return 2;
    char line[64];
    if (fgets(line, 64, f) == 0) return 3;
    int n;
    char word[8];
    if (sscanf(line, "%d %s", &n, word) != 2) return 4;
    if (fgetc(f) != 'Z') return 5;
    if (fgetc(f) != EOF) return 6;
    rewind(f);
    if (fgetc(f) != '1') return 7;
    fclose(f);
    if (remove(path) != 0) return 8;
    printf("parsed %d and %s", n, word);
    return 0;
}
