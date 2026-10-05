// expect: 0
// expect-stdout: 1 2 3 4 5\n5 4 3 2 1\nsum=15 len=5 third=3
#include <stdio.h>
#include <stdlib.h>
struct Node {
    int data;
    struct Node *next;
};
typedef struct Tree {
    int key;
    struct Tree *left, *right;
} Tree;
struct Node *push_back(struct Node *head, int v) {
    struct Node *n = malloc(sizeof(struct Node));
    n->data = v;
    n->next = NULL;
    if (head == NULL) return n;
    struct Node *cur = head;
    while (cur->next != NULL) cur = cur->next;
    cur->next = n;
    return head;
}
struct Node *reverse(struct Node *head) {
    struct Node *prev = NULL;
    while (head != NULL) {
        struct Node *next = head->next;
        head->next = prev;
        prev = head;
        head = next;
    }
    return prev;
}
void print(struct Node *head) {
    for (struct Node *p = head; p != NULL; p = p->next) {
        printf("%d", p->data);
        if (p->next) printf(" ");
    }
    printf("\n");
}
int main() {
    struct Node *list = NULL;
    for (int i = 1; i <= 5; i++) list = push_back(list, i);
    print(list);
    int third = list->next->next->data;
    list = reverse(list);
    print(list);
    int sum = 0, len = 0;
    struct Node *p = list;
    while (p) { sum += p->data; len++; p = p->next; }
    printf("sum=%d len=%d third=%d", sum, len, third);
    Tree t; Tree l;
    t.key = 1; l.key = 2;
    t.left = &l; t.right = NULL; l.left = NULL;
    if (t.left->key != 2 || t.left->left != NULL) return 1;
    while (list) { struct Node *n = list->next; free(list); list = n; }
    return 0;
}
