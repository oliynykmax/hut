mod helpers;

pub(crate) use helpers::{
    APP_MAIN_C, HELLO_WORLD_C, HELLO_WORLD_CPP, LIB_HEADER, LIB_SOURCE, RAYLIB_GAME_C,
    available_compilers, cache_dir, command_exists, find_project_root, hut_home, lockfile_path,
    packages_dir,
};

// Command modules
pub mod add;
pub mod build;
pub mod completions;
pub mod create;
pub mod dev;
pub mod fmt_lint_clean;
pub mod info;
pub mod init;
pub mod install;
pub mod link;
pub mod outdated;
pub mod patch;
pub mod pm;
pub mod remove;
pub mod run;
pub mod search;
pub mod test;
pub mod update;
pub mod upgrade;
pub mod workspace;
// Re-export all command functions
pub use add::cmd_add;
pub use build::cmd_build;
pub use completions::cmd_completions;
pub use create::cmd_create;
pub use dev::cmd_dev;
pub use fmt_lint_clean::{cmd_clean, cmd_fmt, cmd_lint};
pub use info::cmd_info;
pub use init::cmd_init;
pub use install::cmd_install;
pub use link::{cmd_link, cmd_unlink};
pub use outdated::cmd_outdated;
pub use patch::cmd_patch;
pub use pm::{cmd_pm, cmd_publish};
pub use remove::cmd_remove;
pub use run::cmd_run;
pub use search::cmd_search;
pub use test::cmd_test;
pub use update::cmd_update;
pub use upgrade::cmd_upgrade;
pub use workspace::cmd_workspace;
