fn default_keywords() -> String {
    "programming, competition, contest".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaConfig {
    /// Title of the website, shown in tabs and on embeds. Defaults to `branding.name`
    /// On the homepage:
    ///     This will be shown along with the tagline
    /// Other pages:
    ///     The title of the page will be shown followed by a `-` and then this
    title: Option<String>,
    /// Tagline to show on the home page in tabs
    /// This is appended to the title with a `-` separator
    tagline: Option<String>,
    /// Short name of the site, shown as Android app name. Defaults to `title`
    short_name: Option<String>,
    /// Description of the website, shown in embeds and search results
    /// This is used on pages that lack a description,
    /// certain pages such as contests and problems will override this with
    /// their own descriptions
    /// Defaults to `{branding.name}`
    description: Option<String>,
    #[serde(default = "default_keywords")]
    /// A comma-delimited list of keywords for the site, this is
    /// verbatim inserted into [the `keywords` meta tag](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/meta/name#standard_metadata_names_defined_in_the_html_specification)
    /// Defaults to `"programming, competition, contest"`
    keywords: String,
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            title: None,
            tagline: None,
            short_name: None,
            description: None,
            keywords: default_keywords(),
        }
    }
}
