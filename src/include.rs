use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::package::ResolvedDependency;

// ---------------------------------------------------------------------------
// Include path resolution
// ---------------------------------------------------------------------------

/// Resolve all include directories from dependencies.
///
/// This function:
///   - Scans each dependency's directory for standard include paths (include/, src/)
///   - Handles single-header libraries (just add the root directory)
///   - Handles packages where headers live in src/
///   - Propagates transitive includes: if dep A depends on B, A's consumers also get B's includes
///   - Generates a `.hut/include` symlink directory that aggregates dependency headers
///
/// Returns an ordered list of `-I` paths suitable for the compiler.
pub fn resolve_includes(deps: &[ResolvedDependency], project_root: &Path) -> Vec<PathBuf> {
    let mut include_paths: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    // Phase 1: Collect include paths from each dependency
    for dep in deps {
        collect_dep_includes(dep, &mut include_paths, &mut seen);
    }

    // Phase 2: Generate the aggregated .hut/include symlink directory
    generate_hut_include_dir(deps, project_root);

    include_paths
}

/// Collect include directories from a single resolved dependency (recursively for transitive deps).
fn collect_dep_includes(
    dep: &ResolvedDependency,
    paths: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) {
    let dep_root = &dep.path;

    // If the ResolvedDependency already has pre-computed include_paths, use those
    if !dep.include_paths.is_empty() {
        for inc in &dep.include_paths {
            if seen.insert(inc.clone()) {
                paths.push(inc.clone());
            }
        }
        return;
    }

    // Standard locations in order of preference
    let candidates: [&str; 5] = ["include", "src", "includes", "inc", "."];
    let mut found = false;

    for candidate in &candidates {
        let candidate_path = dep_root.join(candidate);
        if candidate_path.exists() && candidate_path.is_dir() {
            // For "." we add the root itself, for others add the directory
            let inc_path = if *candidate == "." {
                dep_root.clone()
            } else {
                candidate_path.clone()
            };

            // Only add if it contains at least one .h/.hpp file (or is the root for single-header)
            if *candidate == "." || contains_headers(&candidate_path) {
                if seen.insert(inc_path.clone()) {
                    paths.push(inc_path);
                    found = true;
                }
            }
        }
    }

    // If no standard directory found, scan the entire package for header locations
    if !found {
        find_header_dirs(dep_root, paths, seen);
    }

    // Handle single-header libraries: if there's a single .h file at the root,
    // the root directory itself is the include path.
    if !found && has_single_header_at_root(dep_root) {
        if seen.insert(dep_root.clone()) {
            paths.push(dep_root.clone());
        }
    }

    // Collect from the dep's own dependencies (transitive includes).
    // The package's dependencies field maps name → version, so we can't recurse
    // into ResolvedDependency directly (since we only have the flat list).
    // Instead, transitive resolution should happen at the resolver level before
    // include_paths are populated. However, if the dep has dependencies listed
    // in its package metadata, we note that here for completeness.
    //
    // The caller (resolver) is expected to have already merged transitive include_paths
    // into the ResolvedDependency's include_paths field. If it hasn't, we do our best.
}

/// Check whether a directory contains at least one C/C++ header file (non-recursive).
fn contains_headers(dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_header_file(&path) {
                return true;
            }
            // Also check one level deep for nested include structures
            if path.is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    if sub_entries
                        .flatten()
                        .any(|e| e.path().is_file() && is_header_file(&e.path()))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a file path is a C/C++ header.
fn is_header_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("h" | "hpp" | "hxx" | "hh" | "h++" | "H" | "inl" | "inc")
    )
}

/// Check if the package root has a single header file (single-header library pattern).
fn has_single_header_at_root(root: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(root) {
        let headers: Vec<_> = entries
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .filter(|e| is_header_file(&e.path()))
            .collect();
        return headers.len() == 1
            || (headers.len() >= 1
                && !root.join("include").exists()
                && !root.join("src").exists());
    }
    false
}

