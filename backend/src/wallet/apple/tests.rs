//! The `.pkpass` built with throwaway credentials (`crate::wallet::testkit`),
//! taken apart again and checked: every file against the manifest, and the
//! manifest against its signature, with the same libraries that made them.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier};
use const_oid::db::{rfc5911, rfc5912};
use der::asn1::OctetString;
use der::{Decode, Encode};
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::Value;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use time::macros::datetime;
use uuid::Uuid;
use x509_cert::Certificate;

use crate::wallet::config::WalletConfig;
use crate::wallet::pass::{Field, Link, PassModel, Standing};
use crate::wallet::testkit::{self, CREDENTIALS, PASS_TYPE_ID, TEAM_ID};
use crate::wallet::{Wallet, WalletPlatform, pem};

fn lookup(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
    let table: HashMap<&str, String> = pairs.into_iter().collect();
    move |name| table.get(name).cloned()
}

pub(crate) fn wallet() -> Wallet {
    let config = WalletConfig::from_lookup(&lookup(testkit::settings())).unwrap();
    Wallet::new(
        &config,
        "https://app.test/",
        Some(b"test-secret-test-secret-test-secret"),
    )
    .unwrap()
}

fn model() -> PassModel {
    let field = |key, label: &str, value: &str| Field {
        key,
        label: label.to_owned(),
        value: value.to_owned(),
    };
    PassModel {
        language: "en",
        product: "Yuppers".to_owned(),
        description: "Yuppers yup AB12-CD34".to_owned(),
        standing: Standing::WaitingForYou,
        status: field("status", "Status", "Waiting for you"),
        reference: Some(field("reference", "Reference", "AB12-CD34")),
        next_due: Some(field("next-due", "Next due", "Oct 9, 2026")),
        outstanding: Some(field("outstanding", "Still to do", "2")),
        other_party: None,
        closed_on: None,
        link: Some(Link {
            label: "Open the yup".to_owned(),
            url: "https://app.test/exchanges/7".to_owned(),
        }),
        note: "This pass shows only how your yup stands.".to_owned(),
    }
}

/// The files of a `.pkpass`.
fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();
            (file.name().to_owned(), contents)
        })
        .collect()
}

/// Checks a detached signature of `manifest` the way a verifier would:
/// the signer's certificate is in it, the signed digest is the manifest's,
/// and the RSA signature over the signed attributes holds under the
/// certificate's key. Returns the certificates it carries.
fn verify(signature: &[u8], manifest: &[u8]) -> Result<Vec<Certificate>, String> {
    let info = ContentInfo::from_der(signature).map_err(|e| e.to_string())?;
    if info.content_type != rfc5911::ID_SIGNED_DATA {
        return Err("not signed data".into());
    }
    let signed: SignedData = info.content.decode_as().map_err(|e| e.to_string())?;
    if signed.encap_content_info.econtent.is_some() {
        return Err("not detached".into());
    }
    let certificates: Vec<Certificate> = signed
        .certificates
        .iter()
        .flat_map(|set| set.0.iter())
        .map(|choice| match choice {
            cms::cert::CertificateChoices::Certificate(certificate) => certificate.clone(),
            _ => panic!("only certificates"),
        })
        .collect();
    let signer = signed.signer_infos.0.iter().next().ok_or("no signer")?;
    let SignerIdentifier::IssuerAndSerialNumber(id) = &signer.sid else {
        return Err("not identified by issuer and serial".into());
    };
    let certificate = certificates
        .iter()
        .find(|c| {
            c.tbs_certificate.issuer == id.issuer
                && c.tbs_certificate.serial_number == id.serial_number
        })
        .ok_or("the signer's certificate is not in the signature")?;
    if signer.digest_alg.oid != rfc5912::ID_SHA_256 {
        return Err("not SHA-256".into());
    }
    let attributes = signer.signed_attrs.as_ref().ok_or("no signed attributes")?;
    let value = |oid| {
        attributes
            .iter()
            .find(|attribute| attribute.oid == oid)
            .and_then(|attribute| attribute.values.iter().next().cloned())
    };
    let content_type: der::asn1::ObjectIdentifier = value(rfc5911::ID_CONTENT_TYPE)
        .ok_or("no content type")?
        .decode_as()
        .map_err(|e| e.to_string())?;
    if content_type != rfc5911::ID_DATA {
        return Err("content type is not id-data".into());
    }
    value(rfc5911::ID_SIGNING_TIME).ok_or("no signing time")?;
    let digest: OctetString = value(rfc5911::ID_MESSAGE_DIGEST)
        .ok_or("no message digest")?
        .decode_as()
        .map_err(|e| e.to_string())?;
    if digest.as_bytes() != Sha256::digest(manifest).as_slice() {
        return Err("the signed digest is not the manifest's".into());
    }
    let public_key = certificate
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .raw_bytes();
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public_key)
        .verify(&attributes.to_der().unwrap(), signer.signature.as_bytes())
        .map_err(|_| "the signature does not hold".to_owned())?;
    Ok(certificates)
}

