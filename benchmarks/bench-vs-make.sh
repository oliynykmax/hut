#!/usr/bin/env bash
set -euo pipefail

NPROC=$(nproc)
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     hut vs make -j$NPROC  — parallel build comparison          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

HUT="target/release/hut"
# Build hut release if not present
[ -f "$HUT" ] || { cargo build --release 2>&1 | tail -1; }

BASE="/tmp/hut-vs-make"
rm -rf "$BASE"
mkdir -p "$BASE"

bench() {
    local n=$1
    local dir="$BASE/n$n"
    mkdir -p "$dir/src"

    # Generate N .c files that all call a shared function
    echo '#include <stdio.h>' > "$dir/src/shared.h"
    echo 'void shared(void) { }' > "$dir/src/shared.c"

    for i in $(seq 1 $n); do
        cat > "$dir/src/file$i.c" <<EOF
#include "shared.h"
int func$i(void) { shared(); return $i; }
EOF
    done

    # Main file
    cat > "$dir/src/main.c" <<'EOF'
#include <stdio.h>
extern int func1(void);
int main(void) { printf("sum=%d\n", func1()); return 0; }
EOF

    # --- hut ---
    cd "$dir"
    "$OLDPWD/$HUT" init --quiet bench$n 2>/dev/null || true
    # Overwrite with our files
    cp src/*.c src/*.h . 2>/dev/null || true

    local hut_cold hut_hot make_cold make_hot makefile_time

    # hut cold
    hut_cold=$( { time "$OLDPWD/$HUT" build 2>&1; } 2>&1 | grep real | awk '{print $2}')

    # hut hot (touch one file)
    touch src/file1.c
    hut_hot=$( { time "$OLDPWD/$HUT" build 2>&1; } 2>&1 | grep real | awk '{print $2}')

    "$OLDPWD/$HUT" clean 2>/dev/null || true
    rm -rf target hut.toml hut.lock
    cd "$OLDPWD"

    # --- Makefile ---
    local srcs=""
    for i in $(seq 1 $n); do srcs="$srcs src/file$i.c"; done

    cat > "$dir/Makefile" <<MAKEFILE
CC = gcc
CFLAGS = -Wall -O0
OBJS = shared.o $(for i in $(seq 1 $n); do echo -n "file$i.o "; done) main.o
TARGET = bench

\$(TARGET): \$(OBJS)
	\$(CC) \$(CFLAGS) -o \$@ \$^

%.o: src/%.c
	\$(CC) \$(CFLAGS) -c -o \$@ \$<

clean:
	rm -f \$(OBJS) \$(TARGET)
MAKEFILE

    makefile_time=$( { time make -C "$dir" -j$NPROC 2>&1; } 2>&1 | grep real | awk '{print $2}')
    make_cold="$makefile_time"

    # make hot
    touch "$dir/src/file1.c"
    make_hot=$( { time make -C "$dir" -j$NPROC 2>&1; } 2>&1 | grep real | awk '{print $2}')

    make -C "$dir" clean >/dev/null 2>&1

    # Format for output
    printf "  %-6s │ %10s │ %10s │ %10s │ %10s │ %10s │ %10s\n" \
        "$n" "$hut_cold" "$make_cold" "$hut_hot" "$make_hot" \
        "$(python3 -c "print(f'{float('${hut_cold%?}')/float('${make_cold%?}'):.2f}x')" 2>/dev/null || echo '-')" \
        "$(python3 -c "print(f'{float('${hut_hot%?}')/float('${make_hot%?}'):.2f}x')" 2>/dev/null || echo '-')"
}

echo "         │ ── Cold build ── │ ── Hot build ── │"
echo "  Files  │     hut     make │     hut     make │ hut/make"
echo "  ───────┼──────────────────┼──────────────────┼──────────"
bench 10
bench 50
bench 100

echo ""
echo "Done. Artifacts in $BASE"
