use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use colored::Colorize;
use walkdir::WalkDir;

use crate::config::HutConfig;
use crate::error::{HutError, HutResult};
use crate::flags;
use crate::include;
use crate::package::ResolvedDependency;

// ---------------------------------------------------------------------------
// Compiler detection
// ---------------------------------------------------------------------------

/// Detected compiler toolchain
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Compiler {
    cc: String,  // e.g. "gcc", "clang"
    cxx: String, // e.g. "g++", "clang++"
    ar: String,  // e.g. "ar"
    is_clang: bool,
}

fn detect_compiler(preference: &str) -> HutResult<Compiler> {
    // 1. Environment variables always take priority
    let cc_env = std::env::var("CC").ok();
    let cxx_env = std::env::var("CXX").ok();

    if let (Some(cc), Some(cxx)) = (&cc_env, &cxx_env) {
        if command_exists(cc) && command_exists(cxx) {
            return Ok(Compiler {
                is_clang: cc.contains("clang"),
                cc: cc.clone(),
                cxx: cxx.clone(),
                ar: detect_ar(),
            });
        }
    }
    if let Some(cc) = &cc_env {
        if command_exists(cc) {
            return Ok(Compiler {
                is_clang: cc.contains("clang"),
                cc: cc.clone(),
                cxx: infer_cxx(cc),
                ar: detect_ar(),
            });
        }
    }

    // 2. Explicit preference from hut.toml
    let candidates: &[&str] = match preference {
        "clang" => &["clang", "clang++"],
        "gcc" => &["gcc", "g++"],
        _ => {
            // "auto": try CC/CXX env first (already done), then gcc, then clang, then cc
            &["gcc", "clang", "cc"]
        }
    };

    for candidate in candidates {
        if command_exists(candidate) {
            let is_clang = candidate.contains("clang");
            return Ok(Compiler {
                is_clang,
                cc: candidate.to_string(),
                cxx: infer_cxx(candidate),
                ar: detect_ar(),
            });
        }
    }

    Err(HutError::NoCompiler)
}

fn detect_ar() -> String {
    if command_exists("ar") {
        "ar".to_string()
    } else if command_exists("llvm-ar") {
        "llvm-ar".to_string()
    } else {
        "ar".to_string()
    }
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn infer_cxx(cc: &str) -> String {
    match cc {
        "gcc" => "g++".to_string(),
        "clang" => "clang++".to_string(),
        "cc" => "c++".to_string(),
        other => {
            if other.ends_with("gcc") {
                other.replace("gcc", "g++")
            } else if other.contains("clang") && !other.contains("++") {
                format!("{other}++")
            } else {
                other.to_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Collect source files
// ---------------------------------------------------------------------------

fn is_c_file(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("c"))
}

fn is_cpp_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("cpp" | "cxx" | "cc" | "c++" | "C" | "c++m" | "cppm")
    )
}

pub fn is_source_file(path: &Path) -> bool {
    is_c_file(path) || is_cpp_file(path)
}

/// Collect all .c / .cpp files from the configured source directories (or default "src/")
pub fn collect_sources(config: &HutConfig, project_root: &Path) -> HutResult<Vec<PathBuf>> {
    let source_dirs: Vec<&str> = vec!["src"];

    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for dir_name in &source_dirs {
        let dir = project_root.join(dir_name);
        if !dir.exists() {
            continue;
        }

        for entry in WalkDir::new(&dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            if is_source_file(&path) && seen.insert(path.clone()) {
                files.push(path);
            }
        }
    }

    if files.is_empty() {
        // Also check the project root itself for loose source files
        for entry in WalkDir::new(project_root)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            if is_source_file(&path) && seen.insert(path.clone()) {
                files.push(path);
            }
        }
    }

    let _ = config; // may be used later for per-package source config
    Ok(files)
}

// ---------------------------------------------------------------------------
// Output paths
// ---------------------------------------------------------------------------

fn output_dir(project_root: &Path, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    project_root.join("target").join(profile)
}

fn object_dir(project_root: &Path, _release: bool) -> PathBuf {
    project_root.join("target").join(".build")
}

fn source_to_object(source: &Path, project_root: &Path, release: bool) -> PathBuf {
    let rel = source
        .strip_prefix(project_root)
        .unwrap_or(source.file_name().unwrap().as_ref());
    let mut obj = object_dir(project_root, release).join(rel);
    obj.set_extension("o");
    obj
}

/// Check if an object artifact is newer than its source file (no recompilation needed).
fn is_object_fresh(source: &Path, object: &Path) -> bool {
    if !object.exists() {
        return false;
    }
    let src_modified = match std::fs::metadata(source).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let obj_modified = match std::fs::metadata(object).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return false,
    };
    obj_modified >= src_modified
}

