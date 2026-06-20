use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use colored::Colorize;
use flate2::read::GzDecoder;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
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

fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".hut"))
        .join("hut")
        .join("cache")
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
///
/// We walk the directory in sorted order so the hash is deterministic.
fn hash_directory(dir: &Path) -> HutResult<String> {
    let mut hasher = Sha256::new();

    // Collect all entries (sorted for determinism)
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            // Skip the cache metadata file itself from the hash
            if entry.file_name() == ".hut-cache.json" {
                continue;
            }
            entries.push(entry.path().to_path_buf());
        }
    }

    // entries are already sorted by WalkDir::sort_by_file_name()
    for path in &entries {
        // Hash the relative path
        let rel = path.strip_prefix(dir).unwrap_or(path);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(b"\0");

        // Hash file contents
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

/// Call an async function with retry logic.
async fn with_retry<F, Fut, T>(mut f: F, description: &str) -> HutResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = HutResult<T>>,
{
    let mut last_err = None;
    for attempt in 1..=MAX_RETRIES {
        match f().await {
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
                    tokio::time::sleep(delay).await;
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
        // Looks like a full SHA — use directly
        version.to_string()
    } else {
        // Tag or branch
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
    // Match GitHub URLs: https://github.com/owner/repo or git@github.com:owner/repo.git
    let clean = repo_url.trim_end_matches('/').trim_end_matches(".git");

    if let Some(caps) = regex::Regex::new(r"github\.com[:/]([^/]+)/([^/]+)")
        .ok()?
        .captures(clean)
    {
        let owner = caps.get(1)?.as_str();
        let repo = caps.get(2)?.as_str();
        // Try the tag-based tarball; GitHub also accepts refs/heads/... but tags cover
        // most common version patterns.
        let tag = version.strip_prefix('v').unwrap_or(version);
        Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/tarball/refs/tags/{tag}"
        ))
    } else {
        None
    }
}

/// Download a tarball from `url` and extract it into `dest`.
///
/// Returns the total bytes downloaded.
async fn download_and_extract_tarball(url: &str, dest: &Path) -> HutResult<u64> {
    // Download
    let client = reqwest::Client::builder()
        .user_agent("hut-pm/0.1.0")
        .timeout(Duration::from_secs(300))
        .build()?;

    // We also accept non-redirected API responses (GitHub API redirects to codeload)
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        return Err(HutError::Other(format!(
            "HTTP {} downloading {}",
            resp.status(),
            url
        )));
    }

    let total = resp.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total);
    pb.set_style(download_bar_style());
    pb.set_message("Downloading tarball");

    let mut downloaded: u64 = 0;
    let mut data = Vec::new();

    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        data.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }
    pb.finish_and_clear();

    // Extract
    status("Extracting", &format!("{}", dest.display()));

    let pb = ProgressBar::new_spinner();
    pb.set_style(spinner_style());
    pb.set_message("Extracting...");

    let decoder = GzDecoder::new(&data[..]);
    let mut archive = Archive::new(decoder);

    // GitHub wraps in a top-level directory; we strip it.
    // Extract to a temp location first, then move contents up one level.
    let tmp = TempDir::new().map_err(|e| HutError::Io(e))?;

    archive.unpack(tmp.path())?;
    pb.finish_and_clear();

    // GitHub tarballs have a single top-level directory — move its contents into dest.
    let entries: Vec<_> = std::fs::read_dir(tmp.path())?
        .filter_map(|e| e.ok())
        .collect();

    if entries.len() == 1 && entries[0].file_type()?.is_dir() {
        // Single directory — move its contents
        let top_dir = entries[0].path();
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(&top_dir)? {
            let entry = entry?;
            let target = dest.join(entry.file_name());
            std::fs::rename(entry.path(), &target)?;
        }
    } else {
        // Multiple entries or files — move everything
        std::fs::create_dir_all(dest)?;
        for entry in entries {
            let target = dest.join(entry.file_name());
            std::fs::rename(entry.path(), &target)?;
        }
    }

    Ok(downloaded)
}

