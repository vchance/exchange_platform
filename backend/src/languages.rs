//! The languages the product speaks (DESIGN.md §4.2).
//!
//! The list is not written here. It is the wording manifest that the web and
//! mobile apps also read, so adding a language is a change to the wording
//! files and nothing else: no code here, no API change, no migration.

use std::sync::LazyLock;

use serde::Deserialize;

const MANIFEST: &str = include_str!("../../packages/shared/wording/languages.json");

#[derive(Deserialize)]
struct Entry {
    code: String,
}

static CODES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let entries: Vec<Entry> =
        serde_json::from_str(MANIFEST).expect("wording/languages.json is a list of languages");
    assert!(
        !entries.is_empty(),
        "wording/languages.json lists no language"
    );
    entries.into_iter().map(|entry| entry.code).collect()
});

/// Every supported language, as BCP 47 tags such as `en` or `pt-BR`.
pub fn supported() -> &'static [String] {
    &CODES
}

/// The language used when someone's preference is not supported.
pub fn default() -> &'static str {
    &CODES[0]
}

/// The supported language for a tag a client sent: the tag itself if it is
/// supported, otherwise its base language (`es-MX` → `es`).
pub fn resolve(tag: &str) -> Option<&'static str> {
    resolve_among(&CODES, tag)
}

/// [`resolve`] against a given list of languages.
pub(crate) fn resolve_among<'a>(codes: &'a [String], tag: &str) -> Option<&'a str> {
    let find = |wanted: &str| {
        codes
            .iter()
            .find(|code| code.eq_ignore_ascii_case(wanted))
            .map(String::as_str)
    };
    let tag = tag.trim();
    find(tag).or_else(|| find(tag.split('-').next()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_lists_the_launch_languages_with_a_default() {
        assert!(supported().iter().any(|code| code == "en"));
        assert!(supported().iter().any(|code| code == "es"));
        assert_eq!(default(), supported()[0]);
    }

    #[test]
    fn a_regional_tag_falls_back_to_its_base_language() {
        assert_eq!(resolve("es"), Some("es"));
        assert_eq!(resolve("es-MX"), Some("es"));
        assert_eq!(resolve(" EN-us "), Some("en"));
    }

    #[test]
    fn an_unsupported_language_resolves_to_nothing() {
        assert_eq!(resolve("tlh"), None);
        assert_eq!(resolve(""), None);
    }
}
