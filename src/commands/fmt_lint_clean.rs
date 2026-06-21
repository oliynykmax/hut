// ── cmd_fmt, cmd_lint, cmd_clean ──────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};

use crate::commands::{command_exists, find_project_root};

fn project_root_and_config() -> HutResult<(PathBuf, HutConfig)> {
    let cwd = std::env::current_dir()?;
    for ancestor in cwd.ancestors() {
        let path = ancestor.join("hut.toml");
        if path.exists() {
            let config = HutConfig::load(&path)?;
            return Ok((ancestor.to_path_buf(), config));
        }
    }
    let name = cwd.file_name().unwrap_or_default().to_string_lossy().to_string();
    Ok((cwd, HutConfig::default_template(&name)))
}

pub fn cmd_fmt(check: bool) -> HutResult<()> {
    if !command_exists("clang-format") {
        return Err(HutError::Other(
            "clang-format not found. Install it:\n  • Ubuntu/Debian:  sudo apt install clang-format\n  • macOS:          brew install clang-format\n  • Arch:           sudo pacman -S clang".into(),
        ));
    }

    let (project_root, config) = project_root_and_config()?;

    let sources =
        hut::builder::collect_sources(&config, &project_root).unwrap_or_else(|_| Vec::new());

    let mut files: Vec<PathBuf> = sources
        .into_iter()
        .filter(|f| {
            f.extension()
                .map(|e| {
                    e == "c"
                        || e == "h"
                        || e == "cpp"
                        || e == "hpp"
                        || e == "cc"
                        || e == "cxx"
                        || e == "hxx"
                })
                .unwrap_or(false)
        })
        .collect();

    // Also format headers in include dirs
    for inc in &["include", "src"] {
        let dir = project_root.join(inc);
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            if ext == "h" || ext == "hpp" || ext == "hxx" {
                                files.push(path);
                            }
                        }
                    }
                }
            }
        }
    }

    files.sort();
    files.dedup();

    if files.is_empty() {
        println!("{} No C/C++ source files found.", "info:".dimmed());
        return Ok(());
    }

    if check {
        println!("{} Checking formatting...", "→".dimmed());
        let mut failed = Vec::new();
        for file in &files {
            let output = std::process::Command::new("clang-format")
                .args(["--dry-run", "--Werror"])
                .arg(file)
                .output()?;
            if !output.status.success() {
                failed.push(file.clone());
            }
        }
        if failed.is_empty() {
            println!(
                "{} All {} file(s) are properly formatted.",
                "✓".green(),
                files.len()
            );
        } else {
            for f in &failed {
                eprintln!("  {} {}", "M".red(), f.display());
            }
            return Err(HutError::Other(format!(
                "{} file(s) need formatting. Run `hut fmt` to fix.",
                failed.len()
            )));
        }
    } else {
        for file in &files {
            print!("{} {}", "fmt".green(), file.display());
            let output = std::process::Command::new("clang-format")
                .arg("-i")
                .arg(file)
                .output()?;
            if output.status.success() {
                println!();
            } else {
                println!("  {} failed", "✗".red());
            }
        }
        println!("{} Formatted {} file(s).", "✓".green(), files.len());
    }

    Ok(())
}

/// 24. `hut lint` — lint C/C++ source files

pub fn cmd_lint() -> HutResult<()> {
    let (project_root, config) = project_root_and_config()?;
    let compiler = config.build.compiler.as_str();

    let cc = match compiler {
        "gcc" | "auto" => {
            if command_exists("gcc") {
                "gcc"
            } else if command_exists("clang") {
                "clang"
            } else {
                return Err(HutError::Other(
                    "No C compiler found. Install gcc or clang.".into(),
                ));
            }
        }
        other => other,
    };

    let sources = hut::builder::collect_sources(&config, &project_root)?;

    // Try clang-tidy first, fall back to compiler warnings
    if command_exists("clang-tidy") {
        println!("{} Running clang-tidy...", "→".dimmed());
        for src in &sources {
            print!("  {} ", "lint".green());
            let status = std::process::Command::new("clang-tidy")
                .arg("--allow-no-checks")
                .arg(src)
                .arg("--")
                .arg("-std=c11")
                .status()?;
            if status.success() {
                println!("{}", src.display());
            } else {
                println!("{} {}", "✗".red(), src.display());
            }
        }
    } else {
        println!(
            "{} clang-tidy not found — using compiler warnings instead.",
            "info:".dimmed()
        );
        println!("   Install clang-tidy: sudo apt install clang-tidy");
        println!();

        for src in &sources {
            print!("  {} ", "lint".green());
            let output = std::process::Command::new(cc)
                .arg("-fsyntax-only")
                .arg("-Wall")
                .arg("-Wextra")
                .arg("-Wpedantic")
                .arg("-std=c11")
                .arg(src)
                .output()?;

            if output.status.success() {
                println!("{}", src.display());
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("{} {}", "✗".red(), src.display());
                for line in stderr.lines().take(10) {
                    eprintln!("    {}", line);
                }
            }
        }
    }

    println!("{} Linted {} source file(s).", "✓".green(), sources.len());
    Ok(())
}

/// 25. `hut clean` — remove build artifacts (target/)

pub fn cmd_clean() -> HutResult<()> {
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
