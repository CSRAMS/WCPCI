use colors::ColorConfig;
use image::ImageConfig;
use meta::MetaConfig;

mod colors;
mod icon;
pub mod image;
mod meta;

pub use meta::SiteMetaInfo;
use rocket::fairing::AdHoc;

// TODO:
// - [x] Setup passing in template.rs, make a new function
// - [x] Inserting colors into Layout.astro
// - [x] Edit frontend to actually use customizations
// - [x] Images (could still be better)
// - [x] Favicons
// - [x] Make browserconfig.xml and friends use this config (new routes?)
// - [x] Monaco editor theme based on colors automatically
// - [x] Highlight.js theme based on colors automatically

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum FooterItem {
    Text {
        text: String,
    },
    Link {
        text: String,
        url: String,
        #[serde(default)]
        new_tab: bool,
    },
}

fn default_name() -> String {
    "OxideJudge".to_string()
}

fn default_sso_name() -> String {
    "SSO".to_string()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct HomepageConfig {
    /// Text to show in the heading of the homepage
    /// Defaults to `{branding.name} - {branding.tagline}`
    heading_text: Option<String>,
    /// Body text to show on the homepage, if omitted no body text will be shown
    body_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandingConfig {
    /// Name of the website, used as default for various fields
    #[serde(default = "default_name")]
    pub name: String,
    /// Path to use for the favicon, this *must* be a PNG file
    /// Ideally, this should be at least a 512x512 image, as that's the largest size used
    icon_path: Option<String>,
    #[serde(default)]
    /// Information about the website to be used in meta tags / files
    meta: MetaConfig,
    #[serde(default)]
    /// Colors to use for the website
    pub colors: ColorConfig,
    #[serde(default)]
    pub images: ImageConfig,
    /// Text to show next to logo in the navbar, defaults to `name`
    navbar_brand_text: Option<String>,
    #[serde(default)]
    /// Configuration for the homepage
    homepage: HomepageConfig,
    #[serde(default = "default_sso_name")]
    /// Text to show in the login and register via SSO buttons, defaults to `SSO`
    /// This will be prepended with `Login with ` and `Register with ` respectively
    sso_name: String,
    #[serde(default)]
    /// Items to show in the footer, ordered from left to right
    footer_items: Vec<FooterItem>,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            meta: MetaConfig::default(),
            colors: ColorConfig::default(),
            icon_path: None,
            images: ImageConfig::default(),
            homepage: HomepageConfig::default(),
            navbar_brand_text: None,
            sso_name: default_sso_name(),
            footer_items: Vec::new(),
        }
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Branding Setup", |rocket| async {
        rocket.attach(icon::stage()).attach(meta::stage())
    })
}
