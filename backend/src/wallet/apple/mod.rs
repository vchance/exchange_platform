//! Apple Wallet: the signed `.pkpass` (DESIGN.md §11).
//!
//! A pass is a zip archive of `pass.json`, its images, a `manifest.json`
//! naming the SHA-1 digest of every other file, and `signature`, a detached
//! CMS signature of the manifest ([`sign`]). `pass.json` points Wallet at the
//! pass web service (`crate::http::wallet`), which a device asks for the
//! latest pass after a push from [`push`].
//!
//! **Style.** The pass is a `generic` pass. A store card is for a balance or
//! points and puts a strip image across the middle; an event ticket, a
//! boarding pass and a coupon each imply something this is not. Generic is
//! Apple's style for anything else, shows a primary field large with
//! secondary and auxiliary rows under it, which is exactly the status, the
//! reference and the counts, and matches Google's generic pass, so both
//! wallets lay the same model out the same way.

use std::io::{Cursor, Write};

use anyhow::Context;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use time::OffsetDateTime;
use zip::write::SimpleFileOptions;

use super::pass::{Field, PassModel};

pub mod push;
pub mod sign;

pub use sign::Signer;

/// The images in every pass, drawn from the Yuppers mark by
/// `apps/mobile/scripts/make-icons.mjs`.
const IMAGES: [(&str, &[u8]); 6] = [
    (
        "icon.png",
        include_bytes!("../../../assets/wallet/icon.png"),
    ),
    (
        "icon@2x.png",
        include_bytes!("../../../assets/wallet/icon@2x.png"),
    ),
    (
        "icon@3x.png",
        include_bytes!("../../../assets/wallet/icon@3x.png"),
    ),
    (
        "logo.png",
        include_bytes!("../../../assets/wallet/logo.png"),
    ),
    (
        "logo@2x.png",
        include_bytes!("../../../assets/wallet/logo@2x.png"),
    ),
    (
        "logo@3x.png",
        include_bytes!("../../../assets/wallet/logo@3x.png"),
    ),
];

/// The app's accent colour behind white text (`apps/web/src/index.css`).
const BACKGROUND: &str = "rgb(11, 87, 176)";
const FOREGROUND: &str = "rgb(255, 255, 255)";
/// Labels, a little softer than the values: 5.4:1 against the background,
/// where white is 7:1.
const LABEL: &str = "rgb(214, 228, 247)";

/// The media type of a pass, which Safari opens in Wallet.
pub const CONTENT_TYPE: &str = "application/vnd.apple.pkpass";

/// What a deployment configures for Apple Wallet (`super::config`).
#[derive(Clone, Debug)]
pub struct AppleSettings {
    /// `pass.` and a reverse domain, as registered with Apple.
    pub pass_type_id: String,
    pub team_id: String,
    pub signer: std::sync::Arc<Signer>,
}

/// Issues Apple passes.
pub struct AppleIssuer {
    pub settings: AppleSettings,
    /// Where `pass.json` sends a device: the pass web service, under the
    /// API, through the web origin. Apple adds `/v1/devices/...` and
    /// `/v1/passes/...` to it.
    pub web_service_url: String,
}

impl AppleIssuer {
    pub fn new(settings: AppleSettings, web_origin: &str) -> Self {
        Self {
            settings,
            web_service_url: format!("{web_origin}{WEB_SERVICE_PATH}"),
        }
    }

    pub fn pass_type_id(&self) -> &str {
        &self.settings.pass_type_id
    }

    /// `pass.json` for `model`.
    pub fn pass_json(&self, model: &PassModel, serial: &str, auth_token: &str) -> Value {
        let fields = |fields: &[Option<&Field>]| -> Vec<Value> {
            fields
                .iter()
                .flatten()
                .map(|field| field_json(field))
                .collect()
        };
        let mut back: Vec<Value> = Vec::new();
        if let Some(link) = &model.link {
            back.push(json!({
                "key": "open",
                "label": link.label,
                "value": link.url,
                "attributedValue": format!("<a href=\"{}\">{}</a>", escape(&link.url), escape(&link.label)),
            }));
        }
        back.push(json!({ "key": "note", "label": model.product, "value": model.note }));

        let mut pass = json!({
            "formatVersion": 1,
            "passTypeIdentifier": self.settings.pass_type_id,
            "serialNumber": serial,
            "teamIdentifier": self.settings.team_id,
            "organizationName": model.product,
            "description": model.description,
            "logoText": model.product,
            "foregroundColor": FOREGROUND,
            "backgroundColor": BACKGROUND,
            "labelColor": LABEL,
            "webServiceURL": self.web_service_url,
            "authenticationToken": auth_token,
            // A pass is one person's view of their agreement.
            "sharingProhibited": true,
            "generic": {
                "primaryFields": fields(&[Some(&model.status)]),
                "secondaryFields": fields(&[model.next_due.as_ref(), model.closed_on.as_ref(), model.outstanding.as_ref()]),
                "auxiliaryFields": fields(&[model.reference.as_ref(), model.other_party.as_ref()]),
                "backFields": back,
            },
        });
        if model.voided() {
            pass["voided"] = json!(true);
        }
        pass
    }

    /// The whole `.pkpass`, signed at `at`.
    pub fn package(
        &self,
        model: &PassModel,
        serial: &str,
        auth_token: &str,
        at: OffsetDateTime,
    ) -> anyhow::Result<Vec<u8>> {
        let pass = serde_json::to_vec_pretty(&self.pass_json(model, serial, auth_token))?;
        let mut files: Vec<(&str, &[u8])> = vec![("pass.json", &pass)];
        files.extend(IMAGES);

        let manifest: serde_json::Map<String, Value> = files
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), Value::String(sha1_hex(bytes))))
            .collect();
        let manifest = serde_json::to_vec_pretty(&manifest)?;
        let signature = self.settings.signer.sign(&manifest, at)?;

        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files.into_iter().chain([
            ("manifest.json", &manifest[..]),
            ("signature", &signature[..]),
        ]) {
            archive.start_file(name, options).context("zip")?;
            archive.write_all(bytes).context("zip")?;
        }
        Ok(archive.finish().context("zip")?.into_inner())
    }
}

/// Where the pass web service is, under the web origin.
pub const WEB_SERVICE_PATH: &str = "/v1/wallet/apple";

fn field_json(field: &Field) -> Value {
    // No `changeMessage`: a change to a pass is never announced on the lock
    // screen (`super::pass`).
    json!({ "key": field.key, "label": field.label, "value": field.value })
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Text inside the little HTML Wallet allows in an attributed value.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
pub(crate) mod tests;
