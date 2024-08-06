//! Modules controls cgroup creation for the container

use std::{fmt::Display, ops::Sub, os::unix::fs::MetadataExt, path::PathBuf};

use anyhow::bail;

use crate::error::prelude::*;

use super::LimitConfig;

#[derive(Debug, Clone)]
pub struct CGroup {
    path: PathBuf,
    pub ephemeral: bool,
}

impl CGroup {
    async fn write_prop<T: AsRef<str>>(&self, prop: &str, val: T) -> Result {
        tokio::fs::write(self.path.join(prop), val.as_ref())
            .await
            .with_context(|| format!("Couldn't write cgroup property ({prop})"))
    }

    fn write_prop_sync(&self, prop: &str, val: &str) -> Result {
        std::fs::write(self.path.join(prop), val)
            .with_context(|| format!("Couldn't write cgroup property ({prop})"))
    }

    async fn read_prop(&self, prop: &str) -> Result<String> {
        tokio::fs::read_to_string(self.path.join(prop))
            .await
            .with_context(|| format!("Couldn't read cgroup property ({prop})"))
    }

    fn get_stat_value(stat: &str, stat_prop: &str) -> Result<String> {
        stat.lines()
            .find(|l| l.starts_with(stat_prop))
            .with_context(|| format!("Couldn't find stat property ({stat_prop})"))?
            .split_whitespace()
            .last()
            .context("Couldn't get stat value")
            .map(|s| s.to_string())
    }

    async fn read_stat_value(&self, stat_file: &str, stat_prop: &str) -> Result<String> {
        let stat = self.read_prop(stat_file).await?;
        Self::get_stat_value(&stat, stat_prop).context("Couldn't get stat value")
    }

    pub async fn get_current() -> Result<Self> {
        // TODO: Make a config option? non-systemd systems might be weird
        // can check mountinfo for cgroup2 fs
        const CGROUP_ROOT: &str = "/sys/fs/cgroup";
        const PROC_SELF_CGROUP: &str = "/proc/self/cgroup";
        let grp_info = tokio::fs::read_to_string(PROC_SELF_CGROUP)
            .await
            .context("Couldn't read cgroup info")?;
        // /proc/self/cgroup is formatted something:something:path/to/cgroup
        // We want the last part, as we need to ensure we can write to it
        let path = grp_info
            .split(':')
            .nth(2)
            .context("Couldn't get cgroup info")?
            .trim_start_matches('/')
            .trim();
        let path = PathBuf::from(CGROUP_ROOT).join(path);
        Ok(Self {
            path,
            ephemeral: false,
        })
    }

    pub async fn verify_access(&self) -> Result {
        let meta = tokio::fs::metadata(&self.path)
            .await
            .context("Couldn't get cgroup metadata")?;
        let current_uid = nix::unistd::Uid::current();
        let owner_uid = nix::unistd::Uid::from_raw(meta.uid());
        let current_gid = nix::unistd::Gid::current();
        let owner_gid = nix::unistd::Gid::from_raw(meta.gid());
        if current_uid != owner_uid || current_gid != owner_gid {
            bail!(
                "cgroup path {} is not owned by the running user / group",
                self.path.display()
            );
        }
        Ok(())
    }

    pub async fn verify_controllers(&self, required_controllers: &[&str]) -> Result {
        let controllers = self.read_prop("cgroup.controllers").await?;
        let controllers = controllers.split_whitespace().collect::<Vec<_>>();
        for controller in required_controllers {
            if !controllers.contains(controller) {
                bail!("cgroup controller {} is not supported", controller);
            }
        }
        Ok(())
    }

    pub async fn create_child(&self, name: &str, ephemeral: bool) -> Result<Self> {
        let path = self.path.join(name);
        tokio::fs::create_dir(&path)
            .await
            .with_context(|| format!("Couldn't create cgroup at {}", path.display()))?;
        Ok(Self { path, ephemeral })
    }

    pub fn get_child(&self, name: &str, ephemeral: bool) -> Self {
        let path = self.path.join(name);
        Self { path, ephemeral }
    }

    pub async fn enable_subtree_control(&self, controllers: &[&str]) -> Result {
        let controllers = controllers
            .iter()
            .map(|s| format!("+{s}"))
            .collect::<Vec<_>>()
            .join(" ");
        self.write_prop("cgroup.subtree_control", controllers).await
    }

    pub async fn move_self(&self) -> Result {
        self.move_pid(std::process::id() as i32).await
    }

    pub async fn move_pid(&self, pid: i32) -> Result {
        self.write_prop("cgroup.procs", pid.to_string()).await
    }

    pub async fn apply_hard_limits(&self, lim: &LimitConfig) -> Result {
        // Set PID limit
        // TODO: Some systems (mine) don't have this, redo REQUIRED_CONTROLLERS
        // to allow for this
        // self.write_prop("pids.max", lim.pid_limit.to_string())
        //     .await?;

        // Set CPU niceness
        self.write_prop("cpu.weight.nice", lim.nice.to_string())
            .await?;

        // Set memory limit
        self.write_prop("memory.max", lim.hard_memory_limit_bytes.to_string())
            .await?;

        // Ensures when one process in the cgroup is OOM killed, all are
        self.write_prop("memory.oom.group", "1").await?;

        Ok(())
    }

    pub async fn apply_soft_limits(&self, _max_cpu: u64, max_mem: u64) -> Result {
        // Set memory limit
        self.write_prop("memory.high", max_mem.to_string()).await?;

        Ok(())
    }

