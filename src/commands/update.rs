// ── cmd_update ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::HutResult;
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    cache_dir,
    lockfile_path,
};

pub fn cmd_update(pkg: Option<&str>) -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;
    let index = hut::index::PackagesIndex::load_builtin()?;

    let to_update: Vec<String> = if let Some(target) = pkg {
        if !config.dependencies.contains_key(target)
            && !config.build_dependencies.contains_key(target)
            && !config.test_dependencies.contains_key(target)
        {
            eprintln!(
                "{} '{}' is not a dependency of this project.",
                "error:".red().bold(),
                target.yellow()
            );
            return Ok(());
        }
        vec![target.to_string()]
    } else {
        config
            .dependencies
            .keys()
            .chain(config.build_dependencies.keys())
            .chain(config.test_dependencies.keys())
            .cloned()
            .collect()
    };

    if to_update.is_empty() {
        println!("{}", "No dependencies to update.".dimmed());
        return Ok(());
    }

    println!(
        "{} {} package(s)...",
        "Updating".bold().cyan(),
        to_update.len().to_string().bold()
    );

    // Remove them from lockfile so the resolver picks the latest
    for name in &to_update {
        lockfile.remove(name);
    }

    // Re-resolve
    let resolved = hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache_dir())?;

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

    println!("{} dependencies updated.", "Updated".green().bold());

    Ok(())
}