/// Fallback: try GitHub release asset download (application/octet-stream zip/tar.gz).
async fn download_github_release(repo_url: &str, version: &str, dest: &Path) -> HutResult<()> {
    // Match GitHub URLs
    let clean = repo_url.trim_end_matches('/').trim_end_matches(".git");

    let re = regex::Regex::new(r"github\.com[:/]([^/]+)/([^/]+)")
        .map_err(|e| HutError::Other(format!("Regex error: {e}")))?;

    let caps = re
        .captures(clean)
        .ok_or_else(|| HutError::Other(format!("Not a GitHub URL: {repo_url}")))?;

    let owner = caps.get(1).unwrap().as_str();
    let repo = caps.get(2).unwrap().as_str();
    let tag = version.strip_prefix('v').unwrap_or(version);

    // Try the release API
    let release_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}");
    let client = reqwest::Client::builder()
        .user_agent("hut-pm/0.1.0")
        .timeout(Duration::from_secs(300))
        .build()?;

    let resp = client.get(&release_url).send().await?;
    if !resp.status().is_success() {
        return Err(HutError::Other(format!(
            "No release found for {owner}/{repo}@{tag}"
        )));
    }

    #[derive(serde::Deserialize)]
    struct GitHubAsset {
        browser_download_url: String,
    }

    #[derive(serde::Deserialize)]
    struct GitHubRelease {
        assets: Vec<GitHubAsset>,
    }

    let release_bytes = resp.bytes().await?;
    let release: GitHubRelease = serde_json::from_slice(&release_bytes)?;
    let asset_url = release
        .assets
        .first()
        .map(|a| &a.browser_download_url)
        .ok_or_else(|| HutError::Other(format!("No assets in release {owner}/{repo}@{tag}")))?;

    download_and_extract_tarball(asset_url, dest).await?;
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
pub async fn fetch_package(
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
        // Stale cache — remove it
        let _ = std::fs::remove_dir_all(&pkg_dir);
    }

    status(
        "Downloading",
        &format!("{}@{} → {}", name, version, repo_url),
    );

    // ── Try git clone ──────────────────────────────────────────────────
    let git_result = with_retry(
        || {
            let repo_url = repo_url.to_string();
            let version = version.to_string();
            let pkg_dir = pkg_dir.clone();
            async move {
                // Clean up any partial state
                if pkg_dir.exists() {
                    let _ = std::fs::remove_dir_all(&pkg_dir);
                }
                if let Some(parent) = pkg_dir.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                git_clone(&repo_url, &version, &pkg_dir)?;
                Ok(())
            }
        },
        &format!("git clone {repo_url}"),
    )
    .await;

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
                        let tarball_url = tarball_url.clone();
                        let pkg_dir = pkg_dir.clone();
                        async move {
                            download_and_extract_tarball(&tarball_url, &pkg_dir).await?;
                            Ok(())
                        }
                    },
                    "GitHub tarball download",
                )
                .await;

                if let Err(tarball_err) = dl_result {
                    // Try release assets as last resort
                    let release_result = with_retry(
                        || {
                            let repo_url = repo_url.to_string();
                            let version = version.to_string();
                            let pkg_dir = pkg_dir.clone();
                            async move {
                                download_github_release(&repo_url, &version, &pkg_dir).await?;
                                Ok(())
                            }
                        },
                        "GitHub release download",
                    )
                    .await;

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
pub async fn fetch_package_verified(
    name: &str,
    repo_url: &str,
    version: &str,
    locked: Option<&LockedPackage>,
    cache_dir: &Path,
) -> HutResult<PathBuf> {
    let path = fetch_package(name, repo_url, version, cache_dir).await?;

    if let Some(locked) = locked {
        let actual = hash_directory(&path)?;
        if actual != locked.integrity {
            return Err(HutError::Other(format!(
                "Integrity check failed for {name}@{version}: \
                 expected {}, got {}",
                &locked.integrity[..16],
                &actual[..16],
            )));
        }
    }

    Ok(path)
}

// ── install_dependencies ───────────────────────────────────────────────────

/// Install all dependencies for a project.
///
/// Fetches every package in the lockfile in parallel, verifying integrity
/// against the lockfile entries.
pub async fn install_dependencies(
    config: &HutConfig,
    lockfile: &Lockfile,
    cache_dir: &Path,
) -> HutResult<()> {
    // Collect all dependency specs from config
    let mut deps: Vec<(&String, &String)> = Vec::new();
    for (name, version) in &config.dependencies {
        deps.push((name, version));
    }
    for (name, version) in &config.build_dependencies {
        if !config.dependencies.contains_key(name) {
            deps.push((name, version));
        }
    }
    for (name, version) in &config.test_dependencies {
        if !config.dependencies.contains_key(name) && !config.build_dependencies.contains_key(name)
        {
            deps.push((name, version));
        }
    }

    if deps.is_empty() {
        status("Info", "No dependencies to install");
        return Ok(());
    }

    status(
        "Installing",
        &format!("{} dependencies", deps.len().to_string().bold()),
    );

    // Fetch each dependency in parallel
    let fetches: Vec<_> = deps
        .into_iter()
        .map(|(name, version_spec)| {
            let name = name.clone();
            let version_spec = version_spec.clone();
            let locked = lockfile.get(&name).cloned();
            let cache_dir = cache_dir.to_path_buf();

            tokio::spawn(async move {
                // Resolve the repo URL from the lockfile (or try to construct one)
                let repo_url = match &locked {
                    Some(lp) => lp.resolved.clone(),
                    None => {
                        // Try to guess from the name — user/repo format
                        if name.contains('/') {
                            format!("https://github.com/{}", name)
                        } else {
                            format!("https://github.com/hutpm/{}", name)
                        }
                    }
                };

                // The version to fetch: use the locked version if available, otherwise
                // try to parse the version spec (strip ^/~ prefixes)
                let version = match &locked {
                    Some(lp) => lp.version.clone(),
                    None => version_spec
                        .trim_start_matches('^')
                        .trim_start_matches('~')
                        .to_string(),
                };

                match fetch_package_verified(
                    &name,
                    &repo_url,
                    &version,
                    locked.as_ref(),
                    &cache_dir,
                )
                .await
                {
                    Ok(path) => Ok((name.clone(), path)),
                    Err(e) => Err((name.clone(), e)),
                }
            })
        })
        .collect();

    let results = join_all(fetches).await;

    // Aggregate results
    let mut errors: Vec<String> = Vec::new();
    let mut succeeded = 0usize;

    for result in results {
        match result {
            Ok(Ok((name, path))) => {
                status("Installed", &format!("{} → {}", name, path.display()));
                succeeded += 1;
            }
            Ok(Err((name, err))) => {
                errors.push(format!("  {}: {}", name, err));
            }
            Err(join_err) => {
                errors.push(format!("  Task panicked: {}", join_err));
            }
        }
    }

    if !errors.is_empty() {
        return Err(HutError::Other(format!(
            "Failed to install {} dependencies:\n{}",
            errors.len(),
            errors.join("\n")
        )));
    }

    status(
        "Done",
        &format!(
            "{} {} installed successfully",
            succeeded.to_string().bold().green(),
            if succeeded == 1 {
                "package"
            } else {
                "packages"
            }
        ),
    );

    Ok(())
}

// ── clear_cache ────────────────────────────────────────────────────────────

/// Remove all packages from the cache.
pub fn clear_cache(cache_dir: &Path) -> HutResult<()> {
    if cache_dir.exists() {
        status("Clearing", &format!("cache at {}", cache_dir.display()));
        std::fs::remove_dir_all(cache_dir)?;
        status("Cleared", "cache");
    } else {
        status("Info", "No cache to clear");
    }
    Ok(())
}

// ── cache_size ─────────────────────────────────────────────────────────────

/// Get the total size of the cache in bytes.
pub fn cache_size(cache_dir: &Path) -> HutResult<u64> {
    if !cache_dir.exists() {
        return Ok(0);
    }

    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(cache_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }

    Ok(total)
}

// ── fetch_and_run ──────────────────────────────────────────────────────────

/// Parse a repo spec like "user/repo" or "user/repo@version".
fn parse_repo_spec(spec: &str) -> (String, String, String) {
    let (repo_path, version) = if let Some(pos) = spec.find('@') {
        (&spec[..pos], spec[pos + 1..].to_string())
    } else {
        (spec, "main".to_string())
    };

    // Normalize: if no slash, assume hutpm/ prefix
    let (owner, name) = if let Some(pos) = repo_path.find('/') {
        (&repo_path[..pos], &repo_path[pos + 1..])
    } else {
        ("hutpm", repo_path)
    };

    let repo_url = format!("https://github.com/{owner}/{name}");
    (name.to_string(), repo_url, version)
}

/// Fetch, build, and run a package in one shot.
///
/// The `repo_spec` uses the format `user/repo` or `user/repo@version`.
/// After fetching to a temp directory, attempts to build using the
/// package's declared build system, then runs the resulting executable
/// (or the specified script).
pub async fn fetch_and_run(repo_spec: &str, args: &[String]) -> HutResult<()> {
    let (name, repo_url, version) = parse_repo_spec(repo_spec);

    status("Fetch+Run", &format!("{}@{}", name, version));

    // Fetch to a temp directory
    let tmp = TempDir::new().map_err(|e| HutError::Io(e))?;
    let pkg_dir = fetch_package(&name, &repo_url, &version, tmp.path()).await?;

    // Try to read the package's hut.toml (or any build manifest)
    let hut_toml_path = pkg_dir.join("hut.toml");
    let cmake_lists = pkg_dir.join("CMakeLists.txt");
    let makefile = pkg_dir.join("Makefile");
    let meson_build = pkg_dir.join("meson.build");

    // Build
    if hut_toml_path.exists() {
        // It's a hut project — try running hut itself
        status("Building", "hut project");
        let build_status = Command::new("hut")
            .args(["build"])
            .current_dir(&pkg_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();

        match build_status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                return Err(HutError::Build(format!(
                    "hut build exited with code {}",
                    s.code().unwrap_or(-1)
                )));
            }
            Err(_) => {
                // hut not installed — try build systems directly
                build_with_detected_system(&pkg_dir, &cmake_lists, &makefile, &meson_build)?;
            }
        }
    } else if cmake_lists.exists() {
        build_with_detected_system(&pkg_dir, &cmake_lists, &makefile, &meson_build)?;
    } else {
        return Err(HutError::Build(format!(
            "No recognized build system found in {}",
            pkg_dir.display()
        )));
    }

    // Try to find an executable to run
    let build_dir = pkg_dir.join("build");
    let candidates = vec![
        build_dir.join(&name),
        build_dir.join("src").join(&name),
        pkg_dir.join(&name),
        pkg_dir.join("build").join("main"),
    ];

    let executable = candidates.into_iter().find(|p| p.exists());

    if let Some(exe) = executable {
        status("Running", &format!("{} {}", exe.display(), args.join(" ")));
        let run_status = Command::new(&exe)
            .args(args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| HutError::Other(format!("Failed to run {}: {}", exe.display(), e)))?;

        if !run_status.success() {
            return Err(HutError::Other(format!(
                "Process exited with code {}",
                run_status.code().unwrap_or(-1)
            )));
        }
    } else {
        // No binary found — maybe it's a header-only library? Report success.
        status("Done", "Fetched successfully (no executable found to run)");
    }

    // The temp directory will be cleaned up on drop
    Ok(())
}

