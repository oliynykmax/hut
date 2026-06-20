#!/usr/bin/env bash
#
# hut benchmark suite: hut vs raw gcc
#
# Measures:
#   1. Compilation speed: cold build (from scratch) vs hot build (cached .o files)
#   2. Scales: N = 10, 50, 100 source files
#   3. Runtime performance of fib(45) compiled by hut vs gcc
#
# Prerequisites: gcc, hut (release binary at ../target/release/hut)
# Usage: ./bench.sh [--quick]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BENCH_DIR="$SCRIPT_DIR/tmp_bench"
HUT_BIN="${HUT_BIN:-$SCRIPT_DIR/../target/release/hut}"
GCC="${GCC:-gcc}"
QUICK=false

if [[ "${1:-}" == "--quick" ]]; then
    QUICK=true
    echo ">>> Quick mode: N=10 only"
fi

# ── Colors ──────────────────────────────────────────────────────────────────
BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

# ── Helpers ─────────────────────────────────────────────────────────────────

fmt_sec() {
    local t="$1"
    if [[ "$t" == "N/A" ]]; then
        printf "${RED}%7s${NC}" "$t"
    elif (( $(echo "$t < 1.0" | bc -l 2>/dev/null || echo 0) )); then
        printf "${GREEN}%.3fs${NC}" "$t"
    elif (( $(echo "$t < 5.0" | bc -l 2>/dev/null || echo 0) )); then
        printf "${YELLOW}%.3fs${NC}" "$t"
    else
        printf "${RED}%.3fs${NC}" "$t"
    fi
}

time_cmd() {
    # Run a command in a given directory, return elapsed wall-clock time
    local workdir="$1"
    shift
    local start end
    start=$(date +%s.%N)
    (cd "$workdir" && "$@") > /dev/null 2>&1
    local rc=$?
    end=$(date +%s.%N)
    if [[ $rc -ne 0 ]]; then
        echo "ERROR"
        return 1
    fi
    echo "$end - $start" | bc -l
}

# ── Generate C project with N source files ──────────────────────────────────

generate_project() {
    local N="$1"
    local dir="$2"

    rm -rf "$dir"
    mkdir -p "$dir/src"

    # Generate main.c that calls all modules
    {
        echo '#include <stdio.h>'
        for i in $(seq 1 "$N"); do
            echo "extern int func_${i}(void);"
        done
        echo ''
        echo 'int main(void) {'
        echo '    int sum = 0;'
        for i in $(seq 1 "$N"); do
            echo "    sum += func_${i}();"
        done
        echo '    printf("Sum: %d\n", sum);'
        echo '    return 0;'
        echo '}'
    } > "$dir/src/main.c"

    # Generate N tiny module files
    for i in $(seq 1 "$N"); do
        echo "int func_${i}(void) { return ${i}; }" > "$dir/src/mod_${i}.c"
    done

    # Create hut.toml
    cat > "$dir/hut.toml" << 'ENDOFHUTTOML'
[package]
name = "bench-PLACEHOLDER"
version = "0.1.0"
edition = "2024"

[build]
kind = "executable"
sources = ["src/*.c"]
ENDOFHUTTOML
    # Replace placeholder with actual N
    sed -i "s/bench-PLACEHOLDER/bench-${N}/" "$dir/hut.toml"

    echo "Generated project with $N source files in $dir"
}

# ── Benchmark: hut build ────────────────────────────────────────────────────

bench_hut() {
    local N="$1"
    local dir="$2"
    local label="$3"  # "cold" or "hot"

    # Clear hut build artifacts for cold builds
    if [[ "$label" == "cold" ]]; then
        rm -rf "$dir/target" "$dir/.hut"
    fi

    time_cmd "$dir" "$HUT_BIN" build
}

# ── Benchmark: raw gcc ──────────────────────────────────────────────────────

