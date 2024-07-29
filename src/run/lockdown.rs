use std::path::{Path, PathBuf};

use anyhow::bail;
use log::debug;
use nix::{
    mount::{MntFlags, MsFlags},
    sched::CloneFlags,
    unistd::ForkResult,
};

use crate::{error::prelude::*, run::WorkerMessage};

use super::job::JobRequest;

pub struct BindMount {
    target: PathBuf,
}

impl BindMount {
    pub fn new(root: &Path, path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            bail!("Path must be absolute");
        }

        debug!("Bind mounting {} to {}", path.display(), root.display());

        let no_root_path = path.strip_prefix("/").context("Couldn't strip prefix /")?;

        let full_path = root.join(no_root_path);

        if path.is_dir() {
            std::fs::create_dir_all(&full_path).context("Couldn't create expose directory")?;
        } else {
            std::fs::create_dir_all(full_path.parent().context("Couldn't get parent")?)
                .context("Couldn't create expose directory")?;
            std::fs::File::create(&full_path).context("Couldn't create expose file")?;
        }

        nix::mount::mount(
            Some(path),
            &full_path,
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_RDONLY | MsFlags::MS_PRIVATE,
            None::<&str>,
        )
        .context("Couldn't run bind mount syscall")?;

        Ok(Self {
            target: path.to_path_buf(),
        })
    }

    pub fn unmount(&self) -> Result {
        if !self.target.exists() {
            return Ok(());
        }

        nix::mount::umount2(&self.target, MntFlags::empty())
            .context("Couldn't unmount bind mount")?;

        if self.target.is_dir() {
            std::fs::remove_dir_all(&self.target).context("Couldn't remove bind mount directory")
        } else {
            std::fs::remove_file(&self.target).context("Couldn't remove bind mount file")
        }
    }
}

impl Drop for BindMount {
    fn drop(&mut self) {
        if let Err(e) = self.unmount() {
            error!("{e}");
        }
    }
}
fn setup_namespaces() -> Result {
    nix::sched::unshare(
        CloneFlags::CLONE_NEWUSER
            | CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWNET
            | CloneFlags::CLONE_NEWIPC
            | CloneFlags::CLONE_NEWCGROUP,
    )
    .context("Couldn't create new namespace(s)")
    // TODO: Set up uid and gid mappings
}

fn setup_environment(req: &JobRequest, new_root: &Path) -> Result<Vec<BindMount>> {
    // TODO(Ellis): tmpfs root?
    // TODO(Ellis mostly): make all mounts & created directories configurable?

    // Bind mount the expose paths needed for the language to run

    let mut mounts = Vec::with_capacity(req.language.expose_paths.len());

    for path in &req.language.expose_paths {
        let mount = BindMount::new(new_root, path).with_context(|| {
            format!(
                "Couldn't bind mount expose path ({})",
                path.to_string_lossy()
            )
        })?;
        mounts.push(mount);
    }

    // Mount the /proc filesystem

    std::fs::create_dir_all(new_root.join("proc")).context("Couldn't create /proc directory")?;

    nix::mount::mount(
        None::<&str>,
        &new_root.join("proc"),
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )
    .context("Couldn't mount /proc")?;

    // Create temp directory for /tmp

    std::fs::create_dir_all(new_root.join("tmp"))
        .context("Couldn't create /tmp directory in new root")?;

    Ok(mounts)
}

fn chroot_jail(new_root: &Path) -> Result {
    // cd and chroot to the new root directory
    std::env::set_current_dir(new_root).context("Couldn't set current directory to new root")?;
    nix::unistd::chroot(new_root).context("Couldn't chroot to new root")?;

    Ok(())
}

fn harden_process() -> Result {
    // TODO: Drop capabilities
    // Make sure to drop set time capabilities, or do CloneFlags::CLONE_NEWUTS in unshare

    // TODO: other security things (seccomp)?
    Ok(())
}

/// Run to lockdown the running process
/// This should *only be run in a worker process*
pub fn lockdown_process(req: &JobRequest, new_root: &Path) -> Result<Vec<BindMount>> {
    // Unshare
    setup_namespaces()?;

    // Fork to new PID namespace as pid 1
    // Ellis says this *should* be safe as long as the parent isn't multithreaded at the time it calls `fork`
    unsafe {
        let res = nix::unistd::fork().context("Couldn't fork PID 1 in new PID namespace")?;

        if let ForkResult::Parent { child } = res {
            let msg = WorkerMessage::ChildPid(child.as_raw());
            let msg = serde_json::to_string(&msg).context("Couldn't serialize WorkerMessage")?;
            println!("{}", msg);
            std::process::exit(0);
        }
    }

    let mounts = setup_environment(req, new_root)?;

    chroot_jail(new_root)?;

    harden_process()?;

    Ok(mounts)
}
