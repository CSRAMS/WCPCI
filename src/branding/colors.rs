use std::str::FromStr;

use color_art::Color;

use crate::error::prelude::*;

macro_rules! default_fn {
    ($name: ident, $val: expr) => {
        fn $name() -> String {
            $val.to_string()
        }
    };
}

default_fn!(text, "#926798");
default_fn!(background, "#956a95");
default_fn!(primary, "#870099");
default_fn!(secondary, "#a253ac");
default_fn!(accent, "#e7b718");

/// Color configuration for the website
/// All colors are parsed similar to css colors, so you can use hex, rgb, rgba, hsl, hsla, etc.
/// Named colors are sourced from [The X11 color names](https://en.wikipedia.org/wiki/X11_color_names)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    #[serde(default = "primary")]
    /// Primary color of the website, used in CTA buttons and other important elements
    pub primary: String,
    #[serde(default = "secondary")]
    /// Secondary color of the website, used in other buttons and sometimes for background
    pub secondary: String,
    #[serde(default = "accent")]
    /// Accent color, used for links and small details
    pub accent: String,
    #[serde(default = "background")]
    /// Background color of the website
    pub background: String,
    #[serde(default = "text")]
    /// Text color of the website
    pub text: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            primary: primary(),
            secondary: secondary(),
            accent: accent(),
            background: background(),
            text: text(),
        }
    }
}

const COLOR_SCALE: [(u16, i8); 11] = [
    (50, 90),
    (100, 70),
    (200, 50),
    (300, 30),
    (400, 10),
    (500, 0),
    (600, -10),
    (700, -30),
    (800, -50),
    (900, -70),
    (950, -90),
];

fn lighten_or_darken(color: &Color, amount: f64) -> Color {
    assert!((-100.0..=100.0).contains(&amount));

    if amount == 0.0 {
        *color
    } else if amount > 0.0 {
        // We need amount needed to get to full light (1 - current light)
        // Then we want to scale it by how much we want to lighten
        let offset = (1.0 - color.lightness()) * (amount / 100.0);
        color.lighten(offset)
    } else {
        // ^^ but for darkening
        let offset = color.lightness() * (amount / 100.0 * -1.0);
        color.darken(offset)
    }
}

fn make_props(name: &str, color: &Color, mul: f64) -> Vec<String> {
    COLOR_SCALE
        .into_iter()
        .map(|(shade, light)| {
            // Hard-coded case because accent (unlike other colors)
            // doesn't have something to backdrop against in light mode
            // so we want it to be a bit darker
            let color = if mul == 1.0 && name == "accent" {
                color.darken(0.2)
            } else {
                *color
            };
            let color = lighten_or_darken(&color, (light as f64) * mul);
            format!("--{}-{}:{};", name, shade, color.hex())
        })
        .collect()
}

fn make_theme(colors: &[(&str, Color)], mul: f64) -> String {
    colors
        .iter()
        .flat_map(|(name, color)| make_props(name, color, mul))
        .collect()
}

const CSS_TEMPLATE: &str =
    ":root{@light}:root.dark{@dark}@media(prefers-color-scheme:dark){:root.system{@dark}}";

// To match --background-100, as the icon should look good against it
const THEME_COLOR_AMOUNT: f64 = 70.0;

impl ColorConfig {
    pub fn parse_colors(&self) -> Result<ParsedColorConfig> {
        let primary = Color::from_str(&self.primary).context("Failed to parse primary color")?;
        let secondary =
            Color::from_str(&self.secondary).context("Failed to parse secondary color")?;
        let accent = Color::from_str(&self.accent).context("Failed to parse accent color")?;
        let background =
            Color::from_str(&self.background).context("Failed to parse background color")?;
        let text = Color::from_str(&self.text).context("Failed to parse text color")?;
        let theme_color = (
            lighten_or_darken(&background, THEME_COLOR_AMOUNT).hex(),
            lighten_or_darken(&background, -THEME_COLOR_AMOUNT).hex(),
        );
        Ok(ParsedColorConfig {
            primary,
            secondary,
            accent,
            background,
            text,
            theme_color,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ParsedColorConfig {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub background: Color,
    pub text: Color,
    // Light, Dark
    pub theme_color: (String, String),
}

impl ParsedColorConfig {
    pub fn generate_theme_css(&self) -> String {
        let colors = [
            ("primary", self.primary),
            ("secondary", self.secondary),
            ("accent", self.accent),
            ("background", self.background),
            ("text", self.text),
        ];
        let light = make_theme(&colors, 1.0);
        let dark = make_theme(&colors, -1.0);
        CSS_TEMPLATE
            .replace("@light", &light)
            .replace("@dark", &dark)
    }
}
