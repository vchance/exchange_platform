//! The text of what the service sends (DESIGN.md §4.2).
//!
//! None of it is written here. It comes from the wording files that the web
//! and mobile apps also read, one per language, all of them embedded at build
//! time. A language added to those files is spoken here without a change to
//! this code.

use std::collections::HashMap;

use serde::Deserialize;

use crate::auth::Purpose;
use crate::domain::notification::Notice;
use crate::languages;

// `WORDING_FILES`, written by the build script: every file in the wording
// directory, so that the set of languages is not spelled out in Rust.
include!(concat!(env!("OUT_DIR"), "/wording_files.rs"));

/// The parts of a wording file the service uses.
#[derive(Deserialize)]
struct File {
    #[serde(rename = "productName")]
    product_name: String,
    notifications: Notifications,
}

#[derive(Deserialize)]
struct Notifications {
    email: EmailWording,
    /// The email that carries a one-time code, one message per purpose, so
    /// the message says what the code does.
    #[serde(rename = "oneTimeCode")]
    one_time_code: CodeWording,
}

#[derive(Deserialize)]
struct CodeWording {
    #[serde(rename = "signIn")]
    sign_in: Message,
    #[serde(rename = "deleteAccount")]
    delete_account: Message,
}

impl CodeWording {
    fn for_purpose(&self, purpose: Purpose) -> &Message {
        match purpose {
            Purpose::SignIn => &self.sign_in,
            Purpose::DeleteAccount => &self.delete_account,
        }
    }
}

#[derive(Deserialize)]
struct EmailWording {
    /// Wraps every message: where the text goes, the link, and why the
    /// reader is getting it.
    layout: String,
    /// Keyed by `Notice::as_str`.
    messages: HashMap<String, Message>,
}

#[derive(Deserialize)]
struct Message {
    subject: String,
    body: String,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum WordingError {
    #[error("the default language `{0}` has no usable notification wording")]
    NoDefault(String),
    #[error("the default language `{language}` has no wording for {notice}")]
    Missing {
        language: String,
        notice: &'static str,
    },
    #[error("the email layout of the default language `{0}` must contain {{body}} and {{link}}")]
    Layout(String),
}

/// Where a message sends its reader. Both are pages of the web app that ask
/// the reader to sign in; neither lets anyone in by itself (invariant 1).
#[derive(Clone, Copy, Debug)]
pub struct Links<'a> {
    /// The exchange. Every message ends with it.
    pub exchange: &'a str,
    /// The exchange's record, laid out to read, print or download. Given in
    /// the messages that tell a signer their agreement is on record.
    pub record: &'a str,
}

/// A message in one language, with its variables filled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub subject: String,
    pub body: String,
}

/// Notification wording for every supported language that has it.
pub struct Wording {
    supported: &'static [String],
    default: &'static str,
    languages: HashMap<&'static str, File>,
}

impl Wording {
    /// The wording built into this binary. Fails if the default language
    /// cannot say everything, since it is what every other language falls
    /// back to.
    pub fn embedded() -> Result<Self, WordingError> {
        Self::from_files(languages::supported(), languages::default(), WORDING_FILES)
    }

    fn from_files(
        supported: &'static [String],
        default: &'static str,
        files: &[(&str, &str)],
    ) -> Result<Self, WordingError> {
        let mut languages: HashMap<&'static str, File> = supported
            .iter()
            .filter_map(|code| {
                let (_, text) = files.iter().find(|(name, _)| name == code)?;
                Some((code.as_str(), serde_json::from_str(text).ok()?))
            })
            .collect();
        // Without these a message would arrive empty, or with no way in.
        let whole = |file: &File| {
            let layout = &file.notifications.email.layout;
            layout.contains("{body}") && layout.contains("{link}")
        };

        let file = languages
            .get(default)
            .ok_or_else(|| WordingError::NoDefault(default.to_owned()))?;
        if !whole(file) {
            return Err(WordingError::Layout(default.to_owned()));
        }
        for notice in Notice::ALL {
            let messages = &file.notifications.email.messages;
            if !messages.contains_key(notice.as_str()) {
                return Err(WordingError::Missing {
                    language: default.to_owned(),
                    notice: notice.as_str(),
                });
            }
        }
        // Any other language whose file is missing or broken is left out, and
        // its readers get the default language. The wording check in the
        // TypeScript build is what refuses to ship one like that.
        languages.retain(|_, file| whole(file));

        Ok(Self {
            supported,
            default,
            languages,
        })
    }

