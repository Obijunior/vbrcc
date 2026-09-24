#ifndef _VBRCC_STDLIB_H
#define _VBRCC_STDLIB_H

#include <stddef.h>

void *malloc(size_t size);
void free(void *p);
int abs(int n);
void exit(int code);

#endif
