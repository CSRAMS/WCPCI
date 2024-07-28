use std::path::{Path, PathBuf};

use anyhow::bail;
use nix::mount::{MntFlags, MsFlags};

use crate::error::prelude::*;

pub struct BindMount {
    target: PathBuf,
}

impl BindMount {
    pub async fn new(root: &Path, path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            bail!("Path ({}) must be absolute", path.to_string_lossy());
        }

        let no_root_path = path.strip_prefix("/").context("Couldn't strip prefix /")?;

        let full_path = root.join(no_root_path);

        if path.is_dir() {
            tokio::fs::create_dir_all(&full_path)
                .await
                .context("Couldn't create expose directory")?;
        } else {
            tokio::fs::create_dir_all(full_path.parent().context("Couldn't get parent")?)
                .await
                .context("Couldn't create expose directory")?;
            tokio::fs::File::create(&full_path)
                .await
                .context("Couldn't create expose file")?;
        }

        nix::mount::mount(
            Some(path),
            &full_path,
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_RDONLY,
            None::<&str>,
        )
        .context("Couldn't run bind mount syscall")?;

        Ok(Self { target: full_path })
    }

    pub fn unmount(&self) -> Result {
        if !self.target.exists() {
            return Ok(());
        }

        nix::mount::umount2(&self.target, MntFlags::empty())
            .context("Couldn't unmount bind mount")?;

        if self.target.is_dir() {
            std::fs::remove_dir_all(&self.target)
                .context("Couldn't remove bind mount directory")?;
        } else {
            std::fs::remove_file(&self.target).context("Couldn't remove bind mount file")?;
        }

        Ok(())
    }
}

impl Drop for BindMount {
    fn drop(&mut self) {
        if let Err(e) = self.unmount() {
            error!("{e}");
        }
    }
}

async fn chroot_jail(new_root: &Path) -> Result<ProcHandle> {
    // cd and chroot to the new root directory
    std::env::set_current_dir(new_root).context("Couldn't set current directory to new root")?;
    nix::unistd::chroot(new_root).context("Couldn't chroot to new root")?;

    // Mount the /proc filesystem

    tokio::fs::create_dir_all("/proc")
        .await
        .context("Couldn't create /proc directory")?;

    nix::mount::mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        nix::mount::MsFlags::empty(),
        None::<&str>,
    )
    .context("Couldn't mount /proc")?;

    // Create temp directory for /tmp

    tokio::fs::create_dir_all("/tmp")
        .await
        .context("Couldn't create /tmp directory")?;

    Ok(ProcHandle)
}

struct ProcHandle;

impl Drop for ProcHandle {
    fn drop(&mut self) {
        if Path::new("/proc").exists() {
            nix::mount::umount("/proc").unwrap_or_else(|why| {
                warn!("Couldn't unmount /proc: {:?}", why);
            });
        }
    }
}

// Holder for any handles that need to run destructors
// in order to clean up the process container
pub struct LockdownHandle {
    #[allow(dead_code)]
    proc_handle: ProcHandle,
}

/// Run to lockdown the running process
/// This should *only be run in a worker process*
pub async fn lockdown_process(new_root: &Path) -> Result<LockdownHandle> {
    let proc_handle = chroot_jail(new_root).await?;

    Ok(LockdownHandle { proc_handle })
}