    /// The email for a notice, in `language` (an account's preference) where
    /// that language has it and in the default language otherwise.
    ///
    /// `code` is the exchange's display code and `links` lead into it.
    /// Nothing else about the exchange can be put in: a message never carries
    /// what the parties agreed or wrote (DESIGN.md §12).
    pub fn email(&self, language: &str, notice: Notice, code: &str, links: Links<'_>) -> Rendered {
        let message_in = |language: &str| {
            let file = self.languages.get(language)?;
            let message = file.notifications.email.messages.get(notice.as_str())?;
            Some((file, message))
        };
        let (file, message) = languages::resolve_among(self.supported, language)
            .and_then(message_in)
            .or_else(|| message_in(self.default))
            .expect("the default language has every notice; checked when loading");

        let values = [
            ("productName", file.product_name.as_str()),
            ("code", code),
            ("link", links.exchange),
            ("recordLink", links.record),
        ];
        let text = fill(&message.body, &values);
        let mut with_text = values.to_vec();
        with_text.push(("body", &text));
        Rendered {
            subject: fill(&message.subject, &values),
            body: fill(&file.notifications.email.layout, &with_text),
        }
    }

    /// The email that carries a one-time code, in `language` where that
    /// language has wording and in the default language otherwise. The
    /// message says what the code is for (`crate::auth::Purpose`). It is not
    /// wrapped in the notification layout: there is no exchange to link to.
    pub fn code_email(&self, language: &str, purpose: Purpose, code: &str) -> Rendered {
        let file = languages::resolve_among(self.supported, language)
            .and_then(|language| self.languages.get(language))
            .or_else(|| self.languages.get(self.default))
            .expect("the default language has wording; checked when loading");
        let message = file.notifications.one_time_code.for_purpose(purpose);
        let values = [("productName", file.product_name.as_str()), ("code", code)];
        Rendered {
            subject: fill(&message.subject, &values),
            body: fill(&message.body, &values),
        }
    }
}

