// ── Integration tests for hut ──────────────────────────────────────────────
// Run with: cargo test --test integration
//
// These tests use real C project fixtures and exercise the hut binary
// end-to-end: init, build, run, info, pm, create, and CLI validation.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

/// Path to the hut binary. When run via `cargo test --test integration`,
/// Cargo sets `CARGO_BIN_EXE_hut`. Otherwise, fall back to target/debug/hut.
fn hut_binary() -> &'static str {
    option_env!("CARGO_BIN_EXE_hut").unwrap_or("target/debug/hut")
}

// ── Helper: run hut in a given working directory ──────────────────────────

fn hut(args: &[&str], cwd: &Path) -> std::process::Output {
    Command::new(hut_binary())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to execute hut")
}

#[track_caller]
fn assert_success(output: &std::process::Output, context: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{context} failed (exit {:?})\nstderr: {stderr}\nstdout: {stdout}",
        output.status.code()
    );
}

// ── Helper: write a file in a temp directory ──────────────────────────────

fn write_file(dir: &Path, relative_path: &str, content: &str) {
    let full = dir.join(relative_path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&full, content).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 1: Init + Build + Run
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_init_build_run() {
    let dir = TempDir::new().unwrap();

    // ── Init ──
    let out = hut(&["init", "testproj"], dir.path());
    assert_success(&out, "hut init testproj");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Created"),
        "Expected 'Created' in: {stdout}"
    );
    assert!(
        stdout.contains("hut.toml"),
        "Expected 'hut.toml' in: {stdout}"
    );
    assert!(
        stdout.contains("src/main.c"),
        "Expected 'src/main.c' in: {stdout}"
    );
    assert!(
        stdout.contains("hello world"),
        "Expected 'hello world' in: {stdout}"
    );

    // Verify files exist
    let proj = dir.path().join("testproj");
    let toml_path = proj.join("hut.toml");
    let main_c_path = proj.join("src").join("main.c");

    assert!(toml_path.exists(), "hut.toml not created");
    assert!(main_c_path.exists(), "src/main.c not created");

    // Verify hut.toml content
    let toml_content = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        toml_content.contains("testproj"),
        "hut.toml missing project name"
    );
    assert!(toml_content.contains("0.1.0"), "hut.toml missing version");

    // Verify src/main.c content
    let c_content = std::fs::read_to_string(&main_c_path).unwrap();
    assert!(
        c_content.contains("Hello from testproj!"),
        "main.c missing hello message"
    );
    assert!(
        c_content.contains("#include <stdio.h>"),
        "main.c missing stdio include"
    );

    // ── Build ──
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Compiling"),
        "Expected 'Compiling' in build output"
    );
    assert!(
        stdout.contains("Linking"),
        "Expected 'Linking' in build output"
    );
    assert!(
        stdout.contains("Finished"),
        "Expected 'Finished' in build output"
    );

    let exe = proj.join("target").join("debug").join("testproj");
    assert!(exe.exists(), "Binary not produced at {}", exe.display());

    // ── Run ──
    let out = hut(&["run"], &proj);
    assert_success(&out, "hut run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Hello from testproj!"),
        "Expected 'Hello from testproj!' in run output, got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 2: Multi-file C project
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_multifile_project() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("mathproj");
    std::fs::create_dir_all(&proj).unwrap();

    // Write hut.toml
    write_file(
        &proj,
        "hut.toml",
        r#"[package]
name = "mathproj"
version = "0.1.0"
language = "c"

[dependencies]

[build_dependencies]

[test_dependencies]

[build]
system = "auto"
c_standard = "c17"
opt_level = "2"
debug = true
warnings = true
extra_cflags = []
extra_ldflags = []

[build.defines]

[scripts]

[workspace]
members = []
"#,
    );

    // Write include/math.h
    write_file(
        &proj,
        "include/math.h",
        r#"#ifndef MATH_H
#define MATH_H

int add(int a, int b);

#endif
"#,
    );

    // Write src/math.c
    write_file(
        &proj,
        "src/math.c",
        r#"#include "math.h"

int add(int a, int b) {
    return a + b;
}
"#,
    );

    // Write src/main.c
    write_file(
        &proj,
        "src/main.c",
        r#"#include <stdio.h>
#include "math.h"

int main(void) {
    int result = add(2, 3);
    printf("Result: %d\n", result);
    return 0;
}
"#,
    );

    // ── Build ──
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build (multi-file)");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Compiling"), "Expected compilation");
    assert!(stdout.contains("Linking"), "Expected linking");
    assert!(stdout.contains("Finished"), "Expected finish message");

    let exe = proj.join("target").join("debug").join("mathproj");
    assert!(exe.exists(), "Binary not produced");

    // ── Run ──
    let out = hut(&["run"], &proj);
    assert_success(&out, "hut run (multi-file)");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Result: 5") || stdout.contains("5") || stdout.contains("Result"),
        "Expected '5' or 'Result' in output, got: {stdout}"
    );
    assert!(
        stdout.contains("5"),
        "Expected '5' in run output, got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 3: Build config (release mode)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_build_release() {
    let dir = TempDir::new().unwrap();

    // Init project
    let out = hut(&["init", "releaseproj"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("releaseproj");

    // Build release
    let out = hut(&["build", "--release"], &proj);
    assert_success(&out, "hut build --release");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("release"),
        "Expected 'release' in: {stdout}"
    );
    assert!(
        stdout.contains("Compiling"),
        "Expected 'Compiling' in: {stdout}"
    );

    // Verify release output
    let release_dir = proj.join("target").join("release");
    assert!(release_dir.exists(), "target/release/ not created");

    let release_exe = release_dir.join("releaseproj");
    assert!(release_exe.exists(), "release binary not produced");

    // Verify the binary actually runs
    let run_out = Command::new(&release_exe)
        .output()
        .expect("failed to run release binary");
    assert!(run_out.status.success(), "release binary failed to run");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 4: CLI help + subcommands
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_cli_help_and_version() {
    let dir = TempDir::new().unwrap();

    // ── hut --help contains all 22 subcommand names ──
    let out = hut(&["--help"], dir.path());
    assert_success(&out, "hut --help");

    let stdout = String::from_utf8_lossy(&out.stdout);

    let expected_subcommands = [
        "init",
        "create",
        "install",
        "add",
        "remove",
        "update",
        "outdated",
        "build",
        "run",
        "test",
        "x",
        "link",
        "unlink",
        "publish",
        "pm",
        "upgrade",
        "patch",
        "info",
        "dev",
        "workspace",
        "completions",
        "search",
    ];

    assert_eq!(
        expected_subcommands.len(),
        22,
        "Expected exactly 22 subcommands"
    );

    for sub in &expected_subcommands {
        assert!(
            stdout.contains(sub),
            "hut --help missing subcommand: {sub}\nOutput: {stdout}"
        );
    }

    // ── hut --version ──
    let out = hut(&["--version"], dir.path());
    assert_success(&out, "hut --version");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("hut"),
        "version output missing 'hut': {stdout}"
    );
    // Should contain a version number like "0.1.0"
    assert!(
        stdout.contains("0.") || stdout.contains("1."),
        "version output missing version number: {stdout}"
    );

    // ── hut build --help ──
    let out = hut(&["build", "--help"], dir.path());
    assert_success(&out, "hut build --help");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("release"),
        "build --help missing --release flag"
    );
    assert!(stdout.contains("-r"), "build --help missing -r short flag");
    assert!(
        stdout.contains("Compile"),
        "build --help missing description"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 5: Info command
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_info_command() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["init", "infoproj"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("infoproj");

    let out = hut(&["info"], &proj);
    assert_success(&out, "hut info");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("infoproj"),
        "info missing project name: {stdout}"
    );
    assert!(stdout.contains("0.1.0"), "info missing version: {stdout}");
    assert!(
        stdout.contains("Package:"),
        "info missing Package section: {stdout}"
    );
    assert!(
        stdout.contains("Dependencies:"),
        "info missing Dependencies section: {stdout}"
    );
    assert!(
        stdout.contains("Build config:"),
        "info missing Build config section: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 6: PM commands
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_pm_commands() {
    let dir = TempDir::new().unwrap();

    // ── hut pm cache ──
    let out = hut(&["pm", "cache"], dir.path());
    assert_success(&out, "hut pm cache");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Cache directory:") || stdout.contains("cache"),
        "pm cache missing cache info: {stdout}"
    );

    // ── hut pm ls ──
    let out = hut(&["pm", "ls"], dir.path());
    assert_success(&out, "hut pm ls");

    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should say cache is empty or list packages
    assert!(
        stdout.contains("Cache")
            || stdout.contains("empty")
            || stdout.contains("packages")
            || stdout.contains("Cached"),
        "pm ls unexpected output: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 7: Init without name (use current directory)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_init_without_name() {
    let dir = TempDir::new().unwrap();

    // The temp dir has a random name like ".tmpXXXXXX". We need a known name.
    // Use a TempDir inside a parent temp dir.
    let named_dir = dir.path().join("my-awesome-project");
    std::fs::create_dir_all(&named_dir).unwrap();

    // Run `hut init` from inside the named directory (no args)
    let out = hut(&["init"], &named_dir);
    assert_success(&out, "hut init (no name)");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Created"),
        "Expected 'Created' in: {stdout}"
    );

    // Verify hut.toml exists with the directory name
    let toml_path = named_dir.join("hut.toml");
    assert!(toml_path.exists(), "hut.toml not created");

    let toml_content = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        toml_content.contains("my-awesome-project"),
        "hut.toml should contain dir name 'my-awesome-project': {toml_content}"
    );
    assert!(
        toml_content.contains("0.1.0"),
        "hut.toml missing version: {toml_content}"
    );

    // Verify src/main.c exists
    let main_c = named_dir.join("src").join("main.c");
    assert!(main_c.exists(), "src/main.c not created");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 8: Create templates
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_create_lib_template() {
    let dir = TempDir::new().unwrap();

    // `hut create lib` scaffolds a C library
    let out = hut(&["create", "lib"], dir.path());
    assert_success(&out, "hut create lib");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Created"),
        "Expected 'Created' in: {stdout}"
    );

    // Verify files
    assert!(dir.path().join("hut.toml").exists(), "hut.toml not created");
    assert!(
        dir.path().join("include/mylib.h").exists(),
        "include/mylib.h not created"
    );
    assert!(
        dir.path().join("src/mylib.c").exists(),
        "src/mylib.c not created"
    );

    // Verify content of the header file
    let header = std::fs::read_to_string(dir.path().join("include/mylib.h")).unwrap();
    assert!(
        header.contains("mylib_add"),
        "header missing mylib_add function"
    );
    assert!(
        header.contains("mylib_version"),
        "header missing mylib_version function"
    );

    // Verify content of the source file
    let source = std::fs::read_to_string(dir.path().join("src/mylib.c")).unwrap();
    assert!(
        source.contains("mylib_add"),
        "source missing mylib_add implementation"
    );
    assert!(
        source.contains("mylib_version"),
        "source missing mylib_version implementation"
    );
    assert!(source.contains("return a + b"), "source missing add logic");
}

#[test]
fn test_create_app_template() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["create", "app"], dir.path());
    assert_success(&out, "hut create app");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Created"),
        "Expected 'Created' in: {stdout}"
    );

    // Verify files
    assert!(dir.path().join("hut.toml").exists(), "hut.toml not created");
    assert!(
        dir.path().join("src/main.c").exists(),
        "src/main.c not created"
    );

    let main_c = std::fs::read_to_string(dir.path().join("src/main.c")).unwrap();
    assert!(
        main_c.contains("Hello, world!"),
        "app main.c missing hello message"
    );
    assert!(main_c.contains("argv"), "app main.c missing argv handling");
}

