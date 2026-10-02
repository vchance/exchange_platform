//! What a deployment tells the processes, read from the environment (and
//! `.env` in development). `.env.example` documents every setting.
//!
//! Two settings have no default on purpose: `CODE_DELIVERY` and
//! `NOTIFICATION_DELIVERY`. A deployment must say how messages reach people,
//! so the development delivery, which writes them to a log, is never used by
//! accident.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use axum::http::HeaderName;

use crate::auth::{CodeSender, LogSender};
use crate::http::TrustedProxies;
use crate::notifications::smtp::{Secret, SmtpSender, SmtpSettings, TlsMode};
use crate::notifications::wording::Wording;
use crate::notifications::{EmailSender, LogEmailSender};

/// Reads a setting. The environment in production, a table in the tests.
type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

fn load_env() {
    dotenvy::dotenv().ok();
}

fn environment(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// A setting that may be left out. One set to nothing counts as left out, so
/// a line in `.env` can show a setting without choosing a value for it.
fn optional(get: Lookup<'_>, name: &str) -> Option<String> {
    get(name).filter(|value| !value.trim().is_empty())
}

fn required(get: Lookup<'_>, name: &str) -> anyhow::Result<String> {
    optional(get, name).with_context(|| format!("{name} is not set"))
}

/// Where the web app is served from, without a trailing slash.
fn web_origin(get: Lookup<'_>) -> anyhow::Result<String> {
    Ok(required(get, "WEB_ORIGIN")?
        .trim_end_matches('/')
        .to_owned())
}

/// How long one SMTP conversation may be silent. Within the outbox's own
/// limit on a send, so that it is this timeout, with its clearer error, that
/// a notification usually fails by.
const SMTP_TIMEOUT: Duration = Duration::from_secs(20);

/// The SMTP server, from `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS`,
/// `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM`. The password is read
/// into a type that cannot be printed.
fn smtp_settings(get: Lookup<'_>) -> anyhow::Result<SmtpSettings> {
    let tls = match optional(get, "SMTP_TLS") {
        None => TlsMode::Tls,
        Some(value) => TlsMode::parse(&value).with_context(|| {
            format!("SMTP_TLS={value} is not one of `tls`, `starttls` or `none`")
        })?,
    };
    let port = match optional(get, "SMTP_PORT") {
        None => tls.default_port(),
        Some(value) => value
            .parse()
            .with_context(|| format!("SMTP_PORT={value} is not a port number"))?,
    };
    let credentials = match (
        optional(get, "SMTP_USERNAME"),
        optional(get, "SMTP_PASSWORD"),
    ) {
        (Some(username), Some(password)) => Some((username, Secret::new(password))),
        (None, None) => None,
        _ => bail!("SMTP_USERNAME and SMTP_PASSWORD must be set together, or neither"),
    };
    Ok(SmtpSettings {
        host: required(get, "SMTP_HOST")?,
        port,
        tls,
        credentials,
        from: required(get, "SMTP_FROM")?,
        timeout: SMTP_TIMEOUT,
    })
}

fn smtp_sender(get: Lookup<'_>) -> anyhow::Result<Arc<SmtpSender>> {
    // The wording is checked now, so a message that cannot be written stops
    // the process from starting instead of failing one send at a time.
    let sender = SmtpSender::new(smtp_settings(get)?, Wording::embedded()?)?;
    Ok(Arc::new(sender))
}

/// The sender named by `NOTIFICATION_DELIVERY`.
fn email_sender(get: Lookup<'_>) -> anyhow::Result<Arc<dyn EmailSender>> {
    match required(get, "NOTIFICATION_DELIVERY")?.as_str() {
        "log" => Ok(Arc::new(LogEmailSender)),
        "smtp" => Ok(smtp_sender(get)?),
        other => bail!("NOTIFICATION_DELIVERY={other} is not supported; use `smtp` or `log`"),
    }
}

/// The sender named by `CODE_DELIVERY`.
fn code_sender(get: Lookup<'_>) -> anyhow::Result<Arc<dyn CodeSender>> {
    match required(get, "CODE_DELIVERY")?.as_str() {
        "log" => Ok(Arc::new(LogSender)),
        "smtp" => Ok(smtp_sender(get)?),
        other => bail!("CODE_DELIVERY={other} is not supported; use `smtp` or `log`"),
    }
}

/// Which proxy header to believe, from `TRUSTED_PROXY_HEADER` and
/// `TRUSTED_PROXIES`. The default is none: the connection's peer is the
/// client.
fn trusted_proxies(get: Lookup<'_>) -> anyhow::Result<TrustedProxies> {
    let header = optional(get, "TRUSTED_PROXY_HEADER");
    let count = optional(get, "TRUSTED_PROXIES");
    match (header, count) {
        (None, None) => Ok(TrustedProxies::none()),
        (None, Some(_)) => bail!("TRUSTED_PROXIES is set but TRUSTED_PROXY_HEADER names no header"),
        (Some(header), count) => {
            let name = HeaderName::try_from(header.trim().to_ascii_lowercase())
                .with_context(|| format!("TRUSTED_PROXY_HEADER={header} is not a header name"))?;
            let count: usize = match count {
                None => 1,
                Some(value) => value
                    .trim()
                    .parse()
                    .ok()
                    .filter(|count| *count >= 1)
                    .with_context(|| {
                        format!("TRUSTED_PROXIES={value} is not a count of 1 or more")
                    })?,
            };
            Ok(TrustedProxies::behind(name, count))
        }
    }
}

/// Configuration for the `worker` process.
pub struct WorkerConfig {
    /// The connection string for the restricted application role.
    pub database_url: String,
    /// Notifications link into the web app.
    pub web_origin: String,
    pub email_sender: Arc<dyn EmailSender>,
}

impl WorkerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        load_env();
        let get: Lookup<'_> = &environment;
        Ok(Self {
            database_url: required(get, "DATABASE_URL")?,
            web_origin: web_origin(get)?,
            email_sender: email_sender(get)?,
        })
    }
}

