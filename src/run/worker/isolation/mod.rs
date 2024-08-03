use std::{path::Path, time::Instant};

use chroot::chroot;
use environment::{setup_environment, setup_environment_post_chroot};
use harden::harden_process;
use id_map::wait_for_id_mapping;
use mounts::mount_root;
use nix::unistd::{Gid, Uid};
use unshare::unshare;
use user::{su_root, su_runner};

use crate::error::prelude::*;

mod chroot;
mod config;
mod environment;
mod harden;
pub mod id_map;
mod mounts;
pub mod seccomp;
mod syscalls;
mod unshare;
mod user;

pub use config::*;

const RUNNER_UID: Uid = Uid::from_raw(1000);
const RUNNER_GID: Gid = Gid::from_raw(100);

/// Isolate a process in a new namespace.
pub fn isolate(config: &IsolationConfig, root: &Path) -> Result {
    debug!("Isolating Process");
    let instant = Instant::now();
    unshare().context("Couldn't unshare")?;
    wait_for_id_mapping()?;
    su_root()?;
    mount_root(root).context("Couldn't mount root")?;
    setup_environment(root, &config.bind_mounts).context("Couldn't setup environment")?;
    chroot(root).context("Couldn't chroot to jail")?;
    setup_environment_post_chroot().context("Couldn't setup environment post chroot")?;
    su_runner()?;
    // TODO: Limits?
    // - Memory
    // - CPU
    // - Disk (tmpfs so still effectively memory, tmpfs mount has a size limit)
    harden_process().context("Couldn't harden process")?;
    let program = config
        .compiled_seccomp_program
        .as_ref()
        .context("Seccomp program not compiled")?;
    seccomp::install_filters(program).context("Couldn't install seccomp filters")?;
    let elapsed = instant.elapsed();
    debug!("Isolation Complete ({elapsed:?})");
    Ok(())
}