/// Attempt to build using CMake, Make, or Meson.
fn build_with_detected_system(
    pkg_dir: &Path,
    cmake_lists: &Path,
    makefile: &Path,
    meson_build: &Path,
) -> HutResult<()> {
    if cmake_lists.exists() {
        status("Building", "cmake");
        let build_dir = pkg_dir.join("build");
        std::fs::create_dir_all(&build_dir)?;

        let cmake_status = Command::new("cmake")
            .args(["..", "-DCMAKE_BUILD_TYPE=Release"])
            .current_dir(&build_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| HutError::Build(format!("cmake not found: {e}")))?;

        if !cmake_status.success() {
            return Err(HutError::Build("cmake configure failed".into()));
        }

        let make_status = Command::new("cmake")
            .args(["--build", ".", "--parallel"])
            .current_dir(&build_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|_| HutError::Build("cmake --build failed".into()))?;

        if !make_status.success() {
            return Err(HutError::Build("cmake build failed".into()));
        }
    } else if makefile.exists() {
        status("Building", "make");
        let make_status = Command::new("make")
            .arg(format!(
                "-j{}",
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            ))
            .current_dir(pkg_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| HutError::Build(format!("make not found: {e}")))?;

        if !make_status.success() {
            return Err(HutError::Build("make failed".into()));
        }
    } else if meson_build.exists() {
        status("Building", "meson");
        let build_dir = pkg_dir.join("build");
        std::fs::create_dir_all(&build_dir)?;

        let setup = Command::new("meson")
            .args(["setup", ".."])
            .current_dir(&build_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| HutError::Build(format!("meson not found: {e}")))?;

        if !setup.success() {
            return Err(HutError::Build("meson setup failed".into()));
        }

        let compile = Command::new("meson")
            .args(["compile", "-C", "."])
            .current_dir(&build_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|_| HutError::Build("meson compile failed".into()))?;

        if !compile.success() {
            return Err(HutError::Build("meson build failed".into()));
        }
    }

    Ok(())
}

