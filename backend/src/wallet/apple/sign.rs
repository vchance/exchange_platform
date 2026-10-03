//! The signature of an Apple pass: a detached CMS (PKCS #7) `SignedData` over
//! the pass's `manifest.json`, made with the pass type certificate's key and
//! carrying that certificate and Apple's WWDR intermediate.
//!
//! The ASN.1 structures are the `cms` crate's (RustCrypto); the RSA signature
//! (PKCS #1 v1.5 with SHA-256) is ring's. What is signed is the usual set of
//! signed attributes: the content type (`id-data`), the signing time and the
//! SHA-256 digest of the manifest.

use std::time::Duration;

use anyhow::{Context, bail};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use const_oid::db::{rfc4519, rfc5911, rfc5912};
use der::asn1::{ObjectIdentifier, OctetString, SetOfVec, UtcTime};
use der::{Any, Decode, Encode};
use ring::rand::SystemRandom;
use ring::signature::{KeyPair, RSA_PKCS1_SHA256, RsaKeyPair};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use x509_cert::Certificate;
use x509_cert::attr::Attribute;
use x509_cert::name::Name;
use x509_cert::spki::AlgorithmIdentifierOwned;

use crate::wallet::pem;

/// The pass type certificate with its key, and the intermediate that issued
/// it. Checked when it is loaded: the key must belong to the certificate.
pub struct Signer {
    certificate: Certificate,
    intermediates: Vec<Certificate>,
    key: RsaKeyPair,
    /// The certificate and its key as DER, for the TLS client certificate
    /// that Apple's push service asks for (`super::push`).
    pub(crate) certificate_der: Vec<u8>,
    pub(crate) key_pem: String,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the key.
        f.debug_struct("Signer")
            .field(
                "subject",
                &self.certificate.tbs_certificate.subject.to_string(),
            )
            .finish_non_exhaustive()
    }
}

impl Signer {
    /// From PEM texts: the pass type certificate, its private key, and
    /// Apple's WWDR intermediate certificate (or several, in order).
    pub fn from_pem(certificate: &str, key: &str, intermediates: &str) -> anyhow::Result<Self> {
        let certificate_der = pem::certificates("APPLE_PASS_CERT", certificate)?
            .into_iter()
            .next()
            .expect("at least one, or it was refused");
        let parsed = Certificate::from_der(&certificate_der)
            .context("APPLE_PASS_CERT is not an X.509 certificate")?;
        let intermediates = pem::certificates("APPLE_WWDR_CERT", intermediates)?
            .iter()
            .map(|der| Certificate::from_der(der))
            .collect::<Result<Vec<_>, _>>()
            .context("APPLE_WWDR_CERT is not an X.509 certificate")?;
        let pair = pem::rsa_key("APPLE_PASS_KEY", key)?;

        // ring's public key is the PKCS #1 RSAPublicKey, which is exactly
        // what an RSA certificate's subjectPublicKey bit string holds.
        let spki = &parsed.tbs_certificate.subject_public_key_info;
        if spki.algorithm.oid != rfc5912::RSA_ENCRYPTION
            || spki.subject_public_key.raw_bytes() != pair.public_key().as_ref()
        {
            bail!("APPLE_PASS_KEY is not the private key of the certificate in APPLE_PASS_CERT");
        }
        Ok(Self {
            certificate: parsed,
            intermediates,
            key: pair,
            certificate_der,
            key_pem: key.to_owned(),
        })
    }

    /// The pass type identifier and team the certificate names, when it names
    /// them as Apple's do: the user ID (`UID=pass.app.yuppers`) and the
    /// organizational unit (`OU=ABCDE12345`).
    pub fn names(&self) -> (Option<String>, Option<String>) {
        let subject = &self.certificate.tbs_certificate.subject;
        (
            attribute(subject, rfc4519::UID),
            attribute(subject, rfc4519::OU),
        )
    }

    /// When the certificate stops being valid. Apple stops accepting passes
    /// and pushes signed with it then.
    pub fn not_after(&self) -> OffsetDateTime {
        let after = self
            .certificate
            .tbs_certificate
            .validity
            .not_after
            .to_unix_duration();
        OffsetDateTime::UNIX_EPOCH + after
    }

