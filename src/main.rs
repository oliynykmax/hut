// ── hut: a Bun-like C/C++ package manager ─────────────────────────────────
//
// CLI binary using clap derive.  All heavy lifting is performed by the
// library modules under `hut::`.

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};
use hut::registry::{self};

// ── Helper: hut home directory ────────────────────────────────────────────

fn hut_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hut")
}

fn cache_dir() -> PathBuf {
    hut::fetcher::get_default_cache_dir()
}

fn packages_dir() -> PathBuf {
    cache_dir()
}

fn lockfile_path() -> PathBuf {
    PathBuf::from("hut.lock")
}

/// Scan for available C compilers on the system
fn available_compilers() -> Vec<String> {
    let mut found = Vec::new();
    for cc in &["gcc", "clang"] {
        if std::process::Command::new("which")
            .arg(cc)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            found.push(cc.to_string());
        }
    }
    found
}

// ── CLI definition ─────────────────────────────────────────────────────────

/// hut — A Bun-inspired build system and package manager for C/C++
#[derive(Parser)]
#[command(name = "hut", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    Install {
        /// Custom registry URL
        #[arg(long)]
        registry: Option<String>,
    },

    /// Add a dependency and install it
    #[command(alias = "a")]
    Add {
        /// Package spec, e.g. "user/libfoo" or "user/libfoo@^1.0"
        pkg: String,
        /// Add as a development dependency
        #[arg(long)]
        dev: bool,
        /// Add as a build dependency
        #[arg(long)]
        build: bool,
        /// Custom registry URL
        #[arg(long)]
        registry: Option<String>,
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

    /// Show self-update instructions
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

    /// Clean build artifacts (removes target/)
    Clean,
}

#[derive(Subcommand)]
enum PmCommand {
    /// Show cache path and disk usage
    Cache,
    /// List all cached packages
    Ls,
    /// Show the hut binary directory
    Bin,
}

#[derive(Subcommand)]
enum WorkspaceCommand {
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

// ── Main entry point ───────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { name } => cmd_init(name).await,
        Commands::Create { template } => cmd_create(&template).await,
        Commands::Install { registry } => cmd_install(registry.as_deref()).await,
        Commands::Add { pkg, dev, build, registry } => cmd_add(&pkg, dev, build, registry.as_deref()).await,
        Commands::Remove { pkg } => cmd_remove(&pkg).await,
        Commands::Update { pkg } => cmd_update(pkg.as_deref()).await,
        Commands::Outdated => cmd_outdated().await,
        Commands::Build { release, compiler } => cmd_build(release, compiler.as_deref()).await,
        Commands::Run { target, args, release, jit } => cmd_run(target, args, release, jit).await,
        Commands::Test => cmd_test().await,
        Commands::X { pkg, args } => cmd_x(&pkg, &args).await,
        Commands::Link { path } => cmd_link(path.as_deref()).await,
        Commands::Unlink { pkg } => cmd_unlink(&pkg).await,
        Commands::Publish => cmd_publish().await,
        Commands::Pm(sub) => cmd_pm(sub).await,
        Commands::Upgrade => cmd_upgrade().await,
        Commands::Patch { pkg } => cmd_patch(&pkg).await,
        Commands::Info => cmd_info().await,
        Commands::Dev => cmd_dev().await,
        Commands::Workspace(sub) => cmd_workspace(sub).await,
        Commands::Completions { shell } => cmd_completions(&shell).await,
        Commands::Search { query } => cmd_search(&query).await,
        Commands::Clean => cmd_clean().await,
    };

    if let Err(e) = result {
        eprintln!("{} {}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}

// ── Command implementations ────────────────────────────────────────────────

/// 1. `hut init [name]`
async fn cmd_init(name: Option<String>) -> HutResult<()> {
    let project_name = name.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-project".to_string())
    });

    // If a project name was explicitly provided, create the directory and use it
    let project_dir = if let Some(ref dir_name) = name {
        let dir = std::env::current_dir()?.join(dir_name);
        std::fs::create_dir_all(&dir)?;
        dir
    } else {
        std::env::current_dir()?
    };

    let config = HutConfig::default_template(&project_name);
    let config_path = project_dir.join("hut.toml");

    if config_path.exists() {
        eprintln!(
            "{} a hut.toml already exists in this directory",
            "warning:".yellow().bold()
        );
        return Ok(());
    }

    config.save(&config_path)?;
    println!("{} {}", "Created".green().bold(), config_path.display());

    // Create src/ directory and a hello-world main.c
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let main_c = src_dir.join("main.c");
    if !main_c.exists() {
        let c_source = HELLO_WORLD_C.replace("{NAME}", &project_name);
        std::fs::write(&main_c, &c_source)?;
        println!("{} src/main.c (hello world)", "Created".green().bold());
    }

    // Create a basic .gitignore
    let gitignore = project_dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, "target/\n*.o\n*.a\n*.so\n")?;
    }

    // Initialize a git repository
    let git_dir = project_dir.join(".git");
    if !git_dir.exists() {
        match std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&project_dir)
            .status()
        {
            Ok(status) if status.success() => {
                println!("{} .git repository", "Initialized".green().bold());
            }
            _ => {
                // git not installed — that's fine
            }
        }
    }

    println!();
    println!("{} Run:", "Next steps:".bold());
    println!("  hut build");
    println!("  hut run");
    Ok(())
}