#[test]
fn a_pass_is_signed_and_every_file_matches_its_manifest() {
    let wallet = wallet();
    let issuer = wallet.apple.as_ref().unwrap();
    let pass = Uuid::new_v4();
    let token = wallet.apple_auth_token(pass);
    let bytes = issuer
        .package(
            &model(),
            "0123abcd",
            &token,
            datetime!(2026-10-03 12:00 UTC),
        )
        .unwrap();
    let files = unzip(&bytes);

    let mut names: Vec<&str> = files.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "icon.png",
            "icon@2x.png",
            "icon@3x.png",
            "logo.png",
            "logo@2x.png",
            "logo@3x.png",
            "manifest.json",
            "pass.json",
            "signature"
        ]
    );

    // The manifest names every other file, with its SHA-1 in hex.
    let manifest: HashMap<String, String> =
        serde_json::from_slice(&files["manifest.json"]).unwrap();
    assert_eq!(manifest.len(), files.len() - 2);
    for (name, contents) in &files {
        if name == "manifest.json" || name == "signature" {
            continue;
        }
        let digest: String = Sha1::digest(contents)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(manifest[name], digest, "{name}");
    }

    // The signature covers the manifest, and carries the pass type
    // certificate and the intermediate that issued it.
    let certificates = verify(&files["signature"], &files["manifest.json"]).unwrap();
    let expected: Vec<Vec<u8>> = [&CREDENTIALS.pass_cert_pem, &CREDENTIALS.wwdr_pem]
        .iter()
        .map(|pem_text| pem::certificates("test", pem_text).unwrap().remove(0))
        .collect();
    let carried: Vec<Vec<u8>> = certificates.iter().map(|c| c.to_der().unwrap()).collect();
    for certificate in &expected {
        assert!(carried.contains(certificate));
    }

    // Changing any byte of the manifest breaks it.
    let mut tampered = files["manifest.json"].clone();
    tampered[5] ^= 1;
    assert!(verify(&files["signature"], &tampered).is_err());
}

#[test]
fn the_pass_names_its_type_team_web_service_and_token_and_never_alerts() {
    let wallet = wallet();
    let issuer = wallet.apple.as_ref().unwrap();
    let pass = Uuid::new_v4();
    let token = wallet.apple_auth_token(pass);
    let bytes = issuer
        .package(
            &model(),
            "0123abcd",
            &token,
            datetime!(2026-10-03 12:00 UTC),
        )
        .unwrap();
    let json: Value = serde_json::from_slice(&unzip(&bytes)["pass.json"]).unwrap();

    assert_eq!(json["formatVersion"], 1);
    assert_eq!(json["passTypeIdentifier"], PASS_TYPE_ID);
    assert_eq!(json["teamIdentifier"], TEAM_ID);
    assert_eq!(json["serialNumber"], "0123abcd");
    assert_eq!(json["authenticationToken"], token.as_str());
    assert!(token.len() >= 16);
    assert_eq!(json["webServiceURL"], "https://app.test/v1/wallet/apple");
    assert_eq!(json["sharingProhibited"], true);
    assert_eq!(
        json["generic"]["primaryFields"][0]["value"],
        "Waiting for you"
    );
    assert_eq!(json["generic"]["auxiliaryFields"][0]["value"], "AB12-CD34");
    assert_eq!(
        json["generic"]["backFields"][0]["value"],
        "https://app.test/exchanges/7"
    );
    assert!(json.get("voided").is_none());
    // No field asks Wallet to announce its change on the lock screen.
    assert!(!json.to_string().contains("changeMessage"));

    let void = crate::wallet::pass::void(wallet.wording.language("en"));
    let json = issuer.pass_json(&void, "0123abcd", &token);
    assert_eq!(json["voided"], true);
    assert_eq!(
        json["generic"]["primaryFields"][0]["value"],
        "No longer in use"
    );
    assert_eq!(json["generic"]["auxiliaryFields"], serde_json::json!([]));
    assert_eq!(json["generic"]["backFields"].as_array().unwrap().len(), 1);
}

