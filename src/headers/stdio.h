#ifndef _VBRCC_STDIO_H
#define _VBRCC_STDIO_H

#include <stddef.h>

/* These resolve against msvcrt at link time. The call limit is four
   arguments, so printf accepts three values and fprintf two. */

/* msvcrt's x64 FILE layout, 48 bytes. The fields are private. The size
   matters to msvcrt's __iob_func, which returns an array of FILE. */
typedef struct {
    char *_ptr;
    int _cnt;
    char *_base;
    int _flag;
    int _file;
    int _charbuf;
    int _bufsiz;
    char *_tmpfname;
} FILE;

#ifdef __VBRCC_MINGW_LINK__
/* --gcc: MinGW's link libraries provide __acrt_iob_func for every C runtime.
   UCRT has no __iob_func. */
FILE *__acrt_iob_func(int index);
#define stdin  (__acrt_iob_func(0))
#define stdout (__acrt_iob_func(1))
#define stderr (__acrt_iob_func(2))
#else
FILE *__iob_func(void);
#define stdin  (&__iob_func()[0])
#define stdout (&__iob_func()[1])
#define stderr (&__iob_func()[2])
#endif

#define EOF (-1)
#define BUFSIZ 512
#define FILENAME_MAX 260
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

int printf(const char *fmt, ...);
int fprintf(FILE *f, const char *fmt, ...);
int sprintf(char *buf, const char *fmt, ...);
int scanf(const char *fmt, ...);
int fscanf(FILE *f, const char *fmt, ...);
int sscanf(const char *s, const char *fmt, ...);

int puts(const char *s);
int putchar(int c);
int getchar(void);

FILE *fopen(const char *path, const char *mode);
int fclose(FILE *f);
int fflush(FILE *f);
int fputc(int c, FILE *f);
int fputs(const char *s, FILE *f);
int fgetc(FILE *f);
char *fgets(char *buf, int n, FILE *f);
int ungetc(int c, FILE *f);
size_t fread(void *buf, size_t size, size_t count, FILE *f);
size_t fwrite(const void *buf, size_t size, size_t count, FILE *f);
int fseek(FILE *f, long offset, int origin);
long ftell(FILE *f);
void rewind(FILE *f);
int feof(FILE *f);
int ferror(FILE *f);
void perror(const char *s);
int remove(const char *path);
int rename(const char *from, const char *to);

#define putc(c, f) fputc(c, f)
#define getc(f) fgetc(f)

#endif
