// ── cmd_run ──────────────────────────────────────────────────────────────

use std::process::Command;

use colored::Colorize;

use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::Lockfile;

use crate::commands::{
    cache_dir, find_project_root,
    lockfile_path,
};

pub fn cmd_run(
    target: Option<String>,
    args: Vec<String>,
    release: bool,
    jit: bool,
) -> HutResult<()> {
    let (config, _config_path) = HutConfig::find()?;
    let project_root = find_project_root()?;

    // ── JIT path (via libtcc, in-process) ────────────────────────────────
    if jit {
        let sources = hut::builder::collect_sources(&config, &project_root)?;

        if sources.is_empty() {
            return Err(HutError::Other(
                "No source files found for JIT compilation. Add .c/.cpp files to src/.".into(),
            ));
        }

        let mut tcc = hut::jit::Tcc::new().ok_or_else(|| {
            HutError::Other(
                "libtcc not found.\n\n\
                 Install TCC (Tiny C Compiler) to use `hut run --jit`:\n\
                   • Ubuntu/Debian:  sudo apt install tcc libtcc-dev\n\
                   • Fedora:         sudo dnf install tcc\n\
                   • Arch:           sudo pacman -S tcc\n\
                   • macOS:          brew install tcc\n\
                   • From source:\n\
                       git clone https://repo.or.cz/tinycc.git\n\
                       cd tinycc && ./configure && make && sudo make install"
                    .into(),
            )
        })?;

        println!(
            "{} {} source file(s)...",
            "   JIT".bold().magenta(),
            sources.len().to_string().bold()
        );

        // ── Add C++ include paths if any source file is C++ ──────────────
        let has_cxx = sources.iter().any(|s| {
            let ext = s.extension().and_then(|e| e.to_str()).unwrap_or("");
            matches!(ext, "cpp" | "cc" | "cxx" | "CPP" | "hpp" | "hh" | "hxx")
        });
        if has_cxx {
            let cxx_paths = hut::jit::Tcc::discover_cxx_include_paths();
            for path in &cxx_paths {
                let added = tcc.add_include_path(&path.display().to_string());
                if added {
                    println!(
                        "{} include path: {}",
                        "   JIT".bold().magenta(),
                        path.display().to_string().dimmed()
                    );
                }
            }
        }

        let mut combined_source = String::new();
        for src in &sources {
            let content = std::fs::read_to_string(src)
                .map_err(|e| HutError::Other(format!("Failed to read {}: {e}", src.display())))?;
            combined_source.push_str(&content);
            combined_source.push('\n');
        }

        // ── Compile all source files ────────────────────────────────────
        tcc.compile(&combined_source)
            .map_err(|e| HutError::Other(format!("JIT compilation failed: {e}")))?;

        tcc.relocate()
            .map_err(|e| HutError::Other(format!("JIT relocation failed: {e}")))?;

        println!(
            "{} {} (JIT)",
            "   Running".bold().green(),
            target.as_deref().unwrap_or(&config.package.name).bold(),
        );

        let exit_code = tcc
            .run_main(&args)
            .map_err(|e| HutError::Other(format!("JIT execution failed: {e}")))?;

        // Flush stdout — JIT'ed code and hut share the same output buffer
        use std::io::Write;
        std::io::stdout().flush().ok();

        if exit_code != 0 {
            return Err(HutError::Other(format!(
                "Process exited with code {exit_code}"
            )));
        }

        return Ok(());
    }

    // ── Normal build + run path ──────────────────────────────────────────
    // Build first
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;
    let resolved = if config.dependencies.is_empty()
        && config.build_dependencies.is_empty()
        && config.test_dependencies.is_empty()
    {
        vec![]
    } else {
        let index = hut::index::PackagesIndex::load_builtin()?;
        hut::resolver::resolve_dependencies(&config, &lockfile, &index, &cache_dir())?
    };

    hut::builder::build_project(&config, &resolved, release)?;

    // Determine the binary to run
    let profile = if release { "release" } else { "debug" };
    let target_name = target.as_deref().unwrap_or(&config.package.name);
    let binary = project_root.join("target").join(profile).join(target_name);

    if !binary.exists() {
        // Maybe it's a script?
        if let Some(script) = config.scripts.get(target_name) {
            println!(
                "{} {}",
                "Running script:".bold().dimmed(),
                target_name.bold()
            );
            let shell = if cfg!(windows) { "cmd" } else { "sh" };
            let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

            let status = Command::new(shell)
                .arg(shell_arg)
                .arg(script)
                .args(&args)
                .status()
                .map_err(|e| HutError::Other(format!("Failed to run script: {e}")))?;

            if !status.success() {
                return Err(HutError::Other(format!(
                    "Script exited with code {}",
                    status.code().unwrap_or(-1)
                )));
            }
            return Ok(());
        }

        return Err(HutError::Build(format!(
            "Binary not found at {}. Did the build succeed?",
            binary.display()
        )));
    }

    println!(
        "{} {} {}",
        "   Running".bold().green(),
        target_name.bold(),
        args.join(" ").dimmed()
    );

    let status = Command::new(&binary)
        .args(&args)
        .status()
        .map_err(|e| HutError::Other(format!("Failed to run {}: {}", binary.display(), e)))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        if code != 0 {
            return Err(HutError::Other(format!("Process exited with code {code}")));
        }
    }

    Ok(())
}
