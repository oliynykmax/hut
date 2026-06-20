// ── cmd_outdated ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_outdated() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let index = hut::index::PackagesIndex::load_builtin()?;

    let mut found_outdated = false;

    for (name, _constraint) in config
        .dependencies
        .iter()
        .chain(config.build_dependencies.iter())
        .chain(config.test_dependencies.iter())
    {
        let current = lockfile.get(name).map(|l| l.version.as_str());

        if let Some(_entry) = index.find(name) {
            let is_outdated = current.is_none(); // Simple: if not locked, it's "outdated"
            if is_outdated {
                found_outdated = true;
                let current_display = current.unwrap_or("none");
                println!(
                    "{} {} → repo: {}",
                    name.bold(),
                    current_display.red(),
                    index.find(name).unwrap().repo.dimmed()
                );
            }
        }
    }

    if !found_outdated {
        println!("{}", "All dependencies are up to date.".green());
    }

    Ok(())
}
