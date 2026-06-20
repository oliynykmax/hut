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

pub fn default_includes() -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // -----------------------------------------------------------------------
    // Package construction & defaults
    // -----------------------------------------------------------------------

    fn make_test_package() -> Package {
        Package {
            name: "testlib".into(),
            version: "1.2.3".into(),
            description: Some("A test library".into()),
            authors: vec!["Alice".into(), "Bob".into()],
            license: None,
            repository: Some("https://github.com/example/testlib".into()),
            homepage: Some("https://example.com/testlib".into()),
            sources: vec!["src".into()],
            includes: default_includes(),
            dependencies: {
                let mut m = BTreeMap::new();
                m.insert("dep1".into(), "^1.0".into());
                m
            },
            build_dependencies: BTreeMap::new(),
            test_dependencies: BTreeMap::new(),
            build: BuildConfig::default(),
            scripts: BTreeMap::new(),
            libraries: vec![],
            executables: vec![],
            tests: vec![],
            cflags: vec![],
            ldflags: vec![],
        }
    }

    #[test]
    fn test_package_creation() {
        let pkg = make_test_package();
        assert_eq!(pkg.name, "testlib");
        assert_eq!(pkg.version, "1.2.3");
        assert_eq!(pkg.description, Some("A test library".into()));
        assert_eq!(pkg.authors, vec!["Alice", "Bob"]);
        assert_eq!(pkg.license, None);
        assert_eq!(
            pkg.repository,
            Some("https://github.com/example/testlib".into())
        );
        assert_eq!(pkg.homepage, Some("https://example.com/testlib".into()));
        assert_eq!(pkg.sources, vec!["src"]);
        assert_eq!(pkg.includes, vec!["include"]);
        assert_eq!(pkg.dependencies.len(), 1);
        assert_eq!(pkg.dependencies.get("dep1").unwrap(), "^1.0");
    }

    #[test]
    fn test_default_includes() {
        let includes = default_includes();
        assert_eq!(includes, vec!["include".to_string()]);
    }

    #[test]
    fn test_buildconfig_default() {
        let bc = BuildConfig::default();
        assert_eq!(bc.system, "auto");
        assert_eq!(bc.c_standard, "c17");
        assert_eq!(bc.cpp_standard, None);
        assert_eq!(bc.opt_level, "2");
        assert!(bc.debug);
        assert!(bc.warnings);
        assert!(!bc.lto);
        assert!(!bc.pic);
        assert_eq!(bc.compiler, "auto");
        assert!(bc.extra_cflags.is_empty());
        assert!(bc.extra_ldflags.is_empty());
        assert!(bc.defines.is_empty());
        assert!(bc.target_cflags.is_empty());
        assert!(bc.target_ldflags.is_empty());
        assert!(bc.platform.is_empty());
        assert!(bc.sanitizers.is_empty());
    }

    #[test]
    fn test_default_build() {
        let bc = Package::default_build();
        assert_eq!(bc.system, "auto");
        assert_eq!(bc.opt_level, "2");
        assert!(bc.debug);
    }

    #[test]
    fn test_library_target_defaults() {
        let lib = LibraryTarget {
            name: "my_shared".into(),
            lib_type: "shared".into(),
        };
        assert_eq!(lib.lib_type, "shared");

        // A LibraryTarget with no explicit lib_type gets "static"
        let lib_json = r#"{"name": "mylib"}"#;
        let lib: LibraryTarget = serde_json::from_str(lib_json).unwrap();
        assert_eq!(lib.name, "mylib");
        assert_eq!(lib.lib_type, "static");
    }

    #[test]
    fn test_executable_target() {
        let exe = ExecutableTarget {
            name: "myapp".into(),
            sources: vec!["main.c".into(), "util.c".into()],
        };
        assert_eq!(exe.name, "myapp");
        assert_eq!(exe.sources.len(), 2);
    }

    #[test]
    fn test_test_target() {
        let tt = TestTarget {
            name: "unit_tests".into(),
            sources: vec!["test_main.c".into()],
            framework: Some("unity".into()),
        };
        assert_eq!(tt.name, "unit_tests");
        assert_eq!(tt.framework, Some("unity".into()));

        // TestTarget with no framework
        let tt2 = TestTarget {
            name: "bare_tests".into(),
            sources: vec!["test.c".into()],
            framework: None,
        };
        assert!(tt2.framework.is_none());
    }

    #[test]
    fn test_platform_buildconfig_default() {
        let plat = PlatformBuildConfig::default();
        assert!(plat.cflags.is_empty());
        assert!(plat.ldflags.is_empty());
        assert!(plat.defines.is_empty());
    }

    // -----------------------------------------------------------------------
    // Serialization / deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_package_serialize_toml() {
        let pkg = make_test_package();
        let toml_str = toml::to_string(&pkg).unwrap();
        assert!(toml_str.contains("name = \"testlib\""));
        assert!(toml_str.contains("version = \"1.2.3\""));
        assert!(toml_str.contains("description = \"A test library\""));
        assert!(!toml_str.contains("license"));
        assert!(toml_str.contains("dep1 = \"^1.0\""));
    }

    #[test]
    fn test_package_deserialize_toml_minimal() {
        let toml_str = r#"
name = "minimal"
version = "0.1.0"
authors = []
"#;
        let pkg: Package = toml::from_str(toml_str).unwrap();
        assert_eq!(pkg.name, "minimal");
        assert_eq!(pkg.version, "0.1.0");
        assert!(pkg.description.is_none());
        assert!(pkg.authors.is_empty());
        assert!(pkg.sources.is_empty());
        // Default includes
        assert_eq!(pkg.includes, vec!["include"]);
        assert!(pkg.dependencies.is_empty());
    }

    #[test]
    fn test_package_deserialize_toml_full() {
        let toml_str = r#"
name = "fullpkg"
version = "2.0.0"
description = "Full featured package"
authors = ["Dev1", "Dev2"]
license = "Apache-2.0"
repository = "https://github.com/example/fullpkg"
homepage = "https://fullpkg.dev"
sources = ["src", "lib"]
includes = ["include", "third_party/include"]
cflags = ["-DFOO", "-DBAR=1"]
ldflags = ["-lm"]

[dependencies]
dep1 = ">=1.0"
dep2 = "^2.3"

[build]
system = "hut"
opt_level = "3"
lto = true
pic = true
sanitizers = ["address"]

[[libraries]]
name = "fullpkg"
lib_type = "shared"

[[executables]]
name = "fullpkg-cli"
sources = ["cli/main.c"]

[[tests]]
name = "integration_test"
sources = ["tests/test.c"]
framework = "unity"
"#;
        let pkg: Package = toml::from_str(toml_str).unwrap();
        assert_eq!(pkg.name, "fullpkg");
        assert_eq!(pkg.version, "2.0.0");
        assert_eq!(pkg.description, Some("Full featured package".into()));
        assert_eq!(pkg.authors, vec!["Dev1", "Dev2"]);
        assert_eq!(pkg.license, Some("Apache-2.0".into()));
        assert_eq!(
            pkg.repository,
            Some("https://github.com/example/fullpkg".into())
        );
        assert_eq!(pkg.homepage, Some("https://fullpkg.dev".into()));
        assert_eq!(pkg.sources, vec!["src", "lib"]);
        assert_eq!(pkg.includes, vec!["include", "third_party/include"]);
        assert_eq!(pkg.cflags, vec!["-DFOO", "-DBAR=1"]);
        assert_eq!(pkg.ldflags, vec!["-lm"]);

        assert_eq!(pkg.dependencies.len(), 2);
        assert_eq!(pkg.dependencies.get("dep1").unwrap(), ">=1.0");
        assert_eq!(pkg.dependencies.get("dep2").unwrap(), "^2.3");

        assert_eq!(pkg.build.system, "hut");
        assert_eq!(pkg.build.opt_level, "3");
        assert!(pkg.build.lto);
        assert!(pkg.build.pic);
        assert_eq!(pkg.build.sanitizers, vec!["address"]);

        assert_eq!(pkg.libraries.len(), 1);
        assert_eq!(pkg.libraries[0].name, "fullpkg");
        assert_eq!(pkg.libraries[0].lib_type, "shared");

        assert_eq!(pkg.executables.len(), 1);
        assert_eq!(pkg.executables[0].name, "fullpkg-cli");

        assert_eq!(pkg.tests.len(), 1);
        assert_eq!(pkg.tests[0].name, "integration_test");
        assert_eq!(pkg.tests[0].framework, Some("unity".into()));
    }

    #[test]
    fn test_package_roundtrip_json() {
        let pkg = make_test_package();
        let json_str = serde_json::to_string_pretty(&pkg).unwrap();
        let pkg2: Package = serde_json::from_str(&json_str).unwrap();
        assert_eq!(pkg.name, pkg2.name);
        assert_eq!(pkg.version, pkg2.version);
        assert_eq!(pkg.description, pkg2.description);
        assert_eq!(pkg.authors, pkg2.authors);
        assert_eq!(pkg.includes, pkg2.includes);
        assert_eq!(pkg.dependencies, pkg2.dependencies);
    }

    #[test]
    fn test_package_roundtrip_toml() {
        let pkg = make_test_package();
        let toml_str = toml::to_string_pretty(&pkg).unwrap();
        let pkg2: Package = toml::from_str(&toml_str).unwrap();
        assert_eq!(pkg.name, pkg2.name);
        assert_eq!(pkg.version, pkg2.version);
        assert_eq!(pkg.description, pkg2.description);
        assert_eq!(pkg.authors, pkg2.authors);
        assert_eq!(pkg.includes, pkg2.includes);
        assert_eq!(pkg.dependencies, pkg2.dependencies);
    }

    #[test]
    fn test_json_default_fields_missing() {
        // When optional fields are missing from JSON, they get defaults.
        // `authors` is required (no #[serde(default)]), so must be present.
        let json_str = r#"{"name": "bare", "version": "0.0.1", "authors": []}"#;
        let pkg: Package = serde_json::from_str(json_str).unwrap();
        assert_eq!(pkg.name, "bare");
        assert_eq!(pkg.version, "0.0.1");
        assert!(pkg.authors.is_empty());
        assert_eq!(pkg.includes, vec!["include"]);
        assert!(pkg.dependencies.is_empty());
        assert!(pkg.build_dependencies.is_empty());
        assert!(pkg.libraries.is_empty());
        assert!(pkg.executables.is_empty());
        assert!(pkg.tests.is_empty());
    }

    // -----------------------------------------------------------------------
    // ResolvedDependency
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolved_dependency_construction() {
        let pkg = make_test_package();

        let dep = ResolvedDependency {
            name: "testlib".into(),
            version: "1.2.3".into(),
            path: std::path::PathBuf::from("/home/user/.hut/testlib"),
            package: pkg,
            include_paths: vec![std::path::PathBuf::from("/home/user/.hut/testlib/include")],
            library_paths: vec![std::path::PathBuf::from("/home/user/.hut/testlib/lib")],
            link_libraries: vec!["m".into(), "pthread".into()],
            cflags: vec!["-DFROM_DEP".into(), "-pthread".into()],
            ldflags: vec!["-ldl".into()],
        };

        assert_eq!(dep.name, "testlib");
        assert_eq!(dep.version, "1.2.3");
        assert!(dep.path.ends_with("testlib"));
        assert_eq!(dep.include_paths.len(), 1);
        assert!(dep.include_paths[0].ends_with("include"));
        assert_eq!(dep.library_paths.len(), 1);
        assert!(dep.library_paths[0].ends_with("lib"));
        assert_eq!(dep.link_libraries, vec!["m", "pthread"]);
        assert_eq!(dep.cflags, vec!["-DFROM_DEP", "-pthread"]);
        assert_eq!(dep.ldflags, vec!["-ldl"]);
        assert_eq!(dep.package.name, "testlib");
    }

    #[test]
    fn test_buildconfig_non_default() {
        let bc = BuildConfig {
            system: "cmake".into(),
            c_standard: "c11".into(),
            cpp_standard: Some("c++20".into()),
            opt_level: "s".into(),
            debug: false,
            warnings: false,
            extra_cflags: vec!["-funroll-loops".into()],
            extra_ldflags: vec!["-static".into()],
            defines: {
                let mut m = BTreeMap::new();
                m.insert("VERSION".into(), "4.2".into());
                m
            },
            target_cflags: BTreeMap::new(),
            target_ldflags: BTreeMap::new(),
            platform: BTreeMap::new(),
            sanitizers: vec!["undefined".into()],
            compiler: "clang".into(),
            lto: true,
            pic: true,
        };

        assert_eq!(bc.system, "cmake");
        assert_eq!(bc.c_standard, "c11");
        assert_eq!(bc.cpp_standard, Some("c++20".into()));
        assert_eq!(bc.opt_level, "s");
        assert!(!bc.debug);
        assert!(!bc.warnings);
        assert_eq!(bc.extra_cflags, vec!["-funroll-loops"]);
        assert_eq!(bc.extra_ldflags, vec!["-static"]);
        assert_eq!(bc.defines.get("VERSION").unwrap(), "4.2");
        assert!(bc.lto);
        assert!(bc.pic);
        assert_eq!(bc.compiler, "clang");
        assert_eq!(bc.sanitizers, vec!["undefined"]);
    }
}
