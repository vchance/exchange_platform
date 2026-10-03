//! Reading the certificates and keys a deployment gives for Wallet passes.
//!
//! Each setting holds either the PEM text itself, for a platform whose
//! secret store hands secrets over as environment variables, or the path of
//! a file holding it, for one that mounts them as files. Text that starts
//! with `-----BEGIN` is taken as PEM; anything else as a path.

use anyhow::{Context, bail};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// The PEM text a setting names: the value itself, or the file it points to.
pub fn text(name: &str, value: &str) -> anyhow::Result<String> {
    let value = value.trim();
    if value.starts_with("-----BEGIN") {
        // A secret store that keeps one line per variable may turn the line
        // breaks into `\n`.
        return Ok(value.replace("\\n", "\n"));
    }
    std::fs::read_to_string(value).with_context(|| format!("{name}: cannot read the file {value}"))
}

/// Every certificate in a PEM text, in order, as DER.
pub fn certificates(name: &str, pem: &str) -> anyhow::Result<Vec<Vec<u8>>> {
    let found: Vec<Vec<u8>> = CertificateDer::pem_slice_iter(pem.as_bytes())
        .map(|certificate| certificate.map(|der| der.as_ref().to_vec()))
        .collect::<Result<_, _>>()
        .map_err(|error| anyhow::anyhow!("{name}: not PEM certificates: {error}"))?;
    if found.is_empty() {
        bail!("{name} holds no certificate (-----BEGIN CERTIFICATE-----)");
    }
    Ok(found)
}

/// An RSA private key from a PEM text, unencrypted, as PKCS #8 or PKCS #1.
pub fn rsa_key(name: &str, pem: &str) -> anyhow::Result<ring::signature::RsaKeyPair> {
    let key = PrivateKeyDer::from_pem_slice(pem.as_bytes()).map_err(|error| {
        anyhow::anyhow!(
            "{name}: no unencrypted private key (-----BEGIN PRIVATE KEY----- or \
             -----BEGIN RSA PRIVATE KEY-----): {error}"
        )
    })?;
    let pair = match &key {
        PrivateKeyDer::Pkcs8(der) => {
            ring::signature::RsaKeyPair::from_pkcs8(der.secret_pkcs8_der())
        }
        PrivateKeyDer::Pkcs1(der) => ring::signature::RsaKeyPair::from_der(der.secret_pkcs1_der()),
        _ => bail!("{name}: the key is not an RSA key"),
    };
    pair.map_err(|error| anyhow::anyhow!("{name}: not a usable RSA key: {error}"))
}

/// The private key as rustls takes it, for a TLS client certificate.
pub fn tls_key(name: &str, pem: &str) -> anyhow::Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_slice(pem.as_bytes())
        .map_err(|error| anyhow::anyhow!("{name}: no unencrypted private key: {error}"))
}
