#ifndef _VBRCC_STDIO_H
#define _VBRCC_STDIO_H

/* These resolve against msvcrt at link time. The call limit is four
   arguments, so printf accepts three values. */
int printf(const char *fmt, ...);
int puts(const char *s);
int putchar(int c);
int getchar(void);

#endif
