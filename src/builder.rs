use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use colored::Colorize;
use tokio::sync::Semaphore;
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

// ---------------------------------------------------------------------------
// Collect source files
// ---------------------------------------------------------------------------

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
// Build system auto-detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildSystem {
    Cmake,
    Make,
    Hut,
}

fn detect_build_system(config: &HutConfig, project_root: &Path) -> BuildSystem {
    let system = config.build.system.as_str();

    if system == "cmake" {
        return BuildSystem::Cmake;
    }
    if system == "make" {
        return BuildSystem::Make;
    }
    if system == "hut" {
        return BuildSystem::Hut;
    }

    // "auto" — sniff filesystem
    if project_root.join("CMakeLists.txt").exists() {
        return BuildSystem::Cmake;
    }
    if project_root.join("Makefile").exists()
        || project_root.join("makefile").exists()
        || project_root.join("GNUmakefile").exists()
    {
        return BuildSystem::Make;
    }

    BuildSystem::Hut
}

// ---------------------------------------------------------------------------
// Compile / link flags — delegated to the flags module
// ---------------------------------------------------------------------------

// (build_compiler_flags and build_linker_flags have been replaced by
//  flags::collect_flags() — see src/flags.rs)

// ---------------------------------------------------------------------------
// Output paths
// ---------------------------------------------------------------------------

fn output_dir(project_root: &Path, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    project_root.join("target").join(profile)
}

fn object_dir(project_root: &Path, release: bool) -> PathBuf {
    output_dir(project_root, release).join("build")
}

fn source_to_object(source: &Path, project_root: &Path, release: bool) -> PathBuf {
    let rel = source
        .strip_prefix(project_root)
        .unwrap_or(source.file_name().unwrap().as_ref());
    let mut obj = object_dir(project_root, release).join(rel);
    obj.set_extension("o");
    obj
}

/// Number of parallel jobs to use (based on available CPUs)
fn parallel_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

// ---------------------------------------------------------------------------
// CMake build
// ---------------------------------------------------------------------------