    pub async fn get_stats(&self) -> Result<CGroupStats> {
        let high_memory_breaks = self.get_memory_high_event_count().await?;
        let cpu_usage_usec = self.read_stat_value("cpu.stat", "user_usec").await?;
        // 1 second = 1,000,000 microseconds
        Ok(CGroupStats {
            high_memory_breaks,
            cpu_usage_usec: cpu_usage_usec
                .trim()
                .parse()
                .context("Couldn't parse cpu usage")?,
        })
    }

    pub async fn get_memory_peak(&self) -> Result<u64> {
        let usage = self.read_prop("memory.peak").await?;
        usage.trim().parse().context("Couldn't parse memory usage")
    }

    pub async fn get_memory_high_event_count(&self) -> Result<u64> {
        let event_count = self.read_prop("memory.events").await?;
        let event_count =
            Self::get_stat_value(&event_count, "high").context("Couldn't get high event count")?;
        event_count
            .parse()
            .context("Couldn't parse high event count")
    }

    async fn rm_dir(&self) -> Result {
        tokio::fs::remove_dir(&self.path)
            .await
            .with_context(|| format!("Couldn't remove cgroup at {}", self.path.display()))
    }

    fn rm_dir_sync(&self) -> Result {
        std::fs::remove_dir(&self.path)
            .with_context(|| format!("Couldn't remove cgroup at {}", self.path.display()))
    }

    // TODO: config
    const SHUTDOWN_KILL_WAIT_MS: u64 = 50;
    const SHUTDOWN_GIVE_UP_COUNT: u64 = 4;

    pub async fn shutdown(&self) -> Result {
        if self.exists() {
            let mut times = 0;
            while self.rm_dir().await.is_err() {
                if times >= Self::SHUTDOWN_GIVE_UP_COUNT {
                    bail!("Couldn't remove cgroup at {}", self.path.display());
                }
                self.write_prop("cgroup.kill", "1").await?;
                tokio::time::sleep(std::time::Duration::from_millis(
                    Self::SHUTDOWN_KILL_WAIT_MS,
                ))
                .await;
                times += 1;
            }
        }
        Ok(())
    }

    pub fn shutdown_sync(&self) -> Result {
        if self.exists() {
            let mut times = 0;
            while self.rm_dir_sync().is_err() {
                if times >= Self::SHUTDOWN_GIVE_UP_COUNT {
                    bail!("Couldn't remove cgroup at {}", self.path.display());
                }
                self.write_prop_sync("cgroup.kill", "1")?;
                std::thread::sleep(std::time::Duration::from_millis(
                    Self::SHUTDOWN_KILL_WAIT_MS,
                ));
                times += 1;
            }
        }
        Ok(())
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    #[allow(dead_code)]
    #[cfg(debug_assertions)]
    pub async fn debug_list_all_props(&self) -> Result {
        let entries = std::fs::read_dir(&self.path).context("Couldn't read cgroup directory")?;
        for entry in entries {
            let entry = entry.context("Couldn't read entry")?;
            let path = entry.path();
            let name = path.file_name().context("Couldn't get file name")?;
            let name = name.to_string_lossy();
            let content = tokio::fs::read_to_string(&path)
                .await
                .context("Couldn't read file content")
                .map_err(|e| format!("[E] {e}"));
            info!("{}: {}", name, content.unwrap_or_else(|e| e));
        }
        Ok(())
    }
}

impl Drop for CGroup {
    fn drop(&mut self) {
        if self.ephemeral && self.exists() {
            let res = self.shutdown_sync();
            if let Err(e) = res {
                error!("{e:?}");
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CGroupStats {
    pub high_memory_breaks: u64,
    pub cpu_usage_usec: u64,
}

impl CGroupStats {
    pub fn check_broke_cpu_time(&self, limit: u64) -> bool {
        self.cpu_usage_usec >= limit
    }

    pub fn check_broke_memory_limit(&self) -> bool {
        self.high_memory_breaks > 0
    }
}

impl Sub for CGroupStats {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            high_memory_breaks: self.high_memory_breaks - rhs.high_memory_breaks,
            cpu_usage_usec: self.cpu_usage_usec - rhs.cpu_usage_usec,
        }
    }
}

impl Display for CGroupStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "High breaks: {}, CPU Used: {} microseconds",
            self.high_memory_breaks, self.cpu_usage_usec
        )
    }
}

pub async fn setup_cgroups() -> Result<(CGroup, CGroup)> {
    // May expand in the future / add config
    const REQUIRED_CONTROLLERS: [&str; 2] = ["memory", "cpu"];
    const SERVICE_CGROUP_NAME: &str = "wcpc_service";

    let root_group = CGroup::get_current().await?;
    info!("Using cgroup at {} as root", root_group.path().display());
    root_group
        .verify_access()
        .await
        .context("Couldn't verify cgroup access")?;
    root_group
        .verify_controllers(&REQUIRED_CONTROLLERS)
        .await
        .context("Couldn't verify cgroup controllers")?;

    let current_child = root_group.get_child(SERVICE_CGROUP_NAME, false);
    if current_child.exists() {
        warn!("Found existing cgroup {SERVICE_CGROUP_NAME}, clearing it out");
        current_child.shutdown().await?;
    }

    let new_group = root_group.create_child(SERVICE_CGROUP_NAME, false).await?;
    info!("Created new cgroup at {}", new_group.path().display());
    new_group.move_self().await?;
    root_group
        .enable_subtree_control(&REQUIRED_CONTROLLERS)
        .await?;
    Ok((root_group, new_group))
}
