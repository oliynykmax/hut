// ── cmd_link, cmd_unlink ──────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use crate::commands::packages_dir;
use hut::config::HutConfig;
use hut::error::{HutError, HutResult};

pub fn cmd_link(path: Option<&str>) -> HutResult<()> {
    let link_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let link_path = std::fs::canonicalize(&link_path)
        .map_err(|_| HutError::Other(format!("Path not found: {}", link_path.display())))?;

    // Read the package name from its hut.toml
    let hut_toml = link_path.join("hut.toml");
    if !hut_toml.exists() {
        return Err(HutError::Other(format!(
            "No hut.toml found in {} — is it a hut package?",
            link_path.display()
        )));
    }

    let pkg_config = HutConfig::load(&hut_toml)?;
    let pkg_name = &pkg_config.package.name;

    // Create symlink in ~/.hut/packages/<name>/linked
    let link_target = packages_dir().join(pkg_name).join("linked");
    if let Some(parent) = link_target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove old link if it exists
    if link_target.exists() || link_target.is_symlink() {
        let _ = std::fs::remove_file(&link_target);
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_path, &link_target)?;
    }
    #[cfg(not(unix))]
    {
        // On non-Unix, store the path in a file
        std::fs::write(&link_target, link_path.to_string_lossy().as_bytes())?;
    }

    println!(
        "{} {} → {}",
        "Linked".green().bold(),
        pkg_name.bold(),
        link_target.display().to_string().dimmed()
    );

    Ok(())
}

/// 13. `hut unlink <pkg>`

pub fn cmd_unlink(pkg: &str) -> HutResult<()> {
    let link_target = packages_dir().join(pkg).join("linked");

    if !link_target.exists() && !link_target.is_symlink() {
        eprintln!(
            "{} '{}' is not currently linked.",
            "info:".yellow().bold(),
            pkg.yellow()
        );
        return Ok(());
    }

    std::fs::remove_file(&link_target)?;
    println!("{} {} unlinked.", "Unlinked".green().bold(), pkg.bold());

    Ok(())
}
