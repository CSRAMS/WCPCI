use std::{collections::HashMap, path::PathBuf, process::Command};

use crate::error::prelude::*;

use serde::Deserialize;

use super::seccomp::{BpfConfig, SockFilter};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
pub struct CommandInfo {
    /// Binary to run
    pub binary: String,
    /// Arguments to pass to the binary
    #[serde(default)]
    args: Vec<String>,
}

impl CommandInfo {
    fn where_is(program: &str) -> Option<PathBuf> {
        let binary = PathBuf::from(program);
        if binary.is_absolute() {
            Some(binary)
        } else {
            let path_var = std::env::var("PATH").unwrap_or_default();
            let paths = std::env::split_paths(&path_var);
            paths
                .into_iter()
                .map(|p| p.join(&binary))
                .find(|p| p.exists())
                .and_then(|p| p.canonicalize().ok())
        }
    }

    pub fn resolve_binary(&self) -> Option<PathBuf> {
        Self::where_is(&self.binary)
    }

    pub fn make_command(&self) -> Command {
        let mut cmd = Command::new(self.binary.clone());
        cmd.args(&self.args);
        cmd
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
pub struct LanguageDisplayInfo {
    /// Name of the language
    pub name: String,
    #[serde(rename = "tablerIcon", alias = "tabler_icon")]
    /// Name of the icon for the language in [tabler icons](https://tabler.io/icons)
    pub tabler_icon: String,
    #[serde(rename = "monacoContribution", alias = "monaco_contribution")]
    /// Name of the monaco contribution for the language
    pub monaco_contribution: String,
    #[serde(rename = "defaultCode", alias = "default_code")]
    /// Default code to show in the editor
    pub default_code: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
pub struct LanguageRunnerInfo {
    #[serde(rename = "fileName", alias = "file_name")]
    /// Name of the file to save user submitted code to
    pub file_name: String,
    /// Command to compile the program.
    pub compile_cmd: Option<CommandInfo>,
    /// Command to run the program. This will be passed the case's input in stdin
    pub run_cmd: CommandInfo,
    /// Additional paths to expose to the runner
    #[serde(default)]
    additional_paths: Vec<String>,
    /// Additional binaries to include in the PATH
    #[serde(default)]
    include_bins: Vec<String>,
    /// Additional Environment variables to set
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
/// Specifies a configuration for a language.
pub struct LanguageConfig {
    pub display: LanguageDisplayInfo,
    pub runner: LanguageRunnerInfo,
}

const fn default_max_program_length() -> usize {
    100_000
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct RunConfig {
    /// Max program length in bytes (max and default is 100,000)
    #[serde(default = "default_max_program_length")]
    pub max_program_length: usize,
    /// Languages that are supported by the runner
    pub languages: HashMap<String, LanguageConfig>,
    /// Default language to use
    pub default_language: String,
    /// Settings for seccomp, syscall filtering
    #[serde(default)]
    pub seccomp: BpfConfig,
    #[serde(default)]
    pub expose_paths: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    // TODO: Make network configurable
}

impl RunConfig {
    pub fn get_languages_for_dropdown(&self) -> Vec<(&String, &String)> {
        let mut res = self
            .languages
            .iter()
            .map(|(k, l)| (k, &l.display.name))
            .collect::<Vec<_>>();
        res.sort_by(|a, b| a.1.cmp(b.1));
        res
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedRunData {
    pub compile_cmd: Option<CommandInfo>,
    pub run_cmd: CommandInfo,
    pub expose_paths: Vec<PathBuf>,
    pub seccomp_program: Vec<SockFilter>,
    pub environment: HashMap<String, String>,
    pub path_var: String,
    pub file_name: String,
}

impl ComputedRunData {
    pub fn compute(run_config: &RunConfig, lang: &LanguageRunnerInfo) -> Result<ComputedRunData> {
        let binaries = vec![
            lang.compile_cmd.as_ref().and_then(|c| c.resolve_binary()),
            lang.run_cmd.resolve_binary(),
            CommandInfo::where_is("env"), // TODO(Ellis): remove?
            CommandInfo::where_is("newuidmap"),
            CommandInfo::where_is("newgidmap"),
        ];

        let path_var = binaries
            .into_iter()
            .flatten()
            .map(|p| p.parent().unwrap().to_string_lossy().to_string())
            .chain(lang.include_bins.iter().filter_map(|b| {
                CommandInfo::where_is(b).map(|p| p.parent().unwrap().to_string_lossy().to_string())
            }))
            .collect::<Vec<_>>();

        let path_var = path_var.join(":");

        let mut expose_paths = run_config
            .expose_paths
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        expose_paths.extend(lang.additional_paths.iter().map(PathBuf::from));

        let env = run_config
            .env
            .iter()
            .chain(lang.env.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<_, _>>();

        let seccomp_program = super::seccomp::compile_filter(run_config)
            .context("Failed to setup seccomp program")?;

        Ok(ComputedRunData {
            compile_cmd: lang.compile_cmd.clone(),
            environment: env,
            run_cmd: lang.run_cmd.clone(),
            expose_paths,
            file_name: lang.file_name.clone(),
            seccomp_program,
            path_var,
        })
    }
}