/// Configuration for the `api` process.
pub struct ApiConfig {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Keys the hashes of one-time codes. At least 32 bytes.
    pub app_secret: Vec<u8>,
    /// Where the web app is served from, such as `https://app.example.com`.
    /// Cookie sessions are only honored for requests from this origin.
    pub web_origin: String,
    pub code_sender: Arc<dyn CodeSender>,
    /// The built web app to serve alongside the API, if any (`WEB_DIR`).
    pub web_dir: Option<PathBuf>,
    pub proxies: TrustedProxies,
}

impl ApiConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        load_env();
        let get: Lookup<'_> = &environment;

        let bind_addr = get("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:8080".to_owned())
            .parse()
            .context("BIND_ADDR is not a valid socket address")?;

        let app_secret = required(get, "APP_SECRET")?.into_bytes();
        if app_secret.len() < 32 {
            bail!("APP_SECRET must be at least 32 bytes");
        }

        Ok(Self {
            database_url: required(get, "DATABASE_URL")?,
            bind_addr,
            app_secret,
            web_origin: web_origin(get)?,
            code_sender: code_sender(get)?,
            web_dir: optional(get, "WEB_DIR").map(PathBuf::from),
            proxies: trusted_proxies(get)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn table(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn lookup(table: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> {
        move |name| table.get(name).cloned()
    }

    const SMTP: &[(&str, &str)] = &[
        ("SMTP_HOST", "smtp.example.test"),
        ("SMTP_FROM", "Exchange <no-reply@example.test>"),
    ];

    #[test]
    fn a_delivery_must_be_named_and_must_be_one_that_exists() {
        let none = table(&[]);
        assert!(email_sender(&lookup(&none)).is_err(), "no default");
        assert!(code_sender(&lookup(&none)).is_err(), "no default");
        for other in ["", "LOG", "ses", "sms"] {
            let table = table(&[("NOTIFICATION_DELIVERY", other), ("CODE_DELIVERY", other)]);
            assert!(email_sender(&lookup(&table)).is_err(), "{other:?}");
            assert!(code_sender(&lookup(&table)).is_err(), "{other:?}");
        }
        let log = table(&[("NOTIFICATION_DELIVERY", "log"), ("CODE_DELIVERY", "log")]);
        assert!(email_sender(&lookup(&log)).is_ok());
        assert!(code_sender(&lookup(&log)).is_ok());
    }

    #[test]
    fn smtp_delivery_needs_a_host_and_a_from_address() {
        let mut pairs = vec![("NOTIFICATION_DELIVERY", "smtp"), ("CODE_DELIVERY", "smtp")];
        let bare = table(&pairs);
        assert!(email_sender(&lookup(&bare)).is_err());
        assert!(code_sender(&lookup(&bare)).is_err());
        pairs.extend_from_slice(SMTP);
        let whole = table(&pairs);
        assert!(email_sender(&lookup(&whole)).is_ok());
        assert!(code_sender(&lookup(&whole)).is_ok());
    }

    #[test]
    fn the_tls_mode_picks_the_port_unless_one_is_given() {
        let settings = |extra: &[(&str, &str)]| {
            let table = table(&[SMTP, extra].concat());
            smtp_settings(&lookup(&table))
        };
        let default = settings(&[]).unwrap();
        assert_eq!((default.tls, default.port), (TlsMode::Tls, 465));
        let starttls = settings(&[("SMTP_TLS", "starttls")]).unwrap();
        assert_eq!((starttls.tls, starttls.port), (TlsMode::StartTls, 587));
        let local = settings(&[("SMTP_TLS", "none"), ("SMTP_PORT", "2525")]).unwrap();
        assert_eq!((local.tls, local.port), (TlsMode::None, 2525));
        assert!(settings(&[("SMTP_TLS", "ssl")]).is_err());
        assert!(settings(&[("SMTP_PORT", "smtp")]).is_err());
    }

    #[test]
    fn smtp_credentials_come_as_a_pair_and_the_password_is_not_printable() {
        let settings = |extra: &[(&str, &str)]| {
            let table = table(&[SMTP, extra].concat());
            smtp_settings(&lookup(&table))
        };
        assert_eq!(settings(&[]).unwrap().credentials, None);
        assert!(settings(&[("SMTP_USERNAME", "u")]).is_err());
        assert!(settings(&[("SMTP_PASSWORD", "p")]).is_err());
        let both = settings(&[("SMTP_USERNAME", "u"), ("SMTP_PASSWORD", "hunter2")]).unwrap();
        assert_eq!(
            both.credentials,
            Some(("u".to_owned(), Secret::new("hunter2".to_owned())))
        );
        assert!(!format!("{both:?}").contains("hunter2"));
    }

    #[test]
    fn no_proxy_header_is_trusted_unless_one_is_named() {
        let none = table(&[]);
        assert_eq!(
            trusted_proxies(&lookup(&none)).unwrap(),
            TrustedProxies::none()
        );
        let blank = table(&[("TRUSTED_PROXY_HEADER", ""), ("TRUSTED_PROXIES", "")]);
        assert_eq!(
            trusted_proxies(&lookup(&blank)).unwrap(),
            TrustedProxies::none()
        );

        let one = table(&[("TRUSTED_PROXY_HEADER", "X-Forwarded-For")]);
        assert_eq!(
            trusted_proxies(&lookup(&one)).unwrap(),
            TrustedProxies::behind(HeaderName::from_static("x-forwarded-for"), 1)
        );
        let two = table(&[
            ("TRUSTED_PROXY_HEADER", "X-Forwarded-For"),
            ("TRUSTED_PROXIES", "2"),
        ]);
        assert_eq!(
            trusted_proxies(&lookup(&two)).unwrap(),
            TrustedProxies::behind(HeaderName::from_static("x-forwarded-for"), 2)
        );

        for wrong in [
            table(&[("TRUSTED_PROXIES", "2")]),
            table(&[
                ("TRUSTED_PROXY_HEADER", "X-Forwarded-For"),
                ("TRUSTED_PROXIES", "0"),
            ]),
            table(&[
                ("TRUSTED_PROXY_HEADER", "X-Forwarded-For"),
                ("TRUSTED_PROXIES", "two"),
            ]),
            table(&[("TRUSTED_PROXY_HEADER", "not a header")]),
        ] {
            assert!(trusted_proxies(&lookup(&wrong)).is_err(), "{wrong:?}");
        }
    }
}
