#ifndef _VBRCC_TIME_H
#define _VBRCC_TIME_H

#include <stddef.h>

/* On x64, msvcrt's time_t is 64 bits. */
typedef long long time_t;
typedef long clock_t;
#define CLOCKS_PER_SEC 1000

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
};

time_t time(time_t *t);
clock_t clock(void);
time_t mktime(struct tm *t);
struct tm *localtime(const time_t *t);
struct tm *gmtime(const time_t *t);
char *asctime(const struct tm *t);
char *ctime(const time_t *t);
size_t strftime(char *buf, size_t max, const char *fmt, const struct tm *t);

#endif
