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
