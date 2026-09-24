// expect: 0
// expect-stdout: 2026-09-24 13:05:09 Thu
#include <time.h>
#include <stdio.h>
int main() {
    time_t now = time(0);
    if (now < 1700000000) return 1;
    time_t again;
    time(&again);
    if (again < now) return 2;
    if (clock() < 0) return 3;
    struct tm t;
    t.tm_year = 126; t.tm_mon = 8; t.tm_mday = 24;
    t.tm_hour = 13; t.tm_min = 5; t.tm_sec = 9;
    t.tm_wday = 4; t.tm_yday = 266; t.tm_isdst = 0;
    char buf[64];
    size_t n = strftime(buf, 64, "%Y-%m-%d %H:%M:%S %a", &t);
    if (n != 23) return 4;
    struct tm *g = gmtime(&now);
    if (g == 0 || g->tm_year < 123) return 5;
    printf("%s", buf);
    return 0;
}
