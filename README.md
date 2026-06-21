# 🛖 hut

<p align="center">
  <strong>A fast build system and package manager for C/C++</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.4.0-blue" alt="Version">
  <img src="https://img.shields.io/badge/language-Rust-red" alt="Language">
  <img src="https://img.shields.io/badge/C%2FC%2B%2B-supported-blueviolet" alt="C/C++">
</p>

```
  _   _   _   _
 / \ / \ / \ / \
( h | u | t | ! )
 \_/ \_/ \_/ \_/
```

hut is a **fast, all-in-one build system and package manager** for C and C++ projects. It replaces cmake + make + vcpkg + conan with a single, zero-config tool.

> **Why "hut"?** A hut is simple, sturdy, and gets the job done — just like this tool. 🛖

---

## ✨ Features

- ⚡ **Zero-config builds** — drop a `hut.toml` and go
- 📦 **Package manager built-in** — no external package manager needed
- 🔒 **Deterministic lockfiles** — reproducible builds with `hut.lock`
- 🔖 **Semver resolution** — `^1.0`, `>=2.0,<3.0`, `=1.2.3` matched against git tags
- 🔥 **Hot rebuilds** — only rebuilds changed files (like ninja)
- 👀 **Watch mode** — `hut dev` rebuilds on file changes
- 🌐 **Package index** — search and install from 160+ curated packages (`hut search`)
- 🏃 **`hut x`** — run any package directly (like `npx` for C/C++)
- 🔗 **Workspaces** — run commands across multiple packages
- 🎨 **Templates** — scaffold projects with `hut create`
- 📊 **Info & tree** — inspect your dependency graph
- 🪡 **Include system** — header-only library handling
- 🔄 **Self-update** — easy upgrades
- 🐚 **Shell completions** — bash, zsh, fish support

---

## 📋 Commands

| Command | Description |
|---------|-------------|
| `hut init [name]` | Create a new hut project |
| `hut create <template>` | Scaffold from template (`lib`, `app`, `raylib-game`) |
| `hut build [--release]` | Compile the project |
| `hut run [target]` | Build and run a target |
| `hut test` | Discover and run test targets |
| `hut dev` | Watch for changes and rebuild |
| `hut install` | Install all dependencies |
| `hut add <pkg>` | Add a dependency |
| `hut remove <pkg>` | Remove a dependency |
| `hut update [pkg]` | Update dependencies |
| `hut outdated` | List outdated dependencies |
| `hut x <pkg>` | Run a remote package (npx-style) |
| `hut link [path]` | Symlink a local package for development |
| `hut unlink <pkg>` | Remove a local dev symlink |
| `hut publish` | Show publishing instructions |
| `hut patch <pkg>` | Extract a dependency's source for local patching |
| `hut info` | Show project info and dependency tree |
| `hut upgrade` | Self-update hut to the latest version |
| `hut workspace add/ls/run` | Manage workspace members |
| `hut pm cache/ls/bin` | Manage the package cache |
| `hut completions <shell>` | Generate shell completions |
| `hut search <query>` | Search the package registry |

---

## 🚀 Quick Start

```bash
# Create a new project
$ hut init my-app
  Created hut.toml
  Created src/main.c (hello world)

  Next steps:
    hut build

$ cd my-app

# Build it
$ hut build
  Compiling src/main.c
  Linking target/debug/my-app
  Done in 0.12s

# Run it
$ hut run
  Hello from my-app!

# Add a dependency
$ hut add jdoe/libjson
  Resolving...
  Fetching jdoe/libjson@1.2.0
  Installed jdoe/libjson@1.2.0
  Updated hut.lock

# Watch and rebuild
$ hut dev
  Watching for changes...
```

---

## 📦 hut.toml

hut uses a simple TOML configuration file. No CMakeLists.txt, no Makefile.

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[build]
kind = "executable"      # or "library", "staticlib"
sources = ["src/*.c"]
opt-level = 2            # 0-3, like Rust
include-dirs = ["include"]

[dependencies]
jdoe/libjson = "^1.2"
alice/httplib = "latest"

[dev-dependencies]
testlib/test = "^0.1"