bench_gcc() {
    local N="$1"
    local dir="$2"
    local label="$3"  # "cold" or "hot"

    local obj_dir="$dir/obj"
    local bin="$dir/bench_gcc"

    if [[ "$label" == "cold" ]]; then
        rm -rf "$obj_dir" "$bin"
    fi

    mkdir -p "$obj_dir"

    # Only compile .c files whose .o is missing or older
    time_cmd "$dir" bash -c "
        for i in \$(seq 1 $N); do
            src='src/mod_'\$i'.c'
            obj='obj/mod_'\$i'.o'
            if [[ ! -f \$obj ]] || [[ \$src -nt \$obj ]]; then
                $GCC -c -O2 \$src -o \$obj
            fi
        done
        src='src/main.c'
        obj='obj/main.o'
        if [[ ! -f \$obj ]] || [[ \$src -nt \$obj ]]; then
            $GCC -c -O2 \$src -o \$obj
        fi
        $GCC obj/*.o -o '$bin'
    "
}

# ── Benchmark: runtime ──────────────────────────────────────────────────────

bench_runtime() {
    local label="$1"
    local workdir="$2"

    local fib_src="$SCRIPT_DIR/fib.c"

    if [[ "$label" == "hut" ]]; then
        rm -rf "$workdir"
        mkdir -p "$workdir/src"
        cp "$fib_src" "$workdir/src/fib.c"

        cat > "$workdir/hut.toml" << 'ENDOFHUTTOML2'
[package]
name = "fib-bench"
version = "0.1.0"
edition = "2024"

[build]
kind = "executable"
sources = ["src/fib.c"]
opt-level = 2
ENDOFHUTTOML2

        # Build with hut
        (cd "$workdir" && "$HUT_BIN" build --release) > /dev/null 2>&1 || {
            echo "ERROR: hut build failed"
            return 1
        }

        # Find the binary
        local bin
        bin=$(find "$workdir/target" -type f -executable -name "fib*" 2>/dev/null | head -1)
        if [[ -z "$bin" ]]; then
            echo "ERROR: hut binary not found in $workdir/target"
            return 1
        fi

        local start end
        start=$(date +%s.%N)
        "$bin" > /dev/null 2>&1
        end=$(date +%s.%N)
        echo "$end - $start" | bc -l

    elif [[ "$label" == "gcc" ]]; then
        rm -rf "$workdir"
        mkdir -p "$workdir"
        cp "$fib_src" "$workdir/fib.c"

        $GCC -O2 -o "$workdir/fib" "$workdir/fib.c" 2>&1 || {
            echo "ERROR: gcc compilation failed"
            return 1
        }

        local start end
        start=$(date +%s.%N)
        "$workdir/fib" > /dev/null 2>&1
        end=$(date +%s.%N)
        echo "$end - $start" | bc -l
    fi
}

# ── Main ────────────────────────────────────────────────────────────────────

main() {
    echo ""
    echo -e "${BOLD}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║              🛖  hut Benchmark Suite                         ║${NC}"
    echo -e "${BOLD}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    # Check prerequisites
    if [[ ! -x "$HUT_BIN" ]]; then
        echo -e "${RED}Error: hut release binary not found at $HUT_BIN${NC}"
        echo "Build it first: cd .. && cargo build --release"
        exit 1
    fi

    local hut_version
    hut_version=$("$HUT_BIN" --version 2>&1 || true)
    echo -e "hut:  ${GREEN}$hut_version${NC}"
    echo -e "gcc:  ${GREEN}$($GCC --version | head -1)${NC}"
    echo ""

    # Determine N values
    local N_VALUES
    if $QUICK; then
        N_VALUES=(10)
    else
        N_VALUES=(10 50 100)
    fi

    # ── Compilation benchmarks ──────────────────────────────────────────────

    echo -e "${BOLD}── Compilation Speed (cold build — from scratch) ──${NC}"
    echo ""
    printf "  %-6s │ %-14s │ %-14s │ %-10s\n" "Files" "hut" "gcc" "Speedup"
    printf "  %-6s─┼─%14s─┼─%14s─┼─%10s\n" "──────" "──────────────" "──────────────" "──────────"

    declare -A hut_cold hut_hot gcc_cold gcc_hot

    for N in "${N_VALUES[@]}"; do
        local d="$BENCH_DIR/n${N}"
        generate_project "$N" "$d"

        # Cold builds
        local ht
        ht=$(bench_hut "$N" "$d" "cold") || ht="N/A"
        local gc
        gc=$(bench_gcc "$N" "$d" "cold") || gc="N/A"

        hut_cold[$N]="$ht"
        gcc_cold[$N]="$gc"

        local speedup="N/A"
        if [[ "$ht" != "N/A" && "$gc" != "N/A" ]]; then
            speedup=$(echo "scale=2; $gc / $ht" | bc -l 2>/dev/null || echo "N/A")
            printf "  %-6s │ %-14s │ %-14s │ ${GREEN}%7sx${NC}\n" \
                "$N" "$(fmt_sec "$ht")" "$(fmt_sec "$gc")" "$speedup"
        else
            printf "  %-6s │ %-14s │ %-14s │ %10s\n" \
                "$N" "$(fmt_sec "$ht")" "$(fmt_sec "$gc")" "N/A"
        fi
    done

    echo ""
    echo -e "${BOLD}── Compilation Speed (hot build — cached .o / incremental) ──${NC}"
    echo ""
    printf "  %-6s │ %-14s │ %-14s │ %-10s\n" "Files" "hut" "gcc" "Speedup"
    printf "  %-6s─┼─%14s─┼─%14s─┼─%10s\n" "──────" "──────────────" "──────────────" "──────────"

    for N in "${N_VALUES[@]}"; do
        local d="$BENCH_DIR/n${N}"

        # Touch one source file to trigger an incremental rebuild
        touch "$d/src/mod_1.c"
        # Also update the function to force actual recompilation
        echo "int func_1(void) { return 999; }" > "$d/src/mod_1.c"

        local ht
        ht=$(bench_hut "$N" "$d" "hot") || ht="N/A"
        local gc
        gc=$(bench_gcc "$N" "$d" "hot") || gc="N/A"

        hut_hot[$N]="$ht"
        gcc_hot[$N]="$gc"

        local speedup="N/A"
        if [[ "$ht" != "N/A" && "$gc" != "N/A" ]]; then
            speedup=$(echo "scale=2; $gc / $ht" | bc -l 2>/dev/null || echo "N/A")
            printf "  %-6s │ %-14s │ %-14s │ ${GREEN}%7sx${NC}\n" \
                "$N" "$(fmt_sec "$ht")" "$(fmt_sec "$gc")" "$speedup"
        else
            printf "  %-6s │ %-14s │ %-14s │ %10s\n" \
                "$N" "$(fmt_sec "$ht")" "$(fmt_sec "$gc")" "N/A"
        fi
    done

    # ── Summary table ──────────────────────────────────────────────────────

    echo ""
    echo -e "${BOLD}── Summary ──${NC}"
    echo ""
    printf "  %-6s │ %10s │ %10s │ %10s │ %10s\n" \
        "Files" "hut cold" "gcc cold" "hut hot" "gcc hot"
    printf "  %-6s─┼─%10s─┼─%10s─┼─%10s─┼─%10s\n" \
        "──────" "──────────" "──────────" "──────────" "──────────"

    for N in "${N_VALUES[@]}"; do
        local hc hh gc gh
        hc="${hut_cold[$N]}"
        gc="${gcc_cold[$N]}"
        hh="${hut_hot[$N]}"
        gh="${gcc_hot[$N]}"
        if [[ "$hc" != "N/A" ]]; then hc="$(printf "%.3fs" "$hc")"; fi
        if [[ "$gc" != "N/A" ]]; then gc="$(printf "%.3fs" "$gc")"; fi
        if [[ "$hh" != "N/A" ]]; then hh="$(printf "%.3fs" "$hh")"; fi
        if [[ "$gh" != "N/A" ]]; then gh="$(printf "%.3fs" "$gh")"; fi
        printf "  %-6s │ %10s │ %10s │ %10s │ %10s\n" \
            "$N" "$hc" "$gc" "$hh" "$gh"
    done

    # ── Runtime benchmark (fib) ─────────────────────────────────────────────

    echo ""
    echo -e "${BOLD}── Runtime: fib(45) ──${NC}"
    echo ""

    local rt_hut rt_gcc
    echo "  Compiling with hut --release..."
    rt_hut=$(bench_runtime "hut" "$BENCH_DIR/fibproj_hut") || rt_hut="N/A"
    echo "  Compiling with gcc -O2..."
    rt_gcc=$(bench_runtime "gcc" "$BENCH_DIR/fibproj_gcc") || rt_gcc="N/A"

    echo ""
    printf "  %-16s │ %-14s\n" "hut (gcc -O2)" "gcc -O2"
    printf "  %-16s─┼─%14s\n" "────────────────" "──────────────"
    printf "  %-16s │ %-14s\n" \
        "$(fmt_sec "$rt_hut")" \
        "$(fmt_sec "$rt_gcc")"

    # ── Done ────────────────────────────────────────────────────────────────

    echo ""
    echo -e "${BOLD}── Done ──${NC}"
    echo ""
    echo "  Benchmark artifacts in: $BENCH_DIR"
    echo "  To clean: rm -rf $BENCH_DIR"
    echo ""
}

main "$@"
