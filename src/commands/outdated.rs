// ── cmd_outdated ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::HutResult;
use hut::lockfile::Lockfile;

use crate::commands::lockfile_path;

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
