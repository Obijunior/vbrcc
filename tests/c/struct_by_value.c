// expect: 30
struct V { int a; int b; int c; };
struct V scale(struct V v, int k) { v.a = v.a * k; v.b = v.b * k; v.c = v.c * k; return v; }
int main() {
    struct V v; v.a = 1; v.b = 2; v.c = 2;
    struct V w = scale(v, 6);
    return w.a + w.b + w.c + v.a - 1;
}
