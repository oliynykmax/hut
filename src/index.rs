//! Local package index — maps package names to GitHub repos + build recipes.
//! The default index is COMPILED INTO the binary (packages.toml baked in).
//! Users can extend it at ~/.config/hut/packages.toml — that file takes
//! precedence over the built-in index.

use std::collections::BTreeMap;
use std::path::PathBuf;

use colored::Colorize;

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
    /// Shell command to build the library (run in fetched source dir).
    /// Output libraries should appear in build/ or the package root.
    #[serde(default)]
    pub build: String,
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

    /// Reseed ~/.config/hut/packages.toml with new entries from the built-in
    /// index. Only appends — never removes or replaces user additions.
    pub fn reseed_user_index() -> HutResult<()> {
        use std::io::Write;

        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("hut");
        let user_path = config_dir.join("packages.toml");

        // Parse the embedded current index
        let builtin: PackagesIndex = toml::from_str(BUILTIN_PACKAGES)
            .map_err(|e| HutError::Other(format!("Invalid built-in packages.toml: {e}")))?;

        // Parse user's local index (or empty if not yet created)
        let user_index = if user_path.exists() {
            Self::load(&user_path).unwrap_or_else(|_| PackagesIndex {
                packages: BTreeMap::new(),
            })
        } else {
            // No user file yet — just write the full built-in index.
            std::fs::create_dir_all(&config_dir)?;
            std::fs::write(&user_path, BUILTIN_PACKAGES)?;
            return Ok(());
        };

        let mut new_count = 0;
        let mut file = std::fs::OpenOptions::new().append(true).open(&user_path)?;

        for (name, entry) in &builtin.packages {
            if !user_index.packages.contains_key(name) {
                writeln!(file)?;
                write!(file, "[packages.{name}]\n")?;
                write!(file, "repo = \"{}\"\n", entry.repo)?;
                if !entry.description.is_empty() {
                    write!(file, "description = \"{}\"\n", entry.description)?;
                }
                if !entry.includes.is_empty() {
                    write!(
                        file,
                        "includes = [{}]\n",
                        entry
                            .includes
                            .iter()
                            .map(|i| format!("\"{}\"", i))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                if !entry.libs.is_empty() {
                    write!(
                        file,
                        "libs = [{}]\n",
                        entry
                            .libs
                            .iter()
                            .map(|l| format!("\"{}\"", l))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                if !entry.sources.is_empty() {
                    write!(
                        file,
                        "sources = [{}]\n",
                        entry
                            .sources
                            .iter()
                            .map(|s| format!("\"{}\"", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                if !entry.defines.is_empty() {
                    write!(
                        file,
                        "defines = [{}]\n",
                        entry
                            .defines
                            .iter()
                            .map(|d| format!("\"{}\"", d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                if !entry.cflags.is_empty() {
                    write!(
                        file,
                        "cflags = [{}]\n",
                        entry
                            .cflags
                            .iter()
                            .map(|c| format!("\"{}\"", c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                if !entry.ldflags.is_empty() {
                    write!(
                        file,
                        "ldflags = [{}]\n",
                        entry
                            .ldflags
                            .iter()
                            .map(|l| format!("\"{}\"", l))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                new_count += 1;
            }
        }

        if new_count > 0 {
            eprintln!(
                "{} Added {new_count} new package(s) to {}",
                "index:".dimmed(),
                user_path.display()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal PackageEntry with just a repo.
    fn test_entry(repo: &str) -> PackageEntry {
        PackageEntry {
            repo: repo.to_string(),
            description: String::new(),
            includes: vec![],
            libs: vec![],
            sources: vec![],
            defines: vec![],
            cflags: vec![],
            ldflags: vec![],
            build: String::new(),
        }
    }

    /// Helper: build a PackagesIndex from a list of (name, PackageEntry) pairs.
    fn test_index(entries: Vec<(&str, PackageEntry)>) -> PackagesIndex {
        let packages: BTreeMap<String, PackageEntry> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        PackagesIndex { packages }
    }

    // ---------------------------------------------------------------------------
    // find()
    // ---------------------------------------------------------------------------

    #[test]
    fn find_existing_package() {
        let idx = test_index(vec![("cli11", test_entry("CLIUtils/CLI11"))]);
        let entry = idx.find("cli11");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().repo, "CLIUtils/CLI11");
    }

    #[test]
    fn find_missing_package() {
        let idx = test_index(vec![("cli11", test_entry("CLIUtils/CLI11"))]);
        assert!(idx.find("nonexistent").is_none());
    }

    #[test]
    fn find_case_sensitive() {
        let idx = test_index(vec![("cli11", test_entry("CLIUtils/CLI11"))]);
        // BTreeMap::get is case-sensitive, so "CLI11" won't match "cli11"
        assert!(idx.find("cli11").is_some());
        assert!(idx.find("CLI11").is_none());
    }

    // ---------------------------------------------------------------------------
    // search()
    // ---------------------------------------------------------------------------

    #[test]
    fn search_by_name() {
        let idx = test_index(vec![
            ("cli11", test_entry("CLIUtils/CLI11")),
            ("fmtlib", test_entry("fmtlib/fmt")),
        ]);
        let results = idx.search("cli");
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0].0, "cli11");
    }

    #[test]
    fn search_by_description() {
        let idx = test_index(vec![
            (
                "fmtlib",
                PackageEntry {
                    description: "modern formatting library".to_string(),
                    ..test_entry("fmtlib/fmt")
                },
            ),
            ("cli11", test_entry("CLIUtils/CLI11")),
        ]);
        let results = idx.search("formatting");
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0].0, "fmtlib");
    }

    #[test]
    fn search_case_insensitive() {
        let idx = test_index(vec![
            (
                "cli11",
                PackageEntry {
                    description: "Command-line parser".to_string(),
                    ..test_entry("CLIUtils/CLI11")
                },
            ),
        ]);
        // Search uppercase query against lowercase name
        let results = idx.search("CLI");
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0].0, "cli11");
    }

    #[test]
    fn search_no_match() {
        let idx = test_index(vec![("cli11", test_entry("CLIUtils/CLI11"))]);
        let results = idx.search("zzz_nonexistent");
        assert!(results.is_empty());
    }

    // ---------------------------------------------------------------------------
    // repo_url()
    // ---------------------------------------------------------------------------

    #[test]
    fn repo_url_known_package() {
        let idx = test_index(vec![("cli11", test_entry("CLIUtils/CLI11"))]);
        let url = idx.repo_url("cli11");
        assert!(url.is_ok());
        assert_eq!(url.unwrap(), "https://github.com/CLIUtils/CLI11.git");
    }

    #[test]
    fn repo_url_unknown_package_returns_error() {
        let idx = test_index(vec![("cli11", test_entry("CLIUtils/CLI11"))]);
        let result = idx.repo_url("nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HutError::PackageNotFound(_)));
        assert!(err.to_string().contains("nonexistent"));
    }

    // ---------------------------------------------------------------------------
    // Deserialization from TOML
    // ---------------------------------------------------------------------------

    #[test]
    fn deserialize_minimal_toml() {
        let toml_str = r#"
[packages]
[packages.cli11]
repo = "CLIUtils/CLI11"
"#;
        let idx: PackagesIndex = toml::from_str(toml_str).expect("valid TOML");
        let entry = idx.find("cli11").expect("cli11 should exist");
        assert_eq!(entry.repo, "CLIUtils/CLI11");
        assert!(entry.description.is_empty());
        assert!(entry.includes.is_empty());
        assert!(entry.libs.is_empty());
        assert!(entry.sources.is_empty());
    }

    #[test]
    fn deserialize_full_toml() {
        let toml_str = r#"
[packages]
[packages.mylib]
repo = "user/mylib"
description = "A test library"
includes = ["include"]
libs = ["mylib"]
sources = ["src/*.c"]
defines = ["MYLIB_STATIC"]
cflags = ["-O2"]
ldflags = ["-lpthread"]
"#;
        let idx: PackagesIndex = toml::from_str(toml_str).expect("valid TOML");
        let entry = idx.find("mylib").expect("mylib should exist");
        assert_eq!(entry.repo, "user/mylib");
        assert_eq!(entry.description, "A test library");
        assert_eq!(entry.includes, vec!["include"]);
        assert_eq!(entry.libs, vec!["mylib"]);
        assert_eq!(entry.sources, vec!["src/*.c"]);
        assert_eq!(entry.defines, vec!["MYLIB_STATIC"]);
        assert_eq!(entry.cflags, vec!["-O2"]);
        assert_eq!(entry.ldflags, vec!["-lpthread"]);
    }

    #[test]
    fn deserialize_missing_repo_field_errors() {
        let toml_str = r#"
[packages]
[packages.badlib]
description = "forgot the repo field"
"#;
        let result: Result<PackagesIndex, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("repo") || msg.contains("missing field"));
    }

    // ---------------------------------------------------------------------------
    // Additional tests
    // ---------------------------------------------------------------------------

    #[test]
    fn search_multiple_matches() {
        let idx = test_index(vec![
            (
                "cli11",
                PackageEntry {
                    description: "C++ command-line parser".to_string(),
                    ..test_entry("CLIUtils/CLI11")
                },
            ),
            (
                "argparse",
                PackageEntry {
                    description: "argument parser".to_string(),
                    ..test_entry("p-ranav/argparse")
                },
            ),
            ("fmtlib", test_entry("fmtlib/fmt")),
        ]);
        let results = idx.search("parser");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_matches_description_case_insensitively() {
        let idx = test_index(vec![
            (
                "mylib",
                PackageEntry {
                    description: "AWESOME Library".to_string(),
                    ..test_entry("user/mylib")
                },
            ),
        ]);
        let results = idx.search("awesome");
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0].0, "mylib");
    }

    #[test]
    fn repo_url_with_slashes_in_repo() {
        let idx = test_index(vec![("tool", test_entry("owner/name/subtool"))]);
        let url = idx.repo_url("tool").expect("should resolve");
        assert_eq!(url, "https://github.com/owner/name/subtool.git");
    }
}
