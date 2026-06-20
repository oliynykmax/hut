use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::error::HutResult;

/// hut.toml — project manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HutConfig {
    pub package: PackageMeta,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub build_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub test_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub build: crate::package::BuildConfig,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    /// Workspace members
    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// "c" or "c++"
    #[serde(default = "default_lang")]
    pub language: String,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default = "crate::package::default_includes")]
    pub includes: Vec<String>,
}

fn default_lang() -> String {
    "c".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub members: Vec<String>,
}

impl HutConfig {
    pub fn load(path: &Path) -> HutResult<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: HutConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> HutResult<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Find hut.toml by walking up from cwd
    pub fn find() -> HutResult<(Self, std::path::PathBuf)> {
        let cwd = std::env::current_dir()?;
        for ancestor in cwd.ancestors() {
            let path = ancestor.join("hut.toml");
            if path.exists() {
                return Ok((Self::load(&path)?, path));
            }
        }
        Err(crate::error::HutError::NotAProject)
    }

    pub fn default_template(name: &str) -> Self {
        HutConfig {
            package: PackageMeta {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: None,
                authors: vec![],
                license: Some("MIT".to_string()),
                language: "c".to_string(),
                repository: None,
                homepage: None,
                sources: vec![],
                includes: vec!["include".to_string()],
            },
            dependencies: BTreeMap::new(),
            build_dependencies: BTreeMap::new(),
            test_dependencies: BTreeMap::new(),
            build: crate::package::BuildConfig::default(),
            scripts: BTreeMap::new(),
            workspace: WorkspaceConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"
[package]
name = "myproject"
version = "0.1.0"
"#;
        let config: HutConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.package.name, "myproject");
        assert_eq!(config.package.version, "0.1.0");
        assert_eq!(config.package.language, "c"); // default
        assert!(config.dependencies.is_empty());
        assert!(config.build_dependencies.is_empty());
        assert!(config.test_dependencies.is_empty());
    }

    #[test]
    fn parse_all_fields() {
        let toml_str = r#"
[package]
name = "fullproject"
version = "2.0.0"
description = "A full-featured project"
authors = ["Alice", "Bob"]
license = "Apache-2.0"
language = "c++"
repository = "https://github.com/example/fullproject"
homepage = "https://example.com"
sources = ["src/main.cpp", "src/util.cpp"]
includes = ["include", "third_party/include"]

[dependencies]
libfoo = "^1.0"
libbar = ">=2.0, <3.0"

[build_dependencies]
cmake-utils = "~0.5"

[test_dependencies]
catch2 = "*"

[build]
system = "cmake"
c_standard = "c17"
opt_level = "3"
debug = false
warnings = true

[scripts]
build = "make all"
test = "make test"
"#;
        let config: HutConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.package.name, "fullproject");
        assert_eq!(config.package.version, "2.0.0");
        assert_eq!(config.package.description, Some("A full-featured project".to_string()));
        assert_eq!(config.package.authors, vec!["Alice", "Bob"]);
        assert_eq!(config.package.license, Some("Apache-2.0".to_string()));
        assert_eq!(config.package.language, "c++");
        assert_eq!(config.package.repository, Some("https://github.com/example/fullproject".to_string()));
        assert_eq!(config.package.sources, vec!["src/main.cpp", "src/util.cpp"]);
        assert_eq!(config.package.includes, vec!["include", "third_party/include"]);

        assert_eq!(config.dependencies.get("libfoo").unwrap(), "^1.0");
        assert_eq!(config.dependencies.get("libbar").unwrap(), ">=2.0, <3.0");
        assert_eq!(config.build_dependencies.get("cmake-utils").unwrap(), "~0.5");
        assert_eq!(config.test_dependencies.get("catch2").unwrap(), "*");

        assert_eq!(config.build.system, "cmake");
        assert_eq!(config.build.c_standard, "c17");
        assert_eq!(config.build.opt_level, "3");
        assert!(!config.build.debug);
        assert!(config.build.warnings);

