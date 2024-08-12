// TODO: Redo this to convert to webp and not use `public`?
// could mean we can get rid of public entirely

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Relative path to the image from the public directory (no leading `/` or `.`)
    public_path: String,
    /// Alt of this image, defaults to brand name if not provided
    alt: Option<String>,
    /// Width of the image, this will *not* be calculated automatically
    /// so make sure it is correct
    width: u32,
    /// Height of the image, this will *not* be calculated automatically
    /// so make sure it is correct
    height: u32,
}

fn default_navbar_logo() -> ImageInfo {
    ImageInfo {
        public_path: "navbar_logo.webp".to_string(),
        alt: None,
        width: 512,
        height: 512,
    }
}

fn default_hero_image() -> ImageInfo {
    ImageInfo {
        public_path: "hero_image.webp".to_string(),
        alt: None,
        width: 310,
        height: 310,
    }
}

/// Config for images used around the site
/// The actual image files must be placed in the public directory
/// (pointed to by `public_dir` in the config)
///
/// If these images are not present, it *will* return a 404
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Logo used in the navbar
    /// Aspect ratio: 1:1
    /// Recommended size: >= 512x512
    #[serde(default = "default_navbar_logo")]
    pub navbar_logo: ImageInfo,

    /// Image used in the hero section of the homepage
    /// Aspect ratio: 1:1
    /// Recommended size: >= 310x310
    #[serde(default = "default_hero_image")]
    pub hero_image: ImageInfo,

    /// Image used for Open Graph and Twitter cards, defaults to `hero_image`
    /// Size can be whatever, this isn't displayed in the actual site
    /// so it has no constraints for looking good
    pub og_image: Option<ImageInfo>,
}

impl Default for ImageConfig {
    fn default() -> Self {
        ImageConfig {
            navbar_logo: default_navbar_logo(),
            hero_image: default_hero_image(),
            og_image: None,
        }
    }
}
