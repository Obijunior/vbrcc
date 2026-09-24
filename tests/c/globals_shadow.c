// expect: 33
int x = 10;
int get() { return x; }
int main() {
    int x = 20;
    x = x + 3;
    return x + get();
}
