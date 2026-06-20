//! Local package index — maps package names to GitHub repos + build recipes.
//! The default index is COMPILED INTO the binary (packages.toml baked in).
//! Users can extend it at ~/.config/hut/packages.toml — that file takes
//! precedence over the built-in index.

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

/// Built-in packages.toml — compiled directly into the binary.
static BUILTIN_PACKAGES: &str = include_str!("../packages.toml");

impl PackagesIndex {
    /// Load the index from a TOML file.
    pub fn load(path: &std::path::Path) -> HutResult<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            HutError::Other(format!(
                "Failed to read packages index {}: {e}",
                path.display()
            ))
        })?;
        let index: PackagesIndex = toml::from_str(&content)
            .map_err(|e| HutError::Other(format!("Invalid packages.toml: {e}")))?;
        Ok(index)
    }

    /// Load the packages index. Order:
    /// 1. ~/.config/hut/packages.toml (user override — takes full precedence)
    /// 2. If that doesn't exist, copy the built-in index there, then use it.
    /// 3. Built-in index (compiled into binary) as ultimate fallback.
    pub fn load_builtin() -> HutResult<Self> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("hut");
        let user_path = config_dir.join("packages.toml");

        // User override exists — use it.
        if user_path.exists() {
            return Self::load(&user_path);
        }

        // Auto-create ~/.config/hut/packages.toml from the built-in index.
        if let Err(e) = std::fs::create_dir_all(&config_dir) {
            eprintln!("warning: Could not create {}: {e}", config_dir.display());
        } else if let Err(e) = std::fs::write(&user_path, BUILTIN_PACKAGES) {
            eprintln!("warning: Could not write {}: {e}", user_path.display());
        }

        // Load from file if it now exists, otherwise fall back to built-in.
        if user_path.exists() {
            Self::load(&user_path)
        } else {
            let index: PackagesIndex = toml::from_str(BUILTIN_PACKAGES)
                .expect("Built-in packages.toml is invalid — fix it before building");
            Ok(index)
        }
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
                name.to_lowercase().contains(&q) || entry.description.to_lowercase().contains(&q)
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