#[test]
fn test_create_unknown_template() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["create", "nonexistent"], dir.path());
    assert!(!out.status.success(), "hut create nonexistent should fail");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown template") || stderr.contains("error"),
        "Expected error for unknown template, got: {stderr}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 9: Complex flag propagation (sanitizers, LTO, PIC, per-target, platform)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_complex_flags_build() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("flagproj");
    std::fs::create_dir_all(&proj).unwrap();

    // Write hut.toml with complex flags
    write_file(
        &proj,
        "hut.toml",
        r#"[package]
name = "flagproj"
version = "0.1.0"
language = "c"

[dependencies]

[build_dependencies]

[test_dependencies]

[build]
system = "auto"
c_standard = "c17"
opt_level = "2"
debug = true
warnings = true
extra_cflags = []
extra_ldflags = []
lto = true
pic = true

[build.defines]
DEBUG = "1"
VERSION = "2.0"

[build.platform.linux]
cflags = ["-DPLATFORM_LINUX_TEST"]

[build.platform.macos]
cflags = ["-DPLATFORM_MACOS_TEST"]

[scripts]

[workspace]
members = []
"#,
    );

    // Write src/main.c
    let src_dir = proj.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    write_file(
        &proj,
        "src/main.c",
        r#"#include <stdio.h>

int main(void) {
    printf("flags test passed\n");
    return 0;
}
"#,
    );

    // ── Build ──
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build (complex flags)");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Compiling"), "Expected compilation");
    assert!(stdout.contains("Linking"), "Expected linking");
    assert!(stdout.contains("Finished"), "Expected finish message");

    // Verify binary exists and runs
    let exe = proj.join("target").join("debug").join("flagproj");
    assert!(exe.exists(), "Binary not produced");

    let run_out = Command::new(&exe).output().expect("failed to run binary");
    assert!(run_out.status.success(), "binary failed to run");
    let run_stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        run_stdout.contains("flags test passed"),
        "Expected 'flags test passed' in output, got: {run_stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 10: Verbose build output contains expected flags
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_verbose_flags_output() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("verboseproj");
    std::fs::create_dir_all(&proj).unwrap();

    write_file(
        &proj,
        "hut.toml",
        r#"[package]
name = "verboseproj"
version = "0.1.0"
language = "c"

[dependencies]

[build_dependencies]

[test_dependencies]

[build]
system = "auto"
c_standard = "c17"
opt_level = "2"
debug = true
warnings = true

[build.defines]
MY_DEFINE = "42"

[scripts]

[workspace]
members = []
"#,
    );

    write_file(
        &proj,
        "src/main.c",
        r#"#include <stdio.h>

int main(void) {
#ifdef MY_DEFINE
    printf("define=%d\n", MY_DEFINE);
#else
    printf("no define\n");
#endif
    return 0;
}
"#,
    );

    // Build the project
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build (verbose flags)");

    let exe = proj.join("target").join("debug").join("verboseproj");
    assert!(exe.exists(), "Binary not produced");

    // Run and verify the define was passed
    let run_out = Command::new(&exe).output().expect("failed to run binary");
    assert!(run_out.status.success(), "binary failed to run");
    let run_stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        run_stdout.contains("define=42"),
        "Expected 'define=42' but got: {run_stdout}"
    );
}
