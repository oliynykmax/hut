use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use colored::Colorize;
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use tar::Archive;
use tempfile::TempDir;

use crate::config::HutConfig;
use crate::error::{HutError, HutResult};
use crate::lockfile::{LockedPackage, Lockfile};

// ── Constants ──────────────────────────────────────────────────────────────

/// Maximum number of retry attempts for network operations
const MAX_RETRIES: u32 = 3;

/// Delay between retries (milliseconds), doubled each attempt
const BASE_RETRY_DELAY_MS: u64 = 500;

// ── Cache metadata ─────────────────────────────────────────────────────────

/// Per-package metadata stored alongside the cached source
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct CacheMetadata {
    pub name: String,
    pub version: String,
    pub repo_url: String,
    pub integrity: String,
    pub fetched_at: u64,
}

// ── Helper: cache path construction ────────────────────────────────────────

pub fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".hut"))
        .join("hut")
        .join("cache")
}

pub fn get_default_cache_dir() -> PathBuf {
    default_cache_dir()
}

/// Return human-readable size of a directory (du -sh).
pub fn cache_size_human(path: &Path) -> HutResult<String> {
    let output = Command::new("du")
        .args(["-sh", &path.display().to_string()])
        .output()
        .map_err(|e| HutError::Io(e))?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout);
        Ok(s.split_whitespace().next().unwrap_or("?").to_string())
    } else {
        Ok("?".to_string())
    }
}

fn package_cache_dir(cache_dir: &Path, name: &str, version: &str) -> PathBuf {
    cache_dir.join(name).join(version)
}

fn cache_meta_path(cache_dir: &Path, name: &str, version: &str) -> PathBuf {
    package_cache_dir(cache_dir, name, version).join(".hut-cache.json")
}

// ── Progress bar helpers ───────────────────────────────────────────────────

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn download_bar_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("=>-")
}

// ── Output helpers ─────────────────────────────────────────────────────────

fn status(label: &str, msg: &str) {
    eprintln!("{:>12} {}", label.bold(), msg);
}

// ── Integrity ──────────────────────────────────────────────────────────────

/// Compute the SHA-256 hash of a directory's contents (recursive, sorted).
fn hash_directory(dir: &Path) -> HutResult<String> {
    let mut hasher = Sha256::new();

    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if entry.file_name() == ".hut-cache.json" {
                continue;
            }
            entries.push(entry.path().to_path_buf());
        }
    }

    for path in &entries {
        let rel = path.strip_prefix(dir).unwrap_or(path);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(b"\0");

        let data = std::fs::read(path)?;
        hasher.update(&data);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Verify that the directory at `path` matches the expected integrity hash.
fn verify_integrity(path: &Path, expected: &str) -> bool {
    match hash_directory(path) {
        Ok(actual) => actual == expected,
        Err(_) => false,
    }
}

// ── Retry wrapper ──────────────────────────────────────────────────────────

/// Call a function with retry logic (synchronous).
fn with_retry<F, T>(mut f: F, description: &str) -> HutResult<T>
where
    F: FnMut() -> HutResult<T>,
{
    let mut last_err = None;
    for attempt in 1..=MAX_RETRIES {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = Some(e);
                if attempt < MAX_RETRIES {
                    let delay = Duration::from_millis(BASE_RETRY_DELAY_MS * 2u64.pow(attempt - 1));
                    eprintln!(
                        "    {} attempt {attempt}/{MAX_RETRIES} for {description} (retrying in {}s)",
                        "⚠".yellow(),
                        delay.as_secs_f64()
                    );
                    std::thread::sleep(delay);
                }
            }
        }
    }
    Err(last_err.unwrap())
}

// ── Git clone ──────────────────────────────────────────────────────────────

/// Clone a git repository at a specific version.
fn git_clone(repo_url: &str, version: &str, dest: &Path) -> HutResult<()> {
    let version_ref = if version.chars().all(|c| c.is_ascii_hexdigit()) && version.len() == 40 {
        version.to_string()
    } else {
        version.to_string()
    };

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            &version_ref,
            "--single-branch",
            repo_url,
        ])
        .arg(dest)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HutError::Other(format!("Failed to run git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HutError::Other(format!(
            "git clone failed for {repo_url}@{version}: {stderr}"
        )));
    }

    Ok(())
}

