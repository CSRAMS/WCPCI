use std::{collections::HashMap, ops::Range, path::PathBuf};

use anyhow::bail;

use crate::{error::prelude::*, run::where_is};

use super::{
    cgroup,
    seccomp::{BpfConfig, SockFilter},
    CGroup,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct BindMountConfig {
    pub src: PathBuf,
    #[serde(default)]
    pub no_exec: bool,
}

fn default_tmpfs_size() -> String {
    "5%".to_string()
}

const fn default_hard_timeout_internal() -> u64 {
    2
}

const fn default_hard_timeout_user() -> u64 {
    30
}

const fn default_hard_memory_limit() -> u64 {
    1024 * 1024 * 350 // 350 MB
}

const fn default_nice() -> i32 {
    10
}

const fn default_shutdown_retry_interval() -> u64 {
    50
}

const fn default_shutdown_retries() -> u64 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct LimitConfig {
    #[serde(default = "default_tmpfs_size")]
    /// Same as `size` for `tmpfs`, taken directly from the man page:  
    /// > Specify an upper limit on the size of the filesystem.  
    /// > The size is given in bytes, and rounded up to entire pages.  The limit is re‐
    /// > moved if the size is 0.
    /// > The size may have a k, m, or g suffix for Ki, Mi, Gi (binary kilo (kibi),
    /// > binary mega (mebi), and binary giga (gibi)).
    /// > The size may also have a % suffix to limit this instance to a
    /// > percentage of physical RAM.
    pub tmpfs_size: String,
    #[serde(default = "default_hard_timeout_internal")]
    /// Timeout assigned to internal worker messages in *real time* seconds
    /// This is for anything in the runner *besides* the user's actual code
    /// Therefore this should be kept relatively low as the worker *should usually*
    /// do this stuff pretty fast and hangs indicate some internal issue
    /// Default: 2 seconds
    /// Set to 0 to not enforce a timeout, be warned this can lead to the worker
    /// potentially hanging forever.
    pub hard_timeout_internal_secs: u64,
    #[serde(default = "default_hard_timeout_user")]
    /// Timeout assigned to user code in *real time* seconds
    /// This timeout applies to *each test case individually* (and the compile step)
    /// So it can be kept relatively low as individual test cases should be fast
    /// This is for the actual code the user submits
    /// Note you should use this as an upper bound for the user code's runtime,
    /// a last resort to stop a user's code from running forever
    /// For a more graceful way to stop a user's code, use CPU time limits per-problem
    /// Default: 30 seconds this is a reasonable default for most problems
    /// But if you're hosting advanced problems, you may want to increase this
    /// Set to 0 to not enforce a timeout, be warned this can lead to users running
    /// code potentially forever.
    pub hard_timeout_user_secs: u64,
    #[serde(default = "default_hard_memory_limit")]
    /// Hard cap on the amount of memory the user's code can use in bytes
    /// Soft limits set by problem settings won't kill the process, but this will
    /// as a hard limit. This should be set above anything you plan to set as a soft limit
    /// Default: 350 MB
    pub hard_memory_limit_bytes: u64,
    #[serde(default = "default_nice")]
    /// The niceness delegated to the worker process
    /// This is a value between -20 and 19, with 19 being the lowest priority
    /// and -20 being the highest priority. This will determine CPU time allocation
    /// for the worker process.
    /// Default: 10
    pub nice: i32,
    #[serde(default = "default_shutdown_retry_interval")]
    /// Milliseconds to wait between trying to force kill the worker
    /// This should only apply when the worker gets to a state beyond saving
    /// and should be a relatively low value (<1 sec) as it may block the
    /// server thread
    /// Default: 50ms
    pub shutdown_retry_interval: u64,
    #[serde(default = "default_shutdown_retries")]
    /// The amount of times to try force killing the worker before giving up
    /// This should be a relatively low value (<5) as it may block the server thread
    /// Default: 4
    pub shutdown_retries: u64,
    /// Optional additional controllers to enable for the cgroup
    /// This is a list of controllers to enable for the cgroup
    /// Keep in mind `memory` and `cpu` are implicitly enabled as they're needed
    /// for the runner to function.
    /// The availability of these controllers is verified before launch
    ///
    /// Default: None
    pub additional_controllers: Option<Vec<String>>,
    /// Optional properties to write to the cgroup, meant to
    /// be used in tandem with `additional_controllers`.
    ///
    /// For example you may want to set `pids.max` for a cgroup,
    /// to do so you'd enable the `pids` controller with `additional_controllers`
    /// and set `pids.max` here.
    ///
    /// Note these are written directly to the cgroup, so
    /// support for these properties aren't checked before launch. Make sure to
    /// test and be careful.
    ///
    /// Also, these properties are applied *before* the compile step is run,
    /// this means they can't be used to set limits on only the user's code.
    /// The runner implicitly uses the following properties, if you set these here they
    /// have a chance of being overridden by the runner, don't depend on them:
    /// - `memory.high` (to problem limit)
    /// - `memory.max` (to hard limit in this config)
    /// - `cpu.weight.nice` (to nice value in this config)
    /// - `memory.oom.group` (to 1)
    ///
    /// Default: None
    pub additional_properties: Option<HashMap<String, String>>,
}

impl Default for LimitConfig {
    fn default() -> Self {
        Self {
            tmpfs_size: default_tmpfs_size(),
            hard_timeout_internal_secs: default_hard_timeout_internal(),
            hard_timeout_user_secs: default_hard_timeout_user(),
            hard_memory_limit_bytes: default_hard_memory_limit(),
            additional_controllers: None,
            additional_properties: None,
            nice: default_nice(),
            shutdown_retry_interval: default_shutdown_retry_interval(),
            shutdown_retries: default_shutdown_retries(),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct IsolationConfig {
    #[serde(default)]
    pub workers_parent: Option<PathBuf>,
    #[serde(default)]
    pub bind_mounts: Vec<BindMountConfig>,
    #[serde(default)]
    include_bins: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub override_subuid: Option<Range<u32>>,
    pub override_subgid: Option<Range<u32>>,
    #[serde(default)]
    seccomp: BpfConfig,
    #[serde(default)]
    pub compiled_seccomp_program: Option<Vec<SockFilter>>,
    #[serde(default)]
    pub limits: LimitConfig,
    #[serde(skip)]
    pub cgroups: Option<(CGroup, CGroup)>,
}

impl IsolationConfig {
    fn add_bins_to_path(&mut self) -> Result {
        let bin_paths = self
            .include_bins
            .iter()
            .filter_map(|s| where_is(s))
            .map(|p| p.parent().map(|pa| pa.to_path_buf()).unwrap())
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let path = std::env::join_paths(bin_paths).context("Couldn't join paths")?;
        let path = path.to_string_lossy();
        self.env
            .entry("PATH".to_string())
            .and_modify(|p| {
                p.push(':');
                p.push_str(&path);
            })
            .or_insert(path.to_string());
        Ok(())
    }

    fn compile_seccomp(&mut self) -> Result {
        let seccomp_program = super::seccomp::compile_filter(&self.seccomp)
            .context("Failed to setup seccomp program")?;
        self.compiled_seccomp_program = Some(seccomp_program);
        Ok(())
    }

    fn verify_tmpfs_limit(&self) -> Result {
        const PATTERN: &str = r"^\d+(?:\.\d+)?(?:k|m|g|%)?$";
        let re = regex::Regex::new(PATTERN).context("Couldn't compile regex")?;
        if !re.is_match(&self.limits.tmpfs_size) {
            bail!("Invalid tmpfs size: {}", self.limits.tmpfs_size);
        }
        Ok(())
    }

    pub async fn setup(&mut self, allow_cgroup_failure: bool) -> Result {
        if self.limits.nice < -20 || self.limits.nice > 19 {
            bail!(
                "Invalid nice value: {}, should be in range [-20, 19]",
                self.limits.nice
            );
        }
        match cgroup::setup_cgroups(&self.limits).await {
            Ok(cgroups) => {
                self.cgroups = Some(cgroups);
            }
            Err(why) => {
                if allow_cgroup_failure {
                    warn!("Couldn't setup cgroups: {:?}", why);
                    warn!("Because of debug mode, we will continue without cgroups");
                    warn!("This WILL MAKE RUNNERS NON-FUNCTIONAL");
                    warn!("In the production profile this will be an error");
                } else {
                    bail!("Couldn't setup cgroups: {}", why);
                }
            }
        }
        self.verify_tmpfs_limit()?;
        self.add_bins_to_path()
            .context("Couldn't resolve binaries")?;
        self.compile_seccomp()?;
        Ok(())
    }
}
