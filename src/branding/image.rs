// TODO: Redo this to convert to webp and not use `public`?
// could mean we can get rid of public entirely

use std::path::PathBuf;

use anyhow::bail;
use image::{imageops::FilterType, ImageFormat, ImageReader};
use rocket::{fairing::AdHoc, http::ContentType, State};

use crate::error::prelude::*;

use super::BrandingConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Path to the image
    path: PathBuf,
    /// Alt of this image, default changes based on context
    alt: Option<String>,
}

/// Config for images used around the site
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Logo used in the navbar
    /// Aspect ratio: 1:1
    /// Recommended size: 512x512
    /// Default Alt: `[brand name] Logo`
    /// Image may either be:
    ///   - PNG or WEBP: Either way they will be converted to WEBP and resized to the recommended size listed below
    ///   - SVG: Will be used as-is, with size set to the recommended size
    pub navbar_logo: Option<ImageInfo>,

    /// Image used in the hero section of the homepage
    /// Aspect ratio: 1:1
    /// Recommended size: 310x310
    /// Default Alt: `[brand name] Hero Image` *It's highly recommended to override this*
    pub hero_image: Option<ImageInfo>,

    /// Image used for Open Graph and Twitter cards, defaults to `hero_image`
    /// Aspect ratio: 1:1
    /// Recommended size: 310x310
    pub og_image: Option<ImageInfo>,
}

#[derive(Debug, Clone)]
pub enum LoadedImage {
    Raster(Vec<u8>),
    Svg(String),
}

impl LoadedImage {
    fn try_from_info(info: ImageInfo, resize: Option<(u32, u32)>, allow_svg: bool) -> Result<Self> {
        let reader = ImageReader::open(&info.path)
            .context("Failed to open image")?
            .with_guessed_format()
            .context("Failed to guess image format")?;
        if reader.format().is_none() {
            if !allow_svg {
                bail!("Failed to get image format");
            }
            let str_contents =
                std::fs::read_to_string(&info.path).context("Failed to read SVG file")?;
            Ok(Self::Svg(str_contents))
        } else {
            let img = reader.decode().context("Failed to decode image")?;
            let img = if let Some((w, h)) = resize {
                img.resize(w, h, FilterType::Lanczos3)
            } else {
                img
            };
            let mut buf = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut buf);
            img.write_to(&mut cursor, ImageFormat::WebP)
                .context("Failed to write image to buffer")?;
            Ok(Self::Raster(buf))
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedImages {
    navbar_logo: Option<LoadedImage>,
    hero_image: Option<Vec<u8>>,
    og_image: Option<Vec<u8>>,
}

impl LoadedImages {
    fn load_img(
        info: &Option<ImageInfo>,
        resize: Option<(u32, u32)>,
        allow_svg: bool,
    ) -> Result<Option<LoadedImage>> {
        info.as_ref()
            .map(|info| LoadedImage::try_from_info(info.clone(), resize, allow_svg))
            .transpose()
    }

    fn load_img_enforce_raster(
        info: &Option<ImageInfo>,
        resize: Option<(u32, u32)>,
    ) -> Result<Option<Vec<u8>>> {
        info.as_ref()
            .map(|info| LoadedImage::try_from_info(info.clone(), resize, false))
            .transpose()
            .map(|img| {
                img.and_then(|img| match img {
                    LoadedImage::Raster(data) => Some(data),
                    LoadedImage::Svg(_) => None,
                })
            })
    }

    pub fn try_from_config(brand_config: &BrandingConfig) -> Result<Self> {
        const NAVBAR_LOGO_SIZE: (u32, u32) = (512, 512);
        const HERO_IMAGE_SIZE: (u32, u32) = (310, 310);

        let navbar_logo = Self::load_img(
            &brand_config.images.navbar_logo,
            Some(NAVBAR_LOGO_SIZE),
            true,
        )?;
        let hero_image =
            Self::load_img_enforce_raster(&brand_config.images.hero_image, Some(HERO_IMAGE_SIZE))?;

        let og_image = if let Some(og_image) = &brand_config.images.og_image {
            Self::load_img_enforce_raster(&Some(og_image.clone()), Some(HERO_IMAGE_SIZE))?
        } else {
            hero_image.clone()
        };

        Ok(Self {
            navbar_logo,
            hero_image,
            og_image,
        })
    }
}

#[get("/navbar_logo")]
fn navbar_logo(images: &State<LoadedImages>) -> Option<(ContentType, &[u8])> {
    images.navbar_logo.as_ref().map(|img| match img {
        LoadedImage::Raster(data) => (ContentType::WEBP, data.as_slice()),
        LoadedImage::Svg(data) => (ContentType::SVG, data.as_bytes()),
    })
}

#[get("/hero_image.webp")]
fn hero_image(images: &State<LoadedImages>) -> Option<(ContentType, &[u8])> {
    images
        .hero_image
        .as_ref()
        .map(|data| (ContentType::WEBP, data.as_slice()))
}

#[get("/og_image.webp")]
fn og_image(images: &State<LoadedImages>) -> Option<(ContentType, &[u8])> {
    images
        .og_image
        .as_ref()
        .map(|data| (ContentType::WEBP, data.as_slice()))
}

pub fn stage(branding_config: &BrandingConfig) -> AdHoc {
    let branding_config = branding_config.clone();
    AdHoc::try_on_ignite("Load images", |rocket| async move {
        info!("Loading images from config");
        let loaded_images = LoadedImages::try_from_config(&branding_config);
        match loaded_images {
            Ok(images) => Ok(rocket
                .manage(images)
                .mount("/", routes![navbar_logo, hero_image, og_image])),
            Err(e) => {
                error!("Failed to load images: {:#}", e);
                Err(rocket)
            }
        }
    })
}