/// 2. `hut create <template>`
async fn cmd_create(template: &str) -> HutResult<()> {
    let cwd = std::env::current_dir()?;

    match template {
        "lib" => {
            let config = HutConfig::default_template("mylib");
            let config_path = cwd.join("hut.toml");
            if config_path.exists() {
                eprintln!("{} hut.toml already exists", "warning:".yellow().bold());
                return Ok(());
            }
            config.save(&config_path)?;
            println!("{} hut.toml", "Created".green().bold());

            // Create include/ and src/ directories
            let inc = cwd.join("include");
            let src = cwd.join("src");
            std::fs::create_dir_all(&inc)?;
            std::fs::create_dir_all(&src)?;

            std::fs::write(inc.join("mylib.h"), LIB_HEADER)?;
            std::fs::write(src.join("mylib.c"), LIB_SOURCE)?;

            println!("{} include/mylib.h", "Created".green().bold());
            println!("{} src/mylib.c", "Created".green().bold());
        }
        "app" => {
            let config = HutConfig::default_template("myapp");
            let config_path = cwd.join("hut.toml");
            if config_path.exists() {
                eprintln!("{} hut.toml already exists", "warning:".yellow().bold());
                return Ok(());
            }
            config.save(&config_path)?;
            println!("{} hut.toml", "Created".green().bold());

            let src = cwd.join("src");
            std::fs::create_dir_all(&src)?;
            std::fs::write(src.join("main.c"), APP_MAIN_C)?;
            println!("{} src/main.c", "Created".green().bold());
        }
        "raylib-game" => {
            let mut config = HutConfig::default_template("raylib-game");
            // Add raylib as a dependency
            config
                .dependencies
                .insert("raylib".to_string(), "^5.0".to_string());
            let config_path = cwd.join("hut.toml");
            if config_path.exists() {
                eprintln!("{} hut.toml already exists", "warning:".yellow().bold());
                return Ok(());
            }
            config.save(&config_path)?;
            println!(
                "{} hut.toml (with raylib dependency)",
                "Created".green().bold()
            );

            let src = cwd.join("src");
            std::fs::create_dir_all(&src)?;
            std::fs::write(src.join("main.c"), RAYLIB_GAME_C)?;
            println!("{} src/main.c", "Created".green().bold());
        }
        _ => {
            eprintln!(
                "{} unknown template '{}'. Available: lib, app, raylib-game",
                "error:".red().bold(),
                template
            );
            std::process::exit(1);
        }
    }

    println!();
    println!("{} Run `hut build` to compile.", "Done!".green().bold());
    Ok(())
}

/// 3. `hut install`
async fn cmd_install(registry_url: Option<&str>) -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;

    if config.dependencies.is_empty()
        && config.build_dependencies.is_empty()
        && config.test_dependencies.is_empty()
    {
        println!("{}", "No dependencies to install.".dimmed());
        return Ok(());
    }

    // Fetch the registry for resolution
    let registry = registry::fetch_registry(registry_url).await?;
    let cache = cache_dir();

    // Resolve all dependencies
    println!("{} dependencies...", "Resolving".bold().cyan());
    let resolved =
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?;

    // Update lockfile with resolved entries
    for dep in &resolved {
        let locked = LockedPackage {
            name: dep.name.clone(),
            version: dep.version.clone(),
            source: String::new(),
            integrity: String::new(),
            resolved: dep.package.repository.clone().unwrap_or_default(),
            dependencies: dep.package.dependencies.clone(),
        };
        lockfile.insert(locked);
    }
    lockfile.save(&lock_path)?;

    println!(
        "{} {} packages resolved",
        "Resolved".green().bold(),
        resolved.len().to_string().bold()
    );

    // Fetch + install
    hut::fetcher::install_dependencies(&config, &lockfile, &cache).await?;

    Ok(())
}

