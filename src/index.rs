//! Local package index — loads packages.toml (shipped with hut or user-extended).
//! No remote registry, no HTTP fetch.  Just a curated TOML file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::{HutError, HutResult};

/// Entry for a single package in the index.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PackageEntry {
    /// GitHub repo: "owner/repo"
    pub repo: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Include directories relative to repo root
    #[serde(default)]
    pub includes: Vec<String>,
    /// Libraries to link against
    #[serde(default)]
    pub libs: Vec<String>,
    /// Source files/globs to compile
    #[serde(default)]
    pub sources: Vec<String>,
    /// Preprocessor defines
    #[serde(default)]
    pub defines: Vec<String>,
    /// Extra compiler flags
    #[serde(default)]
    pub cflags: Vec<String>,
    /// Extra linker flags
    #[serde(default)]
    pub ldflags: Vec<String>,
}

/// The full packages index loaded from a TOML file.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PackagesIndex {
    pub packages: BTreeMap<String, PackageEntry>,
}

impl PackagesIndex {
    /// Load the index from a TOML file.
    pub fn load(path: &std::path::Path) -> HutResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| HutError::Other(format!("Failed to read packages index {}: {e}", path.display())))?;
        let index: PackagesIndex = toml::from_str(&content)
            .map_err(|e| HutError::Other(format!("Invalid packages.toml: {e}")))?;
        Ok(index)
    }

    /// Load the built-in packages.toml shipped with hut.
    /// Falls back to searching standard locations.
    pub fn load_builtin() -> HutResult<Self> {
        // Search order:
        // 1. ~/.config/hut/packages.toml (user override)
        // 2. <hut binary dir>/../packages.toml (installed alongside binary)
        // 3. /usr/local/share/hut/packages.toml (system install)
        // 4. Embedded fallback (compiled into binary)

        let user_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("hut")
            .join("packages.toml");
        if user_path.exists() {
            return Self::load(&user_path);
        }

        // Look relative to the current executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let next_to_bin = parent.join("packages.toml");
                if next_to_bin.exists() {
                    return Self::load(&next_to_bin);
                }
                // Also check one level up (for target/release/ layout)
                if let Some(grandparent) = parent.parent() {
                    let one_up = grandparent.join("packages.toml");
                    if one_up.exists() {
                        return Self::load(&one_up);
                    }
                }
            }
        }

        // System path
        let system_path = PathBuf::from("/usr/local/share/hut/packages.toml");
        if system_path.exists() {
            return Self::load(&system_path);
        }

        Err(HutError::Other(
            "No packages.toml found. Place one at ~/.config/hut/packages.toml".into(),
        ))
    }

    /// Look up a package by name.
    pub fn find(&self, name: &str) -> Option<&PackageEntry> {
        self.packages.get(name)
    }

    /// Search packages by name or description substring.
    pub fn search(&self, query: &str) -> Vec<(&String, &PackageEntry)> {
        let q = query.to_lowercase();
        self.packages
            .iter()
            .filter(|(name, entry)| {
                name.to_lowercase().contains(&q)
                    || entry.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Get the GitHub repo URL for a package.
    pub fn repo_url(&self, name: &str) -> HutResult<String> {
        let entry = self
            .find(name)
            .ok_or_else(|| HutError::PackageNotFound(name.to_string()))?;
        Ok(format!("https://github.com/{}.git", entry.repo))
    }
}
