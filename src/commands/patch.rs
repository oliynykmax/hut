// ── cmd_patch ──────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use crate::cli::{PmCommand, WorkspaceCommand};
use crate::commands::{cache_dir, find_project_root, hut_home, lockfile_path, packages_dir};
use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

pub fn cmd_patch(pkg: &str) -> HutResult<()> {
    let (_config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;

    let locked = lockfile.get(pkg).ok_or_else(|| {
        HutError::Other(format!(
            "'{pkg}' is not in the lockfile. Run `hut install` first."
        ))
    })?;

    let _cache = cache_dir();
    let (_pkg, pkg_dir) =
        hut::fetcher::fetch_package_metadata(pkg, &locked.resolved, &locked.version)?;

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
    println!("  1. Make your changes in: {}", pkg_dir.display());
    println!("  2. To use the patched version, run:");
    println!(
        "     {}",
        format!("hut link {}", pkg_dir.display()).dimmed()
    );

    Ok(())
}
