//! The HTTP client Wallet updates go out on: hyper over rustls with ring,
//! the stack the service already has (axum's hyper, sqlx's and lettre's
//! rustls), with the Mozilla roots that `webpki-roots` carries, so the
//! image needs no system certificate store. HTTP/2 for Apple's push
//! service, which speaks nothing else, and HTTP/1.1 or 2 for Google's APIs.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use rustls::ClientConfig;
use rustls::crypto::CryptoProvider;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

pub type HttpsClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn roots() -> rustls::RootCertStore {
    rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

/// A client for HTTPS servers, HTTP/1.1 or HTTP/2 as the server prefers.
pub fn client() -> anyhow::Result<HttpsClient> {
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .context("TLS versions")?
        .with_root_certificates(roots())
        .with_no_client_auth();
    let https = HttpsConnectorBuilder::new()
        .with_tls_config(config)
        .https_only()
        .enable_http1()
        .enable_http2()
        .build();
    Ok(Client::builder(TokioExecutor::new()).build(https))
}

/// A client that presents a certificate, over HTTP/2 only: Apple's push
/// service authenticates a pass type's pushes by its certificate.
pub fn client_with_certificate(
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> anyhow::Result<HttpsClient> {
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .context("TLS versions")?
        .with_root_certificates(roots())
        .with_client_auth_cert(chain, key)
        .context("the client certificate and its key")?;
    let https = HttpsConnectorBuilder::new()
        .with_tls_config(config)
        .https_only()
        .enable_http2()
        .build();
    Ok(Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build(https))
}

/// A server's answer, read whole.
#[derive(Debug)]
pub struct Answer {
    pub status: StatusCode,
    pub body: Bytes,
}

/// Sends `request` and reads the answer, all within `timeout`. A body longer
/// than 64 KiB is cut short: nothing these servers send back is that long.
pub async fn send(
    client: &HttpsClient,
    request: Request<Full<Bytes>>,
    timeout: Duration,
) -> anyhow::Result<Answer> {
    let exchange = async {
        let response = client.request(request).await?;
        let status = response.status();
        let body = http_body_util::Limited::new(response.into_body(), 64 * 1024)
            .collect()
            .await
            .map_err(|error| anyhow::anyhow!("reading the answer: {error}"))?
            .to_bytes();
        Ok::<_, anyhow::Error>(Answer { status, body })
    };
    tokio::time::timeout(timeout, exchange)
        .await
        .map_err(|_| anyhow::anyhow!("no answer within {timeout:?}"))?
}
