//! Throwaway credentials for the Wallet tests, made when a test run first
//! asks for them and never written anywhere: a stand-in for Apple's WWDR
//! intermediate, a pass type certificate it issued with its key, and a
//! Google service account key file. Nothing here is, or could stand in for,
//! a real credential; every key is new on every run.
//!
//! Shared by the unit tests (`crate::wallet`) and the API tests
//! (`tests/wallet.rs`, which include this file by path), so it names nothing
//! from the crate.

#![allow(dead_code)]

use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;

use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::pkcs1v15::{Signature, SigningKey};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::rand_core::OsRng;
use rsa::sha2::Sha256;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::der::EncodePem;
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::Validity;

pub const PASS_TYPE_ID: &str = "pass.app.yuppers.test";
pub const TEAM_ID: &str = "ABCDE12345";
pub const ISSUER_ID: &str = "3388000000012345678";
pub const CLIENT_EMAIL: &str = "wallet@yuppers-test.iam.gserviceaccount.com";

pub struct TestCredentials {
    /// The stand-in WWDR intermediate, PEM.
    pub wwdr_pem: String,
    /// The pass type certificate, PEM, issued by the stand-in.
    pub pass_cert_pem: String,
    /// Its private key, PKCS #8 PEM.
    pub pass_key_pem: String,
    /// The same key as PKCS #1 PEM, the other form a key may come in.
    pub pass_key_pkcs1_pem: String,
    /// The pass key's public half as a PKCS #1 RSAPublicKey, DER.
    pub pass_public_der: Vec<u8>,
    /// A service account key file, as Google hands it out.
    pub google_key_file: String,
    /// The service account key's public half as a PKCS #1 RSAPublicKey, DER.
    pub google_public_der: Vec<u8>,
    /// A key that belongs to nothing above.
    pub stranger_key_pem: String,
}

fn key() -> RsaPrivateKey {
    RsaPrivateKey::new(&mut OsRng, 2048).expect("an RSA key")
}

fn certificate(
    profile: Profile,
    serial: u32,
    subject: &str,
    subject_key: &RsaPrivateKey,
    issuer_key: &RsaPrivateKey,
) -> x509_cert::Certificate {
    let signer = SigningKey::<Sha256>::new(issuer_key.clone());
    let spki = SubjectPublicKeyInfoOwned::from_key(subject_key.to_public_key()).expect("SPKI");
    CertificateBuilder::new(
        profile,
        SerialNumber::from(serial),
        Validity::from_now(Duration::from_secs(24 * 3600)).expect("validity"),
        Name::from_str(subject).expect("a name"),
        spki,
        &signer,
    )
    .expect("a certificate builder")
    .build::<Signature>()
    .expect("a certificate")
}

pub static CREDENTIALS: LazyLock<TestCredentials> = LazyLock::new(|| {
    let wwdr_key = key();
    let pass_key = key();
    let google_key = key();
    let stranger = key();

    let wwdr_name = "CN=Test Wallet Intermediate,OU=G4,O=Yuppers Tests,C=US";
    let wwdr = certificate(Profile::Root, 1, wwdr_name, &wwdr_key, &wwdr_key);
    let pass = certificate(
        Profile::Leaf {
            issuer: Name::from_str(wwdr_name).expect("a name"),
            enable_key_agreement: false,
            enable_key_encipherment: false,
        },
        2,
        &format!(
            "UID={PASS_TYPE_ID},CN=Pass Type ID: {PASS_TYPE_ID},OU={TEAM_ID},O=Yuppers Tests,C=US"
        ),
        &pass_key,
        &wwdr_key,
    );

    let pem = |key: &RsaPrivateKey| key.to_pkcs8_pem(LineEnding::LF).expect("PEM").to_string();
    let google_key_file = serde_json::json!({
        "type": "service_account",
        "project_id": "yuppers-test",
        "private_key_id": "0123456789abcdef",
        "private_key": pem(&google_key),
        "client_email": CLIENT_EMAIL,
        "client_id": "100000000000000000001",
        "token_uri": "https://oauth2.googleapis.com/token",
    })
    .to_string();

    TestCredentials {
        wwdr_pem: wwdr.to_pem(LineEnding::LF).expect("PEM"),
        pass_cert_pem: pass.to_pem(LineEnding::LF).expect("PEM"),
        pass_key_pem: pem(&pass_key),
        pass_key_pkcs1_pem: rsa::pkcs1::EncodeRsaPrivateKey::to_pkcs1_pem(
            &pass_key,
            LineEnding::LF,
        )
        .expect("PEM")
        .to_string(),
        pass_public_der: pass_key
            .to_public_key()
            .to_pkcs1_der()
            .expect("DER")
            .as_bytes()
            .to_vec(),
        google_key_file,
        google_public_der: google_key
            .to_public_key()
            .to_pkcs1_der()
            .expect("DER")
            .as_bytes()
            .to_vec(),
        stranger_key_pem: pem(&stranger),
    }
});

/// The settings that turn both platforms on with these credentials.
pub fn settings() -> Vec<(&'static str, String)> {
    let credentials = &*CREDENTIALS;
    vec![
        ("APPLE_PASS_TYPE_ID", PASS_TYPE_ID.to_owned()),
        ("APPLE_TEAM_ID", TEAM_ID.to_owned()),
        ("APPLE_PASS_CERT", credentials.pass_cert_pem.clone()),
        ("APPLE_PASS_KEY", credentials.pass_key_pem.clone()),
        ("APPLE_WWDR_CERT", credentials.wwdr_pem.clone()),
        ("GOOGLE_WALLET_ISSUER_ID", ISSUER_ID.to_owned()),
        (
            "GOOGLE_WALLET_SERVICE_ACCOUNT",
            credentials.google_key_file.clone(),
        ),
        ("WALLET_DELIVERY", "log".to_owned()),
    ]
}