    /// Whether the WWDR certificate vouches for the pass type certificate at
    /// `now`: one of the intermediates is valid then, is named as the pass
    /// type certificate's issuer, and its key verifies that certificate's
    /// signature. Says what is wrong if not.
    pub fn check_issuer(&self, now: OffsetDateTime) -> Result<(), String> {
        let issuer_name = &self.certificate.tbs_certificate.issuer;
        let mut problem = format!(
            "the pass type certificate was issued by {issuer_name}, which APPLE_WWDR_CERT does not hold"
        );
        for issuer in self
            .intermediates
            .iter()
            .filter(|intermediate| intermediate.tbs_certificate.subject == *issuer_name)
        {
            match self.issued_by(issuer, now) {
                Ok(()) => return Ok(()),
                Err(found) => problem = found,
            }
        }
        Err(problem)
    }

    /// Whether `issuer` is valid at `now` and signed the certificate.
    fn issued_by(&self, issuer: &Certificate, now: OffsetDateTime) -> Result<(), String> {
        let validity = &issuer.tbs_certificate.validity;
        let instant =
            |time: x509_cert::time::Time| OffsetDateTime::UNIX_EPOCH + time.to_unix_duration();
        if instant(validity.not_after) <= now {
            return Err(format!(
                "the WWDR certificate expired on {}",
                instant(validity.not_after)
            ));
        }
        if instant(validity.not_before) > now {
            return Err("the WWDR certificate is not valid yet".to_owned());
        }
        let tbs = self
            .certificate
            .tbs_certificate
            .to_der()
            .map_err(|_| "the pass type certificate cannot be encoded again".to_owned())?;
        let signature = self
            .certificate
            .signature
            .as_bytes()
            .ok_or("the pass type certificate's signature is not whole bytes")?;
        let spki = &issuer.tbs_certificate.subject_public_key_info;
        let key = spki
            .subject_public_key
            .as_bytes()
            .ok_or("the WWDR certificate's key is not whole bytes")?;
        let curve = spki
            .algorithm
            .parameters
            .as_ref()
            .and_then(|parameters| parameters.decode_as::<ObjectIdentifier>().ok());
        let algorithm: &dyn ring::signature::VerificationAlgorithm =
            match (self.certificate.signature_algorithm.oid, curve) {
                (oid, _) if oid == rfc5912::SHA_256_WITH_RSA_ENCRYPTION => {
                    &ring::signature::RSA_PKCS1_2048_8192_SHA256
                }
                (oid, _) if oid == rfc5912::SHA_384_WITH_RSA_ENCRYPTION => {
                    &ring::signature::RSA_PKCS1_2048_8192_SHA384
                }
                (oid, _) if oid == rfc5912::SHA_512_WITH_RSA_ENCRYPTION => {
                    &ring::signature::RSA_PKCS1_2048_8192_SHA512
                }
                (oid, Some(curve)) if oid == rfc5912::ECDSA_WITH_SHA_256 => {
                    if curve == rfc5912::SECP_256_R_1 {
                        &ring::signature::ECDSA_P256_SHA256_ASN1
                    } else if curve == rfc5912::SECP_384_R_1 {
                        &ring::signature::ECDSA_P384_SHA256_ASN1
                    } else {
                        return Err(format!(
                            "the WWDR certificate's curve {curve} is not supported"
                        ));
                    }
                }
                (oid, Some(curve)) if oid == rfc5912::ECDSA_WITH_SHA_384 => {
                    if curve == rfc5912::SECP_256_R_1 {
                        &ring::signature::ECDSA_P256_SHA384_ASN1
                    } else if curve == rfc5912::SECP_384_R_1 {
                        &ring::signature::ECDSA_P384_SHA384_ASN1
                    } else {
                        return Err(format!(
                            "the WWDR certificate's curve {curve} is not supported"
                        ));
                    }
                }
                (oid, _) => {
                    return Err(format!(
                        "the pass type certificate's signature algorithm {oid} is not supported"
                    ));
                }
            };
        ring::signature::UnparsedPublicKey::new(algorithm, key)
            .verify(&tbs, signature)
            .map_err(|_| {
                "the WWDR certificate's key did not sign the pass type certificate".to_owned()
            })
    }