        assert_eq!(config.scripts.get("build").unwrap(), "make all");
        assert_eq!(config.scripts.get("test").unwrap(), "make test");
    }

    #[test]
    fn parse_invalid_toml_error() {
        let result: Result<HutConfig, toml::de::Error> = toml::from_str("this is not toml @@@");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_package_section() {
        let result: Result<HutConfig, toml::de::Error> = toml::from_str("[dependencies]\nfoo = \"1.0\"");
        assert!(result.is_err());
    }

    #[test]
    fn default_template_creates_config() {
        let config = HutConfig::default_template("myapp");
        assert_eq!(config.package.name, "myapp");
        assert_eq!(config.package.version, "0.1.0");
        assert_eq!(config.package.license, Some("MIT".to_string()));
        assert_eq!(config.package.language, "c");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut config = HutConfig::default_template("testproj");
        config.package.description = Some("A test project".to_string());
        config.dependencies.insert("libfoo".to_string(), "^1.0".to_string());
        config.build_dependencies.insert("cmake".to_string(), "*".to_string());
        config.scripts.insert("build".to_string(), "make".to_string());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hut.toml");

        config.save(&path).unwrap();
        assert!(path.exists());

        let loaded = HutConfig::load(&path).unwrap();
        assert_eq!(loaded.package.name, "testproj");
        assert_eq!(loaded.package.description, Some("A test project".to_string()));
        assert_eq!(loaded.dependencies.get("libfoo").unwrap(), "^1.0");
        assert_eq!(loaded.build_dependencies.get("cmake").unwrap(), "*");
        assert_eq!(loaded.scripts.get("build").unwrap(), "make");
    }

    #[test]
    fn load_nonexistent_file_returns_io_error() {
        let result = HutConfig::load(std::path::Path::new("/nonexistent/hut.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn find_config_in_current_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config = HutConfig::default_template("findme");
        config.save(&dir.path().join("hut.toml")).unwrap();

        // We can't easily test find() because it uses current_dir(),
        // but we can test the load/save cycle is consistent.
        let loaded = HutConfig::load(&dir.path().join("hut.toml")).unwrap();
        assert_eq!(loaded.package.name, "findme");
    }

    #[test]
    fn workspace_config_default() {
        let ws = WorkspaceConfig::default();
        assert!(ws.members.is_empty());
    }

    #[test]
    fn workspace_config_with_members() {
        let config: HutConfig = toml::from_str(r#"
[package]
name = "wsproj"
version = "0.1.0"

[workspace]
members = ["liba", "libb"]
"#).unwrap();
        assert_eq!(config.workspace.members.len(), 2);
        assert_eq!(config.workspace.members[0], "liba");
        assert_eq!(config.workspace.members[1], "libb");
    }

    #[test]
    fn package_meta_defaults() {
        let config: HutConfig = toml::from_str(r#"
[package]
name = "min"
version = "1.0"
"#).unwrap();
        assert_eq!(config.package.language, "c");
        assert_eq!(config.package.includes, crate::package::default_includes());
        assert!(config.package.authors.is_empty());
        assert!(config.package.description.is_none());
    }

    #[test]
    fn build_config_default_values() {
        let config: HutConfig = toml::from_str(r#"
[package]
name = "buildtest"
version = "1.0"
"#).unwrap();
        assert_eq!(config.build.system, "auto");
        assert_eq!(config.build.c_standard, "c17");
        assert_eq!(config.build.opt_level, "2");
        assert!(config.build.debug);
        assert!(config.build.warnings);
    }

    #[test]
    fn serialize_deserialize_json_roundtrip() {
        let config = HutConfig::default_template("jsonpkg");
        let json = serde_json::to_string(&config).unwrap();
        let parsed: HutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.package.name, "jsonpkg");
    }
}
