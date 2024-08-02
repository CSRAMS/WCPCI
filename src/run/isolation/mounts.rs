//! Module for mounting various file systems
//! This is used in the environment module for setting up our jail with
//! required mounts

use std::path::{Path, PathBuf};

use anyhow::bail;
use nix::mount::MsFlags;

use crate::error::prelude::*;

use super::BindMountConfig;

const DEV_BINDS: [&str; 4] = ["/dev/null", "/dev/zero", "/dev/random", "/dev/urandom"];

fn bind_mount(root: &Path, path: &Path, no_exec: bool) -> Result {
    if !path.is_absolute() {
        bail!("Path must be absolute");
    }

    let no_root_path = path.strip_prefix("/").context("Couldn't strip prefix /")?;

    let full_path = root.join(no_root_path);

    debug!(
        "Bind mounting {} to {}",
        full_path.display(),
        path.display()
    );

    if path.is_dir() {
        std::fs::create_dir_all(&full_path).context("Couldn't create expose directory")?;
    } else {
        std::fs::create_dir_all(full_path.parent().context("Couldn't get parent")?)
            .context("Couldn't create expose directory")?;
        std::fs::File::create(&full_path).context("Couldn't create expose file")?;
    }

    let mut flags = MsFlags::MS_BIND
        | MsFlags::MS_RDONLY
        | MsFlags::MS_PRIVATE
        | MsFlags::MS_NOSUID
        | MsFlags::MS_NODEV;

    if no_exec {
        flags |= MsFlags::MS_NOEXEC;
    }

    nix::mount::mount(Some(path), &full_path, None::<&str>, flags, None::<&str>)
        .context("Couldn't run bind mount syscall")
}

fn mount_proc(root: &Path) -> Result {
    let proc_path = root.join("proc");
    std::fs::create_dir_all(&proc_path).context("Couldn't create /proc directory")?;

    debug!("Mounting procfs at {}", proc_path.display());

    // TODO: hidepid={1,2}?
    nix::mount::mount(
        None::<&str>,
        &proc_path,
        Some("proc"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        None::<&str>,
    )
    .context("Couldn't run proc mount syscall")
}

/// Mounts a tmpfs at the given path
/// Used as our root
pub fn mount_root(root: &Path) -> Result {
    debug!("Mounting root tmpfs at {}", root.display());

    nix::mount::mount(
        None::<&str>,
        root,
        Some("tmpfs"),
        MsFlags::MS_NODEV | MsFlags::MS_NOSUID,
        Some("mode=0755"),
    )
    .context("Couldn't mount tmpfs")?;

    std::env::set_current_dir(root).context("Couldn't set current directory to new root")?;

    Ok(())
}

pub fn setup_mounts(root: &Path, bind_mounts: &[BindMountConfig]) -> Result {
    mount_proc(root)?;
    for config in bind_mounts {
        // TODO: Configure NOEXEC per-path
        bind_mount(root, &config.src, config.no_exec).with_context(|| {
            format!(
                "Couldn't bind mount expose path \"{}\"",
                config.src.display()
            )
        })?;
    }
    for dev_path in DEV_BINDS.iter() {
        bind_mount(root, &PathBuf::from(dev_path), true)
            .with_context(|| format!("Couldn't bind mount dev path \"{}\"", dev_path))?;
    }
    Ok(())
}
