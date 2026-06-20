pub mod config;
pub mod error;
pub mod flags;
pub mod lockfile;
pub mod package;
pub mod registry;

// Heavy implementation modules — to be filled by agents
pub mod builder;
pub mod fetcher;
pub mod include;
pub mod jit;
pub mod resolver;

pub use config::HutConfig;
pub use error::{HutError, HutResult};
pub use lockfile::Lockfile;
pub use package::Package;
pub use package::default_includes;
pub use registry::RegistryIndex;

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn test_reexport_hut_config() {
        let cfg = super::HutConfig::default_template("test");
        assert_eq!(cfg.package.name, "test");
    }

    #[test]
    fn test_reexport_hut_error() {
        let err: super::HutError = super::HutError::NotAProject;
        assert!(format!("{err}").contains("hut.toml"));
    }

    #[test]
    fn test_reexport_hut_result() {
        let result: super::HutResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_reexport_lockfile() {
        let lock = super::Lockfile::new();
        assert_eq!(lock.version, 1);
        assert!(lock.packages.is_empty());
    }

    #[test]
    fn test_reexport_package() {
        // Construct Package directly via struct literal (all public fields)
        let pkg = super::Package {
            name: "mypkg".into(),
            version: "0.1.0".into(),
            description: None,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
            sources: vec![],
            includes: vec![],
            dependencies: Default::default(),
            build_dependencies: Default::default(),
            test_dependencies: Default::default(),
            build: Default::default(),
            scripts: Default::default(),
            libraries: vec![],
            executables: vec![],
            tests: vec![],
            cflags: vec![],
            ldflags: vec![],
        };
        assert_eq!(pkg.name, "mypkg");
        assert_eq!(pkg.version, "0.1.0");
    }

    #[test]
    fn test_reexport_default_includes() {
        let includes = super::default_includes();
        assert!(!includes.is_empty());
        assert!(includes.contains(&"include".to_string()));
    }

    #[test]
    fn test_reexport_registry_index() {
        let ri = super::RegistryIndex { packages: vec![] };
        assert!(ri.packages.is_empty());
    }

    #[test]
    fn test_public_modules_accessible() {
        // Verify all public modules are accessible at the crate root
        let _ = super::HutConfig::default_template("t");
        let _ = super::HutError::NotAProject;
        let _ = super::Lockfile::new();
        let _ = super::default_includes();
        let _: std::marker::PhantomData<super::RegistryIndex> = std::marker::PhantomData;
        let _ = super::flags::Flags::default();
    }
}