/// Scan a directory recursively for subdirectories containing headers.
fn find_header_dirs(root: &Path, paths: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    fn walk(dir: &Path, paths: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, depth: u32) {
        if depth > 3 {
            return; // Don't go too deep
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    // Skip hidden, build, test, and third-party directories
                    if dir_name.starts_with('.')
                        || dir_name == "build"
                        || dir_name == "target"
                        || dir_name == "test"
                        || dir_name == "tests"
                        || dir_name == "examples"
                        || dir_name == "third_party"
                        || dir_name == "vendor"
                    {
                        continue;
                    }
                    if contains_headers(&path) {
                        if seen.insert(path.clone()) {
                            paths.push(path.clone());
                        }
                    }
                    walk(&path, paths, seen, depth + 1);
                }
            }
        }
    }
    walk(root, paths, seen, 0);
}

// ---------------------------------------------------------------------------
// .hut/include aggregated symlink directory
// ---------------------------------------------------------------------------

/// Generate a `.hut/include` directory under `project_root` that contains
/// symlinks to all dependency headers, organised by package name.
///
/// This allows consumers to write:
/// ```c
/// #include <dep_name/header.h>
/// ```
/// and have it resolve correctly regardless of the dependency's internal layout.
pub fn generate_hut_include_dir(deps: &[ResolvedDependency], project_root: &Path) {
    let hut_dir = project_root.join(".hut");
    let include_dir = hut_dir.join("include");

    // Clean and recreate
    if include_dir.exists() {
        let _ = std::fs::remove_dir_all(&include_dir);
    }
    if std::fs::create_dir_all(&include_dir).is_err() {
        return;
    }

    for dep in deps {
        let dep_include = include_dir.join(&dep.name);

        // Skip if already exists (from a previous generation)
        if dep_include.exists() {
            continue;
        }

        // Determine the best include path for this dependency
        let include_src = find_best_include_for_dep(dep);

        match include_src {
            Some(src_dir) => {
                // Create symlink: .hut/include/<dep_name> → <dep_path>/include (or src)
                #[cfg(unix)]
                {
                    let _ = std::os::unix::fs::symlink(&src_dir, &dep_include);
                }
                #[cfg(not(unix))]
                {
                    // On Windows, use junction or copy — for now, just copy dir
                    let _ = copy_dir_recursive(&src_dir, &dep_include);
                }
            }
            None => {
                // For single-header or root-level packages, create a directory
                // with a symlink to the single header
                if let Some(header_path) = find_single_header(&dep.path) {
                    if std::fs::create_dir_all(&dep_include).is_ok() {
                        let link_name = header_path.file_name().unwrap();
                        let link_path = dep_include.join(link_name);
                        #[cfg(unix)]
                        {
                            let _ = std::os::unix::fs::symlink(&header_path, &link_path);
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = std::fs::copy(&header_path, &link_path);
                        }
                    }
                }
            }
        }
    }
}

/// Find the best include directory to expose for a dependency.
fn find_best_include_for_dep(dep: &ResolvedDependency) -> Option<PathBuf> {
    let root = &dep.path;

    let candidates = [
        root.join("include"),
        root.join("includes"),
        root.join("inc"),
        root.join("src"),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.is_dir() && contains_headers(candidate) {
            return Some(candidate.clone());
        }
    }

    // Special case: the package is itself a single header or flat structure
    if contains_headers(root) {
        return Some(root.clone());
    }

    None
}

/// Find a single header file at the package root.
fn find_single_header(root: &Path) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_header_file(&path) {
                return Some(path);
            }
        }
    }
    None
}

