use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A C/C++ package definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,

    /// Source directories (relative to package root)
    #[serde(default)]
    pub sources: Vec<String>,

    /// Include directories (relative to package root)
    #[serde(default = "default_includes")]
    pub includes: Vec<String>,

    /// Dependencies
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,

    /// Build dependencies
    #[serde(default)]
    pub build_dependencies: BTreeMap<String, String>,

    /// Test dependencies
    #[serde(default)]
    pub test_dependencies: BTreeMap<String, String>,

    /// Build configuration
    #[serde(default)]
    pub build: BuildConfig,

    /// Scripts (like npm scripts)
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,

    /// Exported libraries
    #[serde(default)]
    pub libraries: Vec<LibraryTarget>,

    /// Exported executables
    #[serde(default)]
    pub executables: Vec<ExecutableTarget>,

    /// Test targets
    #[serde(default)]
    pub tests: Vec<TestTarget>,

    /// Compiler flags
    #[serde(default)]
    pub cflags: Vec<String>,

    /// Linker flags
    #[serde(default)]
    pub ldflags: Vec<String>,
}

fn default_includes() -> Vec<String> {
    vec!["include".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Build system: "auto", "cmake", "make", "hut"
    #[serde(default = "default_build_system")]
    pub system: String,

    /// C standard: "c11", "c17", "c23", etc.
    #[serde(default = "default_c_standard")]
    pub c_standard: String,

    /// C++ standard: "c++17", "c++20", "c++23", etc.
    pub cpp_standard: Option<String>,

    /// Optimization level: "0", "1", "2", "3", "s", "fast"
    #[serde(default = "default_opt_level")]
    pub opt_level: String,

    /// Debug symbols
    #[serde(default = "default_debug")]
    pub debug: bool,

    /// Warning flags
    #[serde(default = "default_warnings")]
    pub warnings: bool,

    /// Extra compiler flags
    #[serde(default)]
    pub extra_cflags: Vec<String>,

    /// Extra linker flags
    #[serde(default)]
    pub extra_ldflags: Vec<String>,

    /// Defines
    #[serde(default)]
    pub defines: BTreeMap<String, String>,

    /// Per-target compiler flags (key = target name or "*" for all)
    #[serde(default)]
    pub target_cflags: BTreeMap<String, Vec<String>>,

    /// Per-target linker flags
    #[serde(default)]
    pub target_ldflags: BTreeMap<String, Vec<String>>,

    /// Platform-conditional overrides
    #[serde(default)]
    pub platform: BTreeMap<String, PlatformBuildConfig>,

    /// Sanitizers: "address", "undefined", "thread", "leak"
    #[serde(default)]
    pub sanitizers: Vec<String>,

    /// Compiler preference: "auto", "gcc", "clang"
    #[serde(default = "default_compiler")]
    pub compiler: String,

    /// Link-time optimization
    #[serde(default)]
    pub lto: bool,

    /// Position-independent code (needed for shared libs)
    #[serde(default)]
    pub pic: bool,
}

fn default_build_system() -> String {
    "auto".to_string()
}
fn default_c_standard() -> String {
    "c17".to_string()
}
fn default_opt_level() -> String {
    "2".to_string()
}
fn default_debug() -> bool {
    true
}
fn default_warnings() -> bool {
    true
}

fn default_compiler() -> String {
    "auto".to_string()
}

impl Default for BuildConfig {
    fn default() -> Self {
        BuildConfig {
            system: default_build_system(),
            c_standard: default_c_standard(),
            cpp_standard: None,
            opt_level: default_opt_level(),
            debug: default_debug(),
            warnings: default_warnings(),
            extra_cflags: vec![],
            extra_ldflags: vec![],
            defines: BTreeMap::new(),
            target_cflags: BTreeMap::new(),
            target_ldflags: BTreeMap::new(),
            platform: BTreeMap::new(),
            sanitizers: vec![],
            compiler: default_compiler(),
            lto: false,
            pic: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformBuildConfig {
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub ldflags: Vec<String>,
    #[serde(default)]
    pub defines: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTarget {
    pub name: String,
    /// "static" or "shared"
    #[serde(default = "default_lib_type")]
    pub lib_type: String,
}

fn default_lib_type() -> String {
    "static".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutableTarget {
    pub name: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestTarget {
    pub name: String,
    pub sources: Vec<String>,
    #[serde(default)]
    pub framework: Option<String>,
}

/// Resolved dependency with its include paths
#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub package: Package,
    /// All include directories (own + transitive)
    pub include_paths: Vec<PathBuf>,
    /// Library paths for linking
    pub library_paths: Vec<PathBuf>,
    /// Libraries to link against
    pub link_libraries: Vec<String>,
    /// Compiler flags inherited from this dependency
    pub cflags: Vec<String>,
    /// Linker flags inherited from this dependency
    pub ldflags: Vec<String>,
}

impl Package {
    pub fn default_build() -> BuildConfig {
        BuildConfig::default()
    }
}
