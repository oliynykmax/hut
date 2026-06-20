// ── Shared helpers and constants ──────────────────────────────────────────

use std::path::PathBuf;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

// ── Directories ───────────────────────────────────────────────────────────

pub fn hut_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hut")
}

pub fn cache_dir() -> PathBuf {
    hut::fetcher::default_cache_dir()
}

pub fn packages_dir() -> PathBuf {
    cache_dir()
}

pub fn lockfile_path() -> PathBuf {
    PathBuf::from("hut.lock")
}

// ── Compiler detection ───────────────────────────────────────────────────

/// Scan for available C compilers on the system
pub fn available_compilers() -> Vec<String> {
    let mut found = Vec::new();
    for cc in &["gcc", "clang"] {
        if std::process::Command::new("which")
            .arg(cc)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            found.push(cc.to_string());
        }
    }
    found
}

/// Check if a command exists in PATH
pub fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Miscellaneous ────────────────────────────────────────────────────────

/// Parse a dependency spec like "user/lib@^1.0" → ("user/lib", Some("^1.0"))
#[allow(dead_code)]
fn parse_dep_spec(spec: &str) -> (String, Option<String>) {
    if let Some(at_pos) = spec.find('@') {
        let name = spec[..at_pos].to_string();
        let version = spec[at_pos + 1..].to_string();
        (name, Some(version))
    } else {
        (spec.to_string(), None)
    }
}

/// Walk up from the current directory to find the project root (where hut.toml lives)
pub fn find_project_root() -> HutResult<PathBuf> {
    let cwd = std::env::current_dir()?;
    for ancestor in cwd.ancestors() {
        if ancestor.join("hut.toml").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(HutError::Other(
        "Not a hut project — no hut.toml found in any parent directory".into(),
    ))
}

// ── Embedded source templates ────────────────────────────────────────────

pub const HELLO_WORLD_C: &str = "#include <stdio.h>\n\nint main() {\n    printf(\"Hello from {NAME}!\\n\");\n    return 0;\n}\n";

pub const HELLO_WORLD_CPP: &str = "#include <iostream>\n\nint main() {\n    std::cout << \"Hello from {NAME}!\" << std::endl;\n    return 0;\n}\n";

pub const LIB_HEADER: &str = "#ifndef MYLIB_H\n#define MYLIB_H\n\n// Public API\nint mylib_add(int a, int b);\nconst char* mylib_version(void);\n\n#endif // MYLIB_H\n";

pub const LIB_SOURCE: &str = "#include \"mylib.h\"\n\nint mylib_add(int a, int b) {\n    return a + b;\n}\n\nconst char* mylib_version(void) {\n    return \"0.1.0\";\n}\n";

pub const APP_MAIN_C: &str = "#include <stdio.h>\n\nint main(int argc, char** argv) {\n    printf(\"Hello, world!\\n\");\n    if (argc > 1) {\n        printf(\"Arguments: %d\\n\", argc - 1);\n        for (int i = 1; i < argc; i++) {\n            printf(\"  %s\\n\", argv[i]);\n        }\n    }\n    return 0;\n}\n";

pub const RAYLIB_GAME_C: &str = "#include \"raylib.h\"\n\nint main() {\n    const int screenWidth = 800;\n    const int screenHeight = 450;\n\n    InitWindow(screenWidth, screenHeight, \"raylib game — built with hut\");\n\n    SetTargetFPS(60);\n\n    while (!WindowShouldClose()) {\n        BeginDrawing();\n        ClearBackground(RAYWHITE);\n        DrawText(\"Hello, raylib!\", 190, 200, 20, LIGHTGRAY);\n        EndDrawing();\n    }\n\n    CloseWindow();\n    return 0;\n}\n";
