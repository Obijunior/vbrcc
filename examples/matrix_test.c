// this won't compile yet, I still need to add support for n-D arrays and using {}
#include <stdio.h>
int main() {
    int matrix[3][3] = {{1,2,3}, {4,5,6}, {7,8,9}};
    int *ptr = &matrix[0][0];
    for(int i = 0; i < 9; i++) {
        printf("%d ", *(ptr + i));
    }
    return 0;
}