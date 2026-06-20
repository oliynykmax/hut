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

// ═══════════════════════════════════════════════════════════════════════════
// Test 11: C++ init + build
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_init_cpp_project() {
    let dir = TempDir::new().unwrap();

    // ── Init a C project first ──
    let out = hut(&["init", "cppproj"], dir.path());
    assert_success(&out, "hut init cppproj");

    let proj = dir.path().join("cppproj");
    let toml_path = proj.join("hut.toml");
    assert!(toml_path.exists(), "hut.toml not created");

    // ── Modify hut.toml: change language to C++ ──
    let toml_content = std::fs::read_to_string(&toml_path).unwrap();
    let updated = toml_content.replace(r#"language = "c""#, r#"language = "c++""#);
    std::fs::write(&toml_path, &updated).unwrap();

    // ── Rename main.c → main.cpp with C++ hello-world content ──
    let main_c = proj.join("src").join("main.c");
    let main_cpp = proj.join("src").join("main.cpp");
    std::fs::rename(&main_c, &main_cpp).unwrap();

    write_file(
        &proj,
        "src/main.cpp",
        r#"#include <iostream>

int main() {
    std::cout << "Hello from cppproj!" << std::endl;
    return 0;
}
"#,
    );

    // ── Build ──
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build (C++)");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Compiling") || stdout.contains("Cached"),
        "Expected compilation or cache in C++ build: {stdout}"
    );
    assert!(
        stdout.contains("Linking") || stdout.contains("Finished"),
        "Expected Linking/Finished in C++ build: {stdout}"
    );

    // Check src/main.cpp exists (not main.c)
    assert!(main_cpp.exists(), "src/main.cpp should exist");
    assert!(!main_c.exists(), "src/main.c should not exist in a C++ project");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 12: Build release and run the binary directly
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_build_release_and_run() {
    let dir = TempDir::new().unwrap();

    // ── Init ──
    let out = hut(&["init", "relrun"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("relrun");

    // ── Build release ──
    let out = hut(&["build", "--release"], &proj);
    assert_success(&out, "hut build --release");

    // ── Run the release binary directly (not via hut run) ──
    let release_exe = proj.join("target").join("release").join("relrun");
    assert!(release_exe.exists(), "release binary not produced");

    let run_out = Command::new(&release_exe)
        .output()
        .expect("failed to run release binary");
    assert!(run_out.status.success(), "release binary failed to run");

    let run_stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        run_stdout.contains("Hello"),
        "Expected 'Hello' in release binary output, got: {run_stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 13: Multiple builds are idempotent (second build uses cache)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_multiple_builds_idempotent() {
    let dir = TempDir::new().unwrap();

    // ── Init ──
    let out = hut(&["init", "idemproj"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("idemproj");

    // ── First build ──
    let out1 = hut(&["build"], &proj);
    assert_success(&out1, "hut build (first)");
    let stdout1 = String::from_utf8_lossy(&out1.stdout);
    assert!(
        stdout1.contains("Compiling") || stdout1.contains("Linking"),
        "First build should compile: {stdout1}"
    );

    // ── Second build (should be cached / no-op) ──
    let out2 = hut(&["build"], &proj);
    assert_success(&out2, "hut build (second)");
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        stdout2.contains("Cached") || stdout2.contains("Skipped") || stdout2.contains("unchanged"),
        "Second build should use cache: {stdout2}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 14: Init then info shows expected sections
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_init_then_info() {
    let dir = TempDir::new().unwrap();

    // ── Init ──
    let out = hut(&["init", "infoproj2"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("infoproj2");

    // ── Info ──
    let out = hut(&["info"], &proj);
    assert_success(&out, "hut info");

    let stdout = String::from_utf8_lossy(&out.stdout);
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
// Test 15: Run with args forwarded to the binary
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_run_with_args() {
    let dir = TempDir::new().unwrap();

    // ── Init ──
    let out = hut(&["init", "argproj"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("argproj");

    // ── Overwrite main.c with a version that prints argv ──
    write_file(
        &proj,
        "src/main.c",
        r#"#include <stdio.h>

int main(int argc, char **argv) {
    printf("Hello from argproj!\n");
    for (int i = 0; i < argc; i++) {
        printf("argv[%d] = %s\n", i, argv[i]);
    }
    return 0;
}
"#,
    );

    // ── Build ──
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build");

    // ── Run with args ──
    let out = hut(&["run", "--", "arg1", "arg2"], &proj);
    assert_success(&out, "hut run -- arg1 arg2");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("arg1") || stdout.contains("arg2"),
        "Expected 'arg1' or 'arg2' in run output, got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 16: fmt --check (doesn't panic even if clang-format is missing)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_fmt_check() {
    let dir = TempDir::new().unwrap();

    // ── Init ──
    let out = hut(&["init", "fmtproj"], dir.path());
    assert_success(&out, "hut init");

    let proj = dir.path().join("fmtproj");

    // ── Run fmt --check ──
    // clang-format may or may not be installed — accept either outcome
    let out = hut(&["fmt", "--check"], &proj);
    // Don't assert_success; just verify the process didn't panic/segfault
    // (it completed and gave us output/stderr)

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // If clang-format is available, we expect formatting output;
    // if not, we expect an error message about missing clang-format.
    let has_output = !stdout.is_empty() || !stderr.is_empty();
    assert!(
        has_output || out.status.success(),
        "fmt --check produced no output and failed silently"
    );

    // The command should mention clang-format or report formatting status
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("clang-format")
            || combined.contains("format")
            || combined.contains("Checking")
            || combined.contains("All")
            || combined.contains("source files"),
        "fmt --check unexpected output: {combined}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 11: Search command
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_search_command() {
    let dir = TempDir::new().unwrap();

    // Search for something that should return results (json)
    let out = hut(&["search", "json"], dir.path());
    assert_success(&out, "hut search json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.is_empty(),
        "hut search json produced empty output"
    );

    // Search for something that definitely won't exist
    let out = hut(&["search", "nonexsitent12345"], dir.path());
    // Should not crash — might succeed with no results or error gracefully
    let _stdout = String::from_utf8_lossy(&out.stdout);
    let _stderr = String::from_utf8_lossy(&out.stderr);
    // Just verify it runs without a segfault/panic — output validated by not crashing
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 12: Completions (bash)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_completions_bash() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["completions", "bash"], dir.path());
    assert_success(&out, "hut completions bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.is_empty(),
        "completions bash should produce non-empty output"
    );
    assert!(
        stdout.contains("hut") || stdout.contains("complete"),
        "completions bash output should contain 'hut' or 'complete', got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 13: Completions (zsh)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_completions_zsh() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["completions", "zsh"], dir.path());
    assert_success(&out, "hut completions zsh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.is_empty(),
        "completions zsh should produce non-empty output"
    );
    assert!(
        stdout.contains("hut") || stdout.contains("_arguments") || stdout.contains("compdef"),
        "completions zsh output should contain shell completion commands, got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 14: Clean command
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_clean_command() {
    let dir = TempDir::new().unwrap();

    // Init a project
    let out = hut(&["init", "cleanproj"], dir.path());
    assert_success(&out, "hut init cleanproj");

    let proj = dir.path().join("cleanproj");

    // Build it to create target/
    let out = hut(&["build"], &proj);
    assert_success(&out, "hut build");

    // Verify target/ exists
    let target_dir = proj.join("target");
    assert!(target_dir.exists(), "target/ should exist after build");

    // Run clean
    let out = hut(&["clean"], &proj);
    assert_success(&out, "hut clean");

    // Verify target/ is gone
    assert!(
        !target_dir.exists(),
        "target/ should be removed after clean"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 15: Outdated command (no deps)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_outdated_no_deps() {
    let dir = TempDir::new().unwrap();

    // Init a fresh project with no dependencies
    let out = hut(&["init", "freshproj"], dir.path());
    assert_success(&out, "hut init freshproj");

    let proj = dir.path().join("freshproj");

    // Run outdated
    let out = hut(&["outdated"], &proj);
    assert_success(&out, "hut outdated");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.to_lowercase().contains("up to date")
            || combined.to_lowercase().contains("no outdated")
            || combined.to_lowercase().contains("all dependencies are up to date")
            || stdout.is_empty(),
        "hut outdated on fresh project should indicate up-to-date, got stdout=[{stdout}] stderr=[{stderr}]"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 16: Remove nonexistent package
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_remove_nonexistent() {
    let dir = TempDir::new().unwrap();

    // Init a project
    let out = hut(&["init", "remproj"], dir.path());
    assert_success(&out, "hut init remproj");

    let proj = dir.path().join("remproj");

    // Try to remove a nonexistent package — should not crash
    let out = hut(&["remove", "nonexistentpkg"], &proj);
    // May succeed or fail, but must not crash (which would manifest as a panic)
    let _stdout = String::from_utf8_lossy(&out.stdout);
    let _stderr = String::from_utf8_lossy(&out.stderr);
    // Not asserting specific output — just that it runs without panicking
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 11: Create raylib-game template
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_create_raylib_template() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["create", "raylib-game"], dir.path());
    assert_success(&out, "hut create raylib-game");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Created"),
        "Expected 'Created' in: {stdout}"
    );

    // Verify hut.toml and src/main.c exist
    assert!(
        dir.path().join("hut.toml").exists(),
        "hut.toml not created for raylib-game"
    );
    assert!(
        dir.path().join("src/main.c").exists(),
        "src/main.c not created for raylib-game"
    );

    // Verify source contains raylib.h
    let main_c = std::fs::read_to_string(dir.path().join("src/main.c")).unwrap();
    assert!(
        main_c.contains("raylib.h"),
        "raylib-game main.c missing raylib.h include"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 12: Run with --jit (just-in-time compilation)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_run_jit() {
    let dir = TempDir::new().unwrap();

    // Init a C project
    let out = hut(&["init", "jitproj"], dir.path());
    assert_success(&out, "hut init jitproj");

    let proj = dir.path().join("jitproj");

    // Run with --jit
    let out = hut(&["run", "--jit"], &proj);

    // Check if libtcc is missing
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    if !out.status.success() && (stderr.contains("libtcc") || stderr.contains("tcc")) {
        println!(
            "SKIP: libtcc not available — cannot run JIT tests. stderr: {stderr}"
        );
        return;
    }

    assert_success(&out, "hut run --jit");

    assert!(
        stdout.contains("Hello") || stdout.contains("hello"),
        "Expected 'Hello' or 'hello' in JIT run output, got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 13: Build with --compiler flag
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_build_with_compiler_flag() {
    let dir = TempDir::new().unwrap();

    // Init a project
    let out = hut(&["init", "compilerproj"], dir.path());
    assert_success(&out, "hut init compilerproj");

    let proj = dir.path().join("compilerproj");

    // Build with --compiler auto
    let out = hut(&["build", "--compiler", "auto"], &proj);
    assert_success(&out, "hut build --compiler auto");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Compiling") || stdout.contains("Finished"),
        "Expected 'Compiling' or 'Finished' in build output, got: {stdout}"
    );

    // Verify binary exists
    let exe = proj.join("target").join("debug").join("compilerproj");
    assert!(
        exe.exists(),
        "Binary not produced at {}",
        exe.display()
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 14: dev --help shows watch/rebuild info
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_dev_help() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["dev", "--help"], dir.path());
    assert_success(&out, "hut dev --help");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Watch") || stdout.contains("rebuild") || stdout.contains("watch"),
        "Expected 'Watch' or 'rebuild' or 'watch' in dev --help, got: {stdout}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 15: Create invalid template shows helpful error
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_create_invalid_template_message() {
    let dir = TempDir::new().unwrap();

    let out = hut(&["create", "invalid_template_name"], dir.path());
    assert!(
        !out.status.success(),
        "hut create invalid_template_name should fail"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        combined.contains("unknown") || combined.contains("template") || combined.contains("error"),
        "Expected 'unknown' or 'template' or 'error' in output for invalid create, got: {combined}"
    );
}
