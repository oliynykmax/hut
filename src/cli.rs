// ── hut CLI definition ────────────────────────────────────────────────────

use clap::{Parser, Subcommand};

/// hut — A fast build system and package manager for C/C++
#[derive(Parser)]
#[command(name = "hut", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new hut project in an existing directory (or a new one)
    Init {
        /// Optional project name (defaults to the current directory name)
        name: Option<String>,
    },

    /// Scaffold a project from a template
    Create {
        /// Template to use: lib, app, raylib-game
        template: String,
    },

    /// Install all dependencies (resolves + fetches, writes lockfile)
    #[command(alias = "i")]
    Install,

    /// Add dependencies and install them
    #[command(alias = "a")]
    Add {
        /// Package names (must exist in packages.toml)
        #[arg(required = true, num_args = 1..)]
        pkgs: Vec<String>,
        /// Add as development dependencies
        #[arg(long)]
        dev: bool,
        /// Add as build dependencies
        #[arg(long)]
        build: bool,
    },

    /// Remove a dependency
    #[command(alias = "rm")]
    Remove {
        /// Package name
        pkg: String,
    },

    /// Update dependencies to the latest compatible versions
    #[command(alias = "up")]
    Update {
        /// Optional: update only this package
        pkg: Option<String>,
    },

    /// List outdated dependencies (registry check required)
    Outdated,

    /// Compile the project
    #[command(alias = "b")]
    Build {
        /// Build in release mode
        #[arg(long, short)]
        release: bool,
        /// Compiler to use: auto, gcc, clang
        #[arg(long, short = 'c')]
        compiler: Option<String>,
    },

    /// Build and execute a target (or run a script)
    Run {
        /// Optional target name or script name
        target: Option<String>,
        /// Arguments forwarded to the target
        #[arg(last = true)]
        args: Vec<String>,
        /// Build in release mode
        #[arg(long, short)]
        release: bool,
        /// JIT compile and run via libtcc (no binaries written)
        #[arg(long)]
        jit: bool,
    },

    /// Discover and run test targets
    #[command(alias = "t")]
    Test,

    /// Fetch, build, and execute a remote package (like npx)
    X {
        /// Package spec: "user/repo" or "user/repo@v1.0"
        pkg: String,
        /// Arguments forwarded to the package's binary
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Symlink a local package for development
    Link {
        /// Optional path to the local package (defaults to cwd)
        path: Option<String>,
    },

    /// Remove a local development symlink
    Unlink {
        /// Package name to unlink
        pkg: String,
    },

    /// Show instructions for publishing to the registry
    Publish,

    /// Manage the package cache
    #[command(subcommand)]
    Pm(PmCommand),

    /// Self-update hut to the latest version
    Upgrade,

    /// Extract a dependency's source for local patching
    Patch {
        /// Package name to extract
        pkg: String,
    },

    /// Show project info and dependency tree
    Info,

    /// Watch for file changes and rebuild automatically
    Dev,

    /// Manage workspace members
    #[command(subcommand)]
    Workspace(WorkspaceCommand),

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        shell: String,
    },

    /// Search the package registry
    Search {
        /// Search query
        query: String,
    },

    /// Format C/C++ source files with clang-format
    Fmt {
        /// Check only — don't write changes (like --check)
        #[arg(long)]
        check: bool,
    },

    /// Lint C/C++ source files with clang-tidy or compiler warnings
    Lint,

    /// Clean build artifacts (removes target/)
    Clean,
}

#[derive(Subcommand)]
pub enum PmCommand {
    /// Show cache path and disk usage
    Cache,
    /// List all cached packages
    Ls,
    /// Show the hut binary directory
    Bin,
}

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// Add a directory to the workspace members
    Add {
        /// Path to the member directory
        path: String,
    },
    /// List workspace members
    Ls,
    /// Run a command in all workspace members
    Run {
        /// Command to run in each workspace member
        command: String,
        /// Arguments for the command
        #[arg(last = true)]
        args: Vec<String>,
    },
}
