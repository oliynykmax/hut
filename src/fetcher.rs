use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;
use flate2::read::GzDecoder;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use tar::Archive;
use tempfile::TempDir;

use crate::config::HutConfig;
use crate::error::{HutError, HutResult};
use crate::package::Package;

// ── Constants ──────────────────────────────────────────────────────────────

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 500;

// ── Cache metadata ─────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct CacheMetadata {
    pub name: String,
    pub version: String,
    pub repo_url: String,
    pub integrity: String,
    pub fetched_at: u64,
}

// ── Cache path helpers ─────────────────────────────────────────────────────

pub fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".hut"))
        .join("hut")
        .join("cache")
}

pub fn get_default_cache_dir() -> PathBuf {
    default_cache_dir()
}

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

// ── Output helpers ─────────────────────────────────────────────────────────

fn status(label: &str, msg: &str) {
    eprintln!("{:>12} {}", label.bold(), msg);
}

// ── Integrity ──────────────────────────────────────────────────────────────

fn hash_directory(dir: &Path) -> HutResult<String> {
    let mut hasher = Sha256::new();
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            // Skip cache metadata and build artifacts.
            if entry.file_name() == ".hut-cache.json" {
                continue;
            }
            if entry.path().extension().map_or(false, |e| e == "a" || e == "o" || e == "so" || e == "dylib") {
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

fn verify_integrity(path: &Path, expected: &str) -> bool {
    match hash_directory(path) {
        Ok(actual) => actual == expected,
        Err(_) => false,
    }
}

// ── Retry wrapper ──────────────────────────────────────────────────────────

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
                    let delay = std::time::Duration::from_millis(
                        BASE_RETRY_DELAY_MS * 2u64.pow(attempt - 1),
                    );
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

fn git_clone(repo_url: &str, dest: &Path, version: &str) -> HutResult<()> {
    let mut args = vec!["clone", "--depth", "1"];
    if version != "main" {
        args.push("--branch");
        args.push(version);
    }
    let output = Command::new("git")
        .args(&args)
        .arg(repo_url)
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

/// List git tags from a remote repository.
pub fn git_ls_remote_tags(repo_url: &str) -> HutResult<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", repo_url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HutError::Other(format!("Failed to run git ls-remote: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HutError::Other(format!(
            "git ls-remote failed for {repo_url}: {stderr}"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut tags: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in stdout.lines() {
        // Format: "<sha>\trefs/tags/<tag-name>"
        if let Some(tag) = line.split('\t').nth(1) {
            let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
            // Skip dereferenced entries (peeled tags like "v1.0^{}")
            if tag.ends_with("^{}") {
                continue;
            }
            if seen.insert(tag.to_string()) {
                tags.push(tag.to_string());
            }
        }
    }

    Ok(tags)
}

// ── GitHub tarball download ────────────────────────────────────────────────

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

fn download_and_extract_tarball(url: &str, dest: &Path) -> HutResult<u64> {
    let tmp_tarball = tempfile::NamedTempFile::new().map_err(|e| HutError::Io(e))?;
    status("Downloading", "tarball...");
    crate::http::http_download(url, tmp_tarball.path())?;

    let meta = std::fs::metadata(tmp_tarball.path())?;
    let total = meta.len();
    status("Extracting", &format!("{}", dest.display()));

    let data = std::fs::read(tmp_tarball.path())?;
    let decoder = GzDecoder::new(&data[..]);
    let mut archive = Archive::new(decoder);
    let tmp = TempDir::new().map_err(|e| HutError::Io(e))?;
    archive.unpack(tmp.path())?;

    let entries: Vec<_> = std::fs::read_dir(tmp.path())?
        .filter_map(|e| e.ok())
        .collect();

    if entries.len() == 1 && entries[0].file_type().map_or(false, |t| t.is_dir()) {
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

#[allow(dead_code)]
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

// ── Package source fetch (for indexed packages) ────────────────────────────

/// Clone a GitHub repo at a specific version and return the path.
/// Does NOT require a hut.toml — this is for packages defined in packages.toml.
pub fn fetch_package_source(name: &str, repo_url: &str, version: &str) -> HutResult<PathBuf> {
    let cache_dir = default_cache_dir();
    let pkg_dir = package_cache_dir(&cache_dir, name, version);
    let meta_path = cache_meta_path(&cache_dir, name, version);

    // Check cache.
    if pkg_dir.exists() && meta_path.exists() {
        if let Ok(meta_data) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<CacheMetadata>(&meta_data) {
                if verify_integrity(&pkg_dir, &meta.integrity) {
                    status("Cached", &format!("{}@{}", name, version));
                    return Ok(pkg_dir);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&pkg_dir);
    }

    status("Cloning", &format!("{}@{} → {}", name, version, repo_url));

    if let Some(parent) = pkg_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let git_result = with_retry(
        || {
            if pkg_dir.exists() {
                let _ = std::fs::remove_dir_all(&pkg_dir);
            }
            git_clone(repo_url, &pkg_dir, version)?;
            Ok(())
        },
        &format!("git clone {repo_url}"),
    );

    match git_result {
        Ok(()) => {
            status("Cloned", &format!("{}@{}", name, version));
        }
        Err(_git_err) => {
            status("Falling back", "tarball download from GitHub");
            if pkg_dir.exists() {
                let _ = std::fs::remove_dir_all(&pkg_dir);
            }
            if let Some(parent) = pkg_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Some(tarball_url) = github_tarball_url(repo_url, version) {
                with_retry(
                    || {
                        download_and_extract_tarball(&tarball_url, &pkg_dir)?;
                        Ok(())
                    },
                    "GitHub tarball download",
                )?;
            } else {
                return Err(HutError::Other(format!(
                    "Failed to fetch {}@{}: not a GitHub URL and git clone failed",
                    name, version
                )));
            }
        }
    }

    // Write cache metadata.
    let integrity = hash_directory(&pkg_dir)?;
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
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;

    status(
        "Fetched",
        &format!("{}@{} (sha256:{})", name, version, &integrity[..12]),
    );
    Ok(pkg_dir)
}

// ── Tag-based version resolution ─────────────────────────────────────────

/// Strip common prefixes from a git tag to extract the semver portion.
/// e.g. "v1.2.3" → "1.2.3", "release-2.0.0" → "2.0.0", "lib-v3.0.0" → "3.0.0"
/// Two-part tags like "5.0" are normalized to "5.0.0".
pub fn strip_tag_prefix(tag: &str) -> String {
    let prefixes = ["v", "release-", "lib-", "lib-v", "version-", "ver-"];
    let result = tag;
    for prefix in &prefixes {
        if let Some(stripped) = result.strip_prefix(prefix) {
            // Only strip once
            if stripped.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                let s = stripped.to_string();
                // Normalize 2-part to 3-part semver (e.g. "5.0" → "5.0.0")
                let parts: Vec<&str> = s.split('.').collect();
                match parts.len() {
                    1 => return format!("{}.0.0", s),
                    2 => return format!("{}.0", s),
                    _ => return s,
                }
            }
        }
    }
    // Also normalize unprefixed tags
    let parts: Vec<&str> = result.split('.').collect();
    match parts.len() {
        1 if parts[0].chars().all(|c| c.is_ascii_digit()) => format!("{}.0.0", result),
        2 if parts[0].chars().all(|c| c.is_ascii_digit()) && parts[1].chars().all(|c| c.is_ascii_digit()) => format!("{}.0", result),
        _ => result.to_string(),
    }
}

/// Resolve the best version matching a semver constraint from remote git tags.
/// Returns the tag name (e.g. "v1.2.3") or "main" as fallback.
pub fn resolve_best_version(
    repo_url: &str,
    constraint: &Option<semver::VersionReq>,
) -> HutResult<String> {
    let tags = git_ls_remote_tags(repo_url)?;

    let mut versions: Vec<(semver::Version, String)> = Vec::new();

    for tag in &tags {
        let ver_str = strip_tag_prefix(tag);
        if let Ok(ver) = semver::Version::parse(&ver_str) {
            if constraint.as_ref().map_or(true, |c| c.matches(&ver)) {
                versions.push((ver, tag.clone()));
            }
        }
    }

    if versions.is_empty() {
        // No matching semver tag — fall back to "main" branch
        return Ok("main".to_string());
    }

    // Sort by version (highest first), then by tag name as tiebreaker
    versions.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(versions[0].1.clone())
}

// ── Package metadata fetch (for packages WITH hut.toml) ───────────────────

/// Clone a GitHub repo at a specific version and read its `hut.toml`.
/// Returns the Package and the path to the cloned directory.
/// Cached in `cache_dir/<name>/<version>/`.
pub fn fetch_package_metadata(
    name: &str,
    repo_url: &str,
    version: &str,
) -> HutResult<(Package, PathBuf)> {
    let cache_dir = default_cache_dir();
    let pkg_dir = package_cache_dir(&cache_dir, name, version);
    let meta_path = cache_meta_path(&cache_dir, name, version);

    // Check cache.
    if pkg_dir.join("hut.toml").exists() && meta_path.exists() {
        if let Ok(meta_data) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<CacheMetadata>(&meta_data) {
                if verify_integrity(&pkg_dir, &meta.integrity) {
                    status("Cached", &format!("{}@{}", name, version));
                    let manifest = std::fs::read_to_string(pkg_dir.join("hut.toml"))?;
                    let cfg: HutConfig = toml::from_str(&manifest)?;
                    let pkg = cfg_to_package(&cfg, repo_url);
                    return Ok((pkg, pkg_dir));
                }
            }
        }
        // Cache invalid — clear it.
        let _ = std::fs::remove_dir_all(&pkg_dir);
    }

    status("Cloning", &format!("{}@{} → {}", name, version, repo_url));

    if let Some(parent) = pkg_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let git_result = with_retry(
        || {
            if pkg_dir.exists() {
                let _ = std::fs::remove_dir_all(&pkg_dir);
            }
            git_clone(repo_url, &pkg_dir, version)?;
            Ok(())
        },
        &format!("git clone {repo_url}"),
    );

    match git_result {
        Ok(()) => {
            status("Cloned", &format!("{}@{}", name, version));
        }
        Err(_git_err) => {
            status("Falling back", "tarball download from GitHub");
            if pkg_dir.exists() {
                let _ = std::fs::remove_dir_all(&pkg_dir);
            }
            if let Some(parent) = pkg_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Some(tarball_url) = github_tarball_url(repo_url, version) {
                let _ = with_retry(
                    || {
                        download_and_extract_tarball(&tarball_url, &pkg_dir)?;
                        Ok(())
                    },
                    "GitHub tarball download",
                );
            } else {
                return Err(HutError::Other(format!(
                    "Failed to fetch {}@{}: not a GitHub URL and git clone failed",
                    name, version
                )));
            }
        }
    }

    // Verify hut.toml exists.
    let manifest_path = pkg_dir.join("hut.toml");
    if !manifest_path.exists() {
        let _ = std::fs::remove_dir_all(&pkg_dir);
        return Err(HutError::Other(format!(
            "No hut.toml found in {repo_url} at version {version}"
        )));
    }

    let manifest = std::fs::read_to_string(&manifest_path)?;
    let cfg: HutConfig = toml::from_str(&manifest)?;
    let pkg = cfg_to_package(&cfg, repo_url);

    // Write cache metadata.
    let integrity = hash_directory(&pkg_dir)?;
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
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;

    status(
        "Fetched",
        &format!("{}@{} (sha256:{})", name, version, &integrity[..12]),
    );
    Ok((pkg, pkg_dir))
}

fn cfg_to_package(cfg: &HutConfig, repo_url: &str) -> Package {
    Package {
        name: cfg.package.name.clone(),
        version: cfg.package.version.clone(),
        description: cfg.package.description.clone(),
        authors: cfg.package.authors.clone(),
        license: cfg.package.license.clone(),
        repository: Some(repo_url.to_string()),
        homepage: cfg.package.homepage.clone(),
        sources: cfg.package.sources.clone(),
        includes: cfg.package.includes.clone(),
        dependencies: cfg.dependencies.clone(),
        build_dependencies: cfg.build_dependencies.clone(),
        test_dependencies: cfg.test_dependencies.clone(),
        build: cfg.build.clone(),
        scripts: cfg.scripts.clone(),
        libraries: vec![],
        executables: vec![],
        tests: vec![],
        cflags: vec![],
        ldflags: vec![],
    }
}

// ── Install dependencies ───────────────────────────────────────────────────

/// Install all dependencies — they are already resolved and fetched to cache
/// by the resolver. This just validates they exist in cache.
pub fn install_dependencies(config: &HutConfig, cache_dir: &Path) -> HutResult<()> {
    let mut tasks: Vec<(&str, &Path)> = Vec::new();

    for name in config
        .dependencies
        .keys()
        .chain(config.build_dependencies.keys())
        .chain(config.test_dependencies.keys())
    {
        let name_leaked: &'static str = Box::leak(name.clone().into_boxed_str());
        let cd: &'static Path = Box::leak(Box::new(cache_dir.to_path_buf()));
        tasks.push((name_leaked, cd));
    }

    if tasks.is_empty() {
        return Ok(());
    }

    let results: Vec<HutResult<()>> = tasks
        .par_iter()
        .map(|(name, cd)| {
            let pkg_dir = crate::fetcher::package_cache_dir(cd, name, "latest");
            // Also try without version
            if !pkg_dir.join("hut.toml").exists() {
                // Check all subdirs
                let parent = cd.join(*name);
                if parent.exists() {
                    return Ok(());
                }
                Err(HutError::Other(format!(
                    "Package {} not found in cache. Run `hut install` first.",
                    name
                )))
            } else {
                Ok(())
            }
        })
        .collect();

    for result in results {
        result?;
    }

    Ok(())
}

/// Scan a directory for `.a` files and return their parent directories.
fn scan_lib_dirs(pkg_path: &Path) -> Vec<PathBuf> {
    let mut lib_dirs: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(pkg_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "a" {
                    if let Some(parent) = entry.path().parent() {
                        let canonical = parent.to_path_buf();
                        if !lib_dirs.contains(&canonical) {
                            lib_dirs.push(canonical);
                        }
                    }
                }
            }
        }
    }
    lib_dirs
}

/// Run the build command for a package in its source directory.
/// Scans for `.a` files afterwards and returns the directories containing them.
pub fn build_package_source(name: &str, pkg_path: &Path, build_cmd: &str) -> HutResult<Vec<PathBuf>> {
    use std::process::Command;

    status("Building", &format!("{} with: {}", name, build_cmd));

    let exit = Command::new("sh")
        .args(["-c", build_cmd])
        .current_dir(pkg_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| HutError::Other(format!("Failed to run build command for {name}: {e}")))?;

    if !exit.success() {
        return Err(HutError::Other(format!(
            "Build command failed for {name} (exit: {})",
            exit.code().unwrap_or(-1)
        )));
    }

    let lib_dirs = scan_lib_dirs(pkg_path);
    status("Built", &format!("{} ({} lib dir(s))", name, lib_dirs.len()));
    Ok(lib_dirs)
}

/// Auto-build a package by compiling its sources and archiving into a static library.
/// Used when `sources` is specified in packages.toml but no explicit `build` command.
pub fn auto_build_package_source(
    name: &str,
    pkg_path: &Path,
    sources: &[String],
    includes: &[String],
    defines: &[String],
    cflags: &[String],
) -> HutResult<Vec<PathBuf>> {
    use std::process::Command;

    // Expand source glob patterns
    let mut source_files: Vec<PathBuf> = Vec::new();
    for pattern in sources {
        let pattern_path = pkg_path.join(pattern);
        let pattern_str = pattern_path.to_string_lossy().to_string();
        match glob::glob(&pattern_str) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(path) = entry {
                        if !source_files.contains(&path) {
                            source_files.push(path);
                        }
                    }
                }
            }
            Err(_) => {
                // Treat as literal file path
                let path = pkg_path.join(pattern);
                if path.exists() {
                    source_files.push(path);
                }
            }
        }
    }

    if source_files.is_empty() {
        return Err(HutError::Other(format!(
            "No source files matched for {name}. Checked patterns: {}",
            sources.join(", ")
        )));
    }

    status("Compiling", &format!("{} ({} file(s))", name, source_files.len()));

    // Build include flags
    let mut inc_flags: Vec<String> = Vec::new();
    for inc in includes {
        let dir = pkg_path.join(inc);
        inc_flags.push(format!("-I{}", dir.display()));
    }

    // Build define flags
    let mut def_flags: Vec<String> = Vec::new();
    for def in defines {
        def_flags.push(format!("-D{}", def));
    }

    // Compile each source file
    let obj_dir = pkg_path.join(".hut-build");
    std::fs::create_dir_all(&obj_dir)?;

    let mut object_files: Vec<PathBuf> = Vec::new();
    for source in &source_files {
        let fname = source.file_name().unwrap_or_default().to_string_lossy().to_string();
        let obj_name = if let Some(stripped) = fname.strip_suffix(".c").or_else(|| fname.strip_suffix(".cpp")).or_else(|| fname.strip_suffix(".cc")).or_else(|| fname.strip_suffix(".cxx")) {
            format!("{}.o", stripped)
        } else {
            format!("{}.o", fname)
        };
        let obj_path = obj_dir.join(&obj_name);

        let is_cpp = source.extension().map_or(false, |e| e == "cpp" || e == "cc" || e == "cxx" || e == "c++");
        let compiler = if is_cpp { "c++" } else { "cc" };

        let mut cmd = Command::new(compiler);
        cmd.arg("-c");
        cmd.arg("-fPIC");
        for flag in &inc_flags {
            cmd.arg(flag);
        }
        for flag in &def_flags {
            cmd.arg(flag);
        }
        for flag in cflags {
            cmd.arg(flag);
        }
        cmd.arg(source);
        cmd.arg("-o");
        cmd.arg(&obj_path);

        let output = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| HutError::Other(format!("Failed to compile {} for {name}: {e}", source.display())))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(HutError::Other(format!(
                "Compilation of {} for {name} failed:\n{stderr}",
                source.display()
            )));
        }

        object_files.push(obj_path);
    }

    // Archive into static library
    let lib_name = format!("lib{name}.a");
    let lib_path = pkg_path.join(&lib_name);

    status("Archiving", &format!("{} → {}", name, lib_name));

    let mut ar_cmd = Command::new("ar");
    ar_cmd.arg("rcs");
    ar_cmd.arg(&lib_path);
    for obj in &object_files {
        ar_cmd.arg(obj);
    }

    let ar_output = ar_cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| HutError::Other(format!("Failed to archive {name}: {e}")))?;

    if !ar_output.status.success() {
        let stderr = String::from_utf8_lossy(&ar_output.stderr);
        return Err(HutError::Other(format!("Archiving of {name} failed:\n{stderr}")));
    }

    // Clean up object files
    let _ = std::fs::remove_dir_all(&obj_dir);

    let lib_dirs = scan_lib_dirs(pkg_path);
    status("Built", &format!("{} ({} lib dir(s))", name, lib_dirs.len()));
    Ok(lib_dirs)
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
    fn test_github_tarball_url() {
        let url = github_tarball_url("https://github.com/user/repo", "v1.0.0");
        assert!(url.is_some());
        let url = url.unwrap();
        assert!(url.contains("api.github.com"));
        assert!(url.contains("tarball"));
        assert!(url.contains("tags/1.0.0"));
    }
}