/// 4. `hut add <pkg> [--dev] [--build]`
async fn cmd_add(pkg_spec: &str, dev: bool, build: bool, registry_url: Option<&str>) -> HutResult<()> {
    let (mut config, config_path) = HutConfig::find()?;

    // Parse package name and optional version constraint
    let (name, constraint) = parse_dep_spec(pkg_spec);
    let constraint = constraint.unwrap_or_else(|| "*".to_string());

    let target_map = if dev {
        &mut config.test_dependencies
    } else if build {
        &mut config.build_dependencies
    } else {
        &mut config.dependencies
    };

    if target_map.contains_key(&name) {
        println!(
            "{} {} is already a dependency (version: {}). Use `hut update` to change it.",
            "info:".cyan().bold(),
            name.bold(),
            target_map[&name]
        );
        return Ok(());
    }

    target_map.insert(name.clone(), constraint.clone());
    config.save(&config_path)?;

    let dep_type = if dev {
        "dev"
    } else if build {
        "build"
    } else {
        ""
    };

    println!(
        "{} {} {} → hut.toml",
        "Added".green().bold(),
        name.bold(),
        dep_type.dimmed()
    );

    // Now install
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;
    let registry = registry::fetch_registry(registry_url).await?;
    let cache = cache_dir();

    // Resolve all dependencies
    println!("{} dependencies...", "Resolving".bold().cyan());
    let resolved =
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?;

    // Update lockfile
    for dep in &resolved {
        let locked = LockedPackage {
            name: dep.name.clone(),
            version: dep.version.clone(),
            source: String::new(),
            integrity: String::new(),
            resolved: dep.package.repository.clone().unwrap_or_default(),
            dependencies: dep.package.dependencies.clone(),
        };
        lockfile.insert(locked);
    }
    lockfile.save(&lock_path)?;

    // Fetch + install
    hut::fetcher::install_dependencies(&config, &lockfile, &cache).await?;

    println!(
        "{} installed {}",
        "Done".green().bold(),
        name.bold()
    );

    Ok(())
}

/// 5. `hut remove <pkg>`
async fn cmd_remove(pkg: &str) -> HutResult<()> {
    let (mut config, config_path) = HutConfig::find()?;

    let removed = config.dependencies.remove(pkg).is_some()
        || config.build_dependencies.remove(pkg).is_some()
        || config.test_dependencies.remove(pkg).is_some();

    if !removed {
        eprintln!(
            "{} '{}' is not a dependency of this project.",
            "info:".yellow().bold(),
            pkg.yellow()
        );
        return Ok(());
    }

    config.save(&config_path)?;
    println!(
        "{} {} removed from hut.toml",
        "Removed".green().bold(),
        pkg.bold()
    );

    // Remove from lockfile
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;
    lockfile.remove(pkg);
    lockfile.save(&lock_path)?;

    println!(
        "{} {} removed from hut.lock",
        "Removed".green().bold(),
        pkg.bold()
    );

    Ok(())
}

/// 6. `hut update [pkg]`
async fn cmd_update(pkg: Option<&str>) -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;
    let registry = registry::fetch_registry(None).await?;

    let to_update: Vec<String> = if let Some(target) = pkg {
        if !config.dependencies.contains_key(target)
            && !config.build_dependencies.contains_key(target)
            && !config.test_dependencies.contains_key(target)
        {
            eprintln!(
                "{} '{}' is not a dependency of this project.",
                "error:".red().bold(),
                target.yellow()
            );
            return Ok(());
        }
        vec![target.to_string()]
    } else {
        config
            .dependencies
            .keys()
            .chain(config.build_dependencies.keys())
            .chain(config.test_dependencies.keys())
            .cloned()
            .collect()
    };

    if to_update.is_empty() {
        println!("{}", "No dependencies to update.".dimmed());
        return Ok(());
    }

    println!(
        "{} {} package(s)...",
        "Updating".bold().cyan(),
        to_update.len().to_string().bold()
    );

    // Remove them from lockfile so the resolver picks the latest
    for name in &to_update {
        lockfile.remove(name);
    }

    // Re-resolve
    let resolved =
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?;

    for dep in &resolved {
        let locked = LockedPackage {
            name: dep.name.clone(),
            version: dep.version.clone(),
            source: String::new(),
            integrity: String::new(),
            resolved: dep.package.repository.clone().unwrap_or_default(),
            dependencies: dep.package.dependencies.clone(),
        };
        lockfile.insert(locked);
    }
    lockfile.save(&lock_path)?;

    // Fetch updated packages
    let cache = cache_dir();
    hut::fetcher::install_dependencies(&config, &lockfile, &cache).await?;

    println!(
        "{} dependencies updated.",
        "Updated".green().bold()
    );

    Ok(())
}

/// 7. `hut outdated`
async fn cmd_outdated() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let registry = registry::fetch_registry(None).await?;

    let mut found_outdated = false;

    for (name, constraint) in config
        .dependencies
        .iter()
        .chain(config.build_dependencies.iter())
        .chain(config.test_dependencies.iter())
    {
        let current = lockfile.get(&name).map(|l| l.version.as_str());

        if let Some(entry) = registry.find(&name) {
            let latest = match hut::registry::resolve_version(entry, constraint) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let is_outdated = match current {
                Some(cur) if cur != latest => true,
                None => true,
                _ => false,
            };

            if is_outdated {
                found_outdated = true;
                let current_display = current.unwrap_or("none");
                println!(
                    "{} {} {} → {}",
                    name.bold(),
                    current_display.red(),
                    "→".dimmed(),
                    latest.green().bold()
                );
            }
        }
    }

    if !found_outdated {
        println!("{}", "All dependencies are up to date.".green());
    }

    Ok(())
}

