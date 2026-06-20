// ── cmd_upgrade ──────────────────────────────────────────────────────────────

use std::path::{Path, PathBuf};

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

use crate::commands::{
    HELLO_WORLD_C, HELLO_WORLD_CPP, available_compilers, cache_dir, find_project_root, hut_home,
    lockfile_path, packages_dir,
};

pub fn cmd_upgrade() -> HutResult<()> {
    use std::process::Command;

    let current_version = env!("CARGO_PKG_VERSION");

    // Find hut's source directory
    let source_dir = find_hut_source();

    let Some(src_dir) = source_dir else {
        eprintln!(
            "{} Could not find hut's git source directory.",
            "error:".red().bold()
        );
        eprintln!();
        eprintln!("  hut upgrade needs the source repo to git pull + rebuild.");
        eprintln!("  Clone it once and hut will self-update from there:");
        eprintln!();
        eprintln!(
            "    {}",
            "git clone git@github.com:oliynykmax/hut.git ~/.hut".dimmed()
        );
        eprintln!("    {}", "cd ~/.hut && cargo build --release".dimmed());
        eprintln!("    {}", "cp target/release/hut ~/.local/bin/".dimmed());
        return Err(HutError::Other(
            "hut source directory not found. Clone to ~/.hut for self-update support.".into(),
        ));
    };

    println!("→ Pulling latest changes...");

    // git pull will fail if the working tree is dirty (e.g., Cargo.lock touched by
    // a previous cargo build). Stash local changes, then reset completely.
    let stash = Command::new("git")
        .args([
            "-C",
            src_dir.to_str().unwrap(),
            "stash",
            "--include-untracked",
        ])
        .output();

    let fetch = Command::new("git")
        .args(["-C", src_dir.to_str().unwrap(), "fetch", "origin"])
        .output()?;

    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        return Err(HutError::Other(format!(
            "git fetch failed: {}",
            stderr.trim()
        )));
    }

    let reset = Command::new("git")
        .args([
            "-C",
            src_dir.to_str().unwrap(),
            "reset",
            "--hard",
            "origin/main",
        ])
        .output()?;

    if !reset.status.success() {
        let stderr = String::from_utf8_lossy(&reset.stderr);
        return Err(HutError::Other(format!(
            "git reset failed: {}",
            stderr.trim()
        )));
    }

    // Clean any leftover untracked files (build artifacts outside target/)
    let _ = Command::new("git")
        .args(["-C", src_dir.to_str().unwrap(), "clean", "-fd"])
        .output();

    // If we stashed, drop the stash (it's just build artifacts, not user edits)
    if let Ok(stash) = stash {
        if stash.status.success() {
            let _ = Command::new("git")
                .args([
                    "-C",
                    src_dir.to_str().unwrap(),
                    "stash",
                    "drop",
                    "stash@{0}",
                ])
                .output();
        }
    }

    let new_version = get_hut_version(&src_dir)?;

    if new_version == current_version {
        println!(
            "{} hut v{} is already the latest version",
            "✓".green(),
            current_version
        );
        return Ok(());
    }

    println!("{} Building hut v{}...", "→".dimmed(), new_version);
    let build = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(src_dir.join("Cargo.toml"))
        .output()?;

    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr);
        return Err(HutError::Other(format!("build failed: {}", stderr.trim())));
    }

    // Replace the current binary atomically.
    // Can't copy over a running binary — use rename (Linux allows it).
    let current_exe = std::env::current_exe()?;
    let new_binary = src_dir.join("target/release/hut");
    let tmp_path = current_exe.with_extension("tmp");

    std::fs::copy(&new_binary, &tmp_path)?;
    std::fs::rename(&tmp_path, &current_exe)?;

    println!(
        "{} hut upgraded from v{} to v{}",
        "✓".green(),
        current_version,
        new_version
    );

    // Reseed ~/.config/hut/packages.toml with new entries from
    // the just-built binary's embedded index.
    let _ = hut::index::PackagesIndex::reseed_user_index();
    Ok(())
}

/// Try to find hut's source directory by checking common locations.
fn find_hut_source() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join(".hut"), // default git clone location
        PathBuf::from("/usr/local/lib/hut"),
        home.join("hut"), // cloned as ~/hut
        std::env::current_dir().ok()?,
    ];

    for cand in &candidates {
        if cand.join("Cargo.toml").exists() && cand.join(".git").exists() {
            return Some(cand.clone());
        }
    }
    None
}

/// Read the version string from a hut checkout's Cargo.toml.
fn get_hut_version(source_dir: &Path) -> HutResult<String> {
    let cargo_toml = source_dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)?;
    for line in content.lines() {
        if let Some(ver) = line.trim().strip_prefix("version = \"")
            && let Some(end) = ver.find('"')
        {
            return Ok(ver[..end].to_string());
        }
    }
    Err(HutError::Other(
        "could not parse version from Cargo.toml".into(),
    ))
}
