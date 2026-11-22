/* Independent layout probe; compile against the installed JDK's bridge headers. */
#include <stddef.h>
#include <stdio.h>
#include <AccessBridgePackages.h>

int main(void) {
    printf("ContextInfo=%zu\n", sizeof(AccessibleContextInfo));
    printf("ContextInfo.align=%zu\n", __alignof(AccessibleContextInfo));
    printf("ContextInfo.role_en_us=%zu\n", offsetof(AccessibleContextInfo, role_en_US));
    printf("ContextInfo.children_count=%zu\n", offsetof(AccessibleContextInfo, childrenCount));
    printf("TextInfo=%zu\n", sizeof(AccessibleTextInfo));
    printf("Actions=%zu\n", sizeof(AccessibleActions));
    printf("ActionsToDo=%zu\n", sizeof(AccessibleActionsToDo));
    return 0;
}
