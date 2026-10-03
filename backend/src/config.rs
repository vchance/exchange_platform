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

use crate::auth::{AuthRules, CodeSender, LogSender};
use crate::client_version::{MinimumClientVersions, parse_version};
use crate::http::web::DEFAULT_ANDROID_PACKAGE;
use crate::http::{AppLinks, TrustedProxies};
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
    let host = required(get, "SMTP_HOST")?;
    if tls == TlsMode::None {
        refuse_plaintext_remote(get, &host, credentials.is_some())?;
    }
    Ok(SmtpSettings {
        host,
        port,
        tls,
        credentials,
        from: required(get, "SMTP_FROM")?,
        timeout: SMTP_TIMEOUT,
    })
}

/// `SMTP_TLS=none` is for a relay on this host: anything said to a server
/// across a network, a password first of all, could be read on the way.
/// Refused for any other host, or with credentials, unless
/// `SMTP_ALLOW_PLAINTEXT_REMOTE=true` says it is a test server.
fn refuse_plaintext_remote(get: Lookup<'_>, host: &str, credentials: bool) -> anyhow::Result<()> {
    match optional(get, "SMTP_ALLOW_PLAINTEXT_REMOTE").as_deref() {
        Some("true") => return Ok(()),
        None | Some("false") => {}
        Some(other) => bail!("SMTP_ALLOW_PLAINTEXT_REMOTE={other} is not `true` or `false`"),
    }
    let host = host.trim().trim_end_matches('.');
    let unbracketed = host.trim_start_matches('[').trim_end_matches(']');
    let loopback = host.eq_ignore_ascii_case("localhost")
        || unbracketed
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if !loopback {
        bail!(
            "SMTP_TLS=none sends everything in plain text, so SMTP_HOST must be this host \
             (localhost, 127.0.0.1 or ::1); set SMTP_ALLOW_PLAINTEXT_REMOTE=true for a test server"
        );
    }
    if credentials {
        bail!(
            "SMTP_TLS=none would send SMTP_USERNAME and SMTP_PASSWORD in plain text; \
             set SMTP_ALLOW_PLAINTEXT_REMOTE=true for a test server"
        );
    }
    Ok(())
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
            // `Forwarded` (RFC 7239) is written `for=192.0.2.7;proto=https`,
            // which is not read here: every request would fall back to the
            // proxy's address and share one set of limits.
            if name == "forwarded" {
                bail!(
                    "TRUSTED_PROXY_HEADER={header}: the RFC 7239 Forwarded header is not \
                     supported; name a header that holds plain addresses, such as \
                     X-Forwarded-For or the proxy's own (CF-Connecting-IP, Fly-Client-IP)"
                );
            }
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

/// Where to serve metrics, from `METRICS_ADDR`. Default none: no metrics
/// listener at all. Never the public port: a listener of its own, which a
/// deployment exposes only to whatever collects the metrics.
fn metrics_addr(get: Lookup<'_>) -> anyhow::Result<Option<SocketAddr>> {
    optional(get, "METRICS_ADDR")
        .map(|value| {
            value.trim().parse().with_context(|| {
                format!("METRICS_ADDR={value} is not an address and port such as 0.0.0.0:9100")
            })
        })
        .transpose()
}

/// An optional limit: unset or empty means `default`, and anything else must
/// be a whole number of 1 or more. Zero is refused rather than read as "no
/// limit", which would turn a limit off by a slip; a deployment that wants
/// one out of the way sets it high.
fn limit(get: Lookup<'_>, name: &str, default: i64) -> anyhow::Result<i64> {
    match optional(get, name) {
        None => Ok(default),
        Some(value) => value
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|limit| *limit >= 1)
            .map(i64::from)
            .with_context(|| {
                format!(
                    "{name}={value} is not a whole number from 1 to {}",
                    i32::MAX
                )
            }),
    }
}