async fn build_cmake(config: &HutConfig, project_root: &Path, release: bool) -> HutResult<()> {
    let out_dir = output_dir(project_root, release);
    std::fs::create_dir_all(&out_dir)?;

    let build_type = if release { "Release" } else { "Debug" };

    println!(
        "{} cmake project {}",
        "   Running".bold().cyan(),
        config.package.name.bold()
    );

    // Configure
    let configure_status = Command::new("cmake")
        .arg(format!("-DCMAKE_BUILD_TYPE={build_type}"))
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", out_dir.display()))
        .arg("-B")
        .arg(&out_dir)
        .arg("-S")
        .arg(project_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| HutError::Build(format!("Failed to run cmake configure: {e}")))?;

    if !configure_status.status.success() {
        let stderr = String::from_utf8_lossy(&configure_status.stderr);
        return Err(HutError::Build(format!(
            "CMake configure failed:\n{stderr}"
        )));
    }

    // Build
    let build_status = Command::new("cmake")
        .arg("--build")
        .arg(&out_dir)
        .arg("--config")
        .arg(build_type)
        .arg("--parallel")
        .arg(parallel_jobs().to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| HutError::Build(format!("Failed to run cmake build: {e}")))?;

    if !build_status.status.success() {
        let stderr = String::from_utf8_lossy(&build_status.stderr);
        return Err(HutError::Build(format!("CMake build failed:\n{stderr}")));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Make build
// ---------------------------------------------------------------------------

async fn build_make(
    config: &HutConfig,
    project_root: &Path,
    release: bool,
    compiler: &Compiler,
    include_paths: &[PathBuf],
) -> HutResult<()> {
    println!(
        "{} make project {}",
        "   Running".bold().cyan(),
        config.package.name.bold()
    );

    let mut cmd = Command::new("make");
    cmd.current_dir(project_root);

    // Pass CC/CXX through environment
    cmd.env("CC", &compiler.cc);
    cmd.env("CXX", &compiler.cxx);

    if release {
        cmd.arg(format!(
            "CFLAGS=-O{} -DNDEBUG",
            if config.build.opt_level.is_empty() {
                "2"
            } else {
                &config.build.opt_level
            }
        ));
        cmd.arg(format!(
            "CXXFLAGS=-O{} -DNDEBUG",
            if config.build.opt_level.is_empty() {
                "2"
            } else {
                &config.build.opt_level
            }
        ));
    } else {
        cmd.arg("CFLAGS=-O0 -g");
        cmd.arg("CXXFLAGS=-O0 -g");
    }

    // Pass include paths
    if !include_paths.is_empty() {
        let inc_flags: Vec<String> = include_paths
            .iter()
            .map(|p| format!("-I{}", p.display()))
            .collect();
        cmd.arg(format!("CPPFLAGS={}", inc_flags.join(" ")));
    }

    let status = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| HutError::Build(format!("Failed to run make: {e}")))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(HutError::Build(format!("Make failed:\n{stderr}")));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Hut direct build
// ---------------------------------------------------------------------------

async fn build_hut(
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
    // Per-file compilation may override some flags.
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

    // Compile all source files in parallel
    let parallel_jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let semaphore = Arc::new(Semaphore::new(parallel_jobs));
    let mut compile_handles = Vec::new();

    for source in &sources {
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
        let source_path = source.clone();
        let sem = Arc::clone(&semaphore);

        // Build per-file flags (per-target flags match against source file)
        let file_flags = flags::collect_flags(
            config,
            deps,
            target_name,
            &all_includes,
            &source_path,
            release,
        );

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            // Ensure parent directory exists
            if let Some(parent) = obj_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            let relative = source_path
                .strip_prefix(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .unwrap_or(&source_path);

            println!(
                "{} {}",
                "   Compiling".bold().green(),
                relative.display().to_string().dimmed()
            );

            let mut cmd = Command::new(&compiler_exe);
            cmd.arg("-c");
            cmd.arg(&source_path);
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
                    HutError::Build(format!("Failed to compile {}: {e}", source_path.display()))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(HutError::Build(format!(
                    "Compilation of {} failed:\n{stderr}",
                    source_path.display()
                )));
            }

            Ok::<PathBuf, HutError>(obj_path)
        });

        compile_handles.push(handle);
    }

    // Await all compilations
    let mut object_files: Vec<PathBuf> = Vec::new();
    for handle in compile_handles {
        match handle.await {
            Ok(Ok(obj)) => object_files.push(obj),
            Ok(Err(e)) => return Err(e),
            Err(join_err) => {
                return Err(HutError::Build(format!(
                    "Compilation task panicked: {join_err}"
                )));
            }
        }
    }

    // Link step
    // Check if this project is a library or executable (default to executable)
    let is_library = false; // TODO: detect from config
    let linking_label = if is_library {
        format!("lib{target_name}.a")
    } else {
        target_name.clone()
    };
    let output_path = out_dir.join(&linking_label);

    println!(
        "{} {}",
        "    Linking".bold().yellow(),
        linking_label.dimmed()
    );

    if is_library {
        // Create static library with ar
        let mut ar_cmd = Command::new(&compiler.ar);
        ar_cmd.arg("rcs");
        ar_cmd.arg(&output_path);
        for obj in &object_files {
            ar_cmd.arg(obj);
        }
        let ar_output = ar_cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| HutError::Build(format!("Failed to run ar: {e}")))?;

        if !ar_output.status.success() {
            let stderr = String::from_utf8_lossy(&ar_output.stderr);
            return Err(HutError::Build(format!(
                "Archive creation failed:\n{stderr}"
            )));
        }
    } else {
        // Link executable
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
///
/// Automatically detects the build system (CMake, Make, or hut direct mode),
/// compiles all sources in parallel, and links the final output.
pub async fn build_project(
    config: &HutConfig,
    deps: &[ResolvedDependency],
    release: bool,
) -> HutResult<()> {
    let project_root = std::env::current_dir()
        .map_err(|e| HutError::Build(format!("Cannot get current directory: {e}")))?;

    // Detect compiler upfront (needed for all build systems)
    let compiler = detect_compiler(&config.build.compiler)?;

    let build_system = detect_build_system(config, &project_root);

    let profile_name = if release { "release" } else { "debug" };

    println!(
        "{} [{}] {} v{}",
        "   Building".bold().cyan(),
        profile_name.bold(),
        config.package.name.bold().white(),
        config.package.version.dimmed()
    );

    match build_system {
        BuildSystem::Cmake => {
            build_cmake(config, &project_root, release).await?;
        }
        BuildSystem::Make => {
            let include_paths = include::resolve_includes(deps, &project_root);
            build_make(config, &project_root, release, &compiler, &include_paths).await?;
        }
        BuildSystem::Hut => {
            build_hut(config, deps, &project_root, release, &compiler).await?;
        }
    }

    let out_dir = output_dir(&project_root, release);

    println!(
        "{} {} target(s) in {:.2}s",
        "    Finished".bold().green(),
        profile_name.dimmed(),
        0.0 // TODO: track actual build time
    );

    println!(
        "  {} {}",
        "Output:".dimmed(),
        out_dir.display().to_string().bold()
    );

    Ok(())
}
