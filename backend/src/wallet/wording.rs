//! What a pass says, in each language: the `wallet` section of the wording
//! files the apps read (DESIGN.md §4.2), embedded at build time like the
//! notifications' (`crate::notifications::wording`). Nothing is written
//! here.

use std::collections::HashMap;

use serde::Deserialize;
use time::Date;

use crate::languages;

// `WORDING_FILES`, written by the build script: every file in the wording
// directory.
include!(concat!(env!("OUT_DIR"), "/wording_files.rs"));

#[derive(Deserialize)]
struct File {
    #[serde(rename = "productName")]
    product_name: String,
    wallet: Section,
}

#[derive(Deserialize)]
struct Section {
    pass: PassLabels,
    status: StatusWords,
    date: String,
    months: Months,
}

/// The labels and fixed text on a pass.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassLabels {
    /// What the pass is, for VoiceOver: `{productName}` and `{code}`.
    pub description: String,
    pub status: String,
    pub reference: String,
    pub next_due: String,
    pub outstanding: String,
    pub with: String,
    pub closed_on: String,
    pub open: String,
    /// On the back: what the pass is and is not. `{productName}`.
    pub note: String,
    /// On the back of a pass that has been revoked.
    pub void: String,
}

/// How the agreement stands, in a word or two.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusWords {
    pub in_force: String,
    pub waiting_for_you: String,
    pub due_soon: String,
    pub overdue: String,
    pub disputed: String,
    pub completed: String,
    pub ended: String,
    pub closed: String,
    pub void: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Months {
    jan: String,
    feb: String,
    mar: String,
    apr: String,
    may: String,
    jun: String,
    jul: String,
    aug: String,
    sep: String,
    oct: String,
    nov: String,
    dec: String,
}

/// One language's pass wording.
#[derive(Clone, Debug)]
pub struct Language {
    pub code: &'static str,
    pub product_name: String,
    pub labels: PassLabels,
    pub status: StatusWords,
    date: String,
    months: [String; 12],
}

impl Language {
    /// A calendar date, as this language writes it: `Oct 5, 2026`,
    /// `5 oct 2026`. Spelled from the wording, not the platform's calendar,
    /// so that both wallets show the same.
    pub fn date(&self, date: Date) -> String {
        let month = &self.months[usize::from(u8::from(date.month())) - 1];
        fill(
            &self.date,
            &[
                ("month", month),
                ("day", &date.day().to_string()),
                ("year", &date.year().to_string()),
            ],
        )
    }
}

/// Pass wording for every supported language that has it.
pub struct WalletWording {
    default: &'static str,
    languages: HashMap<&'static str, Language>,
}

#[derive(Debug, thiserror::Error)]
#[error("the default language `{0}` has no usable wallet wording")]
pub struct NoDefault(String);

impl WalletWording {
    /// The wording built into this binary. Fails if the default language,
    /// which every other falls back to, has none.
    pub fn embedded() -> Result<Self, NoDefault> {
        let default = languages::default();
        let languages: HashMap<&'static str, Language> = languages::supported()
            .iter()
            .filter_map(|code| {
                let (_, text) = WORDING_FILES.iter().find(|(name, _)| name == code)?;
                let file: File = serde_json::from_str(text).ok()?;
                let months = file.wallet.months;
                Some((
                    code.as_str(),
                    Language {
                        code: code.as_str(),
                        product_name: file.product_name,
                        labels: file.wallet.pass,
                        status: file.wallet.status,
                        date: file.wallet.date,
                        months: [
                            months.jan, months.feb, months.mar, months.apr, months.may, months.jun,
                            months.jul, months.aug, months.sep, months.oct, months.nov, months.dec,
                        ],
                    },
                ))
            })
            .collect();
        if !languages.contains_key(default) {
            return Err(NoDefault(default.to_owned()));
        }
        Ok(Self { default, languages })
    }

    /// The wording in `language` (an account's preference) where it exists,
    /// and in the default language otherwise.
    pub fn language(&self, language: &str) -> &Language {
        languages::resolve(language)
            .and_then(|code| self.languages.get(code))
            .unwrap_or_else(|| &self.languages[self.default])
    }
}

/// Replaces each `{name}` that has a value; anything else is left as written.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let value = after
            .find('}')
            .and_then(|end| values.iter().find(|(name, _)| *name == &after[..end]))
            .map(|(name, value)| (name.len(), *value));
        match value {
            Some((length, value)) => {
                out.push_str(&rest[..start]);
                out.push_str(value);
                rest = &after[length + 1..];
            }
            None => {
                out.push_str(&rest[..=start]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    #[test]
    fn every_supported_language_has_the_wallet_wording() {
        let wording = WalletWording::embedded().unwrap();
        for code in languages::supported() {
            assert!(
                wording.languages.contains_key(code.as_str()),
                "{code} has no wallet wording"
            );
            let language = wording.language(code);
            assert!(!language.date(date!(2026 - 10 - 05)).contains('{'));
        }
    }

    #[test]
    fn dates_are_written_as_each_language_writes_them() {
        let wording = WalletWording::embedded().unwrap();
        assert_eq!(
            wording.language("en").date(date!(2026 - 10 - 05)),
            "Oct 5, 2026"
        );
        assert_eq!(
            wording.language("es-MX").date(date!(2026 - 01 - 31)),
            "31 ene 2026"
        );
        // A language with no wording of its own gets the default's.
        assert_eq!(
            wording.language("xx").date(date!(2026 - 12 - 01)),
            "Dec 1, 2026"
        );
    }

    #[test]
    fn filling_leaves_unknown_placeholders_and_what_was_put_in_alone() {
        assert_eq!(fill("{a} and {b}", &[("a", "{b}")]), "{b} and {b}");
        assert_eq!(fill("no braces", &[]), "no braces");
        assert_eq!(fill("{unclosed", &[("unclosed", "x")]), "{unclosed");
    }
}