/// Replaces each `{name}` that has a value. What is put in is not read again,
/// so a value containing braces stays as it is.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let value = after.find('}').and_then(|end| {
            let (_, value) = values.iter().find(|(name, _)| *name == &after[..end])?;
            Some((end, *value))
        });
        match value {
            Some((end, value)) => {
                out.push_str(value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;

    const CODE: &str = "AB12-CD34";
    const LINK: &str = "https://app.test/exchanges/7";
    const RECORD: &str = "https://app.test/exchanges/7/record";
    const LINKS: Links<'static> = Links {
        exchange: LINK,
        record: RECORD,
    };

    #[test]
    fn every_supported_language_has_every_message_with_nothing_left_unfilled() {
        let wording = Wording::embedded().unwrap();
        for language in languages::supported() {
            let file = wording
                .languages
                .get(language.as_str())
                .unwrap_or_else(|| panic!("{language} has no notification wording"));
            for notice in Notice::ALL {
                assert!(
                    file.notifications
                        .email
                        .messages
                        .contains_key(notice.as_str()),
                    "{language} has no wording for {}",
                    notice.as_str()
                );
                let email = wording.email(language, notice, CODE, LINKS);
                for text in [&email.subject, &email.body] {
                    assert!(
                        !text.contains('{') && !text.contains('}'),
                        "{language} {}: an unknown variable in {text:?}",
                        notice.as_str()
                    );
                }
                assert!(!email.subject.trim().is_empty());
                assert!(email.body.contains(LINK), "every message links back");
                // An agreement coming into force is when each signer is told
                // where their copy is (DESIGN.md §14.1).
                let copy = matches!(notice, Notice::AgreementInForce | Notice::AmendmentInForce);
                assert_eq!(
                    email.body.contains(RECORD),
                    copy,
                    "{language} {}: the link to the record",
                    notice.as_str()
                );
            }
            // A wording file with a message the service never sends is a
            // translation nobody will read.
            for name in file.notifications.email.messages.keys() {
                assert!(Notice::parse(name).is_some(), "{language}: stray {name}");
            }
        }
    }

    #[test]
    fn a_message_is_in_the_language_asked_for() {
        let wording = Wording::embedded().unwrap();
        let english = wording.email("en", Notice::DeliveryClaimed, CODE, LINKS);
        let spanish = wording.email("es", Notice::DeliveryClaimed, CODE, LINKS);
        assert_ne!(english, spanish);
        assert!(english.subject.contains(CODE));
        // A regional tag gets its base language.
        assert_eq!(
            wording.email("es-MX", Notice::DeliveryClaimed, CODE, LINKS),
            spanish
        );
    }

    #[test]
    fn a_language_without_wording_falls_back_to_the_default() {
        let wording = Wording::embedded().unwrap();
        let default = wording.email(languages::default(), Notice::DisputeOpened, CODE, LINKS);
        assert_eq!(
            wording.email("tlh", Notice::DisputeOpened, CODE, LINKS),
            default
        );
        assert_eq!(
            wording.email("", Notice::DisputeOpened, CODE, LINKS),
            default
        );
    }

    #[test]
    fn a_code_email_names_the_code_and_what_it_is_for_in_the_language_asked_for() {
        let wording = Wording::embedded().unwrap();
        for language in languages::supported() {
            let sign_in = wording.code_email(language, Purpose::SignIn, "123456");
            let delete = wording.code_email(language, Purpose::DeleteAccount, "123456");
            assert_ne!(
                sign_in, delete,
                "{language}: the two purposes read differently"
            );
            for email in [&sign_in, &delete] {
                assert!(email.subject.contains("123456"), "{language}: {email:?}");
                assert!(email.body.contains("123456"), "{language}: {email:?}");
                for text in [&email.subject, &email.body] {
                    assert!(
                        !text.contains('{') && !text.contains('}'),
                        "{language}: {text:?}"
                    );
                }
            }
        }
        assert_ne!(
            wording.code_email("en", Purpose::SignIn, "123456"),
            wording.code_email("es", Purpose::SignIn, "123456")
        );
        assert_eq!(
            wording.code_email("tlh", Purpose::SignIn, "123456"),
            wording.code_email(languages::default(), Purpose::SignIn, "123456")
        );
    }

    fn file(product: &str, messages: &[Notice]) -> String {
        let messages: serde_json::Map<String, serde_json::Value> = messages
            .iter()
            .map(|notice| {
                let message = serde_json::json!({
                    "subject": format!("{product} {} {{code}}", notice.as_str()),
                    "body": "Text.",
                });
                (notice.as_str().to_owned(), message)
            })
            .collect();
        let code = |what: &str| serde_json::json!({ "subject": format!("{product} {what} {{code}}"), "body": "{code}" });
        serde_json::json!({
            "productName": product,
            "notifications": {
                "email": { "layout": "{body} {link}", "messages": messages },
                "oneTimeCode": { "signIn": code("sign-in"), "deleteAccount": code("delete") },
            },
        })
        .to_string()
    }

    static THREE: LazyLock<Vec<String>> =
        LazyLock::new(|| vec!["en".to_owned(), "fr".to_owned(), "de".to_owned()]);

    #[test]
    fn a_language_is_picked_up_from_its_file_alone() {
        let english = file("Yuppers", &Notice::ALL);
        let french = file("Échange", &Notice::ALL);
        let wording =
            Wording::from_files(&THREE, "en", &[("en", &english), ("fr", &french)]).unwrap();

        let email = wording.email("fr", Notice::EndProposed, CODE, LINKS);
        assert_eq!(email.subject, format!("Échange END_PROPOSED {CODE}"));
        assert_eq!(email.body, format!("Text. {LINK}"));
        // Listed, but with no file yet.
        assert_eq!(
            wording
                .email("de", Notice::EndProposed, CODE, LINKS)
                .subject,
            format!("Yuppers END_PROPOSED {CODE}")
        );
    }

    #[test]
    fn a_message_one_language_lacks_is_sent_whole_in_the_default_language() {
        let english = file("Yuppers", &Notice::ALL);
        let french = file("Échange", &[Notice::EndProposed]);
        let wording =
            Wording::from_files(&THREE, "en", &[("en", &english), ("fr", &french)]).unwrap();

        assert_eq!(
            wording
                .email("fr", Notice::CloseRequested, CODE, LINKS)
                .subject,
            format!("Yuppers CLOSE_REQUESTED {CODE}"),
            "the product name is the default language's too, not a mixture"
        );
    }

    #[test]
    fn an_incomplete_default_language_is_refused() {
        let english = file("Yuppers", &[Notice::EndProposed]);
        assert_eq!(
            Wording::from_files(&THREE, "en", &[("en", &english)]).err(),
            Some(WordingError::Missing {
                language: "en".to_owned(),
                notice: Notice::ALL[0].as_str(),
            })
        );
        assert_eq!(
            Wording::from_files(&THREE, "en", &[]).err(),
            Some(WordingError::NoDefault("en".to_owned()))
        );

        let no_link = file("Yuppers", &Notice::ALL).replace("{link}", "");
        assert_eq!(
            Wording::from_files(&THREE, "en", &[("en", &no_link)]).err(),
            Some(WordingError::Layout("en".to_owned()))
        );
    }

    #[test]
    fn filling_replaces_known_variables_once() {
        let values = [("code", "{link}"), ("link", "L")];
        assert_eq!(fill("a {code} b {link}", &values), "a {link} b L");
        assert_eq!(fill("{unknown} {code", &values), "{unknown} {code");
        assert_eq!(fill("", &values), "");
    }
}
