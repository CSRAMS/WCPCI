use rocket::{http::ContentType, State};
use serde_json::json;

use super::{colors::ParsedColorConfig, BrandingConfig};

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

pub struct SiteMetaInfo {
    web_manifest: String,
    browser_config: String,
    // For now robots.txt will be a compile-time constant as it never changes
    // In the future, this could be made dynamic
    // robots_txt: String,
}

fn make_browser_config(tile_color: &str) -> String {
    include_str!("browserconfig.xml").replace("TILE_COLOR", tile_color)
}

fn make_web_manifest(name: &str, short_name: &str, theme_color: &str) -> String {
    json!({
        "name": name,
        "short_name": short_name,
        "icons": [
            {
                "src": "/android-chrome-192x192.png",
                "sizes": "192x192",
                "type": "image/png"
            },
            {
                "src": "/android-chrome-512x512.png",
                "sizes": "512x512",
                "type": "image/png"
            }
        ],
        "theme_color": theme_color,
        "background_color": theme_color,
        "display": "standalone"
    })
    .to_string()
}

impl SiteMetaInfo {
    pub fn new(branding_config: &BrandingConfig, parsed_colors: &ParsedColorConfig) -> Self {
        let name = branding_config
            .meta
            .title
            .as_deref()
            .unwrap_or(branding_config.name.as_str());
        let short_name = branding_config.meta.short_name.as_deref().unwrap_or(name);

        Self {
            web_manifest: make_web_manifest(name, short_name, &parsed_colors.theme_color.0),
            browser_config: make_browser_config(&parsed_colors.primary.hex()),
        }
    }
}

#[get("/robots.txt")]
fn robots_txt() -> &'static str {
    include_str!("robots.txt")
}

#[get("/site.webmanifest")]
fn web_manifest(meta_info: &State<SiteMetaInfo>) -> (ContentType, &str) {
    (ContentType::JSON, &meta_info.web_manifest)
}

#[get("/browserconfig.xml")]
fn browser_config(meta_info: &State<SiteMetaInfo>) -> (ContentType, &str) {
    (ContentType::XML, &meta_info.browser_config)
}

pub fn stage() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("Site Meta Info", |rocket| async {
        rocket.mount("/", routes![robots_txt, web_manifest, browser_config])
    })
}
