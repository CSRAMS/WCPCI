//! Module for setting RUNNER_UID and RUNNER_GID for the process

use crate::error::prelude::*;

use super::{RUNNER_GID, RUNNER_UID};

fn print_uid_gid() {
    let uid = nix::unistd::getuid();
    let gid = nix::unistd::getgid();
    debug!("UID: {uid}, GID: {gid}");
}

fn set_keepcaps(keepcaps: bool) -> Result {
    nix::sys::prctl::set_keepcaps(keepcaps).context("Couldn't set keepcaps")
}

fn su(uid: nix::unistd::Uid, gid: nix::unistd::Gid, disp_name: &str) -> Result {
    debug!("Switching User to {disp_name}");

    nix::unistd::setresgid(gid, gid, gid).context("Couldn't setresgid")?;
    nix::unistd::setgroups(&[gid]).context("Couldn't setgroups")?;
    nix::unistd::setresuid(uid, uid, uid).context("Couldn't setresuid")?;

    print_uid_gid();

    Ok(())
}

pub fn su_root() -> Result {
    let root_gid = nix::unistd::Gid::from_raw(0);
    let root_uid = nix::unistd::Uid::from_raw(0);
    set_keepcaps(true)?;
    su(root_uid, root_gid, "root").context("Couldn't switch to root")?;
    set_keepcaps(false)
}

pub fn su_runner() -> Result {
    // We don't want to keep capabilities
    set_keepcaps(false)?;
    su(RUNNER_UID, RUNNER_GID, "runner").context("Couldn't switch to runner")
}