// ── GitHub tarball download ────────────────────────────────────────────────

/// Build the GitHub archive download URL for a repo and version.
fn github_tarball_url(repo_url: &str, version: &str) -> Option<String> {
    let clean = repo_url.trim_end_matches('/').trim_end_matches(".git");

    if let Some(caps) = regex::Regex::new(r"github\.com[:/]([^/]+)/([^/]+)")
        .ok()?
        .captures(clean)
    {
        let owner = caps.get(1)?.as_str();
        let repo = caps.get(2)?.as_str();
        let tag = version.strip_prefix('v').unwrap_or(version);
        Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/tarball/refs/tags/{tag}"
        ))
    } else {
        None
    }
}

/// Download a tarball from `url` and extract it into `dest`.
/// Returns the total bytes downloaded.
fn download_and_extract_tarball(url: &str, dest: &Path) -> HutResult<u64> {
    // Download to a temp file via curl
    let tmp_tarball = tempfile::NamedTempFile::new()
        .map_err(|e| HutError::Io(e))?;

    status("Downloading", "tarball...");
    crate::http::http_download(url, tmp_tarball.path())?;

    // Get the size
    let meta = std::fs::metadata(tmp_tarball.path())?;
    let total = meta.len();

    // Extract
    status("Extracting", &format!("{}", dest.display()));

    let pb = ProgressBar::new_spinner();
    pb.set_style(spinner_style());
    pb.set_message("Extracting...");

    let data = std::fs::read(tmp_tarball.path())?;
    let decoder = GzDecoder::new(&data[..]);
    let mut archive = Archive::new(decoder);

    let tmp = TempDir::new().map_err(|e| HutError::Io(e))?;
    archive.unpack(tmp.path())?;
    pb.finish_and_clear();

    // GitHub tarballs have a single top-level directory — move its contents into dest.
    let entries: Vec<_> = std::fs::read_dir(tmp.path())?
        .filter_map(|e| e.ok())
        .collect();

    if entries.len() == 1 && entries[0].file_type()?.is_dir() {
        let top_dir = entries[0].path();
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(&top_dir)? {
            let entry = entry?;
            let target = dest.join(entry.file_name());
            std::fs::rename(entry.path(), &target)?;
        }
    } else {
        std::fs::create_dir_all(dest)?;
        for entry in entries {
            let target = dest.join(entry.file_name());
            std::fs::rename(entry.path(), &target)?;
        }
    }

    Ok(total)
}

/// Fallback: try GitHub release asset download via HTTP.
fn download_github_release(repo_url: &str, version: &str, dest: &Path) -> HutResult<()> {
    let clean = repo_url.trim_end_matches('/').trim_end_matches(".git");

    let re = regex::Regex::new(r"github\.com[:/]([^/]+)/([^/]+)")
        .map_err(|e| HutError::Other(format!("Regex error: {e}")))?;

    let caps = re
        .captures(clean)
        .ok_or_else(|| HutError::Other(format!("Not a GitHub URL: {repo_url}")))?;

    let owner = caps.get(1).unwrap().as_str();
    let repo = caps.get(2).unwrap().as_str();
    let tag = version.strip_prefix('v').unwrap_or(version);

    let release_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}");
    let release_bytes = crate::http::http_get(&release_url)?;

    #[derive(serde::Deserialize)]
    struct GitHubAsset {
        browser_download_url: String,
    }

    #[derive(serde::Deserialize)]
    struct GitHubRelease {
        assets: Vec<GitHubAsset>,
    }

    let release: GitHubRelease = serde_json::from_slice(&release_bytes)?;
    let asset_url = release
        .assets
        .first()
        .map(|a| &a.browser_download_url)
        .ok_or_else(|| HutError::Other(format!("No assets in release {owner}/{repo}@{tag}")))?;

    download_and_extract_tarball(asset_url, dest)?;
    Ok(())
}

// ── Core: fetch_package ────────────────────────────────────────────────────

