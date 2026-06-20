// ── cmd_info ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::HutResult;
use hut::lockfile::Lockfile;

use crate::commands::lockfile_path;

pub fn cmd_info() -> HutResult<()> {
    let (config, config_path) = HutConfig::find()?;
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path)?;

    println!("{}", "Package:".bold().underline());
    println!();
    println!("  name: {}", config.package.name.bold());
    println!("  version: {}", config.package.version);
    println!("  language: {}", config.package.language);
    if let Some(ref desc) = config.package.description {
        println!("  description: {}", desc);
    }
    if let Some(ref lic) = config.package.license {
        println!("  license: {}", lic);
    }
    println!("  config: {}", config_path.display());
    println!();

    // Dependencies
    if !config.dependencies.is_empty() {
        println!("{}", "Dependencies:".bold());
        for (name, version) in &config.dependencies {
            let locked_ver = lockfile
                .get(name)
                .map(|l| format!(" (locked: {})", l.version))
                .unwrap_or_default();
            println!("  {} = {}{}", name.bold(), version, locked_ver.dimmed());
        }
        println!();
    } else {
        println!("{}", "Dependencies:".bold());
        println!("  (none)");
        println!();
    }

    if !config.build_dependencies.is_empty() {
        println!("{}", "Build Dependencies:".bold());
        for (name, version) in &config.build_dependencies {
            println!("  {} = {}", name.bold(), version);
        }
        println!();
    }

    if !config.test_dependencies.is_empty() {
        println!("{}", "Test Dependencies:".bold());
        for (name, version) in &config.test_dependencies {
            println!("  {} = {}", name.bold(), version);
        }
        println!();
    }

    // Build config
    println!("{}", "Build config:".bold());
    println!("  c_standard: {}", config.build.c_standard);
    if let Some(ref cpp) = config.build.cpp_standard {
        println!("  cpp_standard: {}", cpp);
    }
    println!("  opt_level: {}", config.build.opt_level);
    println!("  debug: {}", config.build.debug);
    println!("  compiler: {}", config.build.compiler);
    println!();

    // Scripts
    if !config.scripts.is_empty() {
        println!("{}", "Scripts:".bold());
        for (name, cmd) in &config.scripts {
            println!("  {}: {}", name.bold(), cmd.dimmed());
        }
        println!();
    }

    Ok(())
}
