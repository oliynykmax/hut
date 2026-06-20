# 🛖 hut

<p align="center">
  <strong>A fast build system and package manager for C/C++</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
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
- 🔥 **Hot rebuilds** — only rebuilds changed files (like ninja)
- 👀 **Watch mode** — `hut dev` rebuilds on file changes
- 🧪 **Built-in testing** — auto-discovers and runs test targets
- 🌐 **Registry support** — search and install packages from the community registry
- 🏃 **`hut x`** — run any package directly (like `npx` for C/C++)
- 🔗 **Workspaces** — manage monorepos with ease
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
| **Testing** | ✅ Built-in | ❌ | ❌ | ❌ | ❌ |
| **Workspaces** | ✅ | ❌ | ❌ | ❌ | ❌ |
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
git clone git@github.com:oliynykmax/hut.git ~/.hut
cd ~/.hut
cargo build --release
cp target/release/hut ~/.local/bin/
```

### Via Script

```bash
curl -fsSL https://raw.githubusercontent.com/oliynykmax/hut/main/install.sh | bash
```

### Prerequisites

- **Rust** (to build hut itself)
- **gcc** or **clang** (to compile C/C++ projects)
- **git** (for fetching git-based dependencies)

### Rust Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (22 commands, aliases) |
| `serde` / `serde_json` / `toml` | Config, lockfile, package manifest serialization |
| `reqwest` | HTTP client for registry and package fetching |
| `sha2` / `hex` | Package integrity verification (SHA-256) |
| `tar` / `flate2` | Tarball extraction for fetched packages |
| `semver` | Semantic version parsing and constraint matching |
| `dirs` | XDG-compliant cache/config directory resolution |
| `walkdir` | Recursive directory traversal for builds |
| `glob` | Source file glob pattern matching |
| `chrono` | Timestamps for cache freshness |
| `colored` | Terminal colour output (cargo-style) |
| `indicatif` | Progress bars for downloads |
| `tempfile` | Temporary directories in tests |
| `num_cpus` | Parallel job count detection |
| `libloading` | Dynamic library loading for JIT (libtcc) |
| `tokio` | Async runtime |
| `anyhow` / `thiserror` | Error handling |
| `clap_complete` | Shell completion generation |
| `notify` | File system watcher for `hut dev` |

Dev dependencies: `assert_cmd`, `predicates` (integration tests).

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

*Benchmarks run on: Linux x86_64, GCC 13.3.0, hut 0.1.0 (release)*

### Compilation Speed — Cold Build (from scratch)

| Files | hut | gcc (sequential) | Speedup |
|-------|-----|-------------------|---------|
| 10    | 0.093s | 0.208s | **2.24×** |
| 50    | 0.297s | 0.827s | **2.78×** |
| 100   | 0.568s | 1.603s | **2.82×** |

hut parallelizes compilation across all cores, giving a consistent **~2.8× speedup** over sequential gcc on cold builds.

### Compilation Speed — Hot Build (incremental, 1 file changed)

| Files | hut | gcc (changed-file only) |
|-------|-----|--------------------------|
| 10    | 0.107s | 0.042s |
| 50    | 0.297s | 0.045s |
| 100   | 0.564s | 0.051s |

> **Note:** The current hut hot-build path still performs a full relink. Incremental compilation with true object-file caching is on the roadmap.

### Runtime Performance — fib(45)

| Compiler | Time | Notes |
|----------|------|-------|
| hut (`--release`, `-O2`) | 2.473s | Identical generated code |
| gcc `-O2` | 2.494s | Baseline |

Both produce identical machine code — hut doesn't add any runtime overhead.

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
