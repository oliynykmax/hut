// ── cmd_publish, cmd_pm ──────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use crate::cli::{PmCommand, WorkspaceCommand};
use crate::commands::{cache_dir, find_project_root, hut_home, lockfile_path, packages_dir};
use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

pub fn cmd_publish() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;

    println!("{}", "Publishing guide:".bold().underline());
    println!();
    println!(
        "  Package: {} v{}",
        config.package.name.bold(),
        config.package.version
    );
    println!();
    println!("To make your package installable with hut:");
    println!();
    println!("  1. Push your code to GitHub.");
    println!("  2. Add a hut.toml with [package] metadata.");
    println!("  3. Users can install it by adding your package to");
    println!("     ~/.config/hut/packages.toml:");
    println!();
    println!("     [packages.{}]", config.package.name);
    println!("     repo = \"yourgithub/{}", config.package.name);
    println!("     includes = [\"include\"]");
    println!();

    Ok(())
}

/// 15. `hut pm <subcommand>`

pub fn cmd_pm(sub: PmCommand) -> HutResult<()> {
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
                    if path.is_dir()
                        && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    {
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
