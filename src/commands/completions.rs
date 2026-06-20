// ── cmd_completions ──────────────────────────────────────────────────────────

use clap::CommandFactory;
use clap_complete::{Shell, generate};
use colored::Colorize;

use hut::error::HutResult;

use crate::Cli;

pub fn cmd_completions(shell: &str) -> HutResult<()> {
    let sh = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "pwsh" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        _ => {
            eprintln!(
                "{} Unknown shell '{}'. Supported: bash, zsh, fish, powershell",
                "error:".red().bold(),
                shell
            );
            return Ok(());
        }
    };

    let mut cmd = Cli::command();
    generate(sh, &mut cmd, "hut", &mut std::io::stdout());

    Ok(())
}
