use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::{HutError, HutResult};
use crate::package::Package;

/// Package registry entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub description: String,
    pub repository: String,
    pub versions: BTreeMap<String, VersionInfo>,
    /// Tags for discovery
    #[serde(default)]
    pub tags: Vec<String>,
    /// Download count
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Git tag or commit
    pub r#ref: String,
    /// SHA-256 of the package at this version
    #[serde(default)]
    pub integrity: Option<String>,
    /// Dependencies at this version
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// The full registry index
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryIndex {
    pub packages: Vec<RegistryEntry>,
}

/// Default registry URL
pub const DEFAULT_REGISTRY: &str =
    "https://raw.githubusercontent.com/hutpm/registry/main/index.json";

impl RegistryIndex {
    pub fn find(&self, name: &str) -> Option<&RegistryEntry> {
        self.packages.iter().find(|p| p.name == name)
    }

    pub fn search(&self, query: &str) -> Vec<&RegistryEntry> {
        let q = query.to_lowercase();
        self.packages
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Registry client functions
// ---------------------------------------------------------------------------

/// Cache directory for registry data: `~/.hut/registry/`
fn registry_cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hut")
        .join("registry")
}

/// Fetch the registry index from the given URL (or the default).
/// Results are cached locally at `~/.hut/registry/index.json`.
pub async fn fetch_registry(url: Option<&str>) -> HutResult<RegistryIndex> {
    let url = url.unwrap_or(DEFAULT_REGISTRY);

    let cache_dir = registry_cache_dir();
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = cache_dir.join("index.json");

    // Return cached copy if it is fresh enough (< 1 hour old).
    if cache_path.exists() {
        if let Ok(meta) = std::fs::metadata(&cache_path) {
            if let Ok(modified) = meta.modified() {
                let age = modified
                    .elapsed()
                    .unwrap_or(std::time::Duration::from_secs(u64::MAX));
                if age < std::time::Duration::from_secs(3600) {
                    let body = std::fs::read_to_string(&cache_path)?;
                    let index: RegistryIndex = serde_json::from_str(&body)?;
                    return Ok(index);
                }
            }
        }
    }

    // Download fresh index.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(HutError::Registry(format!(
            "Failed to fetch registry index: HTTP {}",
            resp.status()
        )));
    }

    let body = resp.text().await?;
    let index: RegistryIndex = serde_json::from_str(&body)
        .map_err(|e| HutError::Registry(format!("Invalid registry index JSON: {e}")))?;

    // Write to cache.
    std::fs::write(&cache_path, &body)?;

    Ok(index)
}

/// Fetch package metadata by cloning a git repository at a specific tag and
/// reading its `hut.toml`.  The clone is placed in a cache directory under
/// `~/.hut/packages/<name>/<version>/`.
pub async fn fetch_package_metadata(repo_url: &str, version: &str) -> HutResult<Package> {
    let cache_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hut")
        .join("packages");

    // Derive a directory name from the URL and version.
    let dir_name = sanitise_repo_name(repo_url);
    let pkg_dir = cache_dir.join(&dir_name).join(version);

    // Clone if not already cached.
    if !pkg_dir.join("hut.toml").exists() {
        std::fs::create_dir_all(&pkg_dir)?;

        // We clone into a temp dir first, then checkout the tag and move.
        let tmp = pkg_dir.with_extension("tmp");
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }

        let status = std::process::Command::new("git")
            .args(["clone", "--depth", "1", "--branch", version, repo_url])
            .arg(&tmp)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()?;

        if !status.success() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(HutError::Registry(format!(
                "git clone failed for {repo_url} at tag {version}"
            )));
        }

        // Read hut.toml from the clone.
        let manifest_path = tmp.join("hut.toml");
        if !manifest_path.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(HutError::Registry(format!(
                "No hut.toml found in {repo_url} at tag {version}"
            )));
        }

        let manifest = std::fs::read_to_string(&manifest_path)?;
        let pkg: Package = toml::from_str(&manifest)?;

        // Move tmp -> pkg_dir.
        if pkg_dir.exists() {
            std::fs::remove_dir_all(&pkg_dir)?;
        }
        std::fs::rename(&tmp, &pkg_dir)?;

        return Ok(pkg);
    }

    // Already cached — just read hut.toml.
    let manifest = std::fs::read_to_string(pkg_dir.join("hut.toml"))?;
    let pkg: Package = toml::from_str(&manifest)?;
    Ok(pkg)
}

/// Turn a git URL into a safe directory name.
fn sanitise_repo_name(url: &str) -> String {
    // Extract the last path component, strip ".git", keep only alphanumeric
    // and a few safe characters.
    let stem = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(url)
        .strip_suffix(".git")
        .unwrap_or(url);

    stem.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Given a registry entry and a semver constraint (e.g. `"^1.0"`, `">=1.2"`,
/// `"*"`), return the best matching version string.
pub fn resolve_version(entry: &RegistryEntry, constraint: &str) -> HutResult<String> {
    // A bare version number like "1.2.3" means "=1.2.3".
    let constraint = if constraint == "*" {
        "*".to_string()
    } else if constraint.chars().next().map_or(false, |c| {
        c == '^' || c == '~' || c == '>' || c == '<' || c == '='
    }) {
        constraint.to_string()
    } else {
        // Treat a plain version as an exact requirement.
        format!("={constraint}")
    };

    let req = semver::VersionReq::parse(&constraint).map_err(|e| {
        HutError::Resolution(format!("Invalid version constraint '{constraint}': {e}"))
    })?;

    // Collect every version that satisfies the constraint, then pick the
    // highest (by semver ordering).
    let matching: Vec<semver::Version> = entry
        .versions
        .keys()
        .filter_map(|v| {
            let parsed = semver::Version::parse(v).ok()?;
            if req.matches(&parsed) {
                Some(parsed)
            } else {
                None
            }
        })
        .collect();

    if matching.is_empty() {
        return Err(HutError::VersionNotFound(entry.name.clone(), constraint));
    }

    // Highest version.
    let best = matching.into_iter().max().unwrap();
    Ok(best.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(versions: &[&str]) -> RegistryEntry {
        let mut map = BTreeMap::new();
        for v in versions {
            map.insert(
                v.to_string(),
                VersionInfo {
                    r#ref: format!("v{v}"),
                    integrity: None,
                    dependencies: BTreeMap::new(),
                },
            );
        }
        RegistryEntry {
            name: "test".into(),
            description: "".into(),
            repository: "".into(),
            versions: map,
            tags: vec![],
            downloads: 0,
        }
    }

    #[test]
    fn caret_constraint() {
        let e = make_entry(&["1.2.3", "1.3.0", "1.3.1", "2.0.0"]);
        let v = resolve_version(&e, "^1.2").unwrap();
        assert_eq!(v, "1.3.1");
    }

    #[test]
    fn exact_version() {
        let e = make_entry(&["1.0.0", "1.1.0", "2.0.0"]);
        let v = resolve_version(&e, "1.1.0").unwrap();
        assert_eq!(v, "1.1.0");
    }

    #[test]
    fn star_constraint() {
        let e = make_entry(&["0.9.0", "1.0.0", "2.1.0"]);
        let v = resolve_version(&e, "*").unwrap();
        assert_eq!(v, "2.1.0");
    }

    #[test]
    fn gt_constraint() {
        let e = make_entry(&["1.0.0", "1.5.0", "2.0.0"]);
        let v = resolve_version(&e, ">=1.5").unwrap();
        assert_eq!(v, "2.0.0");
    }
}