/// The rules for one-time codes, with the limits on sign-in that a
/// deployment may set (`SIGN_IN_CODE_REQUESTS_PER_ADDRESS_PER_HOUR`,
/// `SIGN_IN_FAILED_GUESSES_PER_IDENTIFIER_PER_DAY`). Each defaults to the
/// placeholder in [`AuthRules::default`].
///
/// `SIGN_IN_FAILED_GUESSES_PER_ADDRESS_PER_HOUR` was once a setting here.
/// Wrong guesses are no longer limited by address (`crate::auth::verify_code`
/// says why), so a deployment still setting it is refused at start rather
/// than left believing it bounds something.
fn auth_rules(get: Lookup<'_>) -> anyhow::Result<AuthRules> {
    const RETIRED: &str = "SIGN_IN_FAILED_GUESSES_PER_ADDRESS_PER_HOUR";
    if optional(get, RETIRED).is_some() {
        anyhow::bail!(
            "{RETIRED} is no longer a setting: wrong codes are not limited by address. \
             Remove it; see \"Signing in\" in README.md for what bounds guessing"
        );
    }
    let defaults = AuthRules::default();
    Ok(AuthRules {
        code_requests_per_address_per_hour: limit(
            get,
            "SIGN_IN_CODE_REQUESTS_PER_ADDRESS_PER_HOUR",
            defaults.code_requests_per_address_per_hour,
        )?,
        failed_guesses_per_identifier_per_day: limit(
            get,
            "SIGN_IN_FAILED_GUESSES_PER_IDENTIFIER_PER_DAY",
            defaults.failed_guesses_per_identifier_per_day,
        )?,
        ..defaults
    })
}

/// Configuration for the `worker` process.
pub struct WorkerConfig {
    /// The connection string for the restricted application role.
    pub database_url: String,
    /// Notifications link into the web app.
    pub web_origin: String,
    pub email_sender: Arc<dyn EmailSender>,
    /// Where to serve the worker's metrics, if anywhere.
    pub metrics_addr: Option<SocketAddr>,
    /// Wallet passes, for the platforms configured (`crate::wallet::config`).
    pub wallet: crate::wallet::WalletConfig,
}

impl WorkerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        load_env();
        let get: Lookup<'_> = &environment;
        Ok(Self {
            database_url: required(get, "DATABASE_URL")?,
            web_origin: web_origin(get)?,
            email_sender: email_sender(get)?,
            metrics_addr: metrics_addr(get)?,
            wallet: crate::wallet::WalletConfig::from_lookup(get)?,
        })
    }
}

/// An optional `MIN_CLIENT_VERSION_*` value: unset or empty means no minimum,
/// and anything else must read as a version.
fn min_client_version(get: Lookup<'_>, name: &str) -> anyhow::Result<Option<String>> {
    match optional(get, name) {
        None => Ok(None),
        Some(value) => {
            let value = value.trim();
            if parse_version(value).is_none() {
                bail!("{name}={value} is not a version such as 1.4.0");
            }
            Ok(Some(value.to_owned()))
        }
    }
}

/// The oldest build of each client that may still change anything
/// (`crate::client_version`). Nothing is required unless a deployment says so.
fn min_client_versions(get: Lookup<'_>) -> anyhow::Result<MinimumClientVersions> {
    Ok(MinimumClientVersions {
        web: min_client_version(get, "MIN_CLIENT_VERSION_WEB")?,
        ios: min_client_version(get, "MIN_CLIENT_VERSION_IOS")?,
        android: min_client_version(get, "MIN_CLIENT_VERSION_ANDROID")?,
    })
}

