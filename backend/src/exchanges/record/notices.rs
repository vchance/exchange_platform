//! What the record says about itself, in words (DESIGN.md §14, §14.1).
//!
//! None of it is written here. It comes from the same wording files as
//! everything else the product says, under `record.export`, so each language
//! says it in its own words and a new language needs no change to this code.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::dto::{Notices, VerificationMethod};
use crate::languages;

// `WORDING_FILES`, written by the build script: every file in the wording
// directory, as (language code, contents).
include!(concat!(env!("OUT_DIR"), "/wording_files.rs"));

/// The parts of a wording file the record uses.
#[derive(Deserialize)]
struct File {
    record: Record,
}

#[derive(Deserialize)]
struct Record {
    export: Export,
}

#[derive(Debug, Deserialize)]
pub struct Export {
    about: String,
    signatures: String,
    statements: String,
    #[serde(rename = "contentHash")]
    content_hash: String,
    /// Keyed by the stored name of the method, such as `EMAIL_OTP`.
    verification: HashMap<String, String>,
}

impl Export {
    pub fn notices(&self) -> Notices {
        Notices {
            about: self.about.clone(),
            signatures: self.signatures.clone(),
            statements: self.statements.clone(),
            content_hash: self.content_hash.clone(),
        }
    }

    /// How a signer was verified, in words.
    pub fn verification(&self, method: VerificationMethod) -> String {
        self.verification
            .get(method.as_str())
            .cloned()
            .unwrap_or_default()
    }

    fn whole(&self) -> bool {
        let said = |text: &String| !text.trim().is_empty();
        [
            &self.about,
            &self.signatures,
            &self.statements,
            &self.content_hash,
        ]
        .into_iter()
        .all(said)
            && VerificationMethod::ALL
                .iter()
                .all(|method| self.verification.get(method.as_str()).is_some_and(said))
    }
}

impl VerificationMethod {
    pub const ALL: [VerificationMethod; 2] =
        [VerificationMethod::EmailOtp, VerificationMethod::PhoneOtp];

    /// The name the method is stored under.
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationMethod::EmailOtp => "EMAIL_OTP",
            VerificationMethod::PhoneOtp => "PHONE_OTP",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|method| method.as_str() == text)
    }
}

fn read(files: &[(&str, &str)]) -> HashMap<String, Export> {
    files
        .iter()
        .filter_map(|(code, text)| {
            let file: File = serde_json::from_str(text).ok()?;
            // A language that cannot say all of it says none of it; its
            // readers get the default language, whole.
            file.record
                .export
                .whole()
                .then(|| ((*code).to_owned(), file.record.export))
        })
        .collect()
}

static EXPORTS: LazyLock<HashMap<String, Export>> = LazyLock::new(|| read(WORDING_FILES));

/// The record's own wording for a language tag such as an account's
/// preference, and the supported language it is in: that language where it
/// has the wording, the default language otherwise.
pub fn wording(language: &str) -> (&'static str, &'static Export) {
    let find = |code: &'static str| EXPORTS.get(code).map(|export| (code, export));
    languages::resolve(language)
        .and_then(find)
        .or_else(|| find(languages::default()))
        .expect("the default language has the record's wording; the tests check every language")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_language_says_all_of_it() {
        for language in languages::supported() {
            let (found, export) = wording(language);
            assert_eq!(found, language, "{language} lacks the record's wording");
            for text in [
                &export.about,
                &export.signatures,
                &export.statements,
                &export.content_hash,
            ] {
                // These are put into the document as they are, so they
                // cannot hold a variable.
                assert!(!text.contains('{') && !text.contains('}'), "{text}");
            }
            for method in VerificationMethod::ALL {
                assert!(!export.verification(method).trim().is_empty());
            }
            // A description for a method that does not exist is a
            // translation nobody will read.
            for name in export.verification.keys() {
                assert!(VerificationMethod::parse(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn how_a_signer_was_verified_claims_no_more_than_one_code() {
        let (_, export) = wording("en");
        let email = export.verification(VerificationMethod::EmailOtp);
        assert!(email.contains("one-time code"), "{email}");
        assert!(email.contains("email"), "{email}");
        let phone = export.verification(VerificationMethod::PhoneOtp);
        assert!(phone.contains("phone"), "{phone}");
        assert_ne!(email, phone);
    }

    #[test]
    fn a_regional_or_unknown_language_still_gets_whole_wording() {
        assert_eq!(wording("es-MX").0, "es");
        assert_eq!(wording("tlh").0, languages::default());
        assert_eq!(wording("").0, languages::default());
    }

    #[test]
    fn a_language_missing_part_of_it_is_left_out() {
        let whole = r#"{"record":{"export":{"about":"a","signatures":"s","statements":"t",
            "contentHash":"h","verification":{"EMAIL_OTP":"e","PHONE_OTP":"p"}}}}"#;
        let partial = r#"{"record":{"export":{"about":"a","signatures":"s","statements":"t",
            "contentHash":"h","verification":{"EMAIL_OTP":"e"}}}}"#;
        let found = read(&[("en", whole), ("fr", partial), ("de", "{}")]);
        assert_eq!(found.keys().collect::<Vec<_>>(), ["en"]);
    }
}
