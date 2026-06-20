// ---------------------------------------------------------------------------
// Compiler / linker flag system with dependency inheritance, platform
// conditions, per-target flags, sanitizers, LTO, and PIC support.
// ---------------------------------------------------------------------------

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::HutConfig;
use crate::package::ResolvedDependency;

/// Collected compiler and linker flags ready to be passed to the toolchain.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    pub cflags: Vec<String>,
    pub ldflags: Vec<String>,
}

impl Flags {
    /// Return the total length of all arguments (used for response-file
    /// threshold detection).
    pub fn total_len(&self) -> usize {
        self.cflags.iter().map(|f| f.len() + 1).sum::<usize>()
            + self.ldflags.iter().map(|f| f.len() + 1).sum::<usize>()
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Collect all compiler and linker flags for a hut build.
///
/// Flags are merged with *increasing* precedence (later overrides earlier):
///
///   1. Package-level cflags/ldflags from each dependency (inherited)
///   2. Project-level `BuildConfig` extra_cflags / extra_ldflags
///   3. Platform-conditional flags (matched against OS: "linux", "macos", "windows")
///   4. Per-target flags (matched against `target_name` or "*")
///   5. Include paths (from include resolution)
///   6. Library paths + link libraries (from dep tree)
///   7. Standard flags: -std, -O, -g, -Wall, -Wextra, -D defines,
///      sanitizers, -fPIC, -flto, -pthread
///
/// * `config`     — project manifest
/// * `deps`       — resolved dependencies
/// * `target_name`— name of the target being built (used for per-target flags)
/// * `include_paths` — resolved include directories
/// * `source_file`   — path of the current source file (may affect per-target matching)
/// * `release`       — whether this is a release build
pub fn collect_flags(
    config: &HutConfig,
    deps: &[ResolvedDependency],
    target_name: &str,
    include_paths: &[PathBuf],
    source_file: &std::path::Path,
    release: bool,
) -> Flags {
    let build_cfg = &config.build;

    // Determine C vs C++ PER FILE (not project-wide). Fall back to
    // package.language if the extension is ambiguous (.h, .H, etc.).
    let is_cpp = is_cpp_source(source_file)
        .unwrap_or_else(|| matches!(config.package.language.as_str(), "c++" | "cpp"));

    let mut cflags: Vec<String> = Vec::new();
    let mut ldflags: Vec<String> = Vec::new();

    // ── 1. Dependency-inherited compiler / linker flags ──────────────────
    for dep in deps {
        cflags.extend(dep.cflags.clone());
        ldflags.extend(dep.ldflags.clone());
    }

    // ── 2. Project-level extra flags ─────────────────────────────────────
    cflags.extend(build_cfg.extra_cflags.clone());
    ldflags.extend(build_cfg.extra_ldflags.clone());

    // ── 3. Platform-conditional flags ────────────────────────────────────
    let os_key = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => other,
    };
    if let Some(plat) = build_cfg.platform.get(os_key) {
        cflags.extend(plat.cflags.clone());
        ldflags.extend(plat.ldflags.clone());
        for (key, val) in &plat.defines {
            if val.is_empty() {
                cflags.push(format!("-D{key}"));
            } else {
                cflags.push(format!("-D{key}={val}"));
            }
        }
    }

    // ── 4. Per-target flags (match target or "*") ────────────────────────
    // "*" applies to all targets
    if let Some(all_flags) = build_cfg.target_cflags.get("*") {
        cflags.extend(all_flags.clone());
    }
    if let Some(tgt_flags) = build_cfg.target_cflags.get(target_name) {
        cflags.extend(tgt_flags.clone());
    }
    if let Some(all_ld) = build_cfg.target_ldflags.get("*") {
        ldflags.extend(all_ld.clone());
    }
    if let Some(tgt_ld) = build_cfg.target_ldflags.get(target_name) {
        ldflags.extend(tgt_ld.clone());
    }

    // ── 5. Include paths ─────────────────────────────────────────────────
    for inc in include_paths {
        cflags.push(format!("-I{}", inc.display()));
    }

    // ── 6. Library paths + link libraries (from dep tree) ────────────────
    let mut lib_paths_dedup = HashSet::new();
    let mut link_libs_dedup = HashSet::new();
    for dep in deps {
        for lp in &dep.library_paths {
            if lib_paths_dedup.insert(lp.clone()) {
                ldflags.push(format!("-L{}", lp.display()));
            }
        }
        for ll in &dep.link_libraries {
            if link_libs_dedup.insert(ll.clone()) {
                ldflags.push(format!("-l{ll}"));
            }
        }
    }

    // ── 7. Standard flags ────────────────────────────────────────────────
    // Language standard (per-file, normalized)
    if is_cpp {
        if let Some(ref cpp_std) = build_cfg.cpp_standard {
            if !cpp_std.is_empty() {
                cflags.push(format!("-std={}", normalize_std(cpp_std, true)));
            }
        }
    } else {
        let c_std = &build_cfg.c_standard;
        if !c_std.is_empty() {
            cflags.push(format!("-std={}", normalize_std(c_std, false)));
        }
    }

    // Optimization
    if release {
        let opt = &build_cfg.opt_level;
        cflags.push(format!("-O{opt}"));
    } else {
        cflags.push("-O0".to_string());
    }

    // Debug symbols
    if build_cfg.debug && !release {
        cflags.push("-g".to_string());
    }

    // Warnings
    if build_cfg.warnings {
        cflags.push("-Wall".to_string());
        cflags.push("-Wextra".to_string());
    }

    // Defines
    for (key, val) in &build_cfg.defines {
        if val.is_empty() {
            cflags.push(format!("-D{key}"));
        } else {
            cflags.push(format!("-D{key}={val}"));
        }
    }

    // Sanitizers
    for san in &build_cfg.sanitizers {
        cflags.push(format!("-fsanitize={san}"));
        // Sanitizers also need to be passed at link time
        ldflags.push(format!("-fsanitize={san}"));
    }

    // LTO
    if build_cfg.lto {
        cflags.push("-flto".to_string());
        ldflags.push("-flto".to_string());
    }

    // PIC
    if build_cfg.pic {
        cflags.push("-fPIC".to_string());
    }

    // Auto-detect -pthread: if any dependency has -pthread in its cflags,
    // propagate it.
    let needs_pthread = deps
        .iter()
        .any(|d| d.cflags.iter().any(|f| f == "-pthread"));
    if needs_pthread {
        cflags.push("-pthread".to_string());
        ldflags.push("-pthread".to_string());
    }

    // Deduplicate while preserving order
    cflags = dedup_ordered(cflags);
    ldflags = dedup_ordered(ldflags);

    Flags { cflags, ldflags }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deduplicate a vector while preserving insertion order.
fn dedup_ordered<T: Eq + std::hash::Hash + Clone>(v: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    v.into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Per-file language detection
// ---------------------------------------------------------------------------

/// Returns `true` if the file extension indicates C++, `false` for C,
/// `None` if the extension is ambiguous (header files, no extension).
fn is_cpp_source(path: &std::path::Path) -> Option<bool> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("cpp") | Some("cxx") | Some("cc") | Some("C") | Some("c++") => Some(true),
        Some("c") | Some("i") => Some(false),
        _ => None,
    }
}

/// Normalize a standard version string into the full `-std=` flag value.
///
/// Accepts shorthand ("11", "17", "20", "23") and expands to the full
/// form ("c11", "c++20", etc.) based on whether this is a C++ file.
/// Also passes through gnu variants ("gnu17", "gnu++20") and raw forms.
fn normalize_std(std: &str, is_cpp: bool) -> String {
    let s = std.trim();
    // Already fully qualified
    if s.starts_with("c++") || s.starts_with("gnu++") || s.starts_with("c") || s.starts_with("gnu")
    {
        if s.starts_with("gnu") && is_cpp && !s.contains("++") {
            // "gnu17" in C++ context → "gnu++17"
            return format!("gnu++{}", &s[3..]);
        }
        return s.to_string();
    }
    // Shorthand: "11", "17", "20", "23", "2b", "2c"
    if is_cpp {
        format!("c++{s}")
    } else {
        format!("c{s}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HutConfig, PackageMeta, WorkspaceConfig};
    use crate::package::{BuildConfig, PlatformBuildConfig, ResolvedDependency};
    use std::collections::BTreeMap;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_basic_config() -> HutConfig {
        HutConfig {
            package: PackageMeta {
                name: "testproj".into(),
                version: "0.1.0".into(),
                description: None,
                authors: vec![],
                license: None,
                language: "c".into(),
                repository: None,
                homepage: None,
                sources: vec![],
                includes: vec![],
            },
            dependencies: BTreeMap::new(),
            build_dependencies: BTreeMap::new(),
            test_dependencies: BTreeMap::new(),
            build: BuildConfig::default(),
            scripts: BTreeMap::new(),
            workspace: WorkspaceConfig::default(),
        }
    }

    fn make_dep(
        name: &str,
        path: &std::path::Path,
        cflags: Vec<String>,
        ldflags: Vec<String>,
    ) -> ResolvedDependency {
        ResolvedDependency {
            name: name.into(),
            version: "1.0.0".into(),
            path: path.to_path_buf(),
            package: crate::package::Package {
                name: name.into(),
                version: "1.0.0".into(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                homepage: None,
                sources: vec![],
                includes: vec![],
                dependencies: BTreeMap::new(),
                build_dependencies: BTreeMap::new(),
                test_dependencies: BTreeMap::new(),
                build: Default::default(),
                scripts: BTreeMap::new(),
                libraries: vec![],
                executables: vec![],
                tests: vec![],
                cflags: cflags.clone(),
                ldflags: ldflags.clone(),
            },
            include_paths: vec![],
            library_paths: vec![],
            link_libraries: vec![],
            cflags,
            ldflags,
        }
    }

    #[test]
    fn test_empty_flags() {
        let cfg = make_basic_config();
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        // Basic flags should always be present
        assert!(flags.cflags.iter().any(|f| f == "-O0"));
        assert!(flags.cflags.iter().any(|f| f == "-g"));
        assert!(flags.cflags.iter().any(|f| f == "-Wall"));
        assert!(flags.cflags.iter().any(|f| f == "-Wextra"));
    }

    #[test]
    fn test_language_standard_c() {
        let mut cfg = make_basic_config();
        cfg.package.language = "c".into();
        cfg.build.c_standard = "c17".into();
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-std=c17"));
    }

    #[test]
    fn test_language_standard_cpp() {
        let mut cfg = make_basic_config();
        cfg.package.language = "c++".into();
        cfg.build.cpp_standard = Some("c++20".into());
        let src = std::path::Path::new("src/main.cpp");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-std=c++20"));
    }

    #[test]
    fn test_release_optimization() {
        let cfg = make_basic_config();
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, true);
        assert!(flags.cflags.iter().any(|f| f == "-O2"));
        // Debug symbols should NOT be present in release
        assert!(!flags.cflags.iter().any(|f| f == "-g"));
    }

    #[test]
    fn test_sanitizers() {
        let mut cfg = make_basic_config();
        cfg.build.sanitizers = vec!["address".into(), "undefined".into()];
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-fsanitize=address"));
        assert!(flags.cflags.iter().any(|f| f == "-fsanitize=undefined"));
        assert!(flags.ldflags.iter().any(|f| f == "-fsanitize=address"));
    }

    #[test]
    fn test_lto() {
        let mut cfg = make_basic_config();
        cfg.build.lto = true;
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-flto"));
        assert!(flags.ldflags.iter().any(|f| f == "-flto"));
    }

    #[test]
    fn test_pic() {
        let mut cfg = make_basic_config();
        cfg.build.pic = true;
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-fPIC"));
    }

    #[test]
    fn test_per_target_flags() {
        let mut cfg = make_basic_config();
        cfg.build.target_cflags.insert(
            "mylib".into(),
            vec!["-fno-rtti".into(), "-march=native".into()],
        );
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "mylib", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-fno-rtti"));
        assert!(flags.cflags.iter().any(|f| f == "-march=native"));
    }

    #[test]
    fn test_wildcard_target_flags() {
        let mut cfg = make_basic_config();
        cfg.build
            .target_cflags
            .insert("*".into(), vec!["-Werror".into()]);
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "another_target", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-Werror"));
    }

    #[test]
    fn test_platform_flags() {
        let mut cfg = make_basic_config();
        let mut plat = PlatformBuildConfig::default();
        plat.cflags = vec!["-DPLATFORM_TEST".into()];
        plat.ldflags = vec!["-ldl".into()];
        // Use the current OS so the test exercises the real platform pathway.
        let os_key = std::env::consts::OS;
        cfg.build.platform.insert(os_key.to_string(), plat);

        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-DPLATFORM_TEST"));
        assert!(flags.ldflags.iter().any(|f| f == "-ldl"));
    }

    #[test]
    fn test_defines() {
        let mut cfg = make_basic_config();
        cfg.build.defines.insert("DEBUG".into(), "1".into());
        cfg.build.defines.insert("VERSION".into(), "2.0".into());
        cfg.build.defines.insert("FEATURE_X".into(), "".into());
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        assert!(flags.cflags.iter().any(|f| f == "-DDEBUG=1"));
        assert!(flags.cflags.iter().any(|f| f == "-DVERSION=2.0"));
        assert!(flags.cflags.iter().any(|f| f == "-DFEATURE_X"));
    }

    #[test]
    fn test_dependency_flag_inheritance() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("deplib");
        std::fs::create_dir_all(&dep_dir).unwrap();

        let dep = make_dep(
            "deplib",
            &dep_dir,
            vec!["-pthread".into(), "-DUSE_DEP".into()],
            vec!["-ldl".into()],
        );

        let cfg = make_basic_config();
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[dep], "testproj", &[], src, false);

        // Inherited cflags
        assert!(flags.cflags.iter().any(|f| f == "-DUSE_DEP"));
        // -pthread from dep should be auto-detected and added
        assert!(flags.cflags.iter().any(|f| f == "-pthread"));
        assert!(flags.ldflags.iter().any(|f| f == "-pthread"));
        // Inherited ldflags
        assert!(flags.ldflags.iter().any(|f| f == "-ldl"));
    }

    #[test]
    fn test_include_paths_in_flags() {
        let cfg = make_basic_config();
        let includes = vec![
            PathBuf::from("/usr/local/include"),
            PathBuf::from("./include"),
        ];
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &includes, src, false);
        assert!(flags.cflags.iter().any(|f| f == "-I/usr/local/include"));
        assert!(flags.cflags.iter().any(|f| f == "-I./include"));
    }

