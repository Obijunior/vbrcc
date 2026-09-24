// expect: 11
#include <string.h>
int main() {
    char buf[16];
    strcpy(buf, "hello ");
    strcat(buf, "world");
    if (strcmp(buf, "hello world") != 0) return 99;
    return strlen(buf);
}
