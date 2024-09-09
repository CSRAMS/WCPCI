use std::collections::HashMap;

use anyhow::Context;
use markdown::{CompileOptions, Constructs, Options, ParseOptions};
use openssl::{base64, sha::sha256};
use rocket::{fairing::AdHoc, form::Context as FormContext, http::Status};
use rocket_dyn_templates::Template;
use tera::Value;

use crate::{
    branding::{self, BrandingConfig, SiteMetaInfo},
    error::prelude::*,
};

type FunctionArgs<'a> = &'a HashMap<String, Value>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormStatus {
    Success,
    Error,
    None,
}

pub trait TemplatedForm {
    fn get_defaults(&mut self) -> HashMap<String, String>;
}

#[derive(Debug, Serialize)]
pub struct FormTemplateObject {
    data: HashMap<String, String>,
    errors: HashMap<String, Vec<String>>,
    fields: Vec<String>,
    pub status: FormStatus,
}

impl FormTemplateObject {
    pub fn get(mut form: impl TemplatedForm) -> Self {
        let defaults = form.get_defaults();
        let keys = defaults.keys().cloned().collect::<Vec<_>>();
        FormTemplateObject {
            data: defaults,
            errors: HashMap::new(),
            status: FormStatus::None,
            fields: keys,
        }
    }

    pub fn with_data(
        data: HashMap<String, String>,
        errors: HashMap<String, Vec<String>>,
        status: Status,
    ) -> Self {
        let status = match status.code {
            200 => FormStatus::Success,
            400 | 413 => FormStatus::Error,
            _ => FormStatus::None,
        };
        let fields = data.keys().cloned().collect::<Vec<_>>();
        FormTemplateObject {
            data,
            errors,
            status,
            fields,
        }
    }