/// 8. `hut build [--release] [--compiler <auto|gcc|clang>]`
async fn cmd_build(release: bool, compiler_override: Option<&str>) -> HutResult<()> {
    let (mut config, config_path) = HutConfig::find()?;

    // ── Compiler selection ───────────────────────────────────────────────
    if let Some(compiler) = compiler_override {
        if compiler == "auto" {
            let available = available_compilers();
            if available.is_empty() {
                return Err(HutError::NoCompiler);
            }

            if available.len() == 1 {
                config.build.compiler = available[0].clone();
                println!(
                    "{} Using compiler: {}",
                    "info:".cyan().bold(),
                    available[0].bold()
                );
            } else {
                // Interactive prompt – only if TTY
                let is_tty = std::io::IsTerminal::is_terminal(&std::io::stdin());
                if is_tty {
                    println!(
                        "{} Available compilers:",
                        "Select compiler:".bold().yellow()
                    );
                    for (i, cc) in available.iter().enumerate() {
                        println!("  {}) {}", i + 1, cc.bold());
                    }
                    print!("  choice [1-{}]: ", available.len());

                    use std::io::{BufRead, Write};
                    let _ = std::io::stdout().flush();
                    let stdin = std::io::stdin();
                    let mut line = String::new();
                    if stdin.lock().read_line(&mut line).is_ok() {
                        let trimmed = line.trim();
                        if let Ok(idx) = trimmed.parse::<usize>() {
                            if idx >= 1 && idx <= available.len() {
                                let chosen = &available[idx - 1];
                                config.build.compiler = chosen.clone();
                                println!(
                                    "{} Selected {} → saved to hut.toml",
                                    "✓".green().bold(),
                                    chosen.bold()
                                );
                            }
                        }
                    }
                } else {
                    // Non-TTY: pick the first available
                    config.build.compiler = available[0].clone();
                    println!(
                        "{} Non-interactive: using {}",
                        "info:".cyan().bold(),
                        available[0].bold()
                    );
                }
            }

            config.save(&config_path)?;
        } else {
            config.build.compiler = compiler.to_string();
            println!(
                "{} Compiler set to: {}",
                "info:".cyan().bold(),
                compiler.bold()
            );
            config.save(&config_path)?;
        }
    }

    // ── Resolve dependencies before building ─────────────────────────────
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let resolved = if config.dependencies.is_empty()
        && config.build_dependencies.is_empty()
        && config.test_dependencies.is_empty()
    {
        vec![]
    } else {
        let registry = registry::fetch_registry(None).await?;
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?
    };

    // ── Build the project ────────────────────────────────────────────────
    hut::builder::build_project(&config, &resolved, release).await?;

    Ok(())
}

/// 9. `hut run [target] [--release]`
async fn cmd_run(
    target: Option<String>,
    args: Vec<String>,
    release: bool,
    jit: bool,
) -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let project_root = find_project_root()?;

    // ── JIT path (via libtcc, in-process) ────────────────────────────────
    if jit {
        let sources = hut::builder::collect_sources(&config, &project_root)?;

        if sources.is_empty() {
            return Err(HutError::Other(
                "No source files found for JIT compilation. Add .c/.cpp files to src/.".into(),
            ));
        }

        let mut tcc = hut::jit::Tcc::new().ok_or_else(|| {
            HutError::Other(
                "libtcc not found.\n\n\
                 Install TCC (Tiny C Compiler) to use `hut run --jit`:\n\
                   • Ubuntu/Debian:  sudo apt install tcc libtcc-dev\n\
                   • Fedora:         sudo dnf install tcc\n\
                   • Arch:           sudo pacman -S tcc\n\
                   • macOS:          brew install tcc\n\
                   • From source:\n\
                       git clone https://repo.or.cz/tinycc.git\n\
                       cd tinycc && ./configure && make && sudo make install"
                    .into(),
            )
        })?;

        println!(
            "{} {} source file(s)...",
            "   JIT".bold().magenta(),
            sources.len().to_string().bold()
        );

        let mut combined_source = String::new();
        for src in &sources {
            let content = std::fs::read_to_string(src)
                .map_err(|e| HutError::Other(format!("Failed to read {}: {e}", src.display())))?;
            combined_source.push_str(&content);
            combined_source.push('\n');
        }

        // ── Debug / release flags ─────────────────────────────────────
        if release {
            tcc.set_options("-DNDEBUG -O2")
                .map_err(|e| HutError::Other(format!("JIT options failed: {e}")))?;
        } else {
            tcc.set_options("-g -O0")
                .map_err(|e| HutError::Other(format!("JIT options failed: {e}")))?;
        }

        tcc.compile(&combined_source)
            .map_err(|e| HutError::Other(format!("JIT compilation failed: {e}")))?;

        tcc.relocate()
            .map_err(|e| HutError::Other(format!("JIT relocation failed: {e}")))?;

        println!(
            "{} {} (JIT)",
            "   Running".bold().green(),
            target.as_deref().unwrap_or(&config.package.name).bold(),
        );

        let exit_code = tcc
            .run_main(&args)
            .map_err(|e| HutError::Other(format!("JIT execution failed: {e}")))?;

        // Flush stdout — JIT'ed code and hut share the same output buffer
        use std::io::Write;
        std::io::stdout().flush().ok();

        if exit_code != 0 {
            return Err(HutError::Other(format!(
                "Process exited with code {exit_code}"
            )));
        }

        return Ok(());
    }

    // ── Normal build + run path ──────────────────────────────────────────
    // Build first
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let resolved = if config.dependencies.is_empty()
        && config.build_dependencies.is_empty()
        && config.test_dependencies.is_empty()
    {
        vec![]
    } else {
        let registry = registry::fetch_registry(None).await?;
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?
    };

    hut::builder::build_project(&config, &resolved, release).await?;

    // Determine the binary to run
    let profile = if release { "release" } else { "debug" };
    let target_name = target.as_deref().unwrap_or(&config.package.name);
    let binary = project_root.join("target").join(profile).join(target_name);

    if !binary.exists() {
        // Maybe it's a script?
        if let Some(script) = config.scripts.get(target_name) {
            println!("{} {}", "Running script:".bold().dimmed(), target_name.bold());
            let shell = if cfg!(windows) { "cmd" } else { "sh" };
            let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

            let status = Command::new(shell)
                .arg(shell_arg)
                .arg(script)
                .args(&args)
                .status()
                .map_err(|e| HutError::Other(format!("Failed to run script: {e}")))?;

            if !status.success() {
                return Err(HutError::Other(format!(
                    "Script exited with code {}",
                    status.code().unwrap_or(-1)
                )));
            }
            return Ok(());
        }

        return Err(HutError::Build(format!(
            "Binary not found at {}. Did the build succeed?",
            binary.display()
        )));
    }

    println!(
        "{} {} {}",
        "   Running".bold().green(),
        target_name.bold(),
        args.join(" ").dimmed()
    );

    let status = Command::new(&binary)
        .args(&args)
        .status()
        .map_err(|e| HutError::Other(format!("Failed to run {}: {}", binary.display(), e)))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        if code != 0 {
            return Err(HutError::Other(format!("Process exited with code {code}")));
        }
    }

    Ok(())
}

