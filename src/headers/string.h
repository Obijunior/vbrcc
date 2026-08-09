#ifndef _VBRCC_STRING_H
#define _VBRCC_STRING_H

/* strlen returns long, not size_t, until typedef arrives */
long strlen(const char *s);
char *strcpy(char *dst, const char *src);
int strcmp(const char *a, const char *b);
char *strcat(char *dst, const char *src);

#endif
