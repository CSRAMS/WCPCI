use std::{
    collections::HashMap,
    process::{Command, Stdio},
};

use crate::error::prelude::*;

use serde::Deserialize;

use super::worker::IsolationConfig;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
pub struct CommandInfo {
    /// Binary to run
    pub binary: String,
    /// Arguments to pass to the binary
    #[serde(default)]
    pub args: Vec<String>,
}

impl CommandInfo {
    pub fn resolve_binary(&mut self) -> Result {
        let new_bin = super::where_is(&self.binary).map(|p| p.to_string_lossy().to_string());

        if let Some(bin) = new_bin {
            self.binary = bin;
        }
        Ok(())
    }

    pub fn setup(&mut self) -> Result {
        self.resolve_binary().context("Couldn't resolve binary")?;
        Ok(())
    }

    pub fn make_command(&self) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.args(&self.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
pub struct LanguageDisplayInfo {
    /// Name of the language
    pub name: String,
    #[serde(rename = "deviconIcon", alias = "devicon_icon")]
    /// Override name of the icon for the language in [devicon icons](https://devicon.dev/)
    /// Uses the language's key as the icon name if not provided
    pub devicon_icon: Option<String>,
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
    #[serde(default)]
    pub env: HashMap<String, String>,
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
    #[serde(default)]
    pub isolation: IsolationConfig,
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
