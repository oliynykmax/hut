// ── cmd_test ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_test() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;

    // Reuse the builder — for now, just build the project
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let index = hut::index::PackagesIndex::load_builtin()?;
    let resolved = hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache_dir())?;

    hut::builder::build_project(&config, &resolved, false)?;

    println!();
    println!(
        "{} {}",
        "✓".green().bold(),
        "Build succeeded (test runner not yet implemented)".dimmed()
    );

    Ok(())
}
