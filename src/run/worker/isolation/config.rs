use std::{collections::HashMap, ops::Range, path::PathBuf};

use anyhow::bail;

use crate::{error::prelude::*, run::where_is};

use super::seccomp::{BpfConfig, SockFilter};

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
}

impl Default for LimitConfig {
    fn default() -> Self {
        Self {
            tmpfs_size: default_tmpfs_size(),
            hard_timeout_internal_secs: default_hard_timeout_internal(),
            hard_timeout_user_secs: default_hard_timeout_user(),
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

    pub fn setup(&mut self) -> Result {
        self.verify_tmpfs_limit()?;
        self.add_bins_to_path()
            .context("Couldn't resolve binaries")?;
        self.compile_seccomp()?;
        Ok(())
    }
}
