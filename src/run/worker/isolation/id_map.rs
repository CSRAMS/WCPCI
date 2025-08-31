//! Module for mapping UIDs and GIDs for a process
//! Note: This must be run from the *service process*, no the worker
//! That is why it is exposed as part of the isolation module and not
//! bundled with the worker module

use std::ops::Range;

use anyhow::bail;
use tokio::process::Command;

use crate::{error::prelude::*, run::worker::ServiceMessage, wait_for_msg};

use super::{IsolationConfig, RUNNER_GID, RUNNER_UID};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SubIdRange(Range<u32>);

impl SubIdRange {
    pub fn new(range: Range<u32>) -> Self {
        Self(range)
    }

    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            bail!("Invalid range format");
        }
        let start = parts[0].parse().context("Invalid start")?;
        let count = parts[1].parse::<u32>().context("Invalid count")?;

        Ok(Self::new(Range {
            start,
            end: start + count,
        }))
    }

    pub fn parse_all(s: &str, filter_user: &str) -> Result<Vec<Self>> {
        s.lines()
            .filter_map(|line| {
                if line.trim().is_empty() || line.trim().starts_with('#') {
                    return None;
                }

                let (user, rest) = line.split_once(':')?;

                if user == filter_user {
                    Some(Self::parse(rest).context("Couldn't parse range"))
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn from_file(path: &str, filter_user: &str) -> Result<Vec<Self>> {
        let s = tokio::fs::read_to_string(path)
            .await
            .context("Couldn't read file")?;
        Self::parse_all(&s, filter_user).context("Couldn't parse file")
    }

    pub fn get_random_id(&self) -> u32 {
        use rand::Rng;
        let mut rng = rand::rng();
        rng.random_range(self.0.clone())
    }

    pub fn assert_enough(self, count: usize) -> Result<Self> {
        if self.0.len() < count {
            bail!("Not enough IDs in range");
        }
        Ok(self)
    }

    pub fn map_to(&self, inside: u32) -> IdMap {
        IdMap::new(inside, self.get_random_id(), 1)
    }
}

async fn get_range_for(path: &str, over: &Option<Range<u32>>) -> Result<SubIdRange> {
    const REQUIRED_RANGE_LEN: usize = 2;

    if let Some(over) = over {
        SubIdRange::new(over.clone()).assert_enough(REQUIRED_RANGE_LEN)
    } else {
        let username = std::env::var("USER").context("Couldn't get username")?;
        let ranges = SubIdRange::from_file(path, &username).await?;
        if ranges.is_empty() {
            bail!("No ranges found for current user ({username})");
        } else {
            ranges[0].clone().assert_enough(REQUIRED_RANGE_LEN)
        }
    }
}

/// Root user id map, Runner user id map
type MapPair = (IdMap, IdMap);
/// UID maps, GID maps
pub type MapInfo = (MapPair, MapPair);

pub async fn get_uid_gid_maps(iso: &IsolationConfig) -> Result<MapInfo> {
    let uid_range = get_range_for("/etc/subuid", &iso.override_subuid)
        .await
        .context("Couldn't get UID range")?;
    let gid_range = get_range_for("/etc/subgid", &iso.override_subgid)
        .await
        .context("Couldn't get GID range")?;
    Ok((
        (uid_range.map_to(0), uid_range.map_to(RUNNER_UID.as_raw())),
        (gid_range.map_to(0), gid_range.map_to(RUNNER_GID.as_raw())),
    ))
}

async fn map_ids_with_cmd(prog: &str, pid: i32, id_maps: &[IdMap]) -> Result {
    let mut cmd = Command::new(prog);
    cmd.arg(pid.to_string());
    for id_map in id_maps {
        id_map.chain_cmd(&mut cmd);
    }
    cmd.output()
        .await
        .context("Couldn't run new*idmap")
        .and_then(|output| {
            if !output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                bail!("{prog}: {stdout}\n{stderr}");
            }
            Ok(())
        })
}

pub async fn map_uid_gid(pid: i32, info: MapInfo) -> Result {
    // TODO(Ben): Make overrides just write the map directly??

    map_ids_with_cmd("newuidmap", pid, &[info.0 .0, info.0 .1])
        .await
        .context("Couldn't map UIDs")?;

    map_ids_with_cmd("newgidmap", pid, &[info.1 .0, info.1 .1])
        .await
        .context("Couldn't map GIDs")
}

/// Run in *worker* to wait for UID / GID mapping
pub fn wait_for_id_mapping() -> Result {
    debug!("Waiting for UID/GID mapping");
    wait_for_msg!(ServiceMessage::UidGidMapResult(a) => a)
        .context("Failed to get UID/GID mapping answer")
        .and_then(|b| {
            if b {
                Ok(())
            } else {
                bail!("UID/GID mapping failed")
            }
        })?;
    debug!("UID/GID mapping successful");
    let uid_map_file =
        std::fs::read_to_string("/proc/self/uid_map").context("Couldn't read UID map file")?;
    debug!("UID map file:\n{}", uid_map_file);
    let gid_map_file =
        std::fs::read_to_string("/proc/self/gid_map").context("Couldn't read GID map file")?;
    debug!("GID map file:\n{}", gid_map_file);
    Ok(())
}
