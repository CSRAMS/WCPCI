//! Module for setting up the environment for the runner
//! Includes a pre-chroot setup and a post-chroot setup

use std::{
    fs::Permissions,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use super::{mounts::setup_mounts, RUNNER_GID, RUNNER_UID};
use crate::error::prelude::*;

const DEV_LINKS: [(&str, &str); 4] = [
    ("dev/stdin", "/proc/self/fd/0"),
    ("dev/stdout", "/proc/self/fd/1"),
    ("dev/stderr", "/proc/self/fd/2"),
    ("dev/fd", "/proc/self/fd/"),
];

fn link(root: &Path, path: &str, target: &str) -> Result {
    let link_path = root.join(path);

    debug!("Creating symlink {} -> \"{}\"", link_path.display(), target);

    std::os::unix::fs::symlink(target, link_path).context("Couldn't create symlink")
}

// Sticky, read/write/execute for all
const TEMP_FOLDER_PERMS: u32 = 0o1777;

fn mk_temp(root: &Path, path: &str) -> Result {
    let path = root.join(path);

    debug!("Creating {} temp directory in new root", path.display());

    std::fs::create_dir_all(&path).context("Couldn't create temp directory in new root")?;
    std::fs::set_permissions(&path, Permissions::from_mode(TEMP_FOLDER_PERMS))
        .context("Couldn't set permissions on temp directory")
}

const RUNNER_USER: &str = "runner";
const HOME_DIR: &str = "/home/runner";

fn setup_home() -> Result {
    debug!("Setting up {HOME_DIR} directory");

    std::fs::create_dir_all(HOME_DIR).context("Couldn't create runner directory")?;

    nix::unistd::chown(HOME_DIR, Some(RUNNER_UID), Some(RUNNER_GID))
        .context("Couldn't chown runner directory")
}

fn setup_env_vars() -> Result {
    std::env::set_var("HOME", HOME_DIR);
    std::env::set_var("USER", RUNNER_USER);
    std::env::set_current_dir(HOME_DIR).context("Couldn't set current directory to HOME")
}

pub fn setup_environment(root: &Path, bind_mounts: &[PathBuf]) -> Result {
    setup_mounts(root, bind_mounts)?;

    for (link_path, target) in DEV_LINKS.iter() {
        link(root, link_path, target)?;
    }

    mk_temp(root, "tmp")?;
    mk_temp(root, "dev/shm")
}

pub fn setup_environment_post_chroot() -> Result {
    setup_home()?;
    setup_env_vars()
}
