/* No include guard: C99 7.2 lets a program include this again after it
   changes NDEBUG. */
#undef assert

#ifdef NDEBUG
#define assert(e) ((void)0)
#else
/* msvcrt prints "Assertion failed: MSG, file F, line L" and aborts. The C
   type of the line is unsigned int. The message is fixed until the
   preprocessor supports `#` stringizing. */
void _assert(const char *msg, const char *file, int line);
#define assert(e) ((e) ? (void)0 : _assert("assertion failed", __FILE__, __LINE__))
#endif
