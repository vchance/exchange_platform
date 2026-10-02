//! The text of what the service sends (DESIGN.md §4.2).
//!
//! None of it is written here. It comes from the wording files that the web
//! and mobile apps also read, one per language, all of them embedded at build
//! time. A language added to those files is spoken here without a change to
//! this code.

use std::collections::HashMap;

use serde::Deserialize;

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
    /// `code` is the exchange's display code and `link` opens the exchange.
    /// Nothing else about the exchange can be put in: a message never carries
    /// what the parties agreed or wrote (DESIGN.md §12).
    pub fn email(&self, language: &str, notice: Notice, code: &str, link: &str) -> Rendered {
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
            ("link", link),
        ];
        let text = fill(&message.body, &values);
        let mut with_text = values.to_vec();
        with_text.push(("body", &text));
        Rendered {
            subject: fill(&message.subject, &values),
            body: fill(&file.notifications.email.layout, &with_text),
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
                let email = wording.email(language, notice, CODE, LINK);
                for text in [&email.subject, &email.body] {
                    assert!(
                        !text.contains('{') && !text.contains('}'),
                        "{language} {}: an unknown variable in {text:?}",
                        notice.as_str()
                    );
                }
                assert!(!email.subject.trim().is_empty());
                assert!(email.body.contains(LINK), "every message links back");
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
        let english = wording.email("en", Notice::DeliveryClaimed, CODE, LINK);
        let spanish = wording.email("es", Notice::DeliveryClaimed, CODE, LINK);
        assert_ne!(english, spanish);
        assert!(english.subject.contains(CODE));
        // A regional tag gets its base language.
        assert_eq!(
            wording.email("es-MX", Notice::DeliveryClaimed, CODE, LINK),
            spanish
        );
    }

    #[test]
    fn a_language_without_wording_falls_back_to_the_default() {
        let wording = Wording::embedded().unwrap();
        let default = wording.email(languages::default(), Notice::DisputeOpened, CODE, LINK);
        assert_eq!(
            wording.email("tlh", Notice::DisputeOpened, CODE, LINK),
            default
        );
        assert_eq!(
            wording.email("", Notice::DisputeOpened, CODE, LINK),
            default
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
        serde_json::json!({
            "productName": product,
            "notifications": { "email": { "layout": "{body} {link}", "messages": messages } },
        })
        .to_string()
    }

    static THREE: LazyLock<Vec<String>> =
        LazyLock::new(|| vec!["en".to_owned(), "fr".to_owned(), "de".to_owned()]);

    #[test]
    fn a_language_is_picked_up_from_its_file_alone() {
        let english = file("Exchange", &Notice::ALL);
        let french = file("Échange", &Notice::ALL);
        let wording =
            Wording::from_files(&THREE, "en", &[("en", &english), ("fr", &french)]).unwrap();

        let email = wording.email("fr", Notice::EndProposed, CODE, LINK);
        assert_eq!(email.subject, format!("Échange END_PROPOSED {CODE}"));
        assert_eq!(email.body, format!("Text. {LINK}"));
        // Listed, but with no file yet.
        assert_eq!(
            wording.email("de", Notice::EndProposed, CODE, LINK).subject,
            format!("Exchange END_PROPOSED {CODE}")
        );
    }

    #[test]
    fn a_message_one_language_lacks_is_sent_whole_in_the_default_language() {
        let english = file("Exchange", &Notice::ALL);
        let french = file("Échange", &[Notice::EndProposed]);
        let wording =
            Wording::from_files(&THREE, "en", &[("en", &english), ("fr", &french)]).unwrap();

        assert_eq!(
            wording
                .email("fr", Notice::CloseRequested, CODE, LINK)
                .subject,
            format!("Exchange CLOSE_REQUESTED {CODE}"),
            "the product name is the default language's too, not a mixture"
        );
    }

    #[test]
    fn an_incomplete_default_language_is_refused() {
        let english = file("Exchange", &[Notice::EndProposed]);
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

        let no_link = file("Exchange", &Notice::ALL).replace("{link}", "");
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
