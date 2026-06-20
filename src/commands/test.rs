// ── cmd_test ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::HutResult;
use hut::lockfile::Lockfile;

use crate::commands::{
    cache_dir,
    lockfile_path,
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
