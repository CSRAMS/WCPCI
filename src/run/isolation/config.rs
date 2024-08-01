use std::{collections::HashMap, path::PathBuf};

use crate::{error::prelude::*, run::where_is};

use super::seccomp::{BpfConfig, SockFilter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct BindMountConfig {
    pub src: PathBuf,
    #[serde(default)]
    pub no_exec: bool,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct IsolationConfig {
    #[serde(default)]
    pub bind_mounts: Vec<BindMountConfig>,
    #[serde(default)]
    include_bins: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    seccomp: BpfConfig,
    #[serde(default)]
    pub compiled_seccomp_program: Option<Vec<SockFilter>>,
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

    pub fn setup(&mut self) -> Result {
        self.add_bins_to_path()
            .context("Couldn't resolve binaries")?;
        self.compile_seccomp()?;
        Ok(())
    }
}