/// A comma-separated setting as its non-empty items, trimmed.
fn list(get: Lookup<'_>, name: &str) -> Vec<String> {
    optional(get, name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Which apps may open the web origin's invitation links, from
/// `APPLE_APP_ID`, `ANDROID_SHA256_CERT_FINGERPRINTS` and `ANDROID_PACKAGE`
/// (`crate::http::web::AppLinks`). Nothing unless they are set.
fn app_links(get: Lookup<'_>) -> anyhow::Result<AppLinks> {
    let package =
        optional(get, "ANDROID_PACKAGE").unwrap_or_else(|| DEFAULT_ANDROID_PACKAGE.to_owned());
    AppLinks::new(
        &list(get, "APPLE_APP_ID"),
        package.trim(),
        &list(get, "ANDROID_SHA256_CERT_FINGERPRINTS"),
    )
    .map_err(|error| anyhow::anyhow!(error))
    .context("APPLE_APP_ID, ANDROID_SHA256_CERT_FINGERPRINTS or ANDROID_PACKAGE")
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
    pub min_client_versions: MinimumClientVersions,
    /// Which apps may open the web origin's invitation links, if any.
    pub app_links: AppLinks,
    /// Where to serve the API's metrics, if anywhere. Never `bind_addr`.
    pub metrics_addr: Option<SocketAddr>,
    /// The rules for one-time codes, some of them set by the deployment.
    pub auth: AuthRules,
    /// Wallet passes, for the platforms configured (`crate::wallet::config`).
    pub wallet: crate::wallet::WalletConfig,
}

impl ApiConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        load_env();
        let get: Lookup<'_> = &environment;

        let bind_addr: SocketAddr = get("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:8080".to_owned())
            .parse()
            .context("BIND_ADDR is not a valid socket address")?;

        let app_secret = required(get, "APP_SECRET")?.into_bytes();
        if app_secret.len() < 32 {
            bail!("APP_SECRET must be at least 32 bytes");
        }

        let metrics_addr = metrics_addr(get)?;
        if metrics_addr.is_some_and(|metrics| metrics.port() == bind_addr.port()) {
            bail!("METRICS_ADDR must use a port of its own, not BIND_ADDR's");
        }

        Ok(Self {
            metrics_addr,
            database_url: required(get, "DATABASE_URL")?,
            bind_addr,
            app_secret,
            web_origin: web_origin(get)?,
            code_sender: code_sender(get)?,
            web_dir: optional(get, "WEB_DIR").map(PathBuf::from),
            proxies: trusted_proxies(get)?,
            min_client_versions: min_client_versions(get)?,
            app_links: app_links(get)?,
            auth: auth_rules(get)?,
            wallet: crate::wallet::WalletConfig::from_lookup(get)?,
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
        ("SMTP_FROM", "Yuppers <no-reply@example.test>"),
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
        let local = settings(&[
            ("SMTP_TLS", "none"),
            ("SMTP_PORT", "2525"),
            ("SMTP_HOST", "localhost"),
        ])
        .unwrap();
        assert_eq!((local.tls, local.port), (TlsMode::None, 2525));
        assert!(settings(&[("SMTP_TLS", "ssl")]).is_err());
        assert!(settings(&[("SMTP_PORT", "smtp")]).is_err());
    }

    #[test]
    fn plain_text_smtp_is_for_this_host_only_without_a_password() {
        let settings = |extra: &[(&str, &str)]| {
            let table = table(&[SMTP, &[("SMTP_TLS", "none")], extra].concat());
            smtp_settings(&lookup(&table))
        };
        for host in [
            "localhost",
            "LOCALHOST.",
            "127.0.0.1",
            "127.8.9.10",
            "::1",
            "[::1]",
        ] {
            assert!(settings(&[("SMTP_HOST", host)]).is_ok(), "{host}");
        }
        for host in [
            "smtp.example.test",
            "10.0.0.5",
            "::2",
            "localhost.example.test",
        ] {
            assert!(settings(&[("SMTP_HOST", host)]).is_err(), "{host}");
        }
        let password = [
            ("SMTP_HOST", "127.0.0.1"),
            ("SMTP_USERNAME", "u"),
            ("SMTP_PASSWORD", "hunter2"),
        ];
        let refused = settings(&password).unwrap_err();
        assert!(!format!("{refused:#}").contains("hunter2"));

        // Said outright, for a test server.
        let allow = ("SMTP_ALLOW_PLAINTEXT_REMOTE", "true");
        assert!(settings(&[allow]).is_ok());
        assert!(settings(&[&password[..], &[allow]].concat()).is_ok());
        assert!(settings(&[("SMTP_ALLOW_PLAINTEXT_REMOTE", "false")]).is_err());
        assert!(settings(&[("SMTP_ALLOW_PLAINTEXT_REMOTE", "yes")]).is_err());

        // Encrypted connections are not affected.
        let table = table(&[SMTP, &password[1..]].concat());
        assert!(smtp_settings(&lookup(&table)).is_ok());
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
            table(&[("TRUSTED_PROXY_HEADER", "Forwarded")]),
            table(&[("TRUSTED_PROXY_HEADER", " forwarded ")]),
        ] {
            assert!(trusted_proxies(&lookup(&wrong)).is_err(), "{wrong:?}");
        }
    }

    #[test]
    fn metrics_are_off_unless_an_address_is_given() {
        let read = |value: &str| metrics_addr(&lookup(&table(&[("METRICS_ADDR", value)])));
        assert_eq!(metrics_addr(&lookup(&table(&[]))).unwrap(), None);
        assert_eq!(read(" ").unwrap(), None);
        assert_eq!(
            read("0.0.0.0:9100").unwrap(),
            Some("0.0.0.0:9100".parse().unwrap())
        );
        assert!(read("9100").is_err());
        assert!(read("localhost:9100").is_err());
    }

    #[test]
    fn the_sign_in_limits_default_to_the_placeholders_and_may_be_raised_or_lowered() {
        let defaults = AuthRules::default();
        for unset in [
            table(&[]),
            table(&[
                ("SIGN_IN_CODE_REQUESTS_PER_ADDRESS_PER_HOUR", ""),
                ("SIGN_IN_FAILED_GUESSES_PER_ADDRESS_PER_HOUR", " "),
                ("SIGN_IN_FAILED_GUESSES_PER_IDENTIFIER_PER_DAY", ""),
            ]),
        ] {
            let rules = auth_rules(&lookup(&unset)).unwrap();
            assert_eq!(
                (
                    rules.code_requests_per_address_per_hour,
                    rules.failed_guesses_per_identifier_per_day,
                ),
                (
                    defaults.code_requests_per_address_per_hour,
                    defaults.failed_guesses_per_identifier_per_day,
                ),
            );
        }

        let set = table(&[
            ("SIGN_IN_CODE_REQUESTS_PER_ADDRESS_PER_HOUR", "10000"),
            ("SIGN_IN_FAILED_GUESSES_PER_IDENTIFIER_PER_DAY", " 3 "),
        ]);
        let rules = auth_rules(&lookup(&set)).unwrap();
        assert_eq!(rules.code_requests_per_address_per_hour, 10_000);
        assert_eq!(rules.failed_guesses_per_identifier_per_day, 3);
        // Nothing else is a setting.
        assert_eq!(rules.codes_per_hour, defaults.codes_per_hour);
        assert_eq!(
            rules.failed_deletion_guesses_per_day,
            defaults.failed_deletion_guesses_per_day
        );
        assert_eq!(
            rules.deletion_codes_per_hour,
            defaults.deletion_codes_per_hour
        );
    }

    #[test]
    fn a_sign_in_limit_is_a_count_of_one_or_more_and_zero_is_not_no_limit() {
        for name in [
            "SIGN_IN_CODE_REQUESTS_PER_ADDRESS_PER_HOUR",
            "SIGN_IN_FAILED_GUESSES_PER_IDENTIFIER_PER_DAY",
        ] {
            for wrong in ["0", "-1", "ten", "1.5", "2147483648", "unlimited"] {
                let settings = table(&[(name, wrong)]);
                assert!(auth_rules(&lookup(&settings)).is_err(), "{name}={wrong}");
            }
            let most = table(&[(name, "2147483647")]);
            assert!(auth_rules(&lookup(&most)).is_ok(), "{name}");
        }
    }

    #[test]
    fn the_retired_limit_on_wrong_codes_by_address_is_refused_if_set() {
        let name = "SIGN_IN_FAILED_GUESSES_PER_ADDRESS_PER_HOUR";
        // Empty is the same as unset, as for every optional setting.
        assert!(auth_rules(&lookup(&table(&[(name, " ")]))).is_ok());
        for value in ["30", "1000000"] {
            let error = auth_rules(&lookup(&table(&[(name, value)])))
                .err()
                .unwrap_or_else(|| panic!("{name}={value} was accepted"));
            assert!(error.to_string().contains(name), "{error}");
        }
    }

    #[test]
    fn a_minimum_client_version_is_optional_but_must_be_a_version() {
        let name = "MIN_CLIENT_VERSION_WEB";
        let read = |value: Option<&str>| {
            let settings = match value {
                Some(value) => table(&[(name, value)]),
                None => table(&[]),
            };
            min_client_version(&lookup(&settings), name)
        };
        assert_eq!(read(None).unwrap(), None);
        assert_eq!(read(Some(" ")).unwrap(), None);
        assert_eq!(read(Some(" 1.4.0 ")).unwrap().as_deref(), Some("1.4.0"));
        assert!(read(Some("v1")).is_err());
    }

    #[test]
    fn app_links_are_off_unless_named_and_a_wrong_value_stops_the_start() {
        let read = |pairs: &[(&str, &str)]| app_links(&lookup(&table(pairs)));
        assert!(read(&[]).unwrap().served().is_empty());
        assert!(
            read(&[
                ("APPLE_APP_ID", " "),
                ("ANDROID_SHA256_CERT_FINGERPRINTS", "")
            ])
            .unwrap()
            .served()
            .is_empty()
        );

        let fingerprint = ["AB"; 32].join(":");
        let both = read(&[
            (
                "APPLE_APP_ID",
                "ABCDE12345.app.yuppers, VWXYZ67890.app.yuppers",
            ),
            ("ANDROID_SHA256_CERT_FINGERPRINTS", &fingerprint),
        ])
        .unwrap();
        assert_eq!(both.served().len(), 2);

        assert!(read(&[("APPLE_APP_ID", "app.yuppers")]).is_err());
        assert!(read(&[("ANDROID_SHA256_CERT_FINGERPRINTS", "AB:CD")]).is_err());
        assert!(
            read(&[
                ("ANDROID_SHA256_CERT_FINGERPRINTS", &fingerprint),
                ("ANDROID_PACKAGE", "yuppers"),
            ])
            .is_err()
        );
    }
}
