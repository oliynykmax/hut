use thiserror::Error;

#[derive(Error, Debug)]
pub enum HutError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Semver error: {0}")]
    Semver(#[from] semver::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("Dependency resolution failed: {0}")]
    Resolution(String),

    #[error("Build failed: {0}")]
    Build(String),

    #[error("Registry error: {0}")]
    Registry(String),

    #[error("No C/C++ compiler found. Install gcc or clang.")]
    NoCompiler,

    #[error("Not a hut project (no hut.toml found)")]
    NotAProject,

    #[error("{0}")]
    Other(String),
}

pub type HutResult<T> = Result<T, HutError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_io_error() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let hut_err: HutError = err.into();
        assert_eq!(hut_err.to_string(), "IO error: file not found");
    }

    #[test]
    fn display_toml_error() {
        let result: Result<toml::Value, toml::de::Error> = toml::from_str("invalid = [[[");
        let err = result.unwrap_err();
        let hut_err: HutError = err.into();
        assert!(hut_err.to_string().starts_with("TOML parse error:"));
    }

    #[test]
    fn display_toml_ser_error() {
        // toml::ser::Error can be constructed via serde::ser::Error::custom()
        use serde::ser::Error as _;
        let ser_err = toml::ser::Error::custom("serialization failed");
        let hut_err = HutError::TomlSer(ser_err);
        assert!(hut_err.to_string().starts_with("TOML serialize error:"));
    }

    #[test]
    fn display_json_error() {
        let result: Result<serde_json::Value, serde_json::Error> = serde_json::from_str("{bad");
        let err = result.unwrap_err();
        let hut_err: HutError = err.into();
        assert!(hut_err.to_string().starts_with("JSON error:"));
    }

    #[test]
    fn display_semver_error() {
        let result = semver::Version::parse("not.a.version");
        let err = result.unwrap_err();
        let hut_err: HutError = err.into();
        assert!(hut_err.to_string().starts_with("Semver error:"));
    }

    #[test]
    fn display_config_error() {
        let err = HutError::Config("missing field".to_string());
        assert_eq!(err.to_string(), "Config error: missing field");
    }

    #[test]
    fn display_package_not_found() {
        let err = HutError::PackageNotFound("libfoo".to_string());
        assert_eq!(err.to_string(), "Package not found: libfoo");
    }

    #[test]
    fn display_version_not_found() {
        let err = HutError::VersionNotFound("libfoo".to_string(), "1.0.0".to_string());
        assert_eq!(err.to_string(), "Version not found: libfoo@1.0.0");
    }

    #[test]
    fn display_resolution_error() {
        let err = HutError::Resolution("conflict".to_string());
        assert_eq!(err.to_string(), "Dependency resolution failed: conflict");
    }

    #[test]
    fn display_build_error() {
        let err = HutError::Build("compilation failed".to_string());
        assert_eq!(err.to_string(), "Build failed: compilation failed");
    }

    #[test]
    fn display_registry_error() {
        let err = HutError::Registry("not reachable".to_string());
        assert_eq!(err.to_string(), "Registry error: not reachable");
    }

    #[test]
    fn display_no_compiler() {
        let err = HutError::NoCompiler;
        assert_eq!(
            err.to_string(),
            "No C/C++ compiler found. Install gcc or clang."
        );
    }

    #[test]
    fn display_not_a_project() {
        let err = HutError::NotAProject;
        assert_eq!(err.to_string(), "Not a hut project (no hut.toml found)");
    }

    #[test]
    fn display_other_error() {
        let err = HutError::Other("something went wrong".to_string());
        assert_eq!(err.to_string(), "something went wrong");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let hut_err: HutError = io_err.into();
        assert!(matches!(hut_err, HutError::Io(_)));
        assert!(hut_err.to_string().contains("denied"));
    }

    #[test]
    fn from_semver_error() {
        let result = semver::Version::parse("abc");
        let err = result.unwrap_err();
        let hut_err: HutError = err.into();
        assert!(matches!(hut_err, HutError::Semver(_)));
    }

    #[test]
    fn from_json_error() {
        let result: Result<serde_json::Value, serde_json::Error> = serde_json::from_str("{{");
        let err = result.unwrap_err();
        let hut_err: HutError = err.into();
        assert!(matches!(hut_err, HutError::Json(_)));
    }

    #[test]
    fn from_toml_error() {
        let result: Result<toml::Value, toml::de::Error> = toml::from_str("= invalid");
        let err = result.unwrap_err();
        let hut_err: HutError = err.into();
        assert!(matches!(hut_err, HutError::Toml(_)));
    }

    #[test]
    fn error_debug_trait() {
        let err = HutError::NoCompiler;
        // Just verify it doesn't panic
        let _ = format!("{:?}", err);
    }

    #[test]
    fn hut_result_ok() {
        let result: HutResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn hut_result_err() {
        let result: HutResult<i32> = Err(HutError::NotAProject);
        assert!(result.is_err());
    }
}
