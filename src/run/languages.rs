use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
/// Specifies a configuration for a language.
pub struct LanguageConfig {
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
    #[serde(rename = "fileName", alias = "file_name")]
    /// Name of the file to save user submitted code to
    pub file_name: String,
    /// Command to compile the program.
    pub compile_cmd: String,
    /// Command to run the program. This will be passed the case's input in stdin
    pub run_cmd: String,
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
}

impl RunConfig {
    pub fn get_languages_for_dropdown(&self) -> Vec<(&String, &String)> {
        let mut res = self
            .languages
            .iter()
            .map(|(k, l)| (k, &l.name))
            .collect::<Vec<_>>();
        res.sort_by(|a, b| a.1.cmp(b.1));
        res
    }
}
