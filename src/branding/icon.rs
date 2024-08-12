// Icon gen for branding

use std::{collections::HashMap, io::Cursor, path::Path};

use anyhow::bail;
use image::{
    codecs::ico::{IcoEncoder, IcoFrame},
    imageops::FilterType,
    DynamicImage, ExtendedColorType, GenericImageView, ImageFormat, ImageReader,
};
use log::warn;
use rocket::{
    fairing::AdHoc,
    figment::Figment,
    http::{ContentType, Status},
    outcome::{try_outcome, Outcome},
    request::{self, FromRequest},
    Request, State,
};

use crate::error::prelude::*;

type RawAtlasKey = u32;
type RawAtlasValue = Vec<u8>;

#[derive(Debug, Default, Clone)]
struct RawIconAtlas {
    entries: HashMap<RawAtlasKey, RawAtlasValue>,
}

const NEEDED_SIZES: [RawAtlasKey; 21] = [
    16, 32, 36, 48, 57, 60, 70, 72, 76, 96, 114, 120, 144, 150, 152, 180, 192, 256, 310, 384, 512,
];

const MAX_NEEDED_SIZE: RawAtlasKey = 512;

impl RawIconAtlas {
    fn make_for_size(img: &DynamicImage, size: RawAtlasKey) -> Result<RawAtlasValue> {
        let new_img = img.resize(size, size, FilterType::Lanczos3);
        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);
        new_img
            .write_to(&mut cursor, ImageFormat::Png)
            .context("Failed to write image to buffer")?;
        Ok(buf)
    }

    fn check_size(img: &DynamicImage) -> Result {
        let (width, height) = img.dimensions();
        if width != height {
            bail!("aspect ratio of image should be 1:1");
        } else if width < MAX_NEEDED_SIZE {
            bail!("image should be at least {MAX_NEEDED_SIZE}x{MAX_NEEDED_SIZE}");
        }
        Ok(())
    }

    pub fn from_file_path(path: &Path) -> Result<Self> {
        let img = {
            let reader = ImageReader::open(path).context("Failed to open image")?;
            reader.decode().context("Failed to decode image")
        }?;

        Self::try_from(img)
    }

    pub fn get_icon(&self, size: RawAtlasKey) -> Option<&RawAtlasValue> {
        self.entries.get(&size)
    }

    pub fn get_icon_ok(&self, size: RawAtlasKey) -> Result<&RawAtlasValue> {
        self.get_icon(size)
            .with_context(|| format!("Failed to get icon with size {size}x{size}"))
    }
}

impl TryFrom<DynamicImage> for RawIconAtlas {
    type Error = anyhow::Error;