// ---------------------------------------------------------------------------
// Hut build (the only build system)
// ---------------------------------------------------------------------------

fn build(
    config: &HutConfig,
    deps: &[ResolvedDependency],
    project_root: &Path,
    release: bool,
    compiler: &Compiler,
) -> HutResult<()> {
    // Resolve include paths
    let include_paths = include::resolve_includes(deps, project_root);
    let project_include = project_root.join("include");
    let project_src = project_root.join("src");

    let mut all_includes = include_paths.clone();
    if project_include.exists() {
        all_includes.push(project_include);
    }
    if project_src.exists() {
        all_includes.push(project_src);
    }

    // Collect source files
    let sources = collect_sources(config, project_root)?;
    if sources.is_empty() {
        return Err(HutError::Build(
            "No C/C++ source files found. Add .c or .cpp files to src/.".to_string(),
        ));
    }

    // Prepare output directories
    let obj_dir = object_dir(project_root, release);
    let out_dir = output_dir(project_root, release);
    std::fs::create_dir_all(&obj_dir)?;
    std::fs::create_dir_all(&out_dir)?;

    let target_name = &config.package.name;

    // Collect the global set of flags (used for linking and as a baseline).
    let all_flags = flags::collect_flags(
        config,
        deps,
        target_name,
        &all_includes,
        project_root, // dummy path for global flags
        release,
    );

    // Determine if this is C++
    let is_cpp = matches!(config.package.language.as_str(), "c++" | "cpp");

    // Determine which files need recompilation (check .o freshness)
    let mut fresh_files = Vec::new();
    let mut stale_sources = Vec::new();

    for source in &sources {
        let obj_path = source_to_object(source, project_root, release);
        if is_object_fresh(source, &obj_path) {
            fresh_files.push(obj_path);
        } else {
            stale_sources.push(source.clone());
        }
    }

    if !fresh_files.is_empty() {
        println!(
            "{} {} file(s) cached (unchanged)",
            "   Cached".bold().dimmed(),
            fresh_files.len()
        );
    }

    // Compile stale source files in parallel using rayon
    use rayon::prelude::*;

    let compile_results: Vec<HutResult<PathBuf>> = stale_sources
        .par_iter()
        .map(|source| {
            // Per-file compiler selection: .c → always cc, .cpp → always cxx,
            // ambiguous extensions (.h, etc.) → fall back to project language.
            let is_cpp_file = if is_c_file(source) {
                false
            } else if is_cpp_file(source) {
                true
            } else {
                is_cpp
            };
            let compiler_exe = if is_cpp_file {
                compiler.cxx.clone()
            } else {
                compiler.cc.clone()
            };

            let obj_path = source_to_object(source, project_root, release);

            // Build per-file flags
            let file_flags = flags::collect_flags(
                config,
                deps,
                target_name,
                &all_includes,
                source,
                release,
            );

            // Ensure parent directory exists
            if let Some(parent) = obj_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            let relative = source.strip_prefix(project_root).unwrap_or(source);

            println!(
                "{} {}",
                "   Compiling".bold().green(),
                relative.display().to_string().dimmed()
            );

            let mut cmd = Command::new(&compiler_exe);
            cmd.arg("-c");
            cmd.arg(source);
            cmd.arg("-o");
            cmd.arg(&obj_path);

            // Use response file if command line would be too long (> 32KB)
            if file_flags.total_len() > 32 * 1024 {
                let rsp_path = obj_path.with_extension("rsp");
                let rsp_content = file_flags
                    .cflags
                    .iter()
                    .map(|f| escape_rsp_arg(f))
                    .collect::<Vec<_>>()
                    .join("\n");
                std::fs::write(&rsp_path, &rsp_content).ok();
                cmd.arg(format!("@{}", rsp_path.display()));
            } else {
                for flag in &file_flags.cflags {
                    cmd.arg(flag);
                }
            }

            let output = cmd
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .map_err(|e| {
                    HutError::Build(format!("Failed to compile {}: {e}", source.display()))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(HutError::Build(format!(
                    "Compilation of {} failed:\n{stderr}",
                    source.display()
                )));
            }

            Ok(obj_path)
        })
        .collect();

    // Collect results — abort on first error
    let fresh_count = fresh_files.len();
    let mut object_files: Vec<PathBuf> = fresh_files; // pre-load cached .o files
    for result in compile_results {
        match result {
            Ok(obj) => object_files.push(obj),
            Err(e) => return Err(e),
        }
    }

    // Check if the target binary is also fresh (no recompilation needed, no link needed)
    let linking_label = if false {
        // TODO: detect from config
        format!("lib{target_name}.a")
    } else {
        target_name.clone()
    };
    let output_path = out_dir.join(&linking_label);

    if fresh_count == sources.len() && output_path.exists() {
        // All sources cached AND binary exists — check if binary is newer than all sources
        let binary_fresh = sources.iter().all(|s| is_object_fresh(s, &output_path));
        if binary_fresh {
            println!(
                "{} target(s) unchanged — nothing to do",
                "   Skipped".bold().dimmed()
            );
            return Ok(());
        }
    }

    // Link step
    println!(
        "{} {}",
        "    Linking".bold().yellow(),
        linking_label.dimmed()
    );

    {
        let linker_exe = if sources.iter().any(|s| is_cpp_file(s)) {
            &compiler.cxx
        } else {
            &compiler.cc
        };

        let mut link_cmd = Command::new(linker_exe);
        link_cmd.arg("-o");
        link_cmd.arg(&output_path);
        for obj in &object_files {
            link_cmd.arg(obj);
        }

        // Use response file for linker flags if needed
        if all_flags.total_len() > 32 * 1024 {
            let rsp_path = output_path.with_extension("rsp");
            let mut rsp_lines: Vec<String> = Vec::new();
            for f in &all_flags.cflags {
                rsp_lines.push(escape_rsp_arg(f));
            }
            for f in &all_flags.ldflags {
                rsp_lines.push(escape_rsp_arg(f));
            }
            std::fs::write(&rsp_path, rsp_lines.join("\n")).ok();
            link_cmd.arg(format!("@{}", rsp_path.display()));
        } else {
            for flag in &all_flags.cflags {
                link_cmd.arg(flag);
            }
            for flag in &all_flags.ldflags {
                link_cmd.arg(flag);
            }
        }

        let link_output = link_cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| HutError::Build(format!("Failed to run linker: {e}")))?;

        if !link_output.status.success() {
            let stderr = String::from_utf8_lossy(&link_output.stderr);
            return Err(HutError::Build(format!("Linking failed:\n{stderr}")));
        }

        // On Unix, make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&output_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&output_path, perms)?;
        }
    }

    Ok(())
}