    #[test]
    fn test_library_paths_and_libs() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("mylib");
        std::fs::create_dir_all(&dep_dir).unwrap();

        let mut dep = make_dep("mylib", &dep_dir, vec![], vec![]);
        dep.library_paths = vec![dep_dir.join("lib")];
        dep.link_libraries = vec!["m".into(), "pthread".into()];

        let cfg = make_basic_config();
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[dep], "testproj", &[], src, false);

        assert!(flags.ldflags.iter().any(|f| f.starts_with("-L")));
        assert!(flags.ldflags.iter().any(|f| f == "-lm"));
        assert!(flags.ldflags.iter().any(|f| f == "-lpthread"));
    }

    #[test]
    fn test_dedup_preserves_order() {
        let mut cfg = make_basic_config();
        // Add duplicate defines — should only appear once
        cfg.build.extra_cflags = vec!["-DFOO".into(), "-DBAR".into(), "-DFOO".into()];
        let src = std::path::Path::new("src/main.c");
        let flags = collect_flags(&cfg, &[], "testproj", &[], src, false);
        let foo_count = flags.cflags.iter().filter(|f| *f == "-DFOO").count();
        assert_eq!(foo_count, 1, "-DFOO should not be duplicated");
    }

    #[test]
    fn test_total_len() {
        let flags = Flags {
            cflags: vec!["-O2".into(), "-g".into()],
            ldflags: vec!["-lm".into()],
        };
        // "-O2"(3+1) + "-g"(2+1) + "-lm"(3+1) = 11
        assert_eq!(flags.total_len(), 11);
    }

    #[test]
    fn test_normalize_std_shorthand_c() {
        assert_eq!(normalize_std("11", false), "c11");
        assert_eq!(normalize_std("17", false), "c17");
        assert_eq!(normalize_std("23", false), "c23");
    }

    #[test]
    fn test_normalize_std_shorthand_cpp() {
        assert_eq!(normalize_std("11", true), "c++11");
        assert_eq!(normalize_std("17", true), "c++17");
        assert_eq!(normalize_std("20", true), "c++20");
        assert_eq!(normalize_std("23", true), "c++23");
    }

    #[test]
    fn test_normalize_std_full_forms() {
        assert_eq!(normalize_std("c17", false), "c17");
        assert_eq!(normalize_std("c++20", true), "c++20");
        assert_eq!(normalize_std("gnu17", false), "gnu17");
        assert_eq!(normalize_std("gnu++17", true), "gnu++17");
    }

    #[test]
    fn test_normalize_std_gnu_cpp_auto() {
        // "gnu17" in C++ context → "gnu++17"
        assert_eq!(normalize_std("gnu17", true), "gnu++17");
    }

    #[test]
    fn test_is_cpp_source() {
        assert_eq!(is_cpp_source(Path::new("main.cpp")), Some(true));
        assert_eq!(is_cpp_source(Path::new("main.cxx")), Some(true));
        assert_eq!(is_cpp_source(Path::new("main.cc")), Some(true));
        assert_eq!(is_cpp_source(Path::new("main.c")), Some(false));
        assert_eq!(is_cpp_source(Path::new("main.h")), None);
        assert_eq!(is_cpp_source(Path::new("main")), None);
    }

    #[test]
    fn test_per_file_std_flag() {
        let mut cfg = make_basic_config();
        cfg.build.c_standard = "11".into();
        cfg.build.cpp_standard = Some("20".into());
        cfg.package.language = "c++".into();

        // .c file with C++ project → gets C standard (per-file detection)
        let cf = collect_flags(&cfg, &[], "app", &[], Path::new("src/main.c"), false);
        assert!(cf.cflags.contains(&"-std=c11".to_string()));

        // .cpp file → gets C++ standard
        let cppf = collect_flags(&cfg, &[], "app", &[], Path::new("src/main.cpp"), false);
        assert!(cppf.cflags.contains(&"-std=c++20".to_string()));

        // .h file → falls back to project language (C++)
        let hf = collect_flags(&cfg, &[], "app", &[], Path::new("src/types.h"), false);
        assert!(hf.cflags.contains(&"-std=c++20".to_string()));
    }
}
