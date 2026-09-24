#ifndef _VBRCC_ERRNO_H
#define _VBRCC_ERRNO_H

/* msvcrt keeps errno per thread and returns its address. */
int *_errno(void);
#define errno (*_errno())

/* The msvcrt values. */
#define EPERM 1
#define ENOENT 2
#define EINTR 4
#define EIO 5
#define EBADF 9
#define ENOMEM 12
#define EACCES 13
#define EEXIST 17
#define EINVAL 22
#define EDOM 33
#define ERANGE 34

#endif
