#include <stdio.h>
extern int func_1(void);
extern int func_2(void);
extern int func_3(void);
extern int func_4(void);
extern int func_5(void);
extern int func_6(void);
extern int func_7(void);
extern int func_8(void);
extern int func_9(void);
extern int func_10(void);

int main(void) {
    int sum = 0;
    sum += func_1();
    sum += func_2();
    sum += func_3();
    sum += func_4();
    sum += func_5();
    sum += func_6();
    sum += func_7();
    sum += func_8();
    sum += func_9();
    sum += func_10();
    printf("Sum: %d\n", sum);
    return 0;
}
