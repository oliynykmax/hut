// ── cmd_search ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_search(query: &str) -> HutResult<()> {
    let index = hut::index::PackagesIndex::load_builtin()?;
    let results = index.search(query);

    if results.is_empty() {
        println!("{} {}", "No packages found for".dimmed(), query.bold());
        println!(
            "{}",
            "Add custom packages to ~/.config/hut/packages.toml".dimmed()
        );
        return Ok(());
    }

    println!(
        "{} {} results for \"{}\":",
        "Found".green().bold(),
        results.len().to_string().bold(),
        query
    );
    println!();

    for (name, entry) in results {
        println!("  {} — {}", name.bold().cyan(), entry.description.dimmed());
        println!(
            "    repo: {}   includes: [{}]",
            entry.repo.dimmed(),
            entry.includes.join(", ").dimmed()
        );
        if !entry.libs.is_empty() {
            println!("    libs: [{}]", entry.libs.join(", ").dimmed());
        }
        println!();
    }

    Ok(())
}
