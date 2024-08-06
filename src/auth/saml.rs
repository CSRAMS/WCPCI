use std::collections::HashMap;

use log::{info, warn};
use openssl::pkey::PKey;
use rocket::{
    fairing::AdHoc,
    form::Form,
    get,
    http::{CookieJar, Status},
    post,
    response::Redirect,
    routes, FromForm, State,
};
use samael::{
    metadata::{ContactPerson, EntityDescriptor},
    service_provider::{ServiceProvider, ServiceProviderBuilder},
    traits::ToXml,
};
use serde::Deserialize;

use crate::{db::DbConnection, error::prelude::*, messages::Message, run::CodeInfo};

use super::{users::User, REDIRECT_COOKIE_NAME};

fn cn_oid() -> String {
    "urn:oid:2.5.4.3".to_string()
}

fn email_oid() -> String {
    "urn:oid:0.9.2342.19200300.100.1.3".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
struct AttrOptions {
    #[serde(default = "cn_oid")]
    display_name: String,
    #[serde(default = "email_oid")]
    email: String,
}

impl Default for AttrOptions {
    fn default() -> Self {
        Self {
            display_name: cn_oid(),
            email: email_oid(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SamlOptions {
    entity_id: String,
    idp_meta_url: Option<String>,
    certificate: Option<String>,
    private_key: Option<String>,
    contact_person: Option<String>,
    contact_email: Option<String>,
    contact_telephone: Option<String>,
    organization_name: Option<String>,
    #[serde(default)]
    attrs: AttrOptions,
}

pub const PREFERRED_SSO_BINDING: &str = "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect";
const NAME_ID_FORMAT: &str = "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent";

impl SamlOptions {
    pub async fn create_service_provider(&self, url_prefix: &str) -> Result<ServiceProvider> {
        let mut sp = ServiceProviderBuilder::default();
        sp.entity_id(self.entity_id.clone())
            .allow_idp_initiated(true)
            .acs_url(format!("{}/auth/saml/acs", url_prefix))
            .slo_url(format!("{}/auth/saml/slo", url_prefix))
            .metadata_url(format!("{}/auth/saml/metadata", url_prefix))
            .authn_name_id_format(Some(NAME_ID_FORMAT.to_string()))
            .contact_person(ContactPerson {
                sur_name: self.contact_person.clone(),
                email_addresses: self.contact_email.as_ref().map(|e| vec![e.clone()]),
                telephone_numbers: self.contact_telephone.as_ref().map(|t| vec![t.clone()]),
                company: self.organization_name.clone(),
                ..Default::default()
            });

        if let Some(idp_meta_url) = &self.idp_meta_url {
            info!("SAML App is fetching IDP metadata from {idp_meta_url}...");
            let resp = reqwest::get(idp_meta_url)
                .await
                .context("Couldn't fetch IDP metadata")?;
            let text = resp.text().await.context("Couldn't read IDP metadata")?;
            info!("SAML App fetched IDP metadata successfully");
            let idp_meta: EntityDescriptor =
                samael::metadata::de::from_str(&text).context("Couldn't parse IDP metadata")?;
            sp.idp_metadata(idp_meta);
        }

        if let Some(cert_path) = &self.certificate {
            let cert_raw =
                std::fs::read_to_string(cert_path).context("Couldn't read certificate")?;
            let cert = openssl::x509::X509::from_pem(cert_raw.as_bytes())
                .context("Couldn't parse certificate")?;
            sp.certificate(cert);
        }

        if let Some(private_key_path) = &self.private_key {
            let private_key =
                std::fs::read_to_string(private_key_path).context("Couldn't read private key")?;
            let key = openssl::rsa::Rsa::private_key_from_pem(private_key.as_bytes())
                .context("Couldn't parse private key")?;
            let key = PKey::from_rsa(key).context("Couldn't create PKey")?;
            sp.key(Some(key));
        }

        let sp = sp.build().context("Couldn't build ServiceProvider")?;

        if sp.sso_binding_location(PREFERRED_SSO_BINDING).is_none() {
            Err(anyhow!(
                "IDP doesn't support the preferred SSO binding: {PREFERRED_SSO_BINDING}"
            ))
        } else {
            Ok(sp)
        }
    }
}

#[get("/login")]
async fn login(sp: &State<ServiceProvider>, cookies: &CookieJar<'_>) -> ResultResponse<Redirect> {
    let base = sp
        .sso_binding_location(PREFERRED_SSO_BINDING)
        .ok_or_else(|| {
            anyhow!(
                "SAML IDP Doesn't Support Preferred SSO Binding ({}). Did the IDP change metadata?",
                PREFERRED_SSO_BINDING
            )
        })?;
    let req = sp
        .make_authentication_request(&base)
        .map_err(|e| anyhow!("{e:?}"))
        .context("Couldn't Create Authn Request")?;

    let relay = cookies
        .get(REDIRECT_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .unwrap_or_else(|| "/".to_string());

    //cookies.remove(Cookie::from(REDIRECT_COOKIE_NAME));

    let url = if let Some(key) = sp.key.as_ref() {
        req.signed_redirect(&relay, key.clone())
    } else {
        req.redirect(&relay)
    }
    .map_err(|e| anyhow!("{e:?}"))
    .context("Couldn't Generate Redirect")?
    .ok_or_else(|| anyhow!("Couldn't Generate Redirect"))?;

    Ok(Redirect::to(url.to_string()))
}

#[get("/metadata")]
async fn metadata(sp: &State<ServiceProvider>) -> ResultResponse<String> {
    let res = sp
        .metadata()
        .map_err(|e| anyhow!("{e:?}"))
        .context("Couldn't generate metadata")
        .and_then(|x| {
            x.to_string()
                .map_err(|e| anyhow!("{e:?}"))
                .context("Couldn't convert metadata to XML")
        })?;
    Ok(res)
}

#[derive(FromForm, Debug)]
struct SamlAcsForm {
    #[field(name = "SAMLResponse")]
    saml_response: String,
    #[field(name = "RelayState")]
    relay_state: Option<String>,
}

#[post("/acs", data = "<form>")]
async fn acs(
    mut db: DbConnection,
    sp: &State<ServiceProvider>,
    so: &State<SamlOptions>,
    form: Form<SamlAcsForm>,
    code_info: &State<CodeInfo>,
    cookies: &CookieJar<'_>,
) -> ResultResponse<Redirect> {
    let form = form.into_inner();

    let raw = form.saml_response;
    let relay_state = form.relay_state.unwrap_or_else(|| "/".to_string());

    let assertion = sp.parse_base64_response(&raw, None).map_err(|e| {
        warn!("Couldn't parse or validate SAML response: {e}");
        Status::BadRequest
    })?;

    if let Some(attrs) = assertion.attribute_statements {
        let attrs_map = attrs
            .into_iter()
            .flat_map(|x| {
                x.attributes
                    .into_iter()
                    .filter_map(|a| {
                        a.name.and_then(|name| {
                            a.values
                                .into_iter()
                                .next()
                                .and_then(|v| v.value)
                                .map(|value| (name, value))
                        })
                    })
                    .collect::<HashMap<_, _>>()
            })
            .collect::<HashMap<_, _>>();

        let id = assertion.subject.and_then(|s| s.name_id.map(|n| n.value));

        if let (Some(id), Some(display_name), Some(email)) = (
            id,
            attrs_map.get(&so.attrs.display_name),
            attrs_map.get(&so.attrs.email),
        ) {
            let user = User::temporary(
                id,
                email.clone(),
                display_name.clone(),
                &code_info.run_config.default_language,
            );
            let (user, is_new) = user
                .login_or_register(&mut db, cookies)
                .await
                .context("Couldn't log-in / register user")?;

            if is_new {
                Ok(Message::info(&format!("Welcome to WCPC, {}! Please look through your settings before joining a competition", user.default_display_name)).to("/settings/profile"))
            } else {
                Ok(Redirect::to(relay_state))
            }
        } else {
            warn!(
                "No display name or email found in SAML response, looked for {} and {}",
                so.attrs.display_name, so.attrs.email
            );
            warn!("Attributes found in SAML response: {attrs_map:?}");
            Err(Status::BadRequest.into())
        }
    } else {
        warn!("No attributes found in SAML response");
        Err(Status::BadRequest.into())
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("SAML Auth", |rocket| async {
        let figment = rocket.figment();
        let url = figment
            .extract_inner::<String>("url")
            .expect("Couldn't extract URL");
        let saml_options = figment.extract_inner::<SamlOptions>("saml").ok();
        if let Some(saml_options) = saml_options {
            let sp = saml_options
                .create_service_provider(&url)
                .await
                .expect("Couldn't create service provider");
            rocket
                .manage(sp)
                .manage(saml_options)
                .mount("/auth/saml", routes![login, metadata, acs])
        } else {
            warn!("No / Invalid SAML options found, users won't be able to authenticate with SAML");
            rocket
        }
    })
}
