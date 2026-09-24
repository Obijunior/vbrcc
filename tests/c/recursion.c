// expect: 89
int fib(int n) { if (n < 2) return 1; return fib(n - 1) + fib(n - 2); }
int fact(int n) { if (n <= 1) return 1; return n * fact(n - 1); }
int main() { return fib(10) + fact(5) / 120 - 1; }
