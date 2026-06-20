// ── cmd_init ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_init(name: Option<String>) -> HutResult<()> {
    use std::io::{BufRead, IsTerminal, Write};

    let project_name = name
        .as_deref()
        .filter(|n| *n != "." && *n != "./")
        .map(|n| n.to_string())
        .unwrap_or_else(|| {
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

    let config_path = project_dir.join("hut.toml");
    if config_path.exists() {
        eprintln!(
            "{} a hut.toml already exists in this directory",
            "warning:".yellow().bold()
        );
        return Ok(());
    }

    let mut config = HutConfig::default_template(&project_name);

    // ── Interactive prompts (TTY only; non-TTY keeps defaults: C, auto) ────
    if std::io::stdout().is_terminal() {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock();

        // Language
        println!();
        println!("{} Select language:", "→".dimmed());
        println!("  1) C  (default)");
        println!("  2) C++");
        print!("  choice [1]: ");
        std::io::stdout().flush().ok();

        let mut buf = String::new();
        let _ = lines.read_line(&mut buf);
        match buf.trim() {
            "2" => {
                config.package.language = "c++".to_string();
                if config.build.cpp_standard.is_none() {
                    config.build.cpp_standard = Some("c++17".to_string());
                }
            }
            _ => {} // default: C
        }

        // Compiler
        let available = available_compilers();
        println!();
        println!("{} Select compiler:", "→".dimmed());
        for (i, cc) in available.iter().enumerate() {
            println!("  {}) {}", i + 1, cc.bold());
        }
        println!("  a) auto (detect: {})", available.join("→").dimmed());
        print!("  choice [a]: ");
        std::io::stdout().flush().ok();

        buf.clear();
        let _ = lines.read_line(&mut buf);
        let choice = buf.trim();
        match choice.parse::<usize>() {
            Ok(n) if n >= 1 && n <= available.len() => {
                config.build.compiler = available[n - 1].clone();
            }
            _ => {} // default: auto
        }
        println!();
    }

    config.save(&config_path)?;
    println!("{} {}", "Created".green().bold(), config_path.display());

    // Create src/ directory and a hello-world main.c (or main.cpp)
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let is_cpp = config.package.language == "c++";
    let source_ext = if is_cpp { "cpp" } else { "c" };
    let source_path = src_dir.join(format!("main.{}", source_ext));

    if !source_path.exists() {
        let template = if is_cpp {
            HELLO_WORLD_CPP
        } else {
            HELLO_WORLD_C
        };
        let source = template.replace("{NAME}", &project_name);
        std::fs::write(&source_path, &source)?;
        println!(
            "{} src/main.{} (hello world)",
            "Created".green().bold(),
            source_ext
        );
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
