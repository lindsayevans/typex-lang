#include <stdio.h>

void tx_print_int(const char *fmt, long long n)
{
    printf(fmt, n);
}

void tx_puts(const char *s)
{
    fputs(s, stdout);
}
