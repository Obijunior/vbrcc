#ifndef _VBRCC_STDLIB_H
#define _VBRCC_STDLIB_H

#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX 32767

typedef struct { int quot; int rem; } div_t;
typedef struct { long quot; long rem; } ldiv_t;

void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *p, size_t size);
void free(void *p);

int atoi(const char *s);
long atol(const char *s);
long strtol(const char *s, char **end, int base);

int abs(int n);
long labs(long n);
div_t div(int num, int den);
ldiv_t ldiv(long num, long den);

/* The C type of the seed is unsigned int. `unsigned` does not exist yet, and
   the Win64 ABI passes both in the same register. */
int rand(void);
void srand(int seed);

char *getenv(const char *name);
int system(const char *command);
void abort(void);
void exit(int code);

#endif
