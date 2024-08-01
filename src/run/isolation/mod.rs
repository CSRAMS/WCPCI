use std::path::Path;

use chroot::chroot;
use environment::{setup_environment, setup_environment_post_chroot};
use harden::harden_process;
use id_map::wait_for_id_mapping;
use nix::unistd::{Gid, Uid};
use unshare::unshare;
use user::su;

use crate::error::prelude::*;

use super::job::JobRequest;

mod chroot;
mod environment;
mod harden;
pub mod id_map;
mod mounts;
pub mod seccomp;
mod syscalls;
mod unshare;
mod user;

const RUNNER_UID: Uid = Uid::from_raw(1000);
const RUNNER_GID: Gid = Gid::from_raw(100);

/// Isolate a process in a new namespace.
pub fn isolate(req: &JobRequest, root: &Path) -> Result {
    debug!("Isolating Process");
    unshare().context("Couldn't unshare")?;
    wait_for_id_mapping()?;
    setup_environment(root, &req.language.expose_paths).context("Couldn't setup environment")?;
    chroot(root).context("Couldn't chroot to jail")?;
    setup_environment_post_chroot().context("Couldn't setup environment post chroot")?;
    su().context("Couldn't switch to runner user")?;
    harden_process().context("Couldn't harden process")?;
    seccomp::install_filters(&req.language.seccomp_program)
        .context("Couldn't install seccomp filters")?;
    debug!("Isolation Complete");
    Ok(())
}
