//! Delivery by SMTP: the email adapter at the edge (DESIGN.md §12, §13).
//!
//! Nearly every email provider offers SMTP, so a deployment supplies a host
//! and credentials, not code. One sender serves both notifications
//! ([`EmailSender`]) and one-time codes ([`CodeSender`]); codes go by email
//! only, since SMS delivery is not built.
//!
//! What this file never does: write the password anywhere, or put anything in
//! a message that the wording did not give it.

use std::fmt;
use std::time::Duration;

use anyhow::Context;
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use super::wording::Wording;
use super::{Email, EmailSender};
use crate::auth::{CodeMessage, CodeSender, SendFuture};
use crate::domain::identity::Identifier;

/// How the connection to the server is protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsMode {
    /// TLS from the first byte (SMTPS, usually port 465). The default.
    Tls,
    /// A plain connection that must upgrade with `STARTTLS` before anything
    /// else is said (usually port 587). A server that cannot is refused.
    StartTls,
    /// Plain text throughout. For a relay on the same host or a test server,
    /// never for a server reached over a network.
    None,
}

impl TlsMode {
    /// Reads the setting's value: `tls`, `starttls` or `none`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tls" => Some(Self::Tls),
            "starttls" => Some(Self::StartTls),
            "none" => Some(Self::None),
            _ => Option::None,
        }
    }

    /// The port the mode is usually served on.
    pub fn default_port(self) -> u16 {
        match self {
            Self::Tls => 465,
            Self::StartTls => 587,
            Self::None => 25,
        }
    }
}

/// A value that must not appear in logs or error messages. Its `Debug` form
/// says only that it exists.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Where and how to send.
#[derive(Clone, Debug)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub tls: TlsMode,
    /// A username and password, if the server wants them.
    pub credentials: Option<(String, Secret)>,
    /// The sender address, with an optional name: `Exchange <no-reply@example.com>`.
    pub from: String,
    /// How long one connection may be silent before the send fails.
    pub timeout: Duration,
}

/// Sends through one SMTP server.
pub struct SmtpSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    wording: Wording,
}

impl SmtpSender {
    pub fn new(settings: SmtpSettings, wording: Wording) -> anyhow::Result<Self> {
        let from: Mailbox = settings
            .from
            .parse()
            .with_context(|| format!("SMTP_FROM {:?} is not a mailbox", settings.from))?;
        let tls = match settings.tls {
            TlsMode::None => Tls::None,
            mode => {
                let parameters = TlsParameters::new(settings.host.clone())
                    .context("TLS cannot be set up for the SMTP server")?;
                match mode {
                    TlsMode::Tls => Tls::Wrapper(parameters),
                    _ => Tls::Required(parameters),
                }
            }
        };
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(settings.host)
            .port(settings.port)
            .tls(tls)
            .timeout(Some(settings.timeout));
        if let Some((username, Secret(password))) = settings.credentials {
            builder = builder.credentials(Credentials::new(username, password));
        }
        Ok(Self {
            transport: builder.build(),
            from,
            wording,
        })
    }

    async fn deliver(
        &self,
        to: &str,
        subject: &str,
        body: &str,
        id: Option<String>,
    ) -> anyhow::Result<()> {
        let to: Address = to
            .parse()
            .with_context(|| format!("{to:?} is not an address SMTP can deliver to"))?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(Mailbox::new(None, to))
            .subject(subject)
            .message_id(id)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_owned())
            .context("the message could not be built")?;
        self.transport
            .send(message)
            .await
            .context("the SMTP server did not take the message")?;
        Ok(())
    }
}

impl EmailSender for SmtpSender {
    fn send<'a>(&'a self, email: &'a Email) -> SendFuture<'a> {
        Box::pin(async move {
            // The same message sent again after a crash carries the same ID,
            // so a mailbox can tell it is a repeat.
            let id = format!("<outbox-{}@{}>", email.reference, self.from.email.domain());
            self.deliver(&email.to, &email.subject, &email.body, Some(id))
                .await
        })
    }
}

impl CodeSender for SmtpSender {
    fn send<'a>(&'a self, message: CodeMessage<'a>) -> SendFuture<'a> {
        Box::pin(async move {
            let Identifier::Email(to) = message.to else {
                // Refused, not logged: the code is still a code.
                anyhow::bail!("codes can be sent by email only; SMS delivery is not built");
            };
            let rendered = self
                .wording
                .code_email(message.language, message.purpose, message.code);
            self.deliver(to, &rendered.subject, &rendered.body, None)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tls_mode_is_one_of_three_words() {
        assert_eq!(TlsMode::parse("tls"), Some(TlsMode::Tls));
        assert_eq!(TlsMode::parse("starttls"), Some(TlsMode::StartTls));
        assert_eq!(TlsMode::parse("none"), Some(TlsMode::None));
        for other in ["", "TLS", "ssl", "opportunistic"] {
            assert_eq!(TlsMode::parse(other), Option::None, "{other:?}");
        }
        assert_eq!(TlsMode::Tls.default_port(), 465);
        assert_eq!(TlsMode::StartTls.default_port(), 587);
    }

    #[test]
    fn the_password_is_not_in_the_settings_debug_output() {
        let settings = SmtpSettings {
            host: "smtp.example.test".to_owned(),
            port: 465,
            tls: TlsMode::Tls,
            credentials: Some(("user".to_owned(), Secret::new("hunter2".to_owned()))),
            from: "Exchange <no-reply@example.test>".to_owned(),
            timeout: Duration::from_secs(30),
        };
        let shown = format!("{settings:?}");
        assert!(shown.contains("smtp.example.test") && shown.contains("user"));
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[test]
    fn the_from_address_must_be_a_mailbox() {
        let settings = |from: &str| SmtpSettings {
            host: "localhost".to_owned(),
            port: 2525,
            tls: TlsMode::None,
            credentials: Option::None,
            from: from.to_owned(),
            timeout: Duration::from_secs(1),
        };
        let wording = || Wording::embedded().unwrap();
        assert!(SmtpSender::new(settings("not an address"), wording()).is_err());
        assert!(SmtpSender::new(settings("no-reply@example.test"), wording()).is_ok());
        assert!(SmtpSender::new(settings("Exchange <no-reply@example.test>"), wording()).is_ok());
    }
}