#[test]
fn the_authentication_token_is_the_same_each_time_and_differs_per_pass() {
    let wallet = wallet();
    let (one, two) = (Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(wallet.apple_auth_token(one), wallet.apple_auth_token(one));
    assert_ne!(wallet.apple_auth_token(one), wallet.apple_auth_token(two));
}

#[test]
fn a_download_link_works_for_its_pass_until_it_expires_and_cannot_be_forged() {
    let wallet = wallet();
    let pass = Uuid::new_v4();
    let now = datetime!(2026-10-03 12:00 UTC);
    let token = wallet.apple_download_token(pass, now + time::Duration::minutes(10));
    assert_eq!(wallet.apple_download_pass(&token, now), Some(pass));
    assert_eq!(
        wallet.apple_download_pass(&token, now + time::Duration::minutes(10)),
        None
    );
    // Another pass's ID, or a later expiry, without the service's key.
    let mut forged =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &token).unwrap();
    forged[0] ^= 1;
    let forged = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, forged);
    assert_eq!(wallet.apple_download_pass(&forged, now), None);
    assert_eq!(wallet.apple_download_pass("not a token", now), None);
    assert_eq!(wallet.apple_download_pass("", now), None);
}

#[test]
fn both_platforms_are_on_with_all_their_settings_and_off_without_any() {
    let wallet = wallet();
    assert_eq!(
        wallet.platforms(),
        [WalletPlatform::Apple, WalletPlatform::Google]
    );
    assert!(Wallet::off("https://app.test").platforms().is_empty());
    let none = WalletConfig::from_lookup(&lookup(vec![])).unwrap();
    assert!(none.apple.is_none() && none.google.is_none() && none.delivery.is_none());
}

#[test]
fn a_platform_half_configured_or_wrongly_configured_stops_the_start() {
    type Settings = Vec<(&'static str, String)>;
    let with = |change: &dyn Fn(&mut Settings)| {
        let mut settings = testkit::settings();
        change(&mut settings);
        WalletConfig::from_lookup(&lookup(settings))
    };
    let set = |settings: &mut Settings, name: &'static str, value: &str| {
        settings.retain(|(key, _)| *key != name);
        settings.push((name, value.to_owned()));
    };
    assert!(with(&|_| {}).is_ok());
    // The other form a key comes in.
    assert!(with(&|s| set(s, "APPLE_PASS_KEY", &CREDENTIALS.pass_key_pkcs1_pem)).is_ok());

    for (name, value) in [
        ("APPLE_PASS_KEY", ""),
        ("APPLE_WWDR_CERT", " "),
        ("GOOGLE_WALLET_SERVICE_ACCOUNT", ""),
        ("WALLET_DELIVERY", ""),
        ("WALLET_DELIVERY", "push"),
        ("APPLE_PASS_TYPE_ID", "app.yuppers"),
        // The certificate names another pass type, and another team.
        ("APPLE_PASS_TYPE_ID", "pass.app.yuppers.other"),
        ("APPLE_TEAM_ID", "ZZZZZ99999"),
        ("APPLE_TEAM_ID", "short"),
        ("APPLE_PASS_KEY", &CREDENTIALS.stranger_key_pem),
        ("APPLE_PASS_CERT", "/nowhere/pass.pem"),
        ("GOOGLE_WALLET_ISSUER_ID", "issuer"),
        (
            "GOOGLE_WALLET_SERVICE_ACCOUNT",
            "{\"type\":\"authorized_user\"}",
        ),
    ] {
        let refused = with(&|s| set(s, name, value));
        assert!(refused.is_err(), "{name}={value:?} was accepted");
        // A refusal never repeats a key.
        let message = format!("{:#}", refused.err().unwrap());
        assert!(!message.contains("PRIVATE KEY-----\nMII"), "{message}");
    }
}

#[test]
fn settings_may_name_files_instead_of_holding_the_pem() {
    let directory = std::env::temp_dir().join(format!("yuppers-wallet-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let write = |name: &str, contents: &str| {
        let path = directory.join(name);
        std::fs::write(&path, contents).unwrap();
        path.to_str().unwrap().to_owned()
    };
    let mut settings = testkit::settings();
    for (name, file) in [
        ("APPLE_PASS_CERT", "pass.pem"),
        ("APPLE_PASS_KEY", "key.pem"),
        ("APPLE_WWDR_CERT", "wwdr.pem"),
        ("GOOGLE_WALLET_SERVICE_ACCOUNT", "account.json"),
    ] {
        let entry = settings.iter_mut().find(|(key, _)| *key == name).unwrap();
        entry.1 = write(file, &entry.1.clone());
    }
    let config = WalletConfig::from_lookup(&lookup(settings));
    std::fs::remove_dir_all(&directory).unwrap();
    let config = config.unwrap();
    assert!(config.apple.is_some() && config.google.is_some());
}