    pub fn from_rocket_context(mut form: impl TemplatedForm, value: &FormContext<'_>) -> Self {
        let defaults = form.get_defaults();
        let data = value
            .fields()
            .map(|f| {
                let val = value.field_value(f);
                let name = f.to_string();
                (
                    name.clone(),
                    val.map(|s| s.to_string())
                        .unwrap_or_else(|| defaults.get(&name).cloned().unwrap_or_default()),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut errors = HashMap::with_capacity(value.fields().count());
        for e in value.errors() {
            if let Some(ref name) = e.name {
                errors
                    .entry(name.to_string())
                    .or_insert_with(Vec::new)
                    .push(e.to_string());
            }
        }
        Self::with_data(data, errors, value.status())
    }
}

fn in_debug(_: FunctionArgs) -> Result<Value, tera::Error> {
    Ok(tera::Value::Bool(cfg!(debug_assertions)))
}

pub fn gravatar_url(email: &str, size: u64) -> String {
    format!(
        "https://www.gravatar.com/avatar/{}?s={}&d=identicon&r=pg",
        sha256::digest(email),
        size
    )
}

fn gravatar_function(args: FunctionArgs) -> Result<Value, tera::Error> {
    let email = args.get("email").and_then(|o| o.as_str()).unwrap_or("");
    let size = args.get("size").and_then(|o| o.as_u64()).unwrap_or(30);
    Ok(tera::Value::String(gravatar_url(email, size)))
}

fn fake_attr(args: FunctionArgs) -> Result<Value, tera::Error> {
    let attr = args.get("attr").and_then(|o| o.as_str()).unwrap_or("");
    let val = args.get("val").and_then(|o| o.as_str()).unwrap_or("");
    Ok(tera::Value::String(format!("\"{attr}=\"{val}")))
}

fn format_time_taken(args: FunctionArgs) -> Result<Value, tera::Error> {
    let taken = args
        .get("time")
        .and_then(|o| o.as_i64())
        .ok_or(tera::Error::msg("time not passed!"))?;
    if taken == -1 {
        return Ok(tera::Value::String("--".to_string()));
    }

    let hours = taken / 60;
    let minutes = taken % 60;
    let hours_f = if hours == 0 {
        "".to_string()
    } else {
        format!("{hours}h ")
    };
    let minutes_f = format!("{minutes}m");
    Ok(tera::Value::String(format!("{hours_f}{minutes_f}")))
}

fn render_markdown(args: FunctionArgs) -> Result<Value, tera::Error> {
    let text = args
        .get("md")
        .and_then(|o| o.as_str())
        .ok_or(tera::Error::msg("md not passed!"))?;
    let options = Options {
        parse: ParseOptions {
            constructs: Constructs {
                math_text: true,
                math_flow: true,
                ..Constructs::gfm()
            },
            ..ParseOptions::gfm()
        },
        compile: CompileOptions::gfm(),
    };

    let rendered = markdown::to_html_with_options(text, &options)
        .map_err(|e| tera::Error::msg(format!("Failed to render markdown: {:?}", e)))?;
    Ok(tera::Value::String(rendered))
}

fn len_of_form_data_list(args: FunctionArgs) -> Result<Value, tera::Error> {
    let data = args
        .get("data")
        .and_then(|o| o.as_array())
        .ok_or(tera::Error::msg("data not passed!"))
        .and_then(|v| {
            v.iter()
                .map(|s| {
                    s.as_str()
                        .ok_or(tera::Error::msg("data must be list of str!"))
                })
                .collect::<Result<Vec<&str>, _>>()
        })?;
    let list = args
        .get("list")
        .and_then(|s| s.as_str())
        .ok_or(tera::Error::msg("list not passed!"))?;

    let mut dat = data
        .into_iter()
        .filter_map(|name| {
            if name.starts_with(&format!("{list}[")) {
                Some(
                    name[list.len() + 1..]
                        .split(']')
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                )
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    dat.sort();
    dat.dedup();

    Ok(tera::Value::Array(
        dat.into_iter()
            .map(|i| tera::Value::Number(i.into()))
            .collect::<Vec<_>>(),
    ))
}

#[macro_export]
macro_rules! context_with_base {
    ($usr:expr, $($key:ident $(: $value:expr)?),*$(,)?) => {
        context! {
            logged_in: $usr.is_some(),
            user: $usr,
            name: $usr.map(|u| u.display_name()).unwrap_or_default(),
            version: env!("CARGO_PKG_VERSION"),
            $($key $(: $value)?),*
        }
    };
}

#[macro_export]
macro_rules! context_with_base_authed {
    ($usr:expr, $($key:ident $(: $value:expr)?),*$(,)?) => {
        context! {
            logged_in: true,
            user: $usr,
            name: $usr.display_name(),
            version: env!("CARGO_PKG_VERSION"),
            $($key $(: $value)?),*
        }
    };
}

pub fn stage() -> AdHoc {
    AdHoc::try_on_ignite("Templating", |rocket| async {
        let figment = rocket.figment();
        let url_prefix = figment.extract_inner::<String>("url").unwrap_or_default();
        let admins = figment
            .extract_inner::<Vec<String>>("admins")
            .unwrap_or_default();
        let branding = figment
            .extract_inner::<Option<BrandingConfig>>("branding")
            .context("Invalid branding found");

        let branding: BrandingConfig = match branding {
            Ok(b) => b.unwrap_or_default(),
            Err(e) => {
                error!("Failed to load branding: {:?}", e);
                return Err(rocket);
            }
        };

        let parsed_colors = branding.colors.parse_colors();

        let parsed_colors = match parsed_colors {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to parse colors: {:?}", e);
                return Err(rocket);
            }
        };

        let meta_info = SiteMetaInfo::new(&branding, &parsed_colors);

        let color_css = parsed_colors.generate_theme_css();

        let color_css_hash = base64::encode_block(&sha256(color_css.as_bytes()));

        let theme_style_tag = format!(
            "<style integrity=\"sha256-{}\" id=\"theme\">{}</style>",
            color_css_hash.clone(),
            color_css.clone()
        );

        let rocket = rocket
            .attach(crate::csp::stage())
            .attach(branding::image::stage(&branding))
            .manage(branding.clone())
            .manage(parsed_colors.clone())
            .manage(meta_info);

        Ok(rocket.attach(Template::custom(move |e| {
            let url_prefix = url_prefix.clone();
            let admins = admins.clone();
            let branding = branding.clone();
            let parsed_colors = parsed_colors.clone();
            let theme_style_tag = theme_style_tag.clone();
            e.tera
                .register_function("get_branding", move |_: FunctionArgs| {
                    Ok(serde_json::to_value(&branding).unwrap())
                });
            e.tera
                .register_function("get_color_css", move |_: FunctionArgs| {
                    Ok(tera::Value::String(theme_style_tag.clone()))
                });
            e.tera
                .register_function("get_theme_colors", move |_: FunctionArgs| {
                    Ok(serde_json::to_value(&parsed_colors.theme_color).unwrap())
                });
            e.tera.register_function("in_debug", in_debug);
            e.tera.register_function("gravatar", gravatar_function);
            e.tera.register_function("fake_attr", fake_attr);
            e.tera
                .register_function("format_time_taken", format_time_taken);
            e.tera.register_function("render_markdown", render_markdown);
            e.tera
                .register_function("url_prefix", move |_: FunctionArgs| {
                    Ok(tera::Value::String(url_prefix.clone()))
                });
            e.tera
                .register_function("len_of_form_data_list", len_of_form_data_list);
            e.tera
                .register_function("is_admin", move |args: FunctionArgs| {
                    if let Some(user) = args.get("user").and_then(|o| o.as_object()) {
                        Ok(tera::Value::Bool(
                            user.get("email")
                                .and_then(|e| e.as_str())
                                .map(|e| admins.contains(&e.to_string()))
                                .unwrap_or_default(),
                        ))
                    } else {
                        Err(tera::Error::msg("user object not passed!"))
                    }
                });
        })))
    })
}
