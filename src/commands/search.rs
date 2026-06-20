// ── cmd_search ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::error::HutResult;


pub fn cmd_search(query: &str) -> HutResult<()> {
    let index = hut::index::PackagesIndex::load_builtin()?;
    let results = index.search(query);

    if results.is_empty() {
        println!("{} {}", "No packages found for".dimmed(), query.bold());
        println!(
            "{}",
            "Add custom packages to ~/.config/hut/packages.toml".dimmed()
        );
        return Ok(());
    }

    println!(
        "{} {} results for \"{}\":",
        "Found".green().bold(),
        results.len().to_string().bold(),
        query
    );
    println!();

    for (name, entry) in results {
        println!("  {} — {}", name.bold().cyan(), entry.description.dimmed());
        println!(
            "    repo: {}   includes: [{}]",
            entry.repo.dimmed(),
            entry.includes.join(", ").dimmed()
        );
        if !entry.libs.is_empty() {
            println!("    libs: [{}]", entry.libs.join(", ").dimmed());
        }
        println!();
    }

    Ok(())
}
