//! Dependency resolver — resolves packages using the local packages.toml index.
//! Semver-aware resolution: constraints from hut.toml (e.g. "^1.0") are matched
//! against git tags from the remote repo. Lockfile pins exact versions.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use semver::{Version, VersionReq};

use crate::config::HutConfig;
use crate::error::{HutError, HutResult};
use crate::index::PackagesIndex;
use crate::lockfile::Lockfile;
use crate::package::{Package, ResolvedDependency};

// ── Public API ────────────────────────────────────────────────────────────

/// Resolve all dependencies (direct + transitive) for a project.
/// Semver constraints from hut.toml are matched against git tags.
/// Lockfile pins exact tag versions for reproducible builds.
pub fn resolve_dependencies(
    config: &HutConfig,
    lockfile: &Lockfile,
    index: &PackagesIndex,
    _cache_dir: &Path,
) -> HutResult<Vec<ResolvedDependency>> {
    let mut ctx = ResolveContext::new(lockfile, index);

    let mut queue: Vec<(String, String)> = config
        .dependencies
        .iter()
        .chain(config.build_dependencies.iter())
        .chain(config.test_dependencies.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    while let Some((name, constraint)) = queue.pop() {
        if ctx.resolved_names.contains(&name) {
            continue;
        }
        ctx.resolve_one(&name, &constraint, &mut queue)?;
    }

    let mut result: Vec<ResolvedDependency> = ctx.packages.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

// ── Semver helpers ────────────────────────────────────────────────────────

/// Parse a semver constraint string from hut.toml.
/// Returns None for wildcard constraints ("*", "latest", empty) — accept any.
fn parse_constraint(s: &str) -> Option<VersionReq> {
    let s = s.trim();
    if s.is_empty() || s == "*" || s == "latest" || s == "x" {
        return None;
    }
    VersionReq::parse(s).ok()
}

// ── Internal context ──────────────────────────────────────────────────────

struct ResolveContext<'a> {
    lockfile: &'a Lockfile,
    index: &'a PackagesIndex,
    packages: HashMap<String, ResolvedDependency>,
    resolved_names: HashSet<String>,
}

impl<'a> ResolveContext<'a> {
    fn new(lockfile: &'a Lockfile, index: &'a PackagesIndex) -> Self {
        Self {
            lockfile,
            index,
            packages: HashMap::new(),
            resolved_names: HashSet::new(),
        }
    }

    /// Resolve a single package by name, given a semver constraint.
    fn resolve_one(
        &mut self,
        name: &str,
        constraint_str: &str,
        queue: &mut Vec<(String, String)>,
    ) -> HutResult<()> {
        let entry = self
            .index
            .find(name)
            .ok_or_else(|| HutError::PackageNotFound(name.to_string()))?;

        let repo_url = format!("https://github.com/{}.git", entry.repo);
        let constraint = parse_constraint(constraint_str);

        // Determine the version (tag) to fetch.
        let version = self.resolve_version(name, &repo_url, &constraint)?;

        // Fetch package source (clone from GitHub at the resolved tag).
        let pkg_path = crate::fetcher::fetch_package_source(name, &repo_url, &version)?;

        self.resolved_names.insert(name.to_string());

        // For transitive deps: if the cloned repo has a hut.toml, load it.
        let hut_toml = pkg_path.join("hut.toml");
        let pkg = if hut_toml.exists() {
            let manifest = std::fs::read_to_string(&hut_toml)?;
            let cfg: HutConfig = toml::from_str(&manifest)?;

            for (t_name, t_version) in cfg
                .dependencies
                .iter()
                .chain(cfg.build_dependencies.iter())
                .chain(cfg.test_dependencies.iter())
            {
                if !self.resolved_names.contains(t_name) {
                    queue.push((t_name.clone(), t_version.clone()));
                }
            }

            Package {
                name: cfg.package.name,
                version: cfg.package.version,
                description: cfg.package.description,
                authors: cfg.package.authors,
                license: cfg.package.license,
                repository: Some(repo_url.clone()),
                homepage: cfg.package.homepage,
                sources: cfg.package.sources,
                includes: cfg.package.includes,
                dependencies: cfg.dependencies,
                build_dependencies: cfg.build_dependencies,
                test_dependencies: cfg.test_dependencies,
                build: cfg.build,
                scripts: cfg.scripts,
                libraries: vec![],
                executables: vec![],
                tests: vec![],
                cflags: vec![],
                ldflags: vec![],
            }
        } else {
            Package {
                name: name.to_string(),
                version: version.clone(),
                description: Some(entry.description.clone()),
                authors: vec![],
                license: None,
                repository: Some(repo_url.clone()),
                homepage: None,
                sources: vec![],
                includes: entry.includes.clone(),
                dependencies: Default::default(),
                build_dependencies: Default::default(),
                test_dependencies: Default::default(),
                build: Default::default(),
                scripts: Default::default(),
                libraries: vec![],
                executables: vec![],
                tests: vec![],
                cflags: entry.cflags.clone(),
                ldflags: entry.ldflags.clone(),
            }
        };

        let mut include_paths: Vec<PathBuf> = entry
            .includes
            .iter()
            .map(|inc| pkg_path.join(inc))
            .chain(pkg.includes.iter().map(|inc| pkg_path.join(inc)))
            .filter(|p| p.exists())
            .collect();
        if include_paths.is_empty() {
            include_paths.push(pkg_path.clone());
        }

        let library_paths: Vec<PathBuf> = {
            let build_dir = pkg_path.join("build");
            if build_dir.exists() {
                vec![build_dir]
            } else {
                vec![pkg_path.clone()]
            }
        };

        let mut link_libraries: Vec<String> = entry.libs.clone();
        link_libraries.extend(pkg.libraries.iter().map(|lib| lib.name.clone()));

        for t_name in pkg.dependencies.keys() {
            if let Some(rd) = self.packages.get(t_name) {
                include_paths.extend(rd.include_paths.clone());
            }
        }

        let resolved = ResolvedDependency {
            name: name.to_string(),
            version,
            path: pkg_path,
            package: pkg,
            include_paths,
            library_paths,
            link_libraries,
            cflags: entry.cflags.clone(),
            ldflags: entry.ldflags.clone(),
        };

        self.packages.insert(name.to_string(), resolved);
        Ok(())
    }

    /// Resolve the exact version (tag) for a package given a semver constraint.
    ///
    /// Priority:
    /// 1. Lockfile — if pinned version satisfies constraint, use it
    /// 2. Remote tags — fetch tags, filter by constraint, pick highest
    /// 3. Fallback — "main" branch
    fn resolve_version(
        &self,
        name: &str,
        repo_url: &str,
        constraint: &Option<VersionReq>,
    ) -> HutResult<String> {
        // Check lockfile first.
        if let Some(locked) = self.lockfile.get(name) {
            let tag_stripped = crate::fetcher::strip_tag_prefix(&locked.version);
            if let Ok(parsed) = Version::parse(&tag_stripped) {
                if constraint.as_ref().map_or(true, |c| c.matches(&parsed)) {
                    return Ok(locked.version.clone());
                }
            } else if constraint.is_none() {
                // Locked version isn't semver (e.g. "main") and no constraint — use it.
                return Ok(locked.version.clone());
            }
        }

        // Resolve from remote git tags.
        let version = crate::fetcher::resolve_best_version(repo_url, constraint)?;
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_constraint_none() {
        assert!(parse_constraint("").is_none());
        assert!(parse_constraint("*").is_none());
        assert!(parse_constraint("latest").is_none());
        assert!(parse_constraint("x").is_none());
    }

    #[test]
    fn test_parse_constraint_caret() {
        let req = parse_constraint("^1.0").unwrap();
        assert!(req.matches(&Version::new(1, 5, 0)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_parse_constraint_range() {
        let req = parse_constraint(">=1.0,<2.0").unwrap();
        assert!(req.matches(&Version::new(1, 9, 9)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_parse_constraint_tilde() {
        let req = parse_constraint("~0.5").unwrap();
        assert!(req.matches(&Version::new(0, 5, 4)));
        assert!(!req.matches(&Version::new(0, 6, 0)));
    }

    #[test]
    fn test_parse_constraint_exact() {
        let req = parse_constraint("=1.2.3").unwrap();
        assert!(req.matches(&Version::new(1, 2, 3)));
        assert!(!req.matches(&Version::new(1, 2, 4)));
    }
}
