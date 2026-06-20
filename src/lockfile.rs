use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::error::HutResult;

/// hut.lock — pinned dependency versions + integrity hashes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub packages: BTreeMap<String, LockedPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    /// Git commit SHA
    pub source: String,
    /// SHA-256 of the package contents
    pub integrity: String,
    /// Resolved URL
    pub resolved: String,
    /// Transitive dependencies
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

impl Lockfile {
    pub fn new() -> Self {
        Lockfile {
            version: 1,
            packages: BTreeMap::new(),
        }
    }

    pub fn load(path: &Path) -> HutResult<Self> {
        if !path.exists() {
            return Ok(Lockfile::new());
        }
        let content = std::fs::read_to_string(path)?;
        let lockfile: Lockfile = toml::from_str(&content)?;
        Ok(lockfile)
    }

    pub fn save(&self, path: &Path) -> HutResult<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.get(name)
    }

    pub fn insert(&mut self, pkg: LockedPackage) {
        self.packages.insert(pkg.name.clone(), pkg);
    }

    pub fn remove(&mut self, name: &str) {
        self.packages.remove(name);
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}