[scripts]
bench = "./benchmarks/run.sh"
```

---

## 🆚 Comparison

| Feature | hut | CMake | Make | vcpkg | Conan |
|---------|-----|-------|------|-------|-------|
| **Zero-config** | ✅ | ❌ | ❌ | ❌ | ❌ |
| **Package manager** | ✅ Built-in | ❌ | ❌ | ✅ (separate) | ✅ (separate) |
| **Lockfile** | ✅ `hut.lock` | ❌ | ❌ | ❌ | ✅ |
| **Watch mode** | ✅ `hut dev` | ❌ | ❌ | ❌ | ❌ |
| **npx-style** | ✅ `hut x` | ❌ | ❌ | ❌ | ❌ |
| **Single binary** | ✅ ~11MB | ❌ | ❌ | ❌ | ❌ |
| **Templates** | ✅ | ❌ | ❌ | ❌ | ❌ |
| **Shell completions** | ✅ | ❌ | ❌ | ❌ | ❌ |
| **C/C++ support** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **IDE integration** | 🔜 | ✅ | ✅ | ✅ | ✅ |

---

## 📐 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         hut CLI                             │
│                    (clap derive parser)                     │
├─────────────────────────────────────────────────────────────┤
│  Commands                                                   │
│  ┌──────────┬──────────┬──────────┬──────────┬──────────┐  │
│  │  init    │  create  │  build   │  run     │  test    │  │
│  ├──────────┼──────────┼──────────┼──────────┼──────────┤  │
│  │  add     │  remove  │  install │  update  │ outdated │  │
│  ├──────────┼──────────┼──────────┼──────────┼──────────┤  │
│  │  x       │  link    │  publish │  search  │  info    │  │
│  ├──────────┼──────────┼──────────┼──────────┼──────────┤  │
│  │  dev     │  patch   │  pm      │  upgrade │ complet. │  │
│  └──────────┴──────────┴──────────┴──────────┴──────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Core Library (libhut)                                      │
│  ┌──────────┬──────────┬──────────┬──────────┬──────────┐  │
│  │ config   │ resolver │ fetcher  │ builder  │ registry │  │
│  ├──────────┼──────────┼──────────┼──────────┼──────────┤  │
│  │ lockfile │ package  │ include  │  error   │          │  │
│  └──────────┴──────────┴──────────┴──────────┴──────────┘  │
├─────────────────────────────────────────────────────────────┤
│  External                                                  │
│  ┌──────────┬──────────┬──────────┬──────────┐            │
│  │  gcc/    │  git     │  HTTP    │  tar.gz  │            │
│  │  clang   │  repos   │  registry│  archives │            │
│  └──────────┴──────────┴──────────┴──────────┘            │
└─────────────────────────────────────────────────────────────┘
```

---

## 🔧 Installation

### From Source

```bash
git clone https://github.com/oliynykmax/hut.git ~/.hut
cd ~/.hut
cargo build --release
cp target/release/hut ~/.local/bin/
```

### Via Script

```bash
curl -fsSL https://raw.githubusercontent.com/oliynykmax/hut/main/install.sh | bash
```

### Dependencies

hut needs a C/C++ compiler toolchain on your system:

| Dependency | Why | Install |
|---|---|---|
| **gcc** or **clang** | Compiles C/C++ projects | `apt install gcc` or `apt install clang` |
| **g++** or **clang++** | Compiles C++ projects (optional) | `apt install g++` or comes with clang |
| **git** | Fetches git-based package dependencies | `apt install git` |
| **libtcc** | JIT compilation via `hut run --jit` (optional) | `apt install tcc libtcc-dev` |
| **Rust** | Builds hut itself from source | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

> hut auto-detects available compilers at runtime. If you only have gcc, it uses gcc. If you have both, it asks which you prefer (once, saved to `hut.toml`).

---

## 📂 Project Structure

```
my-project/
├── hut.toml          # Project config
├── hut.lock          # Dependency lockfile (auto-generated)
├── src/
│   ├── main.c        # Entry point
│   ├── lib.c         # Library code
│   └── utils.c       # Utilities
├── include/
│   └── mylib.h       # Public headers
├── tests/
│   └── test_main.c   # Test file
└── target/           # Build output (gitignored)
    ├── debug/
    │   └── my-project
    └── release/
        └── my-project
```

---

## 🧪 Include System

hut has a smart include system that automatically manages header-only libraries:

```toml
[build]
kind = "executable"
sources = ["src/*.c"]
include-dirs = ["include", "vendor/stb"]

[include-only]
stb_image = "vendor/stb/stb_image.h"
```

This ensures header-only libraries are tracked for rebuilds and properly included in the build graph.

---

## ⚡ Performance

*Benchmarks run on: Linux x86_64, AMD EPYC 7C13 (16 threads), GCC 13.3.0, hut 0.1.0 (release). 100 C files.*

### Cold Build (from scratch)

| Tool | Time | Notes |
|------|------|-------|
| hut | **0.694s** | Rayon-parallel across 16 threads |
| `make -j16` | **0.707s** | GNU Make with full parallelism |
| `gcc *.c -o app` | **1.668s** | Single-process baseline |

hut uses rayon for true parallel compilation — matching and beating `make -j16` on cold builds.

### Hot Build (no changes, `.o` caching)

| Tool | Time |
|------|------|
| `make -j16` | **0.005s** |
| hut | **0.007s** |

hut uses `.o` timestamp caching — only changed files are recompiled. Within 2ms of `make` on hot builds. Effectively tied.

### Runtime Performance — fib(45)

| Compiler | Time | Notes |
|----------|------|-------|
| hut (`--release`, `-O2`) | 2.434s | Identical generated code |
| gcc `-O2` | 2.428s | Baseline |

Both produce identical machine code — hut adds zero runtime overhead.

> *Run `./benchmarks/bench.sh` to generate fresh numbers on your system.*

---

## 🗺️ Roadmap

- [ ] IDE integration (compile_commands.json, LSP)
- [ ] Cross-compilation targets
- [ ] Pre-compiled headers (PCH) support
- [ ] Distributed build caching
- [ ] Plugin system
- [ ] Official package registry
- [ ] WebAssembly targets
- [ ] CUDA/OpenCL support

---

## 🤝 Contributing

Contributions are welcome! Check the [GitHub repository](https://github.com/oliynykmax/hut) for issues and pull requests.

```bash
git clone git@github.com:oliynykmax/hut.git
cd hut
cargo build
cargo test
```

---

<p align="center">
  <sub>Built with Rust. Simple and fast.</sub>
</p>
