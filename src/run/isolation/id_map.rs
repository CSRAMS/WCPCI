//! Module for mapping UIDs and GIDs for a process
//! Note: This must be run from the *service process*, no the worker
//! That is why it is exposed as part of the isolation module and not
//! bundled with the worker module

use anyhow::bail;
use tokio::process::Command;

use crate::{error::prelude::*, run::worker::get_stdin_answer};

use super::{RUNNER_GID, RUNNER_UID};

pub struct IdMap {
    inside_start: u32,
    outside_start: u32,
    count: u32,
}

impl IdMap {
    pub fn new(inside_start: u32, outside_start: u32, count: u32) -> Self {
        Self {
            inside_start,
            outside_start,
            count,
        }
    }

    fn chain_cmd<'a>(&self, cmd: &'a mut Command) -> &'a mut Command {
        cmd.arg(self.inside_start.to_string())
            .arg(self.outside_start.to_string())
            .arg(self.count.to_string())
    }

    #[allow(dead_code)]
    pub fn to_raw(&self) -> String {
        format!(
            "{}\t{}\t{}",
            self.inside_start, self.outside_start, self.count
        )
    }
}

async fn map_ids_with_cmd(prog: &str, pid: i32, id_maps: &[IdMap]) -> Result {
    let mut cmd = Command::new(prog);
    cmd.arg(pid.to_string());
    for id_map in id_maps {
        id_map.chain_cmd(&mut cmd);
    }
    cmd.output()
        .await
        .context("Couldn't run newgidmap")
        .and_then(|output| {
            if !output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                bail!("{prog}: {stdout}\n{stderr}");
            }
            Ok(())
        })
}

pub async fn map_uid_gid(pid: i32) -> Result {
    // TODO(Ben): Read and parse /etc/subuid and /etc/subgid to know what to pass to newuidmap and newgidmap
    // TODO(Ben): Make configurable, allow overriding /etc/subuid and /etc/subgid
    // TODO(Ben): Pick a random UID and GID in the provided range to map to

    // Temp hardcoded for now
    let uid_m_1 = IdMap::new(0, 1000, 1);
    let uid_m_2 = IdMap::new(RUNNER_UID.as_raw(), 100000, 1);

    // Temp hardcoded for now
    let gid_m_1 = IdMap::new(0, 100, 1);
    let gid_m_2 = IdMap::new(RUNNER_GID.as_raw(), 100000, 1);

    map_ids_with_cmd("newuidmap", pid, &[uid_m_1, uid_m_2]).await?;
    map_ids_with_cmd("newgidmap", pid, &[gid_m_1, gid_m_2]).await?;

    Ok(())
}

/// Run in *worker* to wait for UID / GID mapping
pub fn wait_for_id_mapping() -> Result {
    debug!("Waiting for UID/GID mapping");
    get_stdin_answer()
        .context("Failed to get UID/GID mapping answer")
        .and_then(|b| {
            if b {
                Ok(())
            } else {
                bail!("UID/GID mapping failed")
            }
        })?;
    let uid = nix::unistd::getuid();
    let gid = nix::unistd::getgid();
    debug!("Done; UID: {}, GID: {}", uid, gid);
    Ok(())
}