    /// The detached signature of `manifest`, as DER, made at `at`.
    pub fn sign(&self, manifest: &[u8], at: OffsetDateTime) -> anyhow::Result<Vec<u8>> {
        let digest = Sha256::digest(manifest);
        let seconds = u64::try_from(at.unix_timestamp()).context("a time before 1970")?;
        let signing_time = UtcTime::from_unix_duration(Duration::from_secs(seconds))?;

        let signed_attributes: SetOfVec<Attribute> = SetOfVec::try_from(vec![
            attribute_of(
                rfc5911::ID_CONTENT_TYPE,
                Any::encode_from(&rfc5911::ID_DATA)?,
            )?,
            attribute_of(rfc5911::ID_SIGNING_TIME, Any::encode_from(&signing_time)?)?,
            attribute_of(
                rfc5911::ID_MESSAGE_DIGEST,
                Any::encode_from(&OctetString::new(digest.to_vec())?)?,
            )?,
        ])?;
        // What is signed is the attributes encoded as a SET OF, the tag they
        // have on their own (RFC 5652 §5.4), not the [0] they carry inside
        // the SignerInfo.
        let to_sign = signed_attributes.to_der()?;
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                &to_sign,
                &mut signature,
            )
            .map_err(|_| anyhow::anyhow!("the RSA signature failed"))?;

        let signer = SignerInfo {
            version: CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: self.certificate.tbs_certificate.issuer.clone(),
                serial_number: self.certificate.tbs_certificate.serial_number.clone(),
            }),
            digest_alg: sha256(),
            signed_attrs: Some(signed_attributes),
            signature_algorithm: AlgorithmIdentifierOwned {
                oid: rfc5912::RSA_ENCRYPTION,
                parameters: Some(Any::null()),
            },
            signature: OctetString::new(signature)?,
            unsigned_attrs: None,
        };

        let mut certificates = vec![CertificateChoices::Certificate(self.certificate.clone())];
        certificates.extend(
            self.intermediates
                .iter()
                .cloned()
                .map(CertificateChoices::Certificate),
        );
        let signed = SignedData {
            version: CmsVersion::V1,
            digest_algorithms: SetOfVec::try_from(vec![sha256()])?,
            // Detached: the manifest is beside the signature in the pass.
            encap_content_info: EncapsulatedContentInfo {
                econtent_type: rfc5911::ID_DATA,
                econtent: None,
            },
            certificates: Some(CertificateSet::try_from(certificates)?),
            crls: None,
            signer_infos: SignerInfos::try_from(vec![signer])?,
        };
        let info = ContentInfo {
            content_type: rfc5911::ID_SIGNED_DATA,
            content: Any::encode_from(&signed)?,
        };
        Ok(info.to_der()?)
    }
}

fn sha256() -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned {
        oid: rfc5912::ID_SHA_256,
        parameters: None,
    }
}

fn attribute_of(oid: ObjectIdentifier, value: Any) -> der::Result<Attribute> {
    Ok(Attribute {
        oid,
        values: SetOfVec::try_from(vec![value])?,
    })
}

/// The first value of an attribute in a name, as text.
fn attribute(name: &Name, oid: ObjectIdentifier) -> Option<String> {
    name.0
        .iter()
        .flat_map(|rdn| rdn.0.iter())
        .find(|atv| atv.oid == oid)
        .and_then(|atv| {
            let value = &atv.value;
            value
                .decode_as::<der::asn1::Utf8StringRef<'_>>()
                .map(|text| text.as_str().to_owned())
                .or_else(|_| {
                    value
                        .decode_as::<der::asn1::PrintableStringRef<'_>>()
                        .map(|text| text.as_str().to_owned())
                })
                .or_else(|_| {
                    value
                        .decode_as::<der::asn1::Ia5StringRef<'_>>()
                        .map(|text| text.as_str().to_owned())
                })
                .ok()
        })
}
