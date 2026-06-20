// ── cmd_workspace ──────────────────────────────────────────────────────────

use std::path::{Path, PathBuf};
use std::process::Command;

use colored::Colorize;

use crate::cli::{PmCommand, WorkspaceCommand};
use crate::commands::{cache_dir, find_project_root, hut_home, lockfile_path, packages_dir};
use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

pub fn cmd_workspace(sub: WorkspaceCommand) -> HutResult<()> {
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
                println!("{} Already a workspace member.", "info:".cyan().bold());
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
