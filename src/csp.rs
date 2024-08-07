use std::path::{Path, PathBuf};

use rocket::{fairing::AdHoc, http::Header};
use serde::Deserialize;

const SRI_HASHES_FILE: &str = "sriHashes.json";
const GRAVATAR_URL: &str = "https://www.gravatar.com/avatar/";
const GH_AVATAR_URL: &str = "https://avatars.githubusercontent.com/u/";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SRIHashes {
    inline_script_hashes: Vec<String>,
    //inline_style_hashes: Vec<String>,
    ext_script_hashes: Vec<String>,
    //ext_style_hashes: Vec<String>,
}

fn join_hashes(hashes: &[String]) -> String {
    hashes
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn stage_inner(path: &Path) -> AdHoc {
    let raw_hashes = std::fs::read_to_string(path).unwrap();
    let hashes: SRIHashes = serde_json::from_str(&raw_hashes).unwrap();
    let directives: Vec<String> = vec![
        "default-src 'self'".to_string(),
        "object-src 'none'".to_string(),
        "worker-src 'self' blob:".to_string(),
        "frame-ancestors 'none'".to_string(),
        format!("style-src 'self' 'unsafe-inline'"),
        format!("font-src 'self' data:"),
        format!("img-src 'self' {GRAVATAR_URL} {GH_AVATAR_URL}"),
        format!(
            "script-src 'self' {} {}",
            join_hashes(&hashes.ext_script_hashes),
            join_hashes(&hashes.inline_script_hashes)
        ),
        // format!("style-src-elem 'self' {} {}", join_hashes(&hashes.ext_style_hashes), join_hashes(&hashes.inline_style_hashes)),
    ];
    let value = directives.join("; ");
    AdHoc::on_response("Content-Security-Policy", move |_req, resp| {
        let value = value.clone();
        Box::pin(async move {
            let header = Header::new("Content-Security-Policy", value);
            resp.adjoin_header(header)
        })
    })
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("CSP Setup", |rocket| async {
        let figment = rocket.figment();
        let template_dir = PathBuf::from(
            figment
                .find_value("template_dir")
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "templates".to_string()),
        );
        let path = template_dir.join(SRI_HASHES_FILE);

        rocket.attach(stage_inner(&path))
    })
}
