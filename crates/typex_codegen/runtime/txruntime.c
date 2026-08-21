#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void tx_print_int(const char *fmt, long long n)
{
    printf(fmt, n);
}

void tx_puts(const char *s)
{
    fputs(s, stdout);
}

// allocate a copy of a string
char *tx_str_copy(const char *s)
{
    if (!s)
        return NULL;
    size_t len = strlen(s);
    char *copy = (char *)malloc(len + 1);
    if (copy)
        memcpy(copy, s, len + 1);
    return copy;
}

// concatenate two strings, returning a new heap-allocated string
char *tx_str_concat(const char *a, const char *b)
{
    if (!a)
        a = "";
    if (!b)
        b = "";
    size_t la = strlen(a);
    size_t lb = strlen(b);
    char *result = (char *)malloc(la + lb + 1);
    if (result)
    {
        memcpy(result, a, la);
        memcpy(result + la, b, lb + 1);
    }
    return result;
}

// string length in bytes
long long tx_str_len(const char *s)
{
    if (!s)
        return 0;
    return (long long)strlen(s);
}

// print string variable (not a literal)
void tx_print_str(const char *fmt, const char *s)
{
    printf(fmt, s);
}