#!/usr/bin/env bash
set -euo pipefail

BOLD="\033[1m"
GREEN="\033[32m"
BLUE="\033[34m"
RED="\033[31m"
RESET="\033[0m"

echo -e "${BOLD}🏠 hut installer${RESET}\n"

# ── Rust / Cargo ──────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo -e "${RED}✗ cargo not found${RESET}"
    echo "  Install Rust: https://rustup.rs"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi
echo -e "${GREEN}✓${RESET} cargo $(cargo --version | cut -d' ' -f2)"

# ── C compiler ──────────────────────────────────────────
CC=""
for c in gcc clang cc; do
    if command -v "$c" &>/dev/null; then
        CC="$c"
        break
    fi
done
if [ -z "$CC" ]; then
    echo -e "${RED}✗ No C compiler found (gcc, clang, or cc)${RESET}"
    exit 1
fi
echo -e "${GREEN}✓${RESET} C compiler: $CC"

# ── Clone & build ────────────────────────────────────────
HUT_DIR="${HUT_DIR:-$HOME/.hut}"
REPO="https://github.com/oliynykmax/hut.git"

if [ -d "$HUT_DIR" ]; then
    echo -e "${BLUE}→${RESET} Updating hut in $HUT_DIR ..."
    git -C "$HUT_DIR" pull --ff-only
else
    echo -e "${BLUE}→${RESET} Cloning hut into $HUT_DIR ..."
    git clone "$REPO" "$HUT_DIR"
fi

echo -e "${BLUE}→${RESET} Building hut (release) ..."
cargo build --release --manifest-path "$HUT_DIR/Cargo.toml"

# ── Install binary ───────────────────────────────────────
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$INSTALL_DIR"
cp "$HUT_DIR/target/release/hut" "$INSTALL_DIR/hut"
chmod +x "$INSTALL_DIR/hut"

echo -e "\n${GREEN}✓${RESET} hut installed to ${BOLD}$INSTALL_DIR/hut${RESET}"

# ── PATH check ───────────────────────────────────────────
if ! echo "$PATH" | tr ':' '\n' | grep -qFx "$INSTALL_DIR"; then
    echo -e "${BLUE}→${RESET} Add to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo "  (add to ~/.bashrc or ~/.zshrc)"
fi

echo -e "\n${BOLD}Try it:${RESET} hut --version"
"$INSTALL_DIR/hut" --version 2>/dev/null || true