/// 10. `hut test`
async fn cmd_test() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;

    // Reuse the builder — for now, just build the project
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let registry = registry::fetch_registry(None).await?;
    let resolved =
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?;

    hut::builder::build_project(&config, &resolved, false).await?;

    println!();
    println!("{} {}", "✓".green().bold(), "Build succeeded (test runner not yet implemented)".dimmed());

    Ok(())
}

/// 11. `hut x <pkg> [args...]`
async fn cmd_x(pkg: &str, args: &[String]) -> HutResult<()> {
    hut::fetcher::fetch_and_run(pkg, args).await
}

/// 12. `hut link [path]`
async fn cmd_link(path: Option<&str>) -> HutResult<()> {
    let link_path = path
        .map(|p| PathBuf::from(p))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let link_path = std::fs::canonicalize(&link_path)
        .map_err(|_| HutError::Other(format!("Path not found: {}", link_path.display())))?;

    // Read the package name from its hut.toml
    let hut_toml = link_path.join("hut.toml");
    if !hut_toml.exists() {
        return Err(HutError::Other(format!(
            "No hut.toml found in {} — is it a hut package?",
            link_path.display()
        )));
    }

    let pkg_config = HutConfig::load(&hut_toml)?;
    let pkg_name = &pkg_config.package.name;

    // Create symlink in ~/.hut/packages/<name>/linked
    let link_target = packages_dir().join(pkg_name).join("linked");
    if let Some(parent) = link_target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove old link if it exists
    if link_target.exists() || link_target.is_symlink() {
        let _ = std::fs::remove_file(&link_target);
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_path, &link_target)?;
    }
    #[cfg(not(unix))]
    {
        // On non-Unix, store the path in a file
        std::fs::write(&link_target, link_path.to_string_lossy().as_bytes())?;
    }

    println!(
        "{} {} → {}",
        "Linked".green().bold(),
        pkg_name.bold(),
        link_target.display().to_string().dimmed()
    );

    Ok(())
}

/// 13. `hut unlink <pkg>`
async fn cmd_unlink(pkg: &str) -> HutResult<()> {
    let link_target = packages_dir().join(pkg).join("linked");

    if !link_target.exists() && !link_target.is_symlink() {
        eprintln!(
            "{} '{}' is not currently linked.",
            "info:".yellow().bold(),
            pkg.yellow()
        );
        return Ok(());
    }

    std::fs::remove_file(&link_target)?;
    println!(
        "{} {} unlinked.",
        "Unlinked".green().bold(),
        pkg.bold()
    );

    Ok(())
}

