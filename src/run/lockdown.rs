use std::{
    fs::Permissions,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::bail;
use log::debug;
use nix::{
    mount::MsFlags,
    sched::CloneFlags,
    unistd::{ForkResult, Gid, Uid},
};

use crate::{error::prelude::*, run::WorkerMessage};

use super::{job::JobRequest, seccomp::SockFilter};

fn bind_mount(root: &Path, path: &Path, no_exec: bool) -> Result {
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

fn setup_namespaces() -> Result {
    nix::sched::unshare(
        CloneFlags::CLONE_NEWUSER
            | CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWNET
            | CloneFlags::CLONE_NEWIPC
            | CloneFlags::CLONE_NEWCGROUP
            | CloneFlags::CLONE_NEWUTS,
    )
    .context("Couldn't create new namespace(s)")
}

pub fn map_ids(pid: i32) -> Result {
    // TODO: Make this parse /etc/subuid and /etc/subgid
    // and optionally allow hard coding in config
    // Also, don't go to active user, go to a sub-uid
    // Also also, randomize which uid we map to on the outside

    Command::new("newuidmap")
        .arg(pid.to_string())
        .arg("0")
        .arg("1000")
        .arg("1")
        .arg("1")
        .arg("100000")
        .arg("1")
        .output()
        .context("Couldn't run newuidmap")
        .and_then(|output| {
            if !output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                bail!("uid: {stdout}\n{stderr}");
            }
            Ok(())
        })?;

    Command::new("newgidmap")
        .arg(pid.to_string())
        .arg("0")
        .arg("100")
        .arg("1")
        .arg("1")
        .arg("100000")
        .arg("1")
        .output()
        .context("Couldn't run newgidmap")
        .and_then(|output| {
            if !output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                bail!("gid: {stdout}\n{stderr}");
            }
            Ok(())
        })?;

    // let new_uid_map = std::fs::read_to_string(format!("/proc/{pid}/uid_map"))
    //     .context("Couldn't read uid map")?;

    // info!("UID Map: {new_uid_map}");

    // let new_gid_map = std::fs::read_to_string(format!("/proc/{pid}/gid_map"))
    //     .context("Couldn't read gid map")?;

    // info!("GID Map: {new_gid_map}");

    // Setup uid mappings
    // let uid_map = "0 1000 1\n1 100000 1";
    // std::fs::write(format!("/proc/{pid}/uid_map"), uid_map).context("Couldn't write uid map")?;

    // // Deny setgroups
    // std::fs::write(format!("/proc/{pid}/setgroups"), "deny").context("Couldn't write setgroups")?;

    // Setup gid mappings
    // let gid_map = "0 100 1\n1 100000 1";
    // std::fs::write(format!("/proc/{pid}/gid_map"), gid_map).context("Couldn't write gid map")?;

    Ok(())
}

// TODO: make these configurable, ideally per-language

// TODO: mknod instead of bind mount (Ben: I don't think we can?)
// Docker docs:
// While the root user inside a user-namespaced container process has many of the expected
// privileges of the superuser within the container, the Linux kernel imposes restrictions
// based on internal knowledge that this is a user-namespaced process.
// One notable restriction is the inability to use the mknod command.
// Permission is denied for device creation within the container when run by the root user.
const DEV_BINDS: [&str; 4] = ["/dev/null", "/dev/zero", "/dev/random", "/dev/urandom"];

const DEV_LINKS: [(&str, &str); 4] = [
    ("dev/stdin", "/proc/self/fd/0"),
    ("dev/stdout", "/proc/self/fd/1"),
    ("dev/stderr", "/proc/self/fd/2"),
    ("dev/fd", "/proc/self/fd/"),
];

