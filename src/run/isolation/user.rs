//! Module for setting RUNNER_UID and RUNNER_GID for the process

use crate::error::prelude::*;

use super::{RUNNER_GID, RUNNER_UID};

pub fn su() -> Result {
    debug!("Switching User");

    // Ensure our capabilities aren't kept when we become runner
    nix::sys::prctl::set_keepcaps(false).context("Couldn't set keepcaps to false")?;

    // Set all our RUNNER_GIDs
    nix::unistd::setresgid(RUNNER_GID, RUNNER_GID, RUNNER_GID).context("Couldn't setresgid")?;

    // Set groups
    nix::unistd::setgroups(&[RUNNER_GID]).context("Couldn't setgroups")?;

    let gid = nix::unistd::getgid();
    debug!("GID is now {gid}");

    // Set all our RUNNER_UIDs
    nix::unistd::setresuid(RUNNER_UID, RUNNER_UID, RUNNER_UID).context("Couldn't setresuid")?;

    let uid = nix::unistd::getuid();
    debug!("UID is now {uid}");

    Ok(())
}