/// 14. `hut publish`
async fn cmd_publish() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;

    println!("{}", "Publishing guide:".bold().underline());
    println!();
    println!(
        "  Package: {} v{}",
        config.package.name.bold(),
        config.package.version
    );
    println!();
    println!("To publish to the hut registry:");
    println!();
    println!("  1. Push your code to GitHub (or any git host).");
    println!("  2. Tag a release using git tags matching semver:");
    println!("     {}", "$ git tag v0.1.0 && git push --tags".dimmed());
    println!("  3. Register your package by submitting a PR to:");
    println!(
        "     {}",
        "https://github.com/hutpm/registry".underline()
    );
    println!("     Add your package to the registry index.");
    println!();
    println!("  Your hut.toml must include:");
    println!("  {}", "[package]".dimmed());
    println!("  {}", "name = \"{}\"".dimmed());
    println!("  {}", "version = \"0.1.0\"".dimmed());
    println!("  {}", "repository = \"<your repo URL>\"".dimmed());

    Ok(())
}

/// 15. `hut pm <subcommand>`
async fn cmd_pm(sub: PmCommand) -> HutResult<()> {
    match sub {
        PmCommand::Cache => {
            let cache = cache_dir();
            println!("{} {}", "Cache directory:".bold(), cache.display());

            match hut::fetcher::cache_size_human(&cache) {
                Ok(size) => println!("{} {}", "Disk usage:".bold(), size),
                Err(_) => eprintln!("{}", "Could not determine cache size.".dimmed()),
            }
        }
        PmCommand::Ls => {
            let cache = cache_dir();
            if !cache.exists() {
                println!("{}", "Cache is empty.".dimmed());
                return Ok(());
            }

            println!("{}", "Cached packages:".bold());
            if let Ok(entries) = std::fs::read_dir(&cache) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with('.') {
                                continue;
                            }
                            print!("  {}", name.bold());
                            // List versions
                            if let Ok(versions) = std::fs::read_dir(&path) {
                                let vlist: Vec<_> = versions
                                    .flatten()
                                    .filter(|e| e.path().is_dir())
                                    .map(|e| e.file_name().to_string_lossy().to_string())
                                    .collect();
                                if !vlist.is_empty() {
                                    println!("  ({})", vlist.join(", ").dimmed());
                                } else {
                                    println!();
                                }
                            }
                        }
                    }
                }
            }
        }
        PmCommand::Bin => {
            let hut_home = hut_home();
            let bin_dir = hut_home.join("bin");
            println!("{} {}", "Binary directory:".bold(), bin_dir.display());
            if !bin_dir.exists() {
                println!("{}", "(does not exist yet)".dimmed());
            }
        }
    }

    Ok(())
}

/// 16. `hut upgrade`
async fn cmd_upgrade() -> HutResult<()> {
    println!("{}", "Hut self-upgrade:".bold().underline());
    println!();
    println!("  To upgrade hut to the latest version:");
    println!();
    println!(
        "  {} {}",
        "$".dimmed(),
        "cargo install --git https://github.com/hutpm/hut".dimmed()
    );
    println!();
    println!(
        "  {}",
        "Or download a pre-built binary from the releases page:".dimmed()
    );
    println!(
        "  {}",
        "https://github.com/hutpm/hut/releases".underline()
    );

    Ok(())
}