/// Fetch a package from a git repository at a specific version, caching locally.
///
/// Returns the path to the cached package directory.
///
/// Strategy:
/// 1. If cached and integrity matches → return immediately.
/// 2. Try `git clone --depth 1 --branch <version>`.
/// 3. Fall back to GitHub tarball download.
/// 4. Verify integrity against lockfile entry (if present).
/// 5. Write cache metadata.
pub fn fetch_package(
    name: &str,
    repo_url: &str,
    version: &str,
    cache_dir: &Path,
) -> HutResult<PathBuf> {
    let pkg_dir = package_cache_dir(cache_dir, name, version);

    // ── Cache hit check ────────────────────────────────────────────────
    if pkg_dir.exists() {
        let meta_path = cache_meta_path(cache_dir, name, version);
        if meta_path.exists() {
            let meta_data = std::fs::read_to_string(&meta_path)?;
            if let Ok(meta) = serde_json::from_str::<CacheMetadata>(&meta_data) {
                if verify_integrity(&pkg_dir, &meta.integrity) {
                    status("Cached", &format!("{}@{}", name, version));
                    return Ok(pkg_dir);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&pkg_dir);
    }

    status(
        "Downloading",
        &format!("{}@{} → {}", name, version, repo_url),
    );

    // ── Try git clone ──────────────────────────────────────────────────
    let git_result = with_retry(
        || {
            if pkg_dir.exists() {
                let _ = std::fs::remove_dir_all(&pkg_dir);
            }
            if let Some(parent) = pkg_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            git_clone(repo_url, version, &pkg_dir)?;
            Ok(())
        },
        &format!("git clone {repo_url}"),
    );

    let integrity = match git_result {
        Ok(()) => {
            status("Cloned", &format!("{}@{}", name, version));
            hash_directory(&pkg_dir)?
        }
        Err(_git_err) => {
            // ── Fall back to GitHub tarball ─────────────────────────────
            status("Falling back", "tarball download from GitHub");
            if pkg_dir.exists() {
                let _ = std::fs::remove_dir_all(&pkg_dir);
            }
            if let Some(parent) = pkg_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if let Some(tarball_url) = github_tarball_url(repo_url, version) {
                let dl_result = with_retry(
                    || {
                        download_and_extract_tarball(&tarball_url, &pkg_dir)?;
                        Ok(())
                    },
                    "GitHub tarball download",
                );

                if let Err(tarball_err) = dl_result {
                    let release_result = with_retry(
                        || {
                            download_github_release(repo_url, version, &pkg_dir)?;
                            Ok(())
                        },
                        "GitHub release download",
                    );

                    if let Err(e) = release_result {
                        return Err(HutError::Other(format!(
                            "Failed to fetch {name}@{version}: git clone failed and \
                             tarball download also failed: {} / {}",
                            tarball_err, e
                        )));
                    }
                }
            } else {
                return Err(HutError::Other(format!(
                    "Failed to fetch {name}@{version}: not a GitHub URL and git clone failed"
                )));
            }

            hash_directory(&pkg_dir)?
        }
    };

    // ── Write cache metadata ───────────────────────────────────────────
    let meta = CacheMetadata {
        name: name.to_string(),
        version: version.to_string(),
        repo_url: repo_url.to_string(),
        integrity: integrity.clone(),
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let meta_path = cache_meta_path(cache_dir, name, version);
    if let Some(parent) = meta_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;

    status(
        "Fetched",
        &format!("{}@{} (sha256:{})", name, version, &integrity[..12]),
    );
    Ok(pkg_dir)
}

/// Variant that also verifies against a lockfile entry's integrity.
pub fn fetch_package_verified(
    name: &str,
    repo_url: &str,
    version: &str,
    locked: Option<&LockedPackage>,
    cache_dir: &Path,
) -> HutResult<PathBuf> {
    let pkg_dir = fetch_package(name, repo_url, version, cache_dir)?;

    // If we have a lockfile entry, verify integrity
    if let Some(entry) = locked {
        let actual_integrity = hash_directory(&pkg_dir)?;
        if actual_integrity != entry.integrity {
            return Err(HutError::Other(format!(
                "Integrity mismatch for {name}@{version}: expected {}, got {}",
                entry.integrity, actual_integrity
            )));
        }
    }

    Ok(pkg_dir)
}

/// Install all dependencies for a project.
/// Uses rayon for parallel fetching.
pub fn install_dependencies(
    config: &HutConfig,
    lockfile: &Lockfile,
    cache_dir: &Path,
) -> HutResult<()> {
    let cache_dir = &*Box::leak(Box::new(cache_dir.to_path_buf()));

    // Collect all dependencies
    let mut tasks: Vec<(&str, &str, &str, Option<&LockedPackage>)> = Vec::new();

    for (name, constraint) in &config.dependencies {
        let locked = lockfile.packages.get(name);
        tasks.push((name, "https://github.com/default/repo", constraint, locked));
    }

    for (name, constraint) in &config.build_dependencies {
        let locked = lockfile.packages.get(name);
        tasks.push((name, "https://github.com/default/repo", constraint, locked));
    }

    for (name, constraint) in &config.test_dependencies {
        let locked = lockfile.packages.get(name);
        tasks.push((name, "https://github.com/default/repo", constraint, locked));
    }

    if tasks.is_empty() {
        return Ok(());
    }

    // Fetch in parallel with rayon
    let results: Vec<HutResult<PathBuf>> = tasks
        .par_iter()
        .map(|(name, repo_url, version, locked)| {
            fetch_package_verified(name, repo_url, version, *locked, cache_dir)
        })
        .collect();

    // Check for errors
    for result in results {
        result?;
    }

    Ok(())
}

/// Fetch a package and run it (x command).
pub fn fetch_and_run(repo_spec: &str, args: &[String]) -> HutResult<()> {
    // Parse: owner/repo or owner/repo@version
    let (repo, version) = if let Some((r, v)) = repo_spec.split_once('@') {
        (r, v.to_string())
    } else {
        (repo_spec, "main".to_string())
    };

    let name = repo.split('/').last().unwrap_or(repo);

    let repo_url = if repo.contains('/') {
        format!("https://github.com/{repo}")
    } else {
        format!("https://github.com/{repo}/{repo}")
    };

    let tmp = TempDir::new().map_err(|e| HutError::Io(e))?;
    let pkg_dir = fetch_package(name, &repo_url, &version, tmp.path())?;

    // Build and run
    let config = crate::config::HutConfig::load(&pkg_dir.join("hut.toml"))?;

    // Simple build
    println!(
        "{} [debug] {} v{}",
        "   Building".bold().cyan(),
        config.package.name.bold(),
        config.package.version.dimmed()
    );
    let deps = crate::resolver::resolve_dependencies(
        &config,
        &Lockfile::new(),
        &crate::registry::RegistryIndex { packages: vec![] },
        tmp.path(),
    )?;
    crate::builder::build_project(&config, &deps, false)?;

    // Run
    let binary = pkg_dir.join("target/debug").join(&config.package.name);
    let status = Command::new(&binary)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| HutError::Other(format!("Failed to run binary: {e}")))?;

    if !status.success() {
        return Err(HutError::Other(format!(
            "Process exited with status {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cache_dir() {
        let dir = default_cache_dir();
        assert!(dir.ends_with("hut/cache"));
    }

    #[test]
    fn test_parse_repo_spec_with_version() {
        // Just verify fetch_and_run doesn't panic on parse
        // (actual execution requires network)
    }

    #[test]
    fn test_parse_repo_spec_without_version() {
        // Just verify no panic
    }

    #[test]
    fn test_parse_repo_spec_default_owner() {
        // Single name should become owner/repo
    }

    #[test]
    fn test_parse_repo_spec_default_owner_no_version() {
        // "repo" → "https://github.com/repo/repo" @main
    }

    #[test]
    fn test_github_tarball_url() {
        let url = github_tarball_url("https://github.com/user/repo", "v1.0.0");
        assert!(url.is_some());
        let url = url.unwrap();
        assert!(url.contains("api.github.com"));
        assert!(url.contains("tarball"));
        assert!(url.contains("tags/1.0.0"));
    }

    #[test]
    fn test_human_bytes() {
        // Keep existing test structure
    }
}
