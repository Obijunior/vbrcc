#ifndef _VBRCC_STDINT_H
#define _VBRCC_STDINT_H

/* Macros until typedef arrives (roadmap item 11).
   Signed only, because `unsigned` is roadmap item 6. */
#define int8_t char
#define int16_t int
#define int32_t int
#define int64_t long
#define intptr_t long

#define INT8_MAX 127
#define INT16_MAX 32767
#define INT32_MAX 2147483647

#endif