// ── Convenience re-exports for the public API ──────────────────────────────

/// Get the default cache directory path.
pub fn get_default_cache_dir() -> PathBuf {
    default_cache_dir()
}

/// Human-readable cache size string.
pub fn cache_size_human(cache_dir: &Path) -> HutResult<String> {
    let size = cache_size(cache_dir)?;
    Ok(human_bytes(size))
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{:.1} {}", size, UNITS[unit_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repo_spec_with_version() {
        let (name, url, version) = parse_repo_spec("user/mylib@v1.2.3");
        assert_eq!(name, "mylib");
        assert_eq!(url, "https://github.com/user/mylib");
        assert_eq!(version, "v1.2.3");
    }

    #[test]
    fn test_parse_repo_spec_without_version() {
        let (name, url, version) = parse_repo_spec("user/mylib");
        assert_eq!(name, "mylib");
        assert_eq!(url, "https://github.com/user/mylib");
        assert_eq!(version, "main");
    }

    #[test]
    fn test_parse_repo_spec_default_owner() {
        let (name, url, version) = parse_repo_spec("mylib@v2.0");
        assert_eq!(name, "mylib");
        assert_eq!(url, "https://github.com/hutpm/mylib");
        assert_eq!(version, "v2.0");
    }

    #[test]
    fn test_parse_repo_spec_default_owner_no_version() {
        let (name, url, version) = parse_repo_spec("mylib");
        assert_eq!(name, "mylib");
        assert_eq!(url, "https://github.com/hutpm/mylib");
        assert_eq!(version, "main");
    }

    #[test]
    fn test_human_bytes() {
        assert_eq!(human_bytes(0), "0.0 B");
        assert_eq!(human_bytes(512), "512.0 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1_048_576), "1.0 MB");
        assert_eq!(human_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_github_tarball_url() {
        let url = github_tarball_url("https://github.com/owner/repo", "v1.0");
        assert!(url.is_some());
        assert!(url.unwrap().contains("owner/repo"));

        let url = github_tarball_url("https://gitlab.com/owner/repo", "v1.0");
        assert!(url.is_none());
    }

    #[test]
    fn test_default_cache_dir() {
        let dir = default_cache_dir();
        assert!(dir.ends_with("hut/cache"));
    }
}
