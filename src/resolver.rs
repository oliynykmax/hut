//! Dependency resolver — resolves packages using the local packages.toml index.
//! Packages are cloned from GitHub. The packages.toml recipe provides all build
//! metadata — repos do NOT need their own hut.toml.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::HutConfig;
use crate::error::{HutError, HutResult};
use crate::index::PackagesIndex;
use crate::lockfile::Lockfile;
use crate::package::{Package, ResolvedDependency};

// ── Public API ────────────────────────────────────────────────────────────

/// Resolve all dependencies (direct + transitive) for a project.
/// Uses the local packages.toml index for name → repo + build recipe.
/// Repos are cloned from GitHub; the recipe provides all metadata.
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

    while let Some((name, _version)) = queue.pop() {
        if ctx.resolved_names.contains(&name) {
            continue;
        }
        ctx.resolve_one(&name, &mut queue)?;
    }

    let mut result: Vec<ResolvedDependency> = ctx.packages.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
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

    fn resolve_one(&mut self, name: &str, queue: &mut Vec<(String, String)>) -> HutResult<()> {
        let entry = self
            .index
            .find(name)
            .ok_or_else(|| HutError::PackageNotFound(name.to_string()))?;

        let repo_url = format!("https://github.com/{}.git", entry.repo);

        // Determine version: lockfile pinned version, or default to "main".
        let version = if let Some(locked) = self.lockfile.get(name) {
            locked.version.clone()
        } else {
            "main".to_string()
        };

        // Fetch package source (clone from GitHub).
        let pkg_path = crate::fetcher::fetch_package_source(name, &repo_url, &version)?;

        self.resolved_names.insert(name.to_string());

        // For transitive deps: if the cloned repo has a hut.toml, load it.
        // Otherwise, the package has no transitive deps.
        let hut_toml = pkg_path.join("hut.toml");
        let pkg = if hut_toml.exists() {
            let manifest = std::fs::read_to_string(&hut_toml)?;
            let cfg: HutConfig = toml::from_str(&manifest)?;

            // Enqueue transitive dependencies.
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
            // No hut.toml — use recipe metadata only.
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

        // Collect include paths from recipe + package.
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

        // Gather transitive paths from already-resolved deps.
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
}
