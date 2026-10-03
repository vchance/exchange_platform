//! Google Wallet: the "Save to Google Wallet" link (DESIGN.md §11).
//!
//! When the link is asked for, the generic pass class and this party's
//! object for this exchange are created (or brought up to date) through the
//! Google Wallet API ([`objects`]). The link is then
//! `https://pay.google.com/gp/v/save/<JWT>`, the JWT signed with the issuer's
//! service account key (RS256) and naming only that object's ID and class:
//! it carries no face, so a link used late saves the object as it is then,
//! and a link to a pass voided since saves the void, inactive object; it
//! cannot create a live one. Changes are patched into the object.
//!
//! Google documents no expiry (`exp`) for a save link's JWT, so a link works
//! for as long as Google accepts it; naming an existing object is what keeps
//! an old link harmless.
//!
//! The pass is a Generic pass, the type Google offers any issuer once its
//! account may publish. DESIGN.md §11 leaves open whether to ask for the
//! private pass type instead, once both have been tried on devices; the
//! object here would change in its type name and little else.

use std::sync::Arc;

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;

use super::pass::{Field, PassModel};
use crate::wallet::pem;

pub mod objects;

/// Where a JWT is opened to save the pass.
pub const SAVE_URL: &str = "https://pay.google.com/gp/v/save/";

/// The class every Yuppers pass belongs to, after the issuer ID.
const CLASS_SUFFIX: &str = "yuppers_agreement";

/// The accent colour (`apps/web/src/index.css`).
const BACKGROUND: &str = "#0b57b0";

/// A Google Cloud service account key file, as Google hands it out.
#[derive(Deserialize)]
struct KeyFile {
    #[serde(rename = "type")]
    kind: String,
    client_email: String,
    private_key_id: Option<String>,
    private_key: String,
    token_uri: Option<String>,
}

/// The service account a Wallet issuer acts as.
pub struct ServiceAccount {
    pub client_email: String,
    key_id: Option<String>,
    key: RsaKeyPair,
    /// Where an access token for the Wallet API is asked for.
    pub token_uri: String,
}

impl std::fmt::Debug for ServiceAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the key.
        f.debug_struct("ServiceAccount")
            .field("client_email", &self.client_email)
            .finish_non_exhaustive()
    }
}

impl ServiceAccount {
    /// From the JSON key file's contents.
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let file: KeyFile = serde_json::from_str(json)
            .context("GOOGLE_WALLET_SERVICE_ACCOUNT is not a service account key file (JSON)")?;
        if file.kind != "service_account" {
            bail!(
                "GOOGLE_WALLET_SERVICE_ACCOUNT is a {} key, not a service account's",
                file.kind
            );
        }
        Ok(Self {
            key: pem::rsa_key("GOOGLE_WALLET_SERVICE_ACCOUNT", &file.private_key)?,
            client_email: file.client_email,
            key_id: file.private_key_id,
            token_uri: file
                .token_uri
                .unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_owned()),
        })
    }

    /// `claims` as a JWT signed RS256 with the account's key.
    pub fn sign_jwt(&self, claims: &Value) -> anyhow::Result<String> {
        let mut header = json!({ "alg": "RS256", "typ": "JWT" });
        if let Some(id) = &self.key_id {
            header["kid"] = json!(id);
        }
        let mut token = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?)
        );
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                token.as_bytes(),
                &mut signature,
            )
            .map_err(|_| anyhow::anyhow!("the RSA signature failed"))?;
        token.push('.');
        token.push_str(&URL_SAFE_NO_PAD.encode(signature));
        Ok(token)
    }
}

/// Issues Google passes for one issuer account.
#[derive(Clone, Debug)]
pub struct GoogleIssuer {
    /// The issuer ID from the Google Pay and Wallet console: digits.
    pub issuer_id: String,
    pub account: Arc<ServiceAccount>,
}

impl GoogleIssuer {
    pub fn class_id(&self) -> String {
        format!("{}.{CLASS_SUFFIX}", self.issuer_id)
    }

    pub fn object_id(&self, serial: &str) -> String {
        format!("{}.{serial}", self.issuer_id)
    }

    /// The generic class, as it is created.
    pub fn class(&self) -> Value {
        json!({
            "id": self.class_id(),
            // One person's view of their agreement: not to be passed on.
            "multipleDevicesAndHoldersAllowedStatus": "ONE_USER_ALL_DEVICES",
        })
    }

    /// The generic object for `model`: what is created when the link is made
    /// and what an update patches in.
    pub fn object(&self, model: &PassModel, serial: &str) -> Value {
        let text =
            |value: &str| json!({ "defaultValue": { "language": model.language, "value": value } });
        let module =
            |field: &Field| json!({ "id": field.key, "header": field.label, "body": field.value });
        let mut modules: Vec<Value> = [
            model.reference.as_ref(),
            model.next_due.as_ref(),
            model.closed_on.as_ref(),
            model.outstanding.as_ref(),
            model.other_party.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(module)
        .collect();
        modules.push(json!({ "id": "note", "header": model.product, "body": model.note }));
        let links: Vec<Value> = model
            .link
            .iter()
            .map(|link| json!({ "id": "open", "uri": link.url, "description": link.label }))
            .collect();

        json!({
            "id": self.object_id(serial),
            "classId": self.class_id(),
            // Google keeps a pass that is over, or void, out of the way of
            // the ones in use.
            "state": if model.voided() {
                "INACTIVE"
            } else if model.finished() {
                "COMPLETED"
            } else {
                "ACTIVE"
            },
            "cardTitle": text(&model.product),
            "subheader": text(&model.status.label),
            "header": text(&model.status.value),
            "hexBackgroundColor": BACKGROUND,
            "textModulesData": modules,
            "linksModuleData": { "uris": links },
        })
    }

    /// The "Save to Google Wallet" link for the object of `serial`, which
    /// must already exist at Google ([`objects::WalletObjects::upsert`]),
    /// made at `now`, for a page served from `origin`.
    pub fn save_url(
        &self,
        serial: &str,
        origin: &str,
        now: OffsetDateTime,
    ) -> anyhow::Result<String> {
        let claims = json!({
            "iss": self.account.client_email,
            "aud": "google",
            "typ": "savetowallet",
            "iat": now.unix_timestamp(),
            "origins": [origin],
            "payload": {
                "genericObjects": [{ "id": self.object_id(serial), "classId": self.class_id() }],
            },
        });
        Ok(format!("{SAVE_URL}{}", self.account.sign_jwt(&claims)?))
    }
}

#[cfg(test)]
mod tests;
