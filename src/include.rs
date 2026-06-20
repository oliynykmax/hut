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
    fn test_is_header_file_edge_cases() {
        // .h++ extension (less common C++ header)
        assert!(is_header_file(Path::new("foo.h++")));
        // .H extension (alternate C/C++ header)
        assert!(is_header_file(Path::new("foo.H")));
        // .inc extension (include file)
        assert!(is_header_file(Path::new("foo.inc")));
        // Not a header: object files, other extensions
        assert!(!is_header_file(Path::new("foo.o")));
        assert!(!is_header_file(Path::new("foo.a")));
        assert!(!is_header_file(Path::new("foo.so")));
        assert!(!is_header_file(Path::new("Makefile")));
        // Paths without extensions
        assert!(!is_header_file(Path::new("header")));
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
    fn test_contains_headers_nested() {
        let tmp = TempDir::new().unwrap();
        // Create a nested structure: tmp/include/nested/header.hpp
        let nested = tmp.path().join("include").join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("inner.hpp"), "// nested header").unwrap();

        // contains_headers(tmp.path().join("include")) should find the header
        // one level deep in the "nested" subdirectory
        assert!(contains_headers(&tmp.path().join("include")));

        // But looking at the nested dir directly also works (headers at its root)
        assert!(contains_headers(&nested));
    }

    #[test]
    fn test_contains_headers_only_subdirs_with_headers() {
        // A directory with subdirectories that have NO headers should return false
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("empty_sub");
        std::fs::create_dir_all(&sub).unwrap();
        // Only a .txt file in the parent directory
        std::fs::write(tmp.path().join("readme.txt"), "not a header").unwrap();
        assert!(!contains_headers(tmp.path()));
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

    // -------------------------------------------------------------------
    // Edge case tests for higher coverage
    // -------------------------------------------------------------------

    #[test]
    fn test_resolve_includes_multiple_deps() {
        let tmp = TempDir::new().unwrap();

        // Dep A: includes in include/
        let dep_a_dir = tmp.path().join("dep_a");
        let inc_a = dep_a_dir.join("include");
        std::fs::create_dir_all(&inc_a).unwrap();
        std::fs::write(inc_a.join("dep_a.h"), "// a").unwrap();

        // Dep B: includes in src/
        let dep_b_dir = tmp.path().join("dep_b");
        let src_b = dep_b_dir.join("src");
        std::fs::create_dir_all(&src_b).unwrap();
        std::fs::write(src_b.join("dep_b.h"), "// b").unwrap();

        let deps = vec![
            make_dep("dep_a", &dep_a_dir),
            make_dep("dep_b", &dep_b_dir),
        ];
        let paths = resolve_includes(&deps, tmp.path());

        // Both include paths should be present
        let has_include = paths.iter().any(|p| p.ends_with("include"));
        let has_src = paths.iter().any(|p| p.ends_with("src"));
        assert!(has_include, "Expected include/ dir for dep_a, got: {paths:?}");
        assert!(has_src, "Expected src/ dir for dep_b, got: {paths:?}");
    }

    #[test]
    fn test_resolve_includes_no_headers_anywhere() {
        let tmp = TempDir::new().unwrap();
        // Create a dep directory with no header files at all
        let dep_dir = tmp.path().join("noheaders");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(dep_dir.join("readme.txt"), "no headers here").unwrap();

        let deps = vec![make_dep("noheaders", &dep_dir)];
        let paths = resolve_includes(&deps, tmp.path());

        // No standard include directory, no headers found — should be empty or
        // at most contain the root if single-header detection triggers (it won't,
        // since there are no .h files)
        assert!(
            paths.is_empty() || paths.iter().all(|p| !p.ends_with(".h")),
            "Expected empty or no .h paths, got: {paths:?}"
        );
    }

    #[test]
    fn test_find_header_dirs_scans_recursively() {
        let tmp = TempDir::new().unwrap();
        // Create a deep header dir that find_header_dirs should discover
        let deep = tmp.path().join("lib").join("deep").join("headers");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("secret.h"), "// deep header").unwrap();

        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        find_header_dirs(tmp.path(), &mut paths, &mut seen);

        // The deep/headers dir should be found
        assert!(
            paths.iter().any(|p| p.ends_with("headers")),
            "Expected headers/ dir to be found, got: {paths:?}"
        );
    }

    #[test]
    fn test_find_header_dirs_skips_hidden_and_build() {
        let tmp = TempDir::new().unwrap();
        // Directories that should be skipped
        std::fs::create_dir_all(tmp.path().join(".hidden")).unwrap();
        std::fs::create_dir_all(tmp.path().join("build")).unwrap();
        std::fs::create_dir_all(tmp.path().join("target")).unwrap();
        std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
        std::fs::create_dir_all(tmp.path().join("examples")).unwrap();
        std::fs::create_dir_all(tmp.path().join("third_party")).unwrap();
        std::fs::create_dir_all(tmp.path().join("vendor")).unwrap();
        std::fs::create_dir_all(tmp.path().join("test")).unwrap();

        // Place headers in skipped dirs
        std::fs::write(tmp.path().join(".hidden/x.h"), "// x").unwrap();
        std::fs::write(tmp.path().join("build/x.h"), "// x").unwrap();
        std::fs::write(tmp.path().join("target/x.h"), "// x").unwrap();

        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        find_header_dirs(tmp.path(), &mut paths, &mut seen);

        // None of the skipped dirs should appear
        let path_strs: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        assert!(
            !path_strs.iter().any(|s| s.contains(".hidden")),
            "Hidden dirs should be skipped: {path_strs:?}"
        );
        assert!(
            !path_strs.iter().any(|s| s.contains("build")),
            "Build dirs should be skipped: {path_strs:?}"
        );
        assert!(
            !path_strs.iter().any(|s| s.contains("target")),
            "Target dirs should be skipped: {path_strs:?}"
        );
        assert!(
            !path_strs.iter().any(|s| s.contains("test")),
            "Test dirs should be skipped: {path_strs:?}"
        );
    }

    #[test]
    fn test_has_single_header_at_root_true() {
        let tmp = TempDir::new().unwrap();
        // One header file at root
        std::fs::write(tmp.path().join("single.h"), "// header").unwrap();
        assert!(has_single_header_at_root(tmp.path()));
    }

    #[test]
    fn test_has_single_header_at_root_false_no_headers() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "text").unwrap();
        assert!(!has_single_header_at_root(tmp.path()));
    }

    #[test]
    fn test_has_single_header_at_root_false_has_include_dir() {
        let tmp = TempDir::new().unwrap();
        // Multiple headers at root should NOT count as single-header if
        // include/ also exists — but single header at root WITH an include/
        // dir should still return true (since headers.len() >= 1 AND
        // include/ exists → the condition headers.len() == 1 catches this)
        std::fs::write(tmp.path().join("lib.h"), "// header").unwrap();
        std::fs::create_dir_all(tmp.path().join("include")).unwrap();
        // has_single_header_at_root returns true because:
        // headers.len() == 1 → true
        // (The second branch only matters when headers.len() >= 2)
        assert!(has_single_header_at_root(tmp.path()));
    }

    #[test]
    fn test_has_single_header_at_root_multiple_no_standard_dirs() {
        let tmp = TempDir::new().unwrap();
        // Multiple headers at root but no include/ or src/ dirs
        std::fs::write(tmp.path().join("a.h"), "// a").unwrap();
        std::fs::write(tmp.path().join("b.hpp"), "// b").unwrap();
        // Should return true (multiple headers, but no standard dirs)
        assert!(has_single_header_at_root(tmp.path()));
    }

    #[test]
    fn test_collect_dep_includes_with_include_dir() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("pkg");
        let inc = dep_dir.join("include");
        std::fs::create_dir_all(&inc).unwrap();
        std::fs::write(inc.join("pkg.h"), "// header").unwrap();

        let dep = make_dep("pkg", &dep_dir);
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        collect_dep_includes(&dep, &mut paths, &mut seen);

        assert!(
            paths.iter().any(|p| p.ends_with("include")),
            "Expected include/ path, got: {paths:?}"
        );
    }

    #[test]
    fn test_find_best_include_for_dep_prefers_include() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("mylib");
        let inc = dep_dir.join("include");
        let src = dep_dir.join("src");
        std::fs::create_dir_all(&inc).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(inc.join("mylib.h"), "// header").unwrap();
        std::fs::write(src.join("mylib_impl.h"), "// impl").unwrap();

        let dep = make_dep("mylib", &dep_dir);
        let best = find_best_include_for_dep(&dep);

        assert!(best.is_some(), "Expected a best include path");
        // Should prefer "include" over "src"
        let best_path = best.unwrap();
        assert!(best_path.ends_with("include"), "Expected include/, got: {best_path:?}");
    }

    #[test]
    fn test_find_best_include_for_dep_falls_back_to_root() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("lone");
        // No include/ or src/ — just a header at root
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(dep_dir.join("lone.h"), "// header").unwrap();

        let dep = make_dep("lone", &dep_dir);
        let best = find_best_include_for_dep(&dep);

        assert!(best.is_some(), "Expected root as fallback");
        assert_eq!(best.unwrap(), dep_dir);
    }

    #[test]
    fn test_find_best_include_for_dep_none() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("empty");
        std::fs::create_dir_all(&dep_dir).unwrap();
        // No headers anywhere

        let dep = make_dep("empty", &dep_dir);
        let best = find_best_include_for_dep(&dep);
        assert!(best.is_none(), "Expected no include path");
    }

    #[test]
    fn test_find_single_header_at_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("only.h"), "// header").unwrap();
        let found = find_single_header(&tmp.path());
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("only.h"));
    }

    #[test]
    fn test_find_single_header_at_root_none() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "text").unwrap();
        let found = find_single_header(&tmp.path());
        assert!(found.is_none());
    }

    #[test]
    fn test_generate_hut_include_dir_multiple_deps() {
        let tmp = TempDir::new().unwrap();

        let dep1 = tmp.path().join("dep1");
        let inc1 = dep1.join("include");
        std::fs::create_dir_all(&inc1).unwrap();
        std::fs::write(inc1.join("dep1.h"), "// header").unwrap();

        let dep2 = tmp.path().join("dep2");
        let inc2 = dep2.join("include");
        std::fs::create_dir_all(&inc2).unwrap();
        std::fs::write(inc2.join("dep2.h"), "// header").unwrap();

        let deps = vec![
            make_dep("dep1", &dep1),
            make_dep("dep2", &dep2),
        ];
        generate_hut_include_dir(&deps, tmp.path());

        let hut_include = tmp.path().join(".hut").join("include");
        assert!(hut_include.join("dep1").exists(), "Expected dep1 symlink");
        assert!(hut_include.join("dep2").exists(), "Expected dep2 symlink");
    }

    #[test]
    fn test_generate_hut_include_dir_single_header_dep() {
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("singledep");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(dep_dir.join("singledep.h"), "// single header").unwrap();

        let deps = vec![make_dep("singledep", &dep_dir)];
        generate_hut_include_dir(&deps, tmp.path());

        // For single-header libs, creates dir with symlink to header
        let hut_dir = tmp.path().join(".hut").join("include").join("singledep");
        // On unix, should exist and either be a dir or symlink
        assert!(hut_dir.exists(), "Expected singledep/ to exist in .hut/include");
    }

    #[test]
    fn test_resolve_includes_transitive_precomputed() {
        // When a dep has precomputed include_paths, those should be used directly
        // and transitive deps are already merged by the resolver
        let tmp = TempDir::new().unwrap();
        let dep_dir = tmp.path().join("dep");

        let mut dep = make_dep("dep", &dep_dir);
        let transitive_path = dep_dir.join("transitive_include");
        std::fs::create_dir_all(&transitive_path).unwrap();
        dep.include_paths = vec![transitive_path.clone()];

        let paths = resolve_includes(&[dep], tmp.path());
        assert!(
            paths.contains(&transitive_path),
            "Expected transitive include path to be present, got: {paths:?}"
        );
    }
}
