// ── cmd_install ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_install() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;

    if config.dependencies.is_empty()
        && config.build_dependencies.is_empty()
        && config.test_dependencies.is_empty()
    {
        println!("{}", "No dependencies to install.".dimmed());
        return Ok(());
    }

    let index = hut::index::PackagesIndex::load_builtin()?;
    let cache = cache_dir();

    // Resolve all dependencies using local index
    println!("{} dependencies...", "Resolving".bold().cyan());
    let resolved = hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache)?;

    // Update lockfile with resolved entries
    for dep in &resolved {
        let locked = LockedPackage {
            name: dep.name.clone(),
            version: dep.version.clone(),
            source: dep.package.repository.clone().unwrap_or_default(),
            integrity: String::new(),
            resolved: dep.package.repository.clone().unwrap_or_default(),
            dependencies: dep.package.dependencies.clone(),
        };
        lockfile.insert(locked);
    }
    lockfile.save(&lock_path)?;

    println!(
        "{} {} packages resolved",
        "Resolved".green().bold(),
        resolved.len().to_string().bold()
    );

    // Packages are already fetched by the resolver; nothing more to do.
    Ok(())
}
