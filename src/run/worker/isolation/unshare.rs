//! Module to separate process out into new namespaces and switch us into a new pid namespace.

use nix::{errno::Errno, sched::CloneFlags, sys::signal::Signal, unistd::ForkResult};

use crate::{error::prelude::*, run::worker::WorkerMessage};

fn setup_namespaces() -> Result {
    debug!("Setting up namespaces");
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

fn fork_to_child() -> Result {
    debug!("Forking to child process, output below may be a bit garbled");
    // Fork to new PID namespace as pid 1
    // Spoon says this *should* be safe as long as the parent isn't multithreaded at the time it calls `fork`
    unsafe {
        let res = nix::unistd::fork().context("Couldn't fork PID 1 in new PID namespace")?;

        if let ForkResult::Parent { child } = res {
            debug!("(Parent) Fork complete, sending child PID to service process");
            let res = WorkerMessage::RequestUidGidMap(child.as_raw())
                .send()
                .context("Couldn't send child PID to service process");
            // Manual error handling to ensure child cleanup
            if let Err(why) = res {
                error!("{:?}", why);
                debug!("Killing child process");
                let res = nix::sys::signal::kill(child, Signal::SIGKILL);
                match res {
                    Ok(_) | Err(Errno::ESRCH) => {}
                    Err(why) => error!("Couldn't kill child process: {:?}", why),
                }
            }
            nix::sys::wait::waitpid(child, None).context("Couldn't wait for child process")?;
            std::process::exit(0);
        } else {
            debug!("(Child) Fork complete, continuing");
        }
    }

    Ok(())
}

pub fn unshare() -> Result {
    setup_namespaces()?;
    fork_to_child()
}