/// 17. `hut patch <pkg>`
async fn cmd_patch(pkg: &str) -> HutResult<()> {
    let (_config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;

    let locked = lockfile.get(pkg).ok_or_else(|| {
        HutError::Other(format!(
            "'{pkg}' is not in the lockfile. Run `hut install` first."
        ))
    })?;

    let cache = cache_dir();
    let pkg_dir = hut::fetcher::fetch_package(
        pkg,
        &locked.resolved,
        &locked.version,
        &cache,
    )
    .await?;

    println!("{}", "Patch mode:".bold().underline());
    println!();
    println!(
        "  Package {}@{} extracted to:",
        pkg.bold(),
        locked.version.bold()
    );
    println!("  {}", pkg_dir.display().to_string().dimmed());
    println!();
    println!("  To apply a local patch:");
    println!(
        "  1. Make your changes in: {}",
        pkg_dir.display()
    );
    println!("  2. To use the patched version, run:");
    println!(
        "     {}",
        format!("hut link {}", pkg_dir.display()).dimmed()
    );

    Ok(())
}

/// 18. `hut info`
async fn cmd_info() -> HutResult<()> {
    let (config, config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;

    println!("{}", "Package:".bold().underline());
    println!();
    println!("  name: {}", config.package.name.bold());
    println!("  version: {}", config.package.version);
    println!(
        "  language: {}",
        config.package.language
    );
    if let Some(ref desc) = config.package.description {
        println!("  description: {}", desc);
    }
    if let Some(ref lic) = config.package.license {
        println!("  license: {}", lic);
    }
    println!("  config: {}", config_path.display());
    println!();

    // Dependencies
    if !config.dependencies.is_empty() {
        println!("{}", "Dependencies:".bold());
        for (name, version) in &config.dependencies {
            let locked_ver = lockfile
                .get(name)
                .map(|l| format!(" (locked: {})", l.version))
                .unwrap_or_default();
            println!("  {} = {}{}", name.bold(), version, locked_ver.dimmed());
        }
        println!();
    } else {
        println!("{}", "Dependencies:".bold());
        println!("  (none)");
        println!();
    }

    if !config.build_dependencies.is_empty() {
        println!("{}", "Build Dependencies:".bold());
        for (name, version) in &config.build_dependencies {
            println!("  {} = {}", name.bold(), version);
        }
        println!();
    }

    if !config.test_dependencies.is_empty() {
        println!("{}", "Test Dependencies:".bold());
        for (name, version) in &config.test_dependencies {
            println!("  {} = {}", name.bold(), version);
        }
        println!();
    }

    // Build config
    println!("{}", "Build config:".bold());
    println!("  system: {}", config.build.system);
    println!("  c_standard: {}", config.build.c_standard);
    if let Some(ref cpp) = config.build.cpp_standard {
        println!("  cpp_standard: {}", cpp);
    }
    println!("  opt_level: {}", config.build.opt_level);
    println!("  debug: {}", config.build.debug);
    println!("  compiler: {}", config.build.compiler);
    println!();

    // Scripts
    if !config.scripts.is_empty() {
        println!("{}", "Scripts:".bold());
        for (name, cmd) in &config.scripts {
            println!("  {}: {}", name.bold(), cmd.dimmed());
        }
        println!();
    }

    Ok(())
}

/// 19. `hut dev`
async fn cmd_dev() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;

    println!("{}", "Dev mode (watch + rebuild)".bold().underline());
    println!();
    println!("  Watching for file changes...");

    // Build once
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let registry = registry::fetch_registry(None).await?;
    let resolved =
        hut::resolver::resolve_dependencies(&config, &lockfile, &registry, &packages_dir()).await?;

    hut::builder::build_project(&config, &resolved, false).await?;

    println!();
    println!(
        "{} Watching for changes (press Ctrl+C to stop)...",
        "👀".bold()
    );

    // Simple polling watch loop
    use std::time::{Duration, SystemTime};
    let project_root = find_project_root()?;
    let mut last_build = SystemTime::now();

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut files_changed = false;
        let walker = walkdir::WalkDir::new(&project_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                matches!(ext, "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx")
            });

        for entry in walker {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mod_time) = meta.modified() {
                    if mod_time > last_build {
                        files_changed = true;
                        break;
                    }
                }
            }
        }

        if files_changed {
            println!();
            println!("{} File changed, rebuilding...", "⚡".yellow().bold());
            last_build = SystemTime::now();

            match hut::builder::build_project(&config, &resolved, false).await {
                Ok(()) => {
                    println!("{} Build succeeded.", "✓".green().bold());
                }
                Err(e) => {
                    eprintln!("{} Build failed: {}", "✗".red().bold(), e);
                }
            }
        }
    }
}

