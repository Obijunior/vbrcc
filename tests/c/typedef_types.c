// expect: 42
typedef long long i64;
typedef struct { int x, y; } Point;
typedef int *IntPtr;
int main() {
    Point p; p.x = 40; p.y = 2;
    int n = p.x;
    IntPtr q = &n;
    i64 big = 1;
    return *q + p.y + (int)(big - 1);
}
