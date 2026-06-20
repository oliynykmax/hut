// ── cmd_x ──────────────────────────────────────────────────────────

use std::path::PathBuf;

use colored::Colorize;

use crate::cli::{PmCommand, WorkspaceCommand};
use crate::commands::{cache_dir, find_project_root, hut_home, lockfile_path, packages_dir};
use hut::config::HutConfig;
use hut::error::{HutError, HutResult};
use hut::lockfile::{LockedPackage, Lockfile};

pub fn cmd_x(pkg: &str, args: &[String]) -> HutResult<()> {
    hut::fetcher::fetch_and_run(pkg, args)
}
