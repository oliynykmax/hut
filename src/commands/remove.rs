// ── cmd_remove ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::HutResult;
use hut::lockfile::Lockfile;

use crate::commands::lockfile_path;

pub fn cmd_remove(pkg: &str) -> HutResult<()> {
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
