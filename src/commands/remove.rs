// ── cmd_remove ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

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
