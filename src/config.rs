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