    fn try_from(img: DynamicImage) -> Result<Self> {
        if let Err(warning) = Self::check_size(&img) {
            warn!(
                "Favicon image is not ideal, {}. Continuing anyway...",
                warning
            );
        }

        let entries = NEEDED_SIZES
            .iter()
            .map(|&size| {
                let buf =
                    Self::make_for_size(&img, size).context("Failed to make icon for size")?;
                Ok((size, buf))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        Ok(Self { entries })
    }
}

#[derive(Debug, Default, Clone)]
struct FaviconData {
    png_atlas: RawIconAtlas,
    ico_file: Option<Vec<u8>>,
}

const ICO_SIZES: [u32; 3] = [16, 32, 48];

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum AtlasEntry {
    Ico,
    Png(u32),
}

const ATLAS_ENTRIES: [(&str, AtlasEntry); 25] = [
    // Base Favicons
    ("favicon.ico", AtlasEntry::Ico),
    ("favicon-16x16.png", AtlasEntry::Png(16)),
    ("favicon-32x32.png", AtlasEntry::Png(32)),
    // Android Chrome
    ("android-chrome-36x36.png", AtlasEntry::Png(36)),
    ("android-chrome-48x48.png", AtlasEntry::Png(48)),
    ("android-chrome-72x72.png", AtlasEntry::Png(72)),
    ("android-chrome-96x96.png", AtlasEntry::Png(96)),
    ("android-chrome-144x144.png", AtlasEntry::Png(144)),
    ("android-chrome-192x192.png", AtlasEntry::Png(192)),
    ("android-chrome-256x256.png", AtlasEntry::Png(256)),
    ("android-chrome-384x384.png", AtlasEntry::Png(384)),
    ("android-chrome-512x512.png", AtlasEntry::Png(512)),
    // Windows Start Menu Tiles
    ("mstile-70x70.png", AtlasEntry::Png(70)),
    ("mstile-144x144.png", AtlasEntry::Png(144)),
    ("mstile-150x150.png", AtlasEntry::Png(150)),
    ("mstile-310x310.png", AtlasEntry::Png(310)),
    // Apple Touch Icons
    ("apple-touch-icon-57x57.png", AtlasEntry::Png(57)),
    ("apple-touch-icon-60x60.png", AtlasEntry::Png(60)),
    ("apple-touch-icon-72x72.png", AtlasEntry::Png(72)),
    ("apple-touch-icon-76x76.png", AtlasEntry::Png(76)),
    ("apple-touch-icon-120x120.png", AtlasEntry::Png(120)),
    ("apple-touch-icon-144x144.png", AtlasEntry::Png(144)),
    ("apple-touch-icon-152x152.png", AtlasEntry::Png(152)),
    ("apple-touch-icon-180x180.png", AtlasEntry::Png(180)),
    ("apple-touch-icon.png", AtlasEntry::Png(180)),
];

impl FaviconData {
    pub fn from_file_path(path: &Path) -> Result<Self> {
        let atlas = RawIconAtlas::from_file_path(path)?;

        let ico_file = {
            let mut buf = Vec::new();
            let mut cursor = Cursor::new(&mut buf);
            let encoder = IcoEncoder::new(&mut cursor);
            let frames = ICO_SIZES
                .into_iter()
                .map(|size| {
                    let buf = atlas.get_icon_ok(size)?;
                    IcoFrame::with_encoded(buf, size, size, ExtendedColorType::Rgba8)
                        .context("Failed to make ICO frame")
                })
                .collect::<Result<Vec<_>>>()?;
            encoder
                .encode_images(&frames)
                .context("Failed to encode images into ICO")?;
            buf
        };

        Ok(Self {
            png_atlas: atlas,
            ico_file: Some(ico_file),
        })
    }
}

#[derive(Debug, Default, Clone)]
struct IconAtlas {
    data: FaviconData,
    entries: HashMap<String, AtlasEntry>,
}

impl IconAtlas {
    pub fn new(data: FaviconData) -> Self {
        let entries = ATLAS_ENTRIES
            .into_iter()
            .map(|(name, entry)| (name.to_string(), entry))
            .collect::<HashMap<_, _>>();

        Self { data, entries }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r AtlasEntry {
    type Error = anyhow::Error;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let atlas: &State<IconAtlas> =
            try_outcome!(request.guard::<&State<IconAtlas>>().await.map_error(|_| {
                (
                    Status::InternalServerError,
                    anyhow!("Failed to get icon atlas"),
                )
            }));

        let icon = request.param::<&str>(0).and_then(|r| r.ok());

        if let Some(icon) = icon {
            let entry = atlas.entries.get(icon);
            if let Some(entry) = entry {
                Outcome::Success(entry)
            } else {
                Outcome::Forward(Status::NotFound)
            }
        } else {
            Outcome::Forward(Status::NotFound)
        }
    }
}

#[get("/<icon>")]
fn get_icon<'a>(
    atlas: &'a State<IconAtlas>,
    atlas_icon: &'a AtlasEntry,
    icon: &str,
) -> ResultResponse<(ContentType, &'a [u8])> {
    // Just getting rid of the unused parameter warning, compiles to a nop
    #[allow(dropping_references)]
    drop(icon);

    Ok(match atlas_icon {
        AtlasEntry::Ico => {
            let data = atlas
                .data
                .ico_file
                .as_ref()
                .ok_or::<ResponseErr>(Status::NotFound.into())?;
            (ContentType::Icon, data.as_slice())
        }
        AtlasEntry::Png(size) => {
            let data = atlas
                .data
                .png_atlas
                .get_icon(*size)
                .ok_or::<ResponseErr>(Status::NotFound.into())?;
            (ContentType::PNG, data.as_slice())
        }
    })
}

fn setup(figment: &Figment) -> Result<IconAtlas> {
    let res = figment.extract_inner::<String>("branding.icon_path");

    if let Ok(path) = res {
        let path = Path::new(&path);
        info!("Setting up favicon from path: {}", path.display());
        let data = FaviconData::from_file_path(path)?;
        Ok(IconAtlas::new(data))
    } else {
        warn!("No favicon path provided, skipping favicon setup");
        Ok(IconAtlas::default())
    }
}

pub fn stage() -> AdHoc {
    AdHoc::try_on_ignite("Favicon Setup", move |rocket| async {
        let figment = rocket.figment();
        match setup(figment) {
            Ok(atlas) => Ok(rocket.manage(atlas).mount("/", routes![get_icon])),
            Err(e) => {
                error!("{e:?}");
                Err(rocket)
            }
        }
    })
}
