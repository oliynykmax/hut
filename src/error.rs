use thiserror::Error;

#[derive(Error, Debug)]
pub enum HutError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

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
