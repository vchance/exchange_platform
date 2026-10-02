use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, bail};

use crate::auth::{CodeSender, LogSender};

fn load_env() {
    dotenvy::dotenv().ok();
}

fn required(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("{name} is not set"))
}

/// The connection string for the restricted application role. All the worker
/// needs for now.
pub fn database_url() -> anyhow::Result<String> {
    load_env();
    required("DATABASE_URL")
}

/// Configuration for the `api` process, read from the environment (and `.env`
/// in development).
pub struct ApiConfig {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Keys the hashes of one-time codes. At least 32 bytes.
    pub app_secret: Vec<u8>,
    /// Where the web app is served from, such as `https://app.example.com`.
    /// Cookie sessions are only honored for requests from this origin.
    pub web_origin: String,
    pub code_sender: Arc<dyn CodeSender>,
}

impl ApiConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        load_env();

        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
            .parse()
            .context("BIND_ADDR is not a valid socket address")?;

        let app_secret = required("APP_SECRET")?.into_bytes();
        if app_secret.len() < 32 {
            bail!("APP_SECRET must be at least 32 bytes");
        }

        let web_origin = required("WEB_ORIGIN")?.trim_end_matches('/').to_owned();

        // No default: a deployment must say how codes reach people, so the
        // development sender is never used by accident.
        let code_sender: Arc<dyn CodeSender> = match required("CODE_DELIVERY")?.as_str() {
            "log" => Arc::new(LogSender),
            other => {
                bail!("CODE_DELIVERY={other} is not supported; the only delivery so far is `log`")
            }
        };

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind_addr,
            app_secret,
            web_origin,
            code_sender,
        })
    }
}