/// Escape an argument for use inside a GCC/Clang response file.
/// Response files use whitespace and backslash escaping similar to the shell.
fn escape_rsp_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('\\') || arg.contains('"') {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Build the project at the given root directory.
pub async fn build_project(
    config: &HutConfig,
    deps: &[ResolvedDependency],
    release: bool,
) -> HutResult<()> {
    let project_root = std::env::current_dir()
        .map_err(|e| HutError::Build(format!("Cannot get current directory: {e}")))?;

    // Detect compiler
    let compiler = detect_compiler(&config.build.compiler)?;

    let profile_color = if release {
        colored::Color::Yellow
    } else {
        colored::Color::BrightBlue
    };
    let profile_name = if release { "release" } else { "debug" };
    let build_start = std::time::Instant::now();

    println!(
        "{} [{profile}] {} v{}",
        "   Building".bold().cyan(),
        config.package.name.bold(),
        config.package.version.dimmed(),
        profile = profile_name.color(profile_color).bold()
    );

    build(config, deps, &project_root, release, &compiler)?;

    let out_dir = output_dir(&project_root, release);
    let binary = out_dir.join(&config.package.name);
    let elapsed = build_start.elapsed();

    println!(
        "{} [{profile}] target(s) in {:.2}s",
        "    Finished".bold().green(),
        elapsed.as_secs_f64(),
        profile = profile_name.color(profile_color).bold()
    );

    if binary.exists() {
        println!(
            "  {} {}",
            "Binary:".dimmed(),
            binary.display().to_string().bold()
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HutConfig, PackageMeta, WorkspaceConfig};
    use crate::package::BuildConfig;
    use std::collections::BTreeMap;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_test_config(name: &str) -> HutConfig {
        HutConfig {
            package: PackageMeta {
                name: name.into(),
                version: "0.1.0".into(),
                description: None,
                authors: vec![],
                license: None,
                language: "c".into(),
                repository: None,
                homepage: None,
                sources: vec![],
                includes: vec!["include".into()],
            },
            dependencies: BTreeMap::new(),
            build_dependencies: BTreeMap::new(),
            test_dependencies: BTreeMap::new(),
            build: BuildConfig::default(),
            scripts: BTreeMap::new(),
            workspace: WorkspaceConfig::default(),
        }
    }

    // -----------------------------------------------------------------------
    // is_source_file / is_c_file / is_cpp_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_source_file_c() {
        assert!(is_source_file(Path::new("main.c")));
        assert!(is_source_file(Path::new("src/util.c")));
    }

    #[test]
    fn test_is_source_file_h_is_not_source() {
        // .h and .hpp are NOT source files (they're headers)
        assert!(!is_source_file(Path::new("header.h")));
        assert!(!is_source_file(Path::new("header.hpp")));
    }

    #[test]
    fn test_is_source_file_cpp_variants() {
        assert!(is_source_file(Path::new("main.cpp")));
        assert!(is_source_file(Path::new("main.cc")));
        assert!(is_source_file(Path::new("main.cxx")));
        assert!(is_source_file(Path::new("main.c++")));
    }

    #[test]
    fn test_is_c_file_only_matches_c() {
        assert!(is_c_file(Path::new("file.c")));
        assert!(!is_c_file(Path::new("file.h")));
        assert!(!is_c_file(Path::new("file.cpp")));
        assert!(!is_c_file(Path::new("file.cc")));
    }

    #[test]
    fn test_is_cpp_file_variants() {
        assert!(is_cpp_file(Path::new("file.cpp")));
        assert!(is_cpp_file(Path::new("file.cc")));
        assert!(is_cpp_file(Path::new("file.cxx")));
        assert!(is_cpp_file(Path::new("file.c++")));
        assert!(!is_cpp_file(Path::new("file.c")));
        assert!(!is_cpp_file(Path::new("file.h")));
    }

    // -----------------------------------------------------------------------
    // collect_sources
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_sources_empty_dir() {
        let tmp = TempDir::new().unwrap();
        // Create empty src/ directory
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        let config = make_test_config("empty");
        let sources = collect_sources(&config, tmp.path()).unwrap();
        assert!(sources.is_empty());
    }

    #[test]
    fn test_collect_sources_single_c_file() {
        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.c"), "int main() { return 0; }").unwrap();

        let config = make_test_config("single");
        let sources = collect_sources(&config, tmp.path()).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].ends_with("main.c"));
    }

    #[test]
    fn test_collect_sources_mixed_c_and_cpp() {
        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.c"), "int main() { return 0; }").unwrap();
        std::fs::write(src_dir.join("util.cpp"), "int util() { return 0; }").unwrap();
        std::fs::write(src_dir.join("helper.h"), "// header").unwrap();

        let config = make_test_config("mixed");
        let sources = collect_sources(&config, tmp.path()).unwrap();
        assert_eq!(sources.len(), 2);
        let names: Vec<&str> = sources
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"main.c"));
        assert!(names.contains(&"util.cpp"));
    }

    #[test]
    fn test_collect_sources_nested_directories() {
        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(src_dir.join("sub")).unwrap();
        std::fs::write(src_dir.join("main.c"), "int main() { return 0; }").unwrap();
        std::fs::write(src_dir.join("sub/deep.c"), "int deep() { return 0; }").unwrap();

        let config = make_test_config("nested");
        let sources = collect_sources(&config, tmp.path()).unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_collect_sources_no_src_dir_falls_back_to_root() {
        let tmp = TempDir::new().unwrap();
        // No src/ directory — put file at project root
        std::fs::write(tmp.path().join("standalone.c"), "int main() { return 0; }").unwrap();

        let config = make_test_config("standalone");
        let sources = collect_sources(&config, tmp.path()).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].ends_with("standalone.c"));
    }

    // -----------------------------------------------------------------------
    // escape_rsp_arg
    // -----------------------------------------------------------------------

    #[test]
    fn test_escape_rsp_arg_plain() {
        assert_eq!(escape_rsp_arg("-O2"), "-O2");
        assert_eq!(escape_rsp_arg("-std=c17"), "-std=c17");
        assert_eq!(escape_rsp_arg("-DFOO=bar"), "-DFOO=bar");
    }

    #[test]
    fn test_escape_rsp_arg_with_spaces() {
        // Arguments containing spaces get quoted
        let escaped = escape_rsp_arg("-I/path with spaces/include");
        assert!(escaped.starts_with('"'));
        assert!(escaped.ends_with('"'));
    }

    #[test]
    fn test_escape_rsp_arg_with_quotes() {
        // Arguments with embedded quotes get escaped
        let escaped = escape_rsp_arg("-DFOO=\"bar\"");
        assert!(escaped.contains("\\\""));
    }

    #[test]
    fn test_escape_rsp_arg_empty() {
        assert_eq!(escape_rsp_arg(""), "");
    }

    // -----------------------------------------------------------------------
    // output_dir / object_dir / source_to_object
    // -----------------------------------------------------------------------

    #[test]
    fn test_output_dir_debug() {
        let root = Path::new("/test/project");
        let out = output_dir(root, false);
        assert_eq!(out, Path::new("/test/project/target/debug"));
    }

    #[test]
    fn test_output_dir_release() {
        let root = Path::new("/test/project");
        let out = output_dir(root, true);
        assert_eq!(out, Path::new("/test/project/target/release"));
    }

    #[test]
    fn test_object_dir() {
        let root = Path::new("/test/project");
        let obj = object_dir(root, false);
        assert_eq!(obj, Path::new("/test/project/target/.build"));
        // release flag doesn't change object dir
        let obj_rel = object_dir(root, true);
        assert_eq!(obj_rel, Path::new("/test/project/target/.build"));
    }

    #[test]
    fn test_source_to_object() {
        let root = Path::new("/test/project");
        let source = Path::new("/test/project/src/main.c");
        let obj = source_to_object(source, root, false);
        assert_eq!(obj, Path::new("/test/project/target/.build/src/main.o"));
    }

    #[test]
    fn test_source_to_object_release() {
        let root = Path::new("/test/project");
        let source = Path::new("/test/project/src/main.c");
        let obj = source_to_object(source, root, true);
        // object_dir doesn't differ by profile, so same path
        assert_eq!(obj, Path::new("/test/project/target/.build/src/main.o"));
    }

    // -----------------------------------------------------------------------
    // infer_cxx
    // -----------------------------------------------------------------------

    #[test]
    fn test_infer_cxx_gcc() {
        assert_eq!(infer_cxx("gcc"), "g++");
    }

    #[test]
    fn test_infer_cxx_clang() {
        assert_eq!(infer_cxx("clang"), "clang++");
    }

    #[test]
    fn test_infer_cxx_cc() {
        assert_eq!(infer_cxx("cc"), "c++");
    }

    #[test]
    fn test_infer_cxx_custom_suffix() {
        assert_eq!(infer_cxx("x86_64-linux-gnu-gcc"), "x86_64-linux-gnu-g++");
    }

    #[test]
    fn test_infer_cxx_custom_clang_suffix() {
        assert_eq!(
            infer_cxx("x86_64-linux-gnu-clang"),
            "x86_64-linux-gnu-clang++"
        );
    }

    #[test]
    fn test_infer_cxx_unknown() {
        // Unknown prefix is returned as-is
        assert_eq!(infer_cxx("zig-cc"), "zig-cc");
    }

    // -----------------------------------------------------------------------
    // command_exists
    // -----------------------------------------------------------------------

    #[test]
    fn test_command_exists_shell() {
        // 'sh' should exist on essentially all Unix systems
        let result = command_exists("sh");
        assert!(result, "expected 'sh' to exist on this system");
    }

    #[test]
    fn test_command_exists_nonexistent() {
        let result = command_exists("this_command_definitely_does_not_exist_xyzzy");
        assert!(!result);
    }

    // -----------------------------------------------------------------------
    // detect_ar
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_ar_returns_string() {
        let ar = detect_ar();
        assert!(!ar.is_empty());
        // Should be "ar" or "llvm-ar"
        assert!(
            ar == "ar" || ar == "llvm-ar",
            "Expected 'ar' or 'llvm-ar', got '{ar}'"
        );
    }
}