/// 20. `hut workspace <subcommand>`
async fn cmd_workspace(sub: WorkspaceCommand) -> HutResult<()> {
    match sub {
        WorkspaceCommand::Add { path } => {
            let (mut config, config_path) = HutConfig::find()?;
            let member_path = PathBuf::from(&path);
            let canonical = std::fs::canonicalize(&member_path)
                .map_err(|e| HutError::Other(format!("Invalid path: {e}")))?;

            let relative = canonical
                .strip_prefix(config_path.parent().unwrap_or(Path::new(".")))
                .unwrap_or(&canonical)
                .to_string_lossy()
                .to_string();

            if config.workspace.members.contains(&relative) {
                println!(
                    "{} Already a workspace member.",
                    "info:".cyan().bold()
                );
                return Ok(());
            }

            config.workspace.members.push(relative.clone());
            config.save(&config_path)?;

            println!(
                "{} {} added to workspace.",
                "Added".green().bold(),
                relative.bold()
            );
        }
        WorkspaceCommand::Ls => {
            let (config, _config_path) = HutConfig::find()?;

            if config.workspace.members.is_empty() {
                println!("{}", "No workspace members.".dimmed());
            } else {
                println!("{}", "Workspace members:".bold());
                for member in &config.workspace.members {
                    println!("  {}", member.bold());
                }
            }
        }
        WorkspaceCommand::Run { command, args } => {
            let (config, config_path) = HutConfig::find()?;
            let root_dir = config_path.parent().unwrap_or(Path::new("."));

            if config.workspace.members.is_empty() {
                println!("{}", "No workspace members to run in.".dimmed());
                return Ok(());
            }

            for member in &config.workspace.members {
                let member_dir = root_dir.join(member);
                println!(
                    "{} {} > {}",
                    "▶".cyan().bold(),
                    member.bold(),
                    format!("{command} {}", args.join(" ")).dimmed()
                );

                let status = Command::new(&command)
                    .args(&args)
                    .current_dir(&member_dir)
                    .status();

                match status {
                    Ok(s) if !s.success() => {
                        eprintln!(
                            "  {} Command failed in {} with exit code {}",
                            "✗".red(),
                            member,
                            s.code().unwrap_or(-1)
                        );
                    }
                    Err(e) => {
                        eprintln!("  {} Failed to run in {}: {}", "✗".red(), member, e);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// 21. `hut completions <shell>`
async fn cmd_completions(shell: &str) -> HutResult<()> {
    let sh = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "pwsh" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        _ => {
            eprintln!(
                "{} Unknown shell '{}'. Supported: bash, zsh, fish, powershell",
                "error:".red().bold(),
                shell
            );
            return Ok(());
        }
    };

    let mut cmd = Cli::command();
    generate(sh, &mut cmd, "hut", &mut std::io::stdout());

    Ok(())
}

/// 22. `hut search <query>`
async fn cmd_search(query: &str) -> HutResult<()> {
    let registry = registry::fetch_registry(None).await?;
    let results = registry.search(query);

    if results.is_empty() {
        println!("{} {}", "No packages found for".dimmed(), query.bold());
        return Ok(());
    }

    println!(
        "{} {} results for \"{}\":",
        "Found".green().bold(),
        results.len().to_string().bold(),
        query
    );
    println!();

    for entry in results {
        println!(
            "  {} - {}",
            entry.name.bold().cyan(),
            entry.description.dimmed()
        );
        let version_count = entry.versions.len();
        let latest = entry.versions.keys().last();
        if let Some(lv) = latest {
            print!("    {} {}", "Latest:".dimmed(), lv.bold());
            if version_count > 1 {
                print!(" ({} versions)", version_count);
            }
            println!();
        }
        if !entry.tags.is_empty() {
            println!("    {} {}", "Tags:".dimmed(), entry.tags.join(", ").dimmed());
        }
        println!("    {}", entry.repository.dimmed());
        println!();
    }

    Ok(())
}

/// 23. `hut clean` — remove build artifacts (target/)
async fn cmd_clean() -> HutResult<()> {
    let project_root = find_project_root()?;
    let target_dir = project_root.join("target");

    if !target_dir.exists() {
        println!("{} No build artifacts to clean.", "info:".dimmed());
        return Ok(());
    }

    let size = hut::fetcher::cache_size_human(&target_dir).ok();

    std::fs::remove_dir_all(&target_dir)?;
    print!("{} Removed target/", "Cleaned".green().bold());
    if let Some(ref s) = size {
        println!(" ({s})");
    } else {
        println!();
    }
    Ok(())
}

// ── Helper functions ───────────────────────────────────────────────────────

/// Parse a dependency spec like "user/lib@^1.0" → ("user/lib", Some("^1.0"))
fn parse_dep_spec(spec: &str) -> (String, Option<String>) {
    if let Some(at_pos) = spec.find('@') {
        let name = spec[..at_pos].to_string();
        let version = spec[at_pos + 1..].to_string();
        (name, Some(version))
    } else {
        (spec.to_string(), None)
    }
}

/// Walk up from the current directory to find the project root (where hut.toml lives)
fn find_project_root() -> HutResult<PathBuf> {
    let cwd = std::env::current_dir()?;
    for ancestor in cwd.ancestors() {
        if ancestor.join("hut.toml").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(HutError::NotAProject)
}

// ── Embedded source constants ───────────────────────────────────────────────

const HELLO_WORLD_C: &str = "#include <stdio.h>\n\nint main() {\n    printf(\"Hello from {NAME}!\\n\");\n    return 0;\n}\n";

const LIB_HEADER: &str = "#ifndef MYLIB_H\n#define MYLIB_H\n\n// Public API\nint mylib_add(int a, int b);\nconst char* mylib_version(void);\n\n#endif // MYLIB_H\n";

const LIB_SOURCE: &str = "#include \"mylib.h\"\n\nint mylib_add(int a, int b) {\n    return a + b;\n}\n\nconst char* mylib_version(void) {\n    return \"0.1.0\";\n}\n";

const APP_MAIN_C: &str = "#include <stdio.h>\n\nint main(int argc, char** argv) {\n    printf(\"Hello, world!\\n\");\n    if (argc > 1) {\n        printf(\"Arguments: %d\\n\", argc - 1);\n        for (int i = 1; i < argc; i++) {\n            printf(\"  %s\\n\", argv[i]);\n        }\n    }\n    return 0;\n}\n";

const RAYLIB_GAME_C: &str = "#include \"raylib.h\"\n\nint main() {\n    const int screenWidth = 800;\n    const int screenHeight = 450;\n\n    InitWindow(screenWidth, screenHeight, \"raylib game — built with hut\");\n\n    SetTargetFPS(60);\n\n    while (!WindowShouldClose()) {\n        BeginDrawing();\n        ClearBackground(RAYWHITE);\n        DrawText(\"Hello, raylib!\", 190, 200, 20, LIGHTGRAY);\n        EndDrawing();\n    }\n\n    CloseWindow();\n    return 0;\n}\n";