fn setup_environment(req: &JobRequest, new_root: &Path) -> Result {
    // TODO(Ellis mostly): make all mounts & created directories configurable? (& symlinks)

    // Mount root on a tmpfs

    // TODO: Configure NOEXEC here in config
    let cwd = std::env::current_dir().context("Couldn't get current directory")?;
    nix::mount::mount(
        None::<&str>,
        &cwd,
        Some("tmpfs"),
        MsFlags::MS_NODEV | MsFlags::MS_NOSUID,
        Some("mode=0755"),
    )
    .context("Couldn't mount tmpfs")?;

    // Bind mount the expose paths needed for the language to run

    for path in &req.language.expose_paths {
        bind_mount(new_root, path, false).with_context(|| {
            format!(
                "Couldn't bind mount expose path ({})",
                path.to_string_lossy()
            )
        })?;
    }

    // Bind mount the /dev paths for common devices

    for path in &DEV_BINDS {
        let dev_path = PathBuf::from(path);
        bind_mount(new_root, &dev_path, true).with_context(|| {
            format!(
                "Couldn't bind mount dev path ({})",
                dev_path.to_string_lossy()
            )
        })?;
    }

    // Mount the /proc filesystem

    let proc_dir = new_root.join("proc");

    std::fs::create_dir_all(&proc_dir).context("Couldn't create /proc directory")?;

    // TODO(Ellis): do we want to use hidepid={1,2}
    nix::mount::mount(
        None::<&str>,
        &proc_dir,
        Some("proc"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        None::<&str>,
    )
    .context("Couldn't mount /proc")?;

    // Create symlinks for some fd paths
    for (link, target) in &DEV_LINKS {
        let link_path = new_root.join(link);
        let target_path = PathBuf::from(target);

        debug!(
            "Creating symlink {} -> \"{}\"",
            link_path.display(),
            target_path.display()
        );

        std::os::unix::fs::symlink(target_path, link_path).context("Couldn't create symlink")?;
    }

    debug!("Creating /tmp directory in new root");

    // Sticky, read/write/execute for all
    const TEMP_FOLDER_PERMS: u32 = 0o1777;

    let tmp_path = new_root.join("tmp");
    std::fs::create_dir_all(&tmp_path).context("Couldn't create /tmp directory in new root")?;
    std::fs::set_permissions(&tmp_path, Permissions::from_mode(TEMP_FOLDER_PERMS))
        .context("Couldn't set permissions on /tmp directory")?;

    debug!("Creating /dev/shm directory in new root");

    let shm_path = new_root.join("dev/shm");
    std::fs::create_dir_all(&shm_path).context("Couldn't create /dev/shm directory in new root")?;
    std::fs::set_permissions(&shm_path, Permissions::from_mode(TEMP_FOLDER_PERMS))
        .context("Couldn't set permissions on /dev/shm directory")
}

fn chroot_jail(new_root: &Path) -> Result {
    // cd and chroot to the new root directory
    std::env::set_current_dir(new_root).context("Couldn't set current directory to new root")?;
    nix::unistd::chroot(new_root).context("Couldn't chroot to new root")?;
    // Extra chdir to make sure PWD is 100% correct
    std::env::set_current_dir("/").context("Couldn't set current directory to /")
}

const HOME_DIR: &str = "/home/runner";

fn setup_user() -> Result {
    // Create a working directory for the runner
    std::fs::create_dir_all(HOME_DIR).context("Couldn't create runner directory")?;

    // Chown /runner to runner
    nix::unistd::chown(HOME_DIR, Some(Uid::from_raw(1)), Some(Gid::from_raw(1)))
        .context("Couldn't chown runner directory")?;

    let uid = Uid::from_raw(1);
    let gid = Gid::from_raw(1);

    // Ensure our capabilities aren't kept when we become uid 1
    nix::sys::prctl::set_keepcaps(false).context("Couldn't set keepcaps to false")?;

    // Set all our gids
    nix::unistd::setresgid(gid, gid, gid).context("Couldn't setresgid")?;

    // Set groups
    nix::unistd::setgroups(&[gid]).context("Couldn't setgroups")?;

    let gid = nix::unistd::getgid();
    debug!("GID is now {gid}");

    // Set all our uids
    nix::unistd::setresuid(uid, uid, uid).context("Couldn't setresuid")?;

    let uid = nix::unistd::getuid();
    debug!("UID is now {uid}");

    // Setup some environment variables for the runner
    std::env::set_var("HOME", HOME_DIR);
    std::env::set_var("USER", "runner");
    std::env::set_current_dir(HOME_DIR)
        .context("Couldn't set current directory to runner directory")?;

    Ok(())
}

fn harden_process(bpf_filter: &[SockFilter]) -> Result {
    // Set dumpable to false
    // Set no new privs
    // TODO: Set secure bits
    nix::sys::prctl::set_dumpable(false).context("Couldn't set dumpable to false")?;
    nix::sys::prctl::set_no_new_privs().context("Couldn't set no new privs")?; // This should probably be configurable with the same toggle as clearing bounding cap set

    // PTRACE_MODE_READ_FSCREDS (seems to be default for same-user)

    // Setup seccomp syscall filtering
    let bpf_filter = bpf_filter
        .iter()
        .map(|s| s.clone().into())
        .collect::<Vec<_>>();
    seccompiler::apply_filter(&bpf_filter).context("Couldn't apply seccomp filter")?;

    Ok(())
}

/// Run to lockdown the running process
/// This should *only be run in a worker process*
pub fn lockdown_process(req: &JobRequest, new_root: &Path) -> Result {
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

    debug!("Child process started, awaiting UID / GID Mapping...");

    // Wait for input on stdin to confirm UID / GID mapping has completed
    let mut buffer = String::new();
    std::io::stdin()
        .read_line(&mut buffer)
        .context("Couldn't read from stdin")?;

    if buffer.trim().to_lowercase() != "y" {
        bail!("Exiting due to parent failing UID / GID mapping");
    }

    let my_uid = nix::unistd::getuid();
    let my_gid = nix::unistd::getgid();

    debug!("Done. My UID: {my_uid}, My GID: {my_gid}");

    // Setup environment
    setup_environment(req, new_root)?;

    // Chroot into our newly setup environment
    chroot_jail(new_root)?;

    // Create new user, set uid and gid to new user
    setup_user()?;

    // Drop capabilities, setup seccomp, etc
    harden_process(&req.language.seccomp_program)?;

    Ok(())
}
// TODO(Ellis): see why tree and eza don't exit
