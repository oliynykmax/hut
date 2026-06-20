// ── cmd_dev ──────────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_dev() -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;

    println!("{}", "Dev mode (watch + rebuild)".bold().underline());
    println!();
    println!("  Watching for file changes...");

    // Build once
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;

    // Only resolve deps if there are any
    let resolved = if config.dependencies.is_empty()
        && config.build_dependencies.is_empty()
        && config.test_dependencies.is_empty()
    {
        vec![]
    } else {
        let index = hut::index::PackagesIndex::load_builtin()?;
        hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache_dir())?
    };

    hut::builder::build_project(&config, &resolved, false)?;

    println!();
    println!(
        "{} Watching for changes (press Ctrl+C to stop)...",
        "👀".bold()
    );

    // Simple polling watch loop
    use std::time::{Duration, SystemTime};
    let project_root = find_project_root()?;
    let mut last_build = SystemTime::now();

    loop {
        std::thread::sleep(Duration::from_secs(1));

        let mut files_changed = false;
        let walker = walkdir::WalkDir::new(&project_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                matches!(ext, "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx")
            });

        for entry in walker {
            if let Ok(meta) = entry.metadata()
                && let Ok(mod_time) = meta.modified()
                && mod_time > last_build
            {
                files_changed = true;
                break;
            }
        }

        if files_changed {
            println!();
            println!("{} File changed, rebuilding...", "⚡".yellow().bold());
            last_build = SystemTime::now();

            match hut::builder::build_project(&config, &resolved, false) {
                Ok(()) => {
                    println!("{} Build succeeded.", "✓".green().bold());
                }
                Err(e) => {
                    eprintln!("{} Build failed: {}", "✗".red().bold(), e);
                }
            }
        }
    }
}
