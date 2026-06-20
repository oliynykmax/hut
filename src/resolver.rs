use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::config::HutConfig;
use crate::error::{HutError, HutResult};
use crate::lockfile::Lockfile;
use crate::package::ResolvedDependency;
use crate::registry::{self, RegistryIndex, resolve_version};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve all dependencies (direct + transitive) for a project.
///
/// Uses a work-queue to walk the full dependency tree without recursion,
/// avoiding the infinite-future-size problem of recursive async functions.
///
/// * `config`   — the project manifest (`hut.toml`).
/// * `lockfile` — existing lock file; used to honour pinned versions.
/// * `registry` — the package registry index.
/// * `cache_dir`— where cloned package sources live
///   (usually `~/.hut/packages`).
pub async fn resolve_dependencies(
    config: &HutConfig,
    lockfile: &Lockfile,
    registry: &RegistryIndex,
    cache_dir: &Path,
) -> HutResult<Vec<ResolvedDependency>> {
    let mut ctx = ResolveContext::new(registry, lockfile, cache_dir);

    // Seed the work queue with direct dependencies.
    let mut queue: Vec<(String, String)> = config
        .dependencies
        .iter()
        .chain(config.build_dependencies.iter())
        .chain(config.test_dependencies.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Work through the queue.  New transitive dependencies are appended
    // as they are discovered.
    while let Some((name, constraint)) = queue.pop() {
        if ctx.packages.contains_key(&name) {
            continue; // already resolved
        }

        let (resolved, transitive) = ctx.resolve_one_package(&name, &constraint).await?;

        ctx.packages
            .insert(name.clone(), Arc::new(Mutex::new(resolved)));

        // Push newly discovered transitive deps onto the queue.
        for (t_name, t_constraint) in transitive {
            if !ctx.packages.contains_key(&t_name) {
                queue.push((t_name, t_constraint));
            }
        }
    }

    // Assemble the flat result in deterministic order.
    let mut result: Vec<ResolvedDependency> = ctx
        .packages
        .into_values()
        .map(|arc| arc.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal resolution context
// ---------------------------------------------------------------------------

struct ResolveContext<'a> {
    registry: &'a RegistryIndex,
    lockfile: &'a Lockfile,
    cache_dir: &'a Path,

    /// Fully resolved packages (name → ResolvedDependency).
    packages: HashMap<String, Arc<Mutex<ResolvedDependency>>>,

    /// Versions already selected (for conflict detection).
    selected_versions: HashMap<String, String>,

    /// Constraints seen for each package (for conflict messages).
    constraints_seen: HashMap<String, Vec<String>>,
}

impl<'a> ResolveContext<'a> {
    fn new(registry: &'a RegistryIndex, lockfile: &'a Lockfile, cache_dir: &'a Path) -> Self {
        Self {
            registry,
            lockfile,
            cache_dir,
            packages: HashMap::new(),
            selected_versions: HashMap::new(),
            constraints_seen: HashMap::new(),
        }
    }

    /// Resolve a single package (no recursion).
    ///
    /// Returns the resolved dependency and a list of transitive
    /// (name, constraint) pairs that still need resolving.
    async fn resolve_one_package(
        &mut self,
        name: &str,
        constraint: &str,
    ) -> HutResult<(ResolvedDependency, Vec<(String, String)>)> {
        // ── look up the entry in the registry ────────────────────────
        let entry = self
            .registry
            .find(name)
            .ok_or_else(|| HutError::PackageNotFound(name.to_string()))?;

        // ── determine target version ─────────────────────────────────
        let target_version = if let Some(locked) = self.lockfile.get(name) {
            if !version_satisfies(&locked.version, constraint)? {
                resolve_version(entry, constraint)?
            } else {
                locked.version.clone()
            }
        } else {
            resolve_version(entry, constraint)?
        };

        // ── conflict check ───────────────────────────────────────────
        if let Some(prev) = self.selected_versions.get(name) {
            if prev != &target_version {
                let prev_constraints = self.constraints_seen.get(name).cloned().unwrap_or_default();
                let all_constraints: Vec<&str> = prev_constraints
                    .iter()
                    .map(|s| s.as_str())
                    .chain(std::iter::once(constraint))
                    .collect();

                let chosen = resolve_version(entry, constraint)?;
                if chosen != *prev {
                    let prev_ok = all_constraints
                        .iter()
                        .all(|c| version_satisfies(prev, c).unwrap_or(false));
                    let new_ok = all_constraints
                        .iter()
                        .all(|c| version_satisfies(&chosen, c).unwrap_or(false));

                    if !prev_ok && !new_ok {
                        return Err(HutError::Resolution(format!(
                            "Version conflict for '{name}': \
                             {prev} was selected (from {prev_constraints:?}), \
                             but '{constraint}' requires {chosen} — \
                             no version satisfies all constraints"
                        )));
                    }
                    // Otherwise one or both satisfy — we'll keep the
                    // highest compatible version.
                }
            }
        }

        // Record the selection.
        self.selected_versions
            .insert(name.to_string(), target_version.clone());
        self.constraints_seen
            .entry(name.to_string())
            .or_default()
            .push(constraint.to_string());

        // ── fetch package metadata ───────────────────────────────────
        let version_info = entry
            .versions
            .get(&target_version)
            .ok_or_else(|| HutError::VersionNotFound(name.to_string(), target_version.clone()))?;

        let pkg = registry::fetch_package_metadata(&entry.repository, &version_info.r#ref).await?;

        let pkg_path = self
            .cache_dir
            .join(sanitise_repo_name(&entry.repository))
            .join(&target_version);

        // ── collect transitive dependencies ──────────────────────────
        let mut transitive: Vec<(String, String)> = Vec::new();

        for (dep_name, dep_constraint) in &version_info.dependencies {
            if !self.packages.contains_key(dep_name)
                && !self.selected_versions.contains_key(dep_name)
            {
                transitive.push((dep_name.clone(), dep_constraint.clone()));
            }
        }

        let dep_map: BTreeMap<String, String> = pkg
            .dependencies
            .iter()
            .chain(pkg.build_dependencies.iter())
            .chain(pkg.test_dependencies.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (dep_name, dep_constraint) in &dep_map {
            if dep_name == name {
                continue;
            }
            if !self.packages.contains_key(dep_name)
                && !self.selected_versions.contains_key(dep_name)
            {
                transitive.push((dep_name.clone(), dep_constraint.clone()));
            }
        }

        // ── collect transitive paths from already-resolved deps ──────
        let mut transitive_include_paths: Vec<PathBuf> = Vec::new();
        let mut transitive_library_paths: Vec<PathBuf> = Vec::new();
        let mut transitive_link_libraries: Vec<String> = Vec::new();

        for dep_name in version_info.dependencies.keys() {
            if let Some(arc) = self.packages.get(dep_name) {
                let rd = arc.lock().unwrap_or_else(|e| e.into_inner());
                transitive_include_paths.extend(rd.include_paths.clone());
                transitive_library_paths.extend(rd.library_paths.clone());
                transitive_link_libraries.extend(rd.link_libraries.clone());
            }
        }
        for dep_name in dep_map.keys() {
            if let Some(arc) = self.packages.get(dep_name) {
                let rd = arc.lock().unwrap_or_else(|e| e.into_inner());
                transitive_include_paths.extend(rd.include_paths.clone());
                transitive_library_paths.extend(rd.library_paths.clone());
                transitive_link_libraries.extend(rd.link_libraries.clone());
            }
        }

        // ── own include / library paths ──────────────────────────────
        let mut own_include_paths: Vec<PathBuf> = pkg
            .includes
            .iter()
            .map(|inc| pkg_path.join(inc))
            .chain(pkg.sources.iter().map(|s| pkg_path.join(s)))
            .filter(|p| p.exists())
            .collect();

        if own_include_paths.is_empty() {
            own_include_paths.push(pkg_path.clone());
        }

        let own_library_paths: Vec<PathBuf> = {
            let build_dir = pkg_path.join("build");
            if build_dir.exists() {
                vec![build_dir]
            } else {
                vec![pkg_path.clone()]
            }
        };

        let own_link_libraries: Vec<String> =
            pkg.libraries.iter().map(|lib| lib.name.clone()).collect();

        // Merge own + transitive (own first).
        let mut include_paths = own_include_paths;
        include_paths.extend(transitive_include_paths);

        let mut library_paths = own_library_paths;
        library_paths.extend(transitive_library_paths);

        let mut link_libraries = own_link_libraries;
        link_libraries.extend(transitive_link_libraries);

        // Collect and merge cflags/ldflags from the package and transitive deps.
        let mut own_cflags: Vec<String> = pkg.cflags.clone();
        let mut own_ldflags: Vec<String> = pkg.ldflags.clone();
        for dep_name in version_info.dependencies.keys().chain(dep_map.keys()) {
            if let Some(arc) = self.packages.get(dep_name) {
                let rd = arc.lock().unwrap_or_else(|e| e.into_inner());
                // Inherit transitive flags (dedup later)
                own_cflags.extend(rd.cflags.clone());
                own_ldflags.extend(rd.ldflags.clone());
            }
        }

        // Deduplicate while preserving order.
        include_paths = dedup_vec(include_paths);
        library_paths = dedup_vec(library_paths);
        link_libraries = dedup_vec(link_libraries);
        own_cflags = dedup_vec(own_cflags);
        own_ldflags = dedup_vec(own_ldflags);

        let resolved_dep = ResolvedDependency {
            name: name.to_string(),
            version: target_version,
            path: pkg_path,
            package: pkg,
            include_paths,
            library_paths,
            link_libraries,
            cflags: own_cflags,
            ldflags: own_ldflags,
        };

        Ok((resolved_dep, transitive))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether `version` satisfies the given semver constraint.
fn version_satisfies(version: &str, constraint: &str) -> HutResult<bool> {
    let v = semver::Version::parse(version)?;

    let adjusted = if constraint == "*" {
        "*".to_string()
    } else if constraint.chars().next().map_or(false, |c| {
        c == '^' || c == '~' || c == '>' || c == '<' || c == '='
    }) {
        constraint.to_string()
    } else {
        format!("={constraint}")
    };

    let req = semver::VersionReq::parse(&adjusted).map_err(|e| {
        HutError::Resolution(format!("Invalid version constraint '{constraint}': {e}"))
    })?;

    Ok(req.matches(&v))
}

/// Deduplicate a vector while preserving insertion order.
fn dedup_vec<T: Eq + std::hash::Hash + Clone>(v: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    v.into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

/// Turn a git URL into a safe directory name.
fn sanitise_repo_name(url: &str) -> String {
    let raw = url.trim_end_matches('/').rsplit('/').next().unwrap_or(url);
    let stem = raw.strip_suffix(".git").unwrap_or(raw);

    stem.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // version_satisfies tests
    // -----------------------------------------------------------------------

    #[test]
    fn exact_version_match() {
        assert!(version_satisfies("1.2.3", "1.2.3").unwrap());
    }

    #[test]
    fn exact_version_mismatch() {
        assert!(!version_satisfies("1.2.3", "1.2.4").unwrap());
    }

    #[test]
    fn equals_constraint() {
        assert!(version_satisfies("1.0.0", "=1.0.0").unwrap());
        assert!(!version_satisfies("1.0.1", "=1.0.0").unwrap());
    }

    #[test]
    fn caret_constraint() {
        // ^1.2.3 means >=1.2.3, <2.0.0
        assert!(version_satisfies("1.2.3", "^1.2.3").unwrap());
        assert!(version_satisfies("1.9.0", "^1.2.3").unwrap());
        assert!(!version_satisfies("2.0.0", "^1.2.3").unwrap());
        assert!(!version_satisfies("0.1.0", "^1.2.3").unwrap());
    }

    #[test]
    fn caret_zero_major() {
        // ^0.2.3 means >=0.2.3, <0.3.0
        assert!(version_satisfies("0.2.3", "^0.2.3").unwrap());
        assert!(version_satisfies("0.2.9", "^0.2.3").unwrap());
        assert!(!version_satisfies("0.3.0", "^0.2.3").unwrap());
    }

    #[test]
    fn tilde_constraint() {
        // ~1.2.3 means >=1.2.3, <1.3.0
        assert!(version_satisfies("1.2.3", "~1.2.3").unwrap());
        assert!(version_satisfies("1.2.9", "~1.2.3").unwrap());
        assert!(!version_satisfies("1.3.0", "~1.2.3").unwrap());
    }

    #[test]
    fn tilde_minor_only() {
        // ~1.2 means >=1.2.0, <1.3.0
        assert!(version_satisfies("1.2.0", "~1.2").unwrap());
        assert!(version_satisfies("1.2.5", "~1.2").unwrap());
        assert!(!version_satisfies("1.3.0", "~1.2").unwrap());
    }

    #[test]
    fn greater_than_constraint() {
        assert!(version_satisfies("2.0.0", ">1.0.0").unwrap());
        assert!(!version_satisfies("1.0.0", ">1.0.0").unwrap());
        assert!(!version_satisfies("0.9.0", ">1.0.0").unwrap());
    }

    #[test]
    fn greater_than_or_equal() {
        assert!(version_satisfies("1.0.0", ">=1.0.0").unwrap());
        assert!(version_satisfies("2.0.0", ">=1.0.0").unwrap());
        assert!(!version_satisfies("0.9.0", ">=1.0.0").unwrap());
    }

    #[test]
    fn less_than_constraint() {
        assert!(version_satisfies("0.9.0", "<1.0.0").unwrap());
        assert!(!version_satisfies("1.0.0", "<1.0.0").unwrap());
        assert!(!version_satisfies("1.1.0", "<1.0.0").unwrap());
    }

    #[test]
    fn less_than_or_equal() {
        assert!(version_satisfies("1.0.0", "<=1.0.0").unwrap());
        assert!(version_satisfies("0.9.0", "<=1.0.0").unwrap());
        assert!(!version_satisfies("1.1.0", "<=1.0.0").unwrap());
    }

    #[test]
    fn wildcard_star_constraint() {
        assert!(version_satisfies("1.0.0", "*").unwrap());
        assert!(version_satisfies("0.0.1", "*").unwrap());
        assert!(version_satisfies("999.999.999", "*").unwrap());
    }

    #[test]
    fn compound_constraint() {
        // >=1.5.0, <2.0.0
        assert!(version_satisfies("1.5.0", ">=1.5.0, <2.0.0").unwrap());
        assert!(version_satisfies("1.9.9", ">=1.5.0, <2.0.0").unwrap());
        assert!(!version_satisfies("1.4.9", ">=1.5.0, <2.0.0").unwrap());
        assert!(!version_satisfies("2.0.0", ">=1.5.0, <2.0.0").unwrap());
    }

    #[test]
    fn bare_version_is_exact() {
        // Bare "1.2.3" is treated as "=1.2.3"
        assert!(version_satisfies("1.2.3", "1.2.3").unwrap());
        assert!(!version_satisfies("1.2.4", "1.2.3").unwrap());
    }

    #[test]
    fn pre_release_versions() {
        assert!(version_satisfies("1.0.0-alpha.1", ">=1.0.0-alpha").unwrap());
        assert!(version_satisfies("1.0.0", ">=1.0.0-alpha").unwrap());
    }

    #[test]
    fn invalid_version_returns_error() {
        let result = version_satisfies("not-a-version", ">=1.0");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HutError::Semver(_)));
    }

    #[test]
    fn invalid_constraint_returns_error() {
        let result = version_satisfies("1.0.0", "not-a-constraint!!!");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HutError::Resolution(_)));
    }

    #[test]
    fn version_with_build_metadata() {
        // semver should handle build metadata
        assert!(version_satisfies("1.0.0+build.1", ">=1.0.0").unwrap());
    }

    // -----------------------------------------------------------------------
    // dedup_vec tests
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_empty() {
        let v: Vec<i32> = vec![];
        let result: Vec<i32> = dedup_vec(v);
        assert!(result.is_empty());
    }

    #[test]
    fn dedup_no_duplicates() {
        let v = vec![1, 2, 3];
        assert_eq!(dedup_vec(v), vec![1, 2, 3]);
    }

    #[test]
    fn dedup_with_duplicates() {
        let v = vec![1, 2, 2, 3, 1, 4, 3];
        assert_eq!(dedup_vec(v), vec![1, 2, 3, 4]);
    }

    #[test]
    fn dedup_all_same() {
        let v = vec!["a", "a", "a"];
        assert_eq!(dedup_vec(v), vec!["a"]);
    }

    #[test]
    fn dedup_preserves_order() {
        let v = vec!["c", "a", "b", "a", "c", "d"];
        assert_eq!(dedup_vec(v), vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn dedup_strings() {
        let v = vec!["hello".to_string(), "world".to_string(), "hello".to_string()];
        assert_eq!(dedup_vec(v), vec!["hello".to_string(), "world".to_string()]);
    }

    // -----------------------------------------------------------------------
    // sanitise_repo_name tests
    // -----------------------------------------------------------------------

    #[test]
    fn sanitise_github_url() {
        let name = sanitise_repo_name("https://github.com/user/repo.git");
        assert_eq!(name, "repo");
    }

    #[test]
    fn sanitise_url_without_git_suffix() {
        let name = sanitise_repo_name("https://github.com/user/repo");
        assert_eq!(name, "repo");
    }

    #[test]
    fn sanitise_url_with_trailing_slash() {
        let name = sanitise_repo_name("https://github.com/user/repo/");
        assert_eq!(name, "repo");
    }

    #[test]
    fn sanitise_git_ssh_url() {
        let name = sanitise_repo_name("git@github.com:user/my-package.git");
        assert_eq!(name, "my-package");
    }

    #[test]
    fn sanitise_plain_name() {
        let name = sanitise_repo_name("mylib");
        assert_eq!(name, "mylib");
    }

    #[test]
    fn sanitise_special_chars() {
        let name = sanitise_repo_name("https://example.com/user/repo@name.git");
        assert_eq!(name, "repo_name");
    }

    #[test]
    fn sanitise_hyphens_and_underscores() {
        let name = sanitise_repo_name("https://github.com/user/my_lib-utils");
        assert_eq!(name, "my_lib-utils");
    }

    #[test]
    fn sanitise_empty_url_uses_raw() {
        // Empty string is handled: rsplit('/').next() gives ""
        let name = sanitise_repo_name("");
        assert_eq!(name, "");
    }

    // -----------------------------------------------------------------------
    // version_satisfies edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn caret_zero_zero_version() {
        // ^0.0.3 means >=0.0.3, <0.0.4
        assert!(version_satisfies("0.0.3", "^0.0.3").unwrap());
        assert!(!version_satisfies("0.0.4", "^0.0.3").unwrap());
    }

    #[test]
    fn multiple_version_ranges() {
        // >=1.0, <1.5 || >=2.0, <2.5 — semver doesn't support || directly
        // but we test individual range components
        assert!(version_satisfies("1.3.0", ">=1.0.0, <1.5.0").unwrap());
        assert!(version_satisfies("2.3.0", ">=2.0.0, <2.5.0").unwrap());
    }

    #[test]
    fn exact_constraint_with_patch() {
        // "=1.2" should match 1.2.x
        assert!(version_satisfies("1.2.0", "=1.2").unwrap());
        assert!(version_satisfies("1.2.5", "=1.2").unwrap());
    }
}
