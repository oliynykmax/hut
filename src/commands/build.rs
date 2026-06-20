// ── cmd_build ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::Lockfile;

use crate::commands::{
    available_compilers, cache_dir,
    lockfile_path,
};

pub fn cmd_build(release: bool, compiler_override: Option<&str>) -> HutResult<()> {
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
                        if let Ok(idx) = trimmed.parse::<usize>()
                            && idx >= 1
                            && idx <= available.len()
                        {
                            let chosen = &available[idx - 1];
                            config.build.compiler = chosen.clone();
                            println!(
                                "{} Selected {} → saved to hut.toml",
                                "✓".green().bold(),
                                chosen.bold()
                            );
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
        let index = hut::index::PackagesIndex::load_builtin()?;
        hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache_dir())?
    };

    // ── Build the project ────────────────────────────────────────────────
    hut::builder::build_project(&config, &resolved, release)?;

    Ok(())
}