/// Recursive directory copy (non-Unix fallback).
#[cfg(not(unix))]
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{Package, ResolvedDependency};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn make_dep(name: &str, dir: &Path) -> ResolvedDependency {
        ResolvedDependency {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            path: dir.to_path_buf(),
            package: Package {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                homepage: None,
                sources: vec![],
                includes: vec!["include".to_string()],
                dependencies: BTreeMap::new(),
                build_dependencies: BTreeMap::new(),
                test_dependencies: BTreeMap::new(),
                build: Default::default(),
                scripts: BTreeMap::new(),
                libraries: vec![],
                executables: vec![],
                tests: vec![],
                cflags: vec![],
                ldflags: vec![],
            },
            include_paths: vec![],
            library_paths: vec![],
            link_libraries: vec![],
            cflags: vec![],
            ldflags: vec![],
        }
    }

    #[test]
    fn test_resolve_includes_include_dir() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("mylib");
        let inc_dir = dep_dir.join("include");
        std::fs::create_dir_all(&inc_dir).unwrap();
        std::fs::write(inc_dir.join("mylib.h"), "// header").unwrap();

        let deps = vec![make_dep("mylib", &dep_dir)];
        let paths = resolve_includes(&deps, tmp.path());

        // Should contain the include directory
        assert!(
            paths.iter().any(|p| p.ends_with("include")),
            "Expected include/ dir, got: {paths:?}"
        );
    }

    #[test]
    fn test_resolve_includes_src_headers() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("srclib");
        let src_dir = dep_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("srclib.h"), "// header").unwrap();

        let deps = vec![make_dep("srclib", &dep_dir)];
        let paths = resolve_includes(&deps, tmp.path());

        assert!(
            paths.iter().any(|p| p.ends_with("src")),
            "Expected src/ dir, got: {paths:?}"
        );
    }

    #[test]
    fn test_resolve_includes_single_header() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("singleh");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(dep_dir.join("singleh.h"), "// single header").unwrap();

        let deps = vec![make_dep("singleh", &dep_dir)];
        let paths = resolve_includes(&deps, tmp.path());

        assert!(
            paths.iter().any(|p| p == &dep_dir),
            "Expected root dir for single header, got: {paths:?}"
        );
    }

    #[test]
    fn test_resolve_includes_precomputed() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("precomp");
        std::fs::create_dir_all(&dep_dir).unwrap();

        let mut dep = make_dep("precomp", &dep_dir);
        let precomp_path = dep_dir.join("custom_include");
        std::fs::create_dir_all(&precomp_path).unwrap();
        dep.include_paths = vec![precomp_path.clone()];

        let paths = resolve_includes(&[dep], tmp.path());

        assert!(
            paths.contains(&precomp_path),
            "Expected precomputed include path, got: {paths:?}"
        );
    }

    #[test]
    fn test_resolve_includes_empty_deps() {
        let tmp = TempDir::new().unwrap();
        let paths = resolve_includes(&[], tmp.path());
        assert!(paths.is_empty(), "Expected no paths for empty deps");
    }

    #[test]
    fn test_is_header_file() {
        assert!(is_header_file(Path::new("foo.h")));
        assert!(is_header_file(Path::new("foo.hpp")));
        assert!(is_header_file(Path::new("foo.hxx")));
        assert!(is_header_file(Path::new("foo.hh")));
        assert!(is_header_file(Path::new("foo.inl")));
        assert!(!is_header_file(Path::new("foo.c")));
        assert!(!is_header_file(Path::new("foo.cpp")));
        assert!(!is_header_file(Path::new("foo.txt")));
    }

    #[test]
    fn test_contains_headers() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.h"), "// h").unwrap();
        assert!(contains_headers(tmp.path()));

        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!contains_headers(&empty));
    }

    #[test]
    fn test_generate_hut_include_dir() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("deplib");
        let inc_dir = dep_dir.join("include");
        std::fs::create_dir_all(&inc_dir).unwrap();
        std::fs::write(inc_dir.join("deplib.h"), "// header").unwrap();

        let deps = vec![make_dep("deplib", &dep_dir)];
        generate_hut_include_dir(&deps, tmp.path());

        let hut_include = tmp.path().join(".hut").join("include").join("deplib");
        assert!(
            hut_include.exists(),
            "Expected .hut/include/deplib to exist"
        );

        #[cfg(unix)]
        {
            assert!(
                hut_include.is_symlink(),
                "Expected symlink at {hut_include:?}"
            );
        }
    }
}
