// ── cmd_create ──────────────────────────────────────────────────────────────


use colored::Colorize;

use hut::config::HutConfig;
use hut::error::HutResult;

use crate::commands::{
    APP_MAIN_C, LIB_HEADER, LIB_SOURCE, RAYLIB_GAME_C,
};

pub fn cmd_create(template: &str) -> HutResult<()> {
    let cwd = std::env::current_dir()?;

    match template {
        "lib" => {
            let config = HutConfig::default_template("mylib");
            let config_path = cwd.join("hut.toml");
            if config_path.exists() {
                eprintln!("{} hut.toml already exists", "warning:".yellow().bold());
                return Ok(());
            }
            config.save(&config_path)?;
            println!("{} hut.toml", "Created".green().bold());

            // Create include/ and src/ directories
            let inc = cwd.join("include");
            let src = cwd.join("src");
            std::fs::create_dir_all(&inc)?;
            std::fs::create_dir_all(&src)?;

            std::fs::write(inc.join("mylib.h"), LIB_HEADER)?;
            std::fs::write(src.join("mylib.c"), LIB_SOURCE)?;

            println!("{} include/mylib.h", "Created".green().bold());
            println!("{} src/mylib.c", "Created".green().bold());
        }
        "app" => {
            let config = HutConfig::default_template("myapp");
            let config_path = cwd.join("hut.toml");
            if config_path.exists() {
                eprintln!("{} hut.toml already exists", "warning:".yellow().bold());
                return Ok(());
            }
            config.save(&config_path)?;
            println!("{} hut.toml", "Created".green().bold());

            let src = cwd.join("src");
            std::fs::create_dir_all(&src)?;
            std::fs::write(src.join("main.c"), APP_MAIN_C)?;
            println!("{} src/main.c", "Created".green().bold());
        }
        "raylib-game" => {
            let mut config = HutConfig::default_template("raylib-game");
            // Add raylib as a dependency
            config
                .dependencies
                .insert("raylib".to_string(), "^5.0".to_string());
            let config_path = cwd.join("hut.toml");
            if config_path.exists() {
                eprintln!("{} hut.toml already exists", "warning:".yellow().bold());
                return Ok(());
            }
            config.save(&config_path)?;
            println!(
                "{} hut.toml (with raylib dependency)",
                "Created".green().bold()
            );

            let src = cwd.join("src");
            std::fs::create_dir_all(&src)?;
            std::fs::write(src.join("main.c"), RAYLIB_GAME_C)?;
            println!("{} src/main.c", "Created".green().bold());
        }
        _ => {
            eprintln!(
                "{} unknown template '{}'. Available: lib, app, raylib-game",
                "error:".red().bold(),
                template
            );
            std::process::exit(1);
        }
    }

    println!();
    println!("{} Run `hut build` to compile.", "Done!".green().bold());
    Ok(())
}
