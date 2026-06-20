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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_locked_pkg(name: &str, version: &str) -> LockedPackage {
        LockedPackage {
            name: name.to_string(),
            version: version.to_string(),
            source: "abc123def456".to_string(),
            integrity: "sha256-abcdef".to_string(),
            resolved: format!("https://github.com/hutpm/{name}"),
            dependencies: BTreeMap::new(),
        }
    }

    #[test]
    fn new_lockfile_is_empty() {
        let lf = Lockfile::new();
        assert_eq!(lf.version, 1);
        assert!(lf.packages.is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let lf = Lockfile::default();
        assert_eq!(lf.version, 1);
        assert!(lf.packages.is_empty());
    }

    #[test]
    fn insert_and_get() {
        let mut lf = Lockfile::new();
        let pkg = make_locked_pkg("libfoo", "1.0.0");
        lf.insert(pkg);

        let got = lf.get("libfoo");
        assert!(got.is_some());
        assert_eq!(got.unwrap().version, "1.0.0");
    }

    #[test]
    fn get_missing_returns_none() {
        let lf = Lockfile::new();
        assert!(lf.get("nonexistent").is_none());
    }

    #[test]
    fn remove_package() {
        let mut lf = Lockfile::new();
        lf.insert(make_locked_pkg("libfoo", "1.0.0"));
        lf.insert(make_locked_pkg("libbar", "2.0.0"));
        assert_eq!(lf.packages.len(), 2);

        lf.remove("libfoo");
        assert_eq!(lf.packages.len(), 1);
        assert!(lf.get("libfoo").is_none());
        assert!(lf.get("libbar").is_some());
    }

    #[test]
    fn remove_nonexistent_does_nothing() {
        let mut lf = Lockfile::new();
        lf.insert(make_locked_pkg("libfoo", "1.0.0"));
        lf.remove("nonexistent");
        assert_eq!(lf.packages.len(), 1);
    }

    #[test]
    fn insert_overwrites_existing() {
        let mut lf = Lockfile::new();
        lf.insert(make_locked_pkg("libfoo", "1.0.0"));
        lf.insert(make_locked_pkg("libfoo", "2.0.0"));
        assert_eq!(lf.get("libfoo").unwrap().version, "2.0.0");
        assert_eq!(lf.packages.len(), 1);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut lf = Lockfile::new();
        let mut pkg = make_locked_pkg("libfoo", "1.2.3");
        pkg.dependencies.insert("libbar".to_string(), "^1.0".to_string());
        lf.insert(pkg);
        lf.insert(make_locked_pkg("libbar", "1.0.1"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hut.lock");

        lf.save(&path).unwrap();
        assert!(path.exists());

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.packages.len(), 2);

        let foo = loaded.get("libfoo").unwrap();
        assert_eq!(foo.version, "1.2.3");
        assert_eq!(foo.source, "abc123def456");
        assert_eq!(foo.integrity, "sha256-abcdef");
        assert_eq!(foo.resolved, "https://github.com/hutpm/libfoo");
        assert_eq!(foo.dependencies.get("libbar").unwrap(), "^1.0");

        let bar = loaded.get("libbar").unwrap();
        assert_eq!(bar.version, "1.0.1");
    }

    #[test]
    fn load_nonexistent_file_returns_empty() {
        let lf = Lockfile::load(std::path::Path::new("/nonexistent/path/hut.lock")).unwrap();
        assert_eq!(lf.version, 1);
        assert!(lf.packages.is_empty());
    }

    #[test]
    fn load_invalid_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hut.lock");
        std::fs::write(&path, "not valid toml {{{").unwrap();

        let result = Lockfile::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn serialize_deserialize_json_roundtrip() {
        let mut pkg = make_locked_pkg("mypkg", "3.2.1");
        pkg.dependencies.insert("dep".to_string(), ">=2.0".to_string());

        let lf = Lockfile {
            version: 1,
            packages: {
                let mut m = BTreeMap::new();
                m.insert("mypkg".to_string(), pkg);
                m
            },
        };

        let json = serde_json::to_string(&lf).unwrap();
        let parsed: Lockfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.packages.len(), 1);
        let p = parsed.get("mypkg").unwrap();
        assert_eq!(p.version, "3.2.1");
        assert_eq!(p.dependencies.get("dep").unwrap(), ">=2.0");
    }

    #[test]
    fn lockfile_packages_sorted_by_name() {
        let mut lf = Lockfile::new();
        lf.insert(make_locked_pkg("zlib", "1.0"));
        lf.insert(make_locked_pkg("alib", "2.0"));
        lf.insert(make_locked_pkg("mlib", "3.0"));

        let names: Vec<&str> = lf.packages.keys().map(|s| s.as_str()).collect();
        assert_eq!(names, vec!["alib", "mlib", "zlib"]);
    }

    #[test]
    fn locked_package_optional_dependencies() {
        let json = r#"{"name":"pkg","version":"1.0","source":"abc","integrity":"sha","resolved":"url"}"#;
        let pkg: LockedPackage = serde_json::from_str(json).unwrap();
        assert!(pkg.dependencies.is_empty());
    }
}
