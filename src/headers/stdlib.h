#ifndef _VBRCC_STDLIB_H
#define _VBRCC_STDLIB_H

/* malloc takes long, not size_t, until typedef arrives. */
void *malloc(long size);
void free(void *p);
int abs(int n);
void exit(int code);

#endif
