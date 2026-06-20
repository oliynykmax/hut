// ── cmd_add ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_add(pkgs: &[String], dev: bool, build: bool) -> HutResult<()> {
    let (mut config, config_path) = HutConfig::find()?;
    let index = hut::index::PackagesIndex::load_builtin()?;

    let target_map = if dev {
        &mut config.test_dependencies
    } else if build {
        &mut config.build_dependencies
    } else {
        &mut config.dependencies
    };

    let dep_type = if dev {
        "dev"
    } else if build {
        "build"
    } else {
        ""
    };

    // Add all packages to hut.toml (validation + config update)
    let mut errors = Vec::new();
    let mut added: Vec<String> = Vec::new();
    for name in pkgs {
        let name = name.trim();
        if target_map.contains_key(name) {
            println!(
                "{} {} is already a dependency. Use `hut update` to change it.",
                "info:".cyan().bold(),
                name.bold()
            );
            continue;
        }
        if index.find(name).is_none() {
            eprintln!(
                "{} Package '{}' not found in packages.toml.\n       Add it to ~/.config/hut/packages.toml or use a supported package.",
                "error:".red().bold(),
                name
            );
            errors.push(name.to_string());
            continue;
        }
        target_map.insert(name.to_string(), "latest".to_string());
        println!(
            "{} {} {} → hut.toml",
            "Added".green().bold(),
            name.bold(),
            dep_type.dimmed()
        );
        added.push(name.to_string());
    }

    if !errors.is_empty() {
        return Err(HutError::Other(format!(
            "packages not found: {}",
            errors.join(", ")
        )));
    }

    if added.is_empty() {
        return Ok(());
    }

    config.save(&config_path)?;

    // Resolve and install once for all dependencies
    let lock_path = lockfile_path();
    let mut lockfile = Lockfile::load(&lock_path)?;
    let cache = cache_dir();

    println!("{} dependencies...", "Resolving".bold().cyan());
    let resolved = hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache)?;

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

    let pkg_list: Vec<_> = pkgs.iter().map(|s| s.trim()).collect();
    println!(
        "{} installed {}",
        "Done".green().bold(),
        pkg_list.join(", ").bold()
    );
    Ok(())
}
