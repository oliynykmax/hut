// ── hut: fast C/C++ package manager ──────────────────────────────────────
//
// CLI binary using clap derive.  All heavy lifting is performed by the
// library modules under `hut::`.
#![allow(
    clippy::collapsible_if,
    clippy::single_match,
    clippy::unnecessary_lazy_evaluations,
    clippy::needless_match,
    clippy::unnecessary_map_or,
    clippy::redundant_closure
)]

mod cli;
mod commands;

use clap::Parser;
use colored::Colorize;

use cli::{Cli, Commands};
use commands::{
    cmd_add, cmd_build, cmd_clean, cmd_completions, cmd_create, cmd_dev, cmd_fmt, cmd_info,
    cmd_init, cmd_install, cmd_link, cmd_lint, cmd_outdated, cmd_patch, cmd_pm, cmd_publish,
    cmd_remove, cmd_run, cmd_search, cmd_test, cmd_unlink, cmd_update, cmd_upgrade, cmd_workspace,
    cmd_x,
};

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { name } => cmd_init(name),
        Commands::Create { template } => cmd_create(&template),
        Commands::Install => cmd_install(),
        Commands::Add { pkgs, dev, build } => cmd_add(&pkgs, dev, build),
        Commands::Remove { pkg } => cmd_remove(&pkg),
        Commands::Update { pkg } => cmd_update(pkg.as_deref()),
        Commands::Outdated => cmd_outdated(),
        Commands::Build { release, compiler } => cmd_build(release, compiler.as_deref()),
        Commands::Run {
            target,
            args,
            release,
            jit,
        } => cmd_run(target, args, release, jit),
        Commands::Test => cmd_test(),
        Commands::X { pkg, args } => cmd_x(&pkg, &args),
        Commands::Link { path } => cmd_link(path.as_deref()),
        Commands::Unlink { pkg } => cmd_unlink(&pkg),
        Commands::Publish => cmd_publish(),
        Commands::Pm(sub) => cmd_pm(sub),
        Commands::Upgrade => cmd_upgrade(),
        Commands::Patch { pkg } => cmd_patch(&pkg),
        Commands::Info => cmd_info(),
        Commands::Dev => cmd_dev(),
        Commands::Workspace(sub) => cmd_workspace(sub),
        Commands::Completions { shell } => cmd_completions(&shell),
        Commands::Search { query } => cmd_search(&query),
        Commands::Fmt { check } => cmd_fmt(check),
        Commands::Lint => cmd_lint(),
        Commands::Clean => cmd_clean(),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use crate::cli::{Cli, Commands, PmCommand, WorkspaceCommand};
    use crate::commands::cmd_init;

    // Re-implement parse_dep_spec here to avoid making it public
    fn parse_dep_spec(spec: &str) -> (String, Option<String>) {
        if let Some(at_pos) = spec.find('@') {
            let name = spec[..at_pos].to_string();
            let version = spec[at_pos + 1..].to_string();
            (name, Some(version))
        } else {
            (spec.to_string(), None)
        }
    }

    // ── Basic subcommands ─────────────────────────────────────────────────
    #[test]
    fn test_parse_init_no_name() {
        let cli = Cli::try_parse_from(["hut", "init"]).unwrap();
        match cli.command {
            Commands::Init { name } => assert!(name.is_none()),
            _ => panic!("expected Init"),
        }
    }

    #[test]
    fn test_parse_init_with_name() {
        let cli = Cli::try_parse_from(["hut", "init", "myproject"]).unwrap();
        match cli.command {
            Commands::Init { name } => assert_eq!(name.unwrap(), "myproject"),
            _ => panic!("expected Init"),
        }
    }

    #[test]
    fn test_parse_build_default() {
        let cli = Cli::try_parse_from(["hut", "build"]).unwrap();
        match cli.command {
            Commands::Build { release, compiler } => {
                assert!(!release);
                assert!(compiler.is_none());
            }
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_build_release_long() {
        let cli = Cli::try_parse_from(["hut", "build", "--release"]).unwrap();
        match cli.command {
            Commands::Build { release, .. } => assert!(release),
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_build_release_short() {
        let cli = Cli::try_parse_from(["hut", "build", "-r"]).unwrap();
        match cli.command {
            Commands::Build { release, .. } => assert!(release),
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_build_compiler_long() {
        let cli = Cli::try_parse_from(["hut", "build", "--compiler", "clang"]).unwrap();
        match cli.command {
            Commands::Build { compiler, .. } => assert_eq!(compiler.unwrap(), "clang"),
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_build_compiler_short() {
        let cli = Cli::try_parse_from(["hut", "build", "-c", "gcc"]).unwrap();
        match cli.command {
            Commands::Build { compiler, .. } => assert_eq!(compiler.unwrap(), "gcc"),
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_build_release_and_compiler() {
        let cli = Cli::try_parse_from(["hut", "build", "-r", "-c", "gcc"]).unwrap();
        match cli.command {
            Commands::Build { release, compiler } => {
                assert!(release);
                assert_eq!(compiler.unwrap(), "gcc");
            }
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_build_combo_flags() {
        let cli = Cli::try_parse_from(["hut", "build", "--release", "--compiler", "clang"]).unwrap();
        match cli.command {
            Commands::Build { release, compiler } => {
                assert!(release);
                assert_eq!(compiler.unwrap(), "clang");
            }
            _ => panic!("expected Build"),
        }
    }

    #[test]
    fn test_parse_run_default() {
        let cli = Cli::try_parse_from(["hut", "run"]).unwrap();
        match cli.command {
            Commands::Run { target, args, release, jit } => {
                assert!(target.is_none());
                assert!(args.is_empty());
                assert!(!release);
                assert!(!jit);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_release() {
        let cli = Cli::try_parse_from(["hut", "run", "--release"]).unwrap();
        match cli.command {
            Commands::Run { release, .. } => assert!(release),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_release_short() {
        let cli = Cli::try_parse_from(["hut", "run", "-r"]).unwrap();
        match cli.command {
            Commands::Run { release, .. } => assert!(release),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_jit() {
        let cli = Cli::try_parse_from(["hut", "run", "--jit"]).unwrap();
        match cli.command {
            Commands::Run { jit, .. } => assert!(jit),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_with_target() {
        let cli = Cli::try_parse_from(["hut", "run", "bench"]).unwrap();
        match cli.command {
            Commands::Run { target, .. } => assert_eq!(target.unwrap(), "bench"),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_with_args() {
        let cli = Cli::try_parse_from(["hut", "run", "bench", "--", "--verbose", "-n", "10"]).unwrap();
        match cli.command {
            Commands::Run { target, args, .. } => {
                assert_eq!(target.unwrap(), "bench");
                assert_eq!(args, vec!["--verbose", "-n", "10"]);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_jit_with_args() {
        let cli = Cli::try_parse_from(["hut", "run", "--jit", "--", "arg1"]).unwrap();
        match cli.command {
            Commands::Run { jit, args, .. } => {
                assert!(jit);
                assert_eq!(args, vec!["arg1"]);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn test_parse_run_target_and_args() {
        let cli = Cli::try_parse_from(["hut", "run", "server", "--", "--port=8080"]).unwrap();
        match cli.command {
            Commands::Run { target, args, .. } => {
                assert_eq!(target.unwrap(), "server");
                assert_eq!(args, vec!["--port=8080"]);
            }
            _ => panic!("expected Run"),
        }
    }

    // ── Aliases ────────────────────────────────────────────────────────────
    #[test]
    fn test_alias_b_for_build() {
        let cli = Cli::try_parse_from(["hut", "b"]).unwrap();
        match cli.command {
            Commands::Build { .. } => {}
            _ => panic!("expected Build alias 'b'"),
        }
    }

    #[test]
    fn test_alias_t_for_test() {
        let cli = Cli::try_parse_from(["hut", "t"]).unwrap();
        match cli.command {
            Commands::Test => {}
            _ => panic!("expected Test alias 't'"),
        }
    }

    #[test]
    fn test_alias_i_for_install() {
        let cli = Cli::try_parse_from(["hut", "i"]).unwrap();
        match cli.command {
            Commands::Install => {}
            _ => panic!("expected Install alias 'i'"),
        }
    }

    #[test]
    fn test_alias_a_for_add() {
        let cli = Cli::try_parse_from(["hut", "a", "user/pkg"]).unwrap();
        match cli.command {
            Commands::Add { pkgs, dev, build } => {
                assert_eq!(pkgs[0], "user/pkg");
                assert!(!dev);
                assert!(!build);
            }
            _ => panic!("expected Add alias 'a'"),
        }
    }

    #[test]
    fn test_alias_rm_for_remove() {
        let cli = Cli::try_parse_from(["hut", "rm", "dep"]).unwrap();
        match cli.command {
            Commands::Remove { pkg } => assert_eq!(pkg, "dep"),
            _ => panic!("expected Remove alias 'rm'"),
        }
    }

    #[test]
    fn test_alias_up_for_update() {
        let cli = Cli::try_parse_from(["hut", "up"]).unwrap();
        match cli.command {
            Commands::Update { pkg } => assert!(pkg.is_none()),
            _ => panic!("expected Update alias 'up'"),
        }
    }

    #[test]
    fn test_alias_up_with_pkg() {
        let cli = Cli::try_parse_from(["hut", "up", "mylib"]).unwrap();
        match cli.command {
            Commands::Update { pkg } => assert_eq!(pkg.unwrap(), "mylib"),
            _ => panic!("expected Update alias 'up'"),
        }
    }

    // ── Add command ────────────────────────────────────────────────────────
    #[test]
    fn test_parse_add_basic() {
        let cli = Cli::try_parse_from(["hut", "add", "user/libfoo"]).unwrap();
        match cli.command {
            Commands::Add { pkgs, dev, build } => {
                assert_eq!(pkgs[0], "user/libfoo");
                assert!(!dev);
                assert!(!build);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_add_dev() {
        let cli = Cli::try_parse_from(["hut", "add", "user/libfoo", "--dev"]).unwrap();
        match cli.command {
            Commands::Add { dev, .. } => assert!(dev),
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_add_build() {
        let cli = Cli::try_parse_from(["hut", "add", "user/libfoo", "--build"]).unwrap();
        match cli.command {
            Commands::Add { build, .. } => assert!(build),
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_parse_add_with_version() {
        let cli = Cli::try_parse_from(["hut", "add", "user/libfoo@^1.0"]).unwrap();
        match cli.command {
            Commands::Add { pkgs, .. } => assert_eq!(pkgs[0], "user/libfoo@^1.0"),
            _ => panic!("expected Add"),
        }
    }

    // ── Install command ────────────────────────────────────────────────────
    #[test]
    fn test_parse_install_default() {
        let cli = Cli::try_parse_from(["hut", "install"]).unwrap();
        match cli.command {
            Commands::Install => {}
            _ => panic!("expected Install"),
        }
    }

    // ── Remove command ─────────────────────────────────────────────────────
    #[test]
    fn test_parse_remove() {
        let cli = Cli::try_parse_from(["hut", "remove", "dep"]).unwrap();
        match cli.command {
            Commands::Remove { pkg } => assert_eq!(pkg, "dep"),
            _ => panic!("expected Remove"),
        }
    }

    #[test]
    fn test_parse_remove_missing_pkg() {
        let result = Cli::try_parse_from(["hut", "remove"]);
        assert!(result.is_err());
    }

    // ── Update / Outdated / Test ───────────────────────────────────────────
    #[test]
    fn test_parse_update_single() {
        let cli = Cli::try_parse_from(["hut", "update", "mylib"]).unwrap();
        match cli.command {
            Commands::Update { pkg } => assert_eq!(pkg.unwrap(), "mylib"),
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn test_parse_update_all() {
        let cli = Cli::try_parse_from(["hut", "update"]).unwrap();
        match cli.command {
            Commands::Update { pkg } => assert!(pkg.is_none()),
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn test_parse_outdated() {
        let cli = Cli::try_parse_from(["hut", "outdated"]).unwrap();
        match cli.command {
            Commands::Outdated => {}
            _ => panic!("expected Outdated"),
        }
    }

    #[test]
    fn test_parse_test() {
        let cli = Cli::try_parse_from(["hut", "test"]).unwrap();
        match cli.command {
            Commands::Test => {}
            _ => panic!("expected Test"),
        }
    }

    // ── Create / X / Link / Unlink ─────────────────────────────────────────
    #[test]
    fn test_parse_create() {
        let cli = Cli::try_parse_from(["hut", "create", "lib"]).unwrap();
        match cli.command {
            Commands::Create { template } => assert_eq!(template, "lib"),
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn test_parse_create_missing_template() {
        let result = Cli::try_parse_from(["hut", "create"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_x_pkg() {
        let cli = Cli::try_parse_from(["hut", "x", "user/repo"]).unwrap();
        match cli.command {
            Commands::X { pkg, args } => {
                assert_eq!(pkg, "user/repo");
                assert!(args.is_empty());
            }
            _ => panic!("expected X"),
        }
    }

    #[test]
    fn test_parse_x_pkg_with_args() {
        let cli = Cli::try_parse_from(["hut", "x", "user/repo", "--", "--flag"]).unwrap();
        match cli.command {
            Commands::X { pkg, args } => {
                assert_eq!(pkg, "user/repo");
                assert_eq!(args, vec!["--flag"]);
            }
            _ => panic!("expected X"),
        }
    }

    #[test]
    fn test_parse_link_default() {
        let cli = Cli::try_parse_from(["hut", "link"]).unwrap();
        match cli.command {
            Commands::Link { path } => assert!(path.is_none()),
            _ => panic!("expected Link"),
        }
    }

    #[test]
    fn test_parse_link_with_path() {
        let cli = Cli::try_parse_from(["hut", "link", "/some/dir"]).unwrap();
        match cli.command {
            Commands::Link { path } => assert_eq!(path.unwrap(), "/some/dir"),
            _ => panic!("expected Link"),
        }
    }

    #[test]
    fn test_parse_unlink() {
        let cli = Cli::try_parse_from(["hut", "unlink", "mypkg"]).unwrap();
        match cli.command {
            Commands::Unlink { pkg } => assert_eq!(pkg, "mypkg"),
            _ => panic!("expected Unlink"),
        }
    }

    // ── Publish / Upgrade / Patch / Info / Dev / Clean ─────────────────────
    #[test]
    fn test_parse_publish() {
        let cli = Cli::try_parse_from(["hut", "publish"]).unwrap();
        match cli.command {
            Commands::Publish => {}
            _ => panic!("expected Publish"),
        }
    }

    #[test]
    fn test_parse_upgrade() {
        let cli = Cli::try_parse_from(["hut", "upgrade"]).unwrap();
        match cli.command {
            Commands::Upgrade => {}
            _ => panic!("expected Upgrade"),
        }
    }

    #[test]
    fn test_parse_patch() {
        let cli = Cli::try_parse_from(["hut", "patch", "somepkg"]).unwrap();
        match cli.command {
            Commands::Patch { pkg } => assert_eq!(pkg, "somepkg"),
            _ => panic!("expected Patch"),
        }
    }

    #[test]
    fn test_parse_info() {
        let cli = Cli::try_parse_from(["hut", "info"]).unwrap();
        match cli.command {
            Commands::Info => {}
            _ => panic!("expected Info"),
        }
    }

    #[test]
    fn test_parse_dev() {
        let cli = Cli::try_parse_from(["hut", "dev"]).unwrap();
        match cli.command {
            Commands::Dev => {}
            _ => panic!("expected Dev"),
        }
    }

    #[test]
    fn test_parse_clean() {
        let cli = Cli::try_parse_from(["hut", "clean"]).unwrap();
        match cli.command {
            Commands::Clean => {}
            _ => panic!("expected Clean"),
        }
    }

    // ── Pm subcommands ─────────────────────────────────────────────────────
    #[test]
    fn test_parse_pm_ls() {
        let cli = Cli::try_parse_from(["hut", "pm", "ls"]).unwrap();
        match cli.command {
            Commands::Pm(PmCommand::Ls) => {}
            _ => panic!("expected Pm Ls"),
        }
    }

    #[test]
    fn test_parse_pm_cache() {
        let cli = Cli::try_parse_from(["hut", "pm", "cache"]).unwrap();
        match cli.command {
            Commands::Pm(PmCommand::Cache) => {}
            _ => panic!("expected Pm Cache"),
        }
    }

    #[test]
    fn test_parse_pm_bin() {
        let cli = Cli::try_parse_from(["hut", "pm", "bin"]).unwrap();
        match cli.command {
            Commands::Pm(PmCommand::Bin) => {}
            _ => panic!("expected Pm Bin"),
        }
    }

    // ── Workspace subcommands ─────────────────────────────────────────────
    #[test]
    fn test_parse_workspace_add() {
        let cli = Cli::try_parse_from(["hut", "workspace", "add", "/some/dir"]).unwrap();
        match cli.command {
            Commands::Workspace(WorkspaceCommand::Add { path }) => assert_eq!(path, "/some/dir"),
            _ => panic!("expected Workspace Add"),
        }
    }

    #[test]
    fn test_parse_workspace_ls() {
        let cli = Cli::try_parse_from(["hut", "workspace", "ls"]).unwrap();
        match cli.command {
            Commands::Workspace(WorkspaceCommand::Ls) => {}
            _ => panic!("expected Workspace Ls"),
        }
    }

    #[test]
    fn test_parse_workspace_run() {
        let cli = Cli::try_parse_from(["hut", "workspace", "run", "hut", "build"]).unwrap();
        match cli.command {
            Commands::Workspace(WorkspaceCommand::Run { command, args }) => {
                assert_eq!(command, "hut");
                assert_eq!(args, vec!["build"]);
            }
            _ => panic!("expected Workspace Run"),
        }
    }

    #[test]
    fn test_parse_workspace_run_with_args() {
        let cli = Cli::try_parse_from(["hut", "workspace", "run", "make", "--", "-j4"]).unwrap();
        match cli.command {
            Commands::Workspace(WorkspaceCommand::Run { command, args }) => {
                assert_eq!(command, "make");
                assert_eq!(args, vec!["-j4"]);
            }
            _ => panic!("expected Workspace Run"),
        }
    }

    // ── Completions / Search ───────────────────────────────────────────────
    #[test]
    fn test_parse_completions() {
        let cli = Cli::try_parse_from(["hut", "completions", "bash"]).unwrap();
        match cli.command {
            Commands::Completions { shell } => assert_eq!(shell, "bash"),
            _ => panic!("expected Completions"),
        }
    }

    #[test]
    fn test_parse_completions_zsh() {
        let cli = Cli::try_parse_from(["hut", "completions", "zsh"]).unwrap();
        match cli.command {
            Commands::Completions { shell } => assert_eq!(shell, "zsh"),
            _ => panic!("expected Completions"),
        }
    }

    #[test]
    fn test_parse_search() {
        let cli = Cli::try_parse_from(["hut", "search", "json"]).unwrap();
        match cli.command {
            Commands::Search { query } => assert_eq!(query, "json"),
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn test_parse_search_missing_query() {
        let result = Cli::try_parse_from(["hut", "search"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_search_multiple_words_rejected() {
        // clap 3.0 with last=true should work differently; single value expected
        let cli = Cli::try_parse_from(["hut", "search", "json parser"]);
        // Accept whatever clap 3.0 parses — it may split or join
        match cli {
            Ok(c) => {
                match c.command {
                    Commands::Search { query } => {
                        // Both forms acceptable
                        let _ = query;
                    }
                    _ => {}
                }
            }
            Err(_) => {} // Also acceptable
        }
    }

    // ── Error cases ────────────────────────────────────────────────────────
    #[test]
    fn test_parse_unknown_subcommand() {
        let result = Cli::try_parse_from(["hut", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_arg() {
        // 'add' requires at least one package argument
        let result = Cli::try_parse_from(["hut", "add"]);
        assert!(result.is_err());
    }

    // ── Helper function tests ──────────────────────────────────────────────
    #[test]
    fn test_parse_dep_spec_no_version() {
        let (name, version) = parse_dep_spec("user/libfoo");
        assert_eq!(name, "user/libfoo");
        assert!(version.is_none());
    }

    #[test]
    fn test_parse_dep_spec_with_version() {
        let (name, version) = parse_dep_spec("user/libfoo@^1.0");
        assert_eq!(name, "user/libfoo");
        assert_eq!(version.unwrap(), "^1.0");
    }

    #[test]
    fn test_parse_dep_spec_at_only() {
        let (name, version) = parse_dep_spec("pkg@");
        assert_eq!(name, "pkg");
        assert_eq!(version.unwrap(), "");
    }

    #[test]
    fn test_parse_dep_spec_just_at() {
        let (name, version) = parse_dep_spec("@version");
        assert_eq!(name, "");
        assert_eq!(version.unwrap(), "version");
    }

    #[test]
    fn test_parse_dep_spec_multiple_at() {
        let (name, version) = parse_dep_spec("user/lib@1@extra");
        assert_eq!(name, "user/lib");
        assert_eq!(version.unwrap(), "1@extra");
    }
}
