#ifndef _VBRCC_UNISTD_H
#define _VBRCC_UNISTD_H

/* POSIX, not C99. Windows has no unistd.h, but msvcrt exports most of these
   calls with a leading underscore, so each plain name maps onto that, as in
   MinGW. Not available on Windows: fork, usleep, getopt.

   The plain names are macros, so `#define read _read` also renames a struct
   member or a variable called `read`. */

#define STDIN_FILENO  0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

#define F_OK 0
#define W_OK 2
#define R_OK 4

/* The C types of the counts and of `_sleep`'s argument are unsigned. `unsigned`
   does not exist yet, and the Win64 ABI passes both in the same register. */
int _read(int fd, void *buf, int count);
int _write(int fd, const void *buf, int count);
int _close(int fd);
long _lseek(int fd, long offset, int origin);
int _access(const char *path, int mode);
int _unlink(const char *path);
int _rmdir(const char *path);
int _chdir(const char *path);
char *_getcwd(char *buf, int size);
int _dup(int fd);
int _dup2(int fd, int fd2);
int _isatty(int fd);
int _getpid(void);
void _sleep(int milliseconds);

#define read   _read
#define write  _write
#define close  _close
#define lseek  _lseek
#define access _access
#define unlink _unlink
#define rmdir  _rmdir
#define chdir  _chdir
#define getcwd _getcwd
#define dup    _dup
#define dup2   _dup2
#define isatty _isatty
#define getpid _getpid
/* POSIX sleep takes seconds; msvcrt's _sleep takes milliseconds. */
#define sleep(s) _sleep((s) * 1000)

#endif
