#ifndef _VBRCC_LIMITS_H
#define _VBRCC_LIMITS_H

#define CHAR_BIT 8
#define SCHAR_MIN (-128)
#define SCHAR_MAX 127
#define UCHAR_MAX 255
#define CHAR_MIN SCHAR_MIN
#define CHAR_MAX SCHAR_MAX
#define SHRT_MIN (-32768)
#define SHRT_MAX 32767

/* Written as (-MAX - 1) because 2147483648 does not fit in an int. */
#define INT_MIN (-2147483647 - 1)
#define INT_MAX 2147483647
#define LONG_MIN (-2147483647 - 1)
#define LONG_MAX 2147483647

#endif
