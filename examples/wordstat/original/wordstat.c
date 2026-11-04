/* wordstat — count words in a text file. Tiny original used to demonstrate a PortCode port.
 * usage: wordstat [--top N] [--min-len L] FILE
 * words = maximal runs of ASCII letters/digits, lower-cased. */
#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { char *w; int n; } Entry;

static int cmp(const void *a, const void *b) {
    const Entry *x = a, *y = b;
    if (x->n != y->n) return y->n - x->n;
    return strcmp(x->w, y->w);
}

int main(int argc, char **argv) {
    int top = 10, minlen = 1;
    const char *path = NULL;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--top") && i + 1 < argc) top = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--min-len") && i + 1 < argc) minlen = atoi(argv[++i]);
        else path = argv[i];
    }
    if (!path) { fprintf(stderr, "usage: wordstat [--top N] [--min-len L] FILE\n"); return 2; }
    FILE *f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "wordstat: cannot open %s\n", path); return 1; }

    Entry *es = NULL; int ne = 0, cap = 0;
    long total = 0, chars = 0;
    char buf[256]; int len = 0;
    int c;
    do {
        c = fgetc(f);
        if (c != EOF && isalnum((unsigned char)c)) {
            if (len < 255) buf[len++] = (char)tolower(c);
            continue;
        }
        if (len > 0 && len >= minlen) {
            buf[len] = 0;
            int k = 0;
            for (; k < ne; k++) if (!strcmp(es[k].w, buf)) break;
            if (k == ne) {
                if (ne == cap) { cap = cap ? cap * 2 : 64; es = realloc(es, cap * sizeof(Entry)); }
                es[ne].w = strdup(buf); es[ne].n = 0; ne++;
            }
            es[k].n++; total++; chars += len;
        }
        len = 0;
    } while (c != EOF);
    fclose(f);

    qsort(es, ne, sizeof(Entry), cmp);
    printf("file: %s\n", path);
    printf("words: %ld\n", total);
    printf("unique: %d\n", ne);
    printf("avg word length: %.3f\n", total ? (double)chars / total : 0.0);
    int shown = top < ne ? top : ne;
    printf("top %d:\n", shown);
    for (int i = 0; i < shown; i++) printf("  %-12s %5d\n", es[i].w, es[i].n);
    return 0;
}
