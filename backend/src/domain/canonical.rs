//! The content hash a signature binds to (DESIGN.md §6): SHA-256 over the
//! canonical JSON (RFC 8785) of what the revision says.
//!
//! Only the service computes this, so there is one implementation. The format
//! is still written down precisely, because a signed hash must be reproducible
//! years later and by an independent party holding the same terms.

use std::cmp::Ordering;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::revision::{Contribution, Due, Kind, MAX_AMOUNT_MINOR, Revision, Settlement};

/// Bumped whenever the hashed document changes shape, so old hashes stay
/// verifiable against the format they were made with.
const FORMAT_VERSION: u8 = 1;

/// Hashes the agreed terms of a revision.
///
/// Covered: the exchange it belongs to, its currency, the timezone its due
/// dates are read in, both party names, the
/// terms text, every contribution in order, and the hashes of attachments.
/// Not covered: the note and the expiry, which are about the offer, not the
/// agreement.
pub fn content_hash(
    exchange: Uuid,
    currency: &str,
    timezone: &str,
    revision: &Revision,
) -> [u8; 32] {
    Sha256::digest(canonical_document(exchange, currency, timezone, revision).as_bytes()).into()
}

/// The exact text that is hashed.
pub fn canonical_document(
    exchange: Uuid,
    currency: &str,
    timezone: &str,
    revision: &Revision,
) -> String {
    let document = json!({
        "v": FORMAT_VERSION,
        "exchange": exchange.to_string(),
        "currency": currency,
        "timezone": timezone,
        "parties": { "A": revision.party_a, "B": revision.party_b },
        "terms": revision.terms,
        "contributions": revision.contributions.iter().map(contribution).collect::<Vec<_>>(),
        "attachments": revision.attachments.iter().map(|hash| hex(hash)).collect::<Vec<_>>(),
    });
    canonicalize(&document)
}

fn contribution(contribution: &Contribution) -> Value {
    let (kind, amount_minor, settlement) = match contribution.kind {
        Kind::Item => ("ITEM", None, None),
        Kind::Service => ("SERVICE", None, None),
        Kind::Task => ("TASK", None, None),
        Kind::Other => ("OTHER", None, None),
        Kind::Money {
            amount_minor,
            settlement,
        } => (
            "MONEY",
            Some(amount_minor),
            Some(match settlement {
                Settlement::OffPlatform => "OFF_PLATFORM",
                Settlement::Processor => "PROCESSOR",
            }),
        ),
    };
    let due = match contribution.due {
        Due::Date(date) => json!({
            "kind": "DATE",
            "date": format!("{:04}-{:02}-{:02}", date.year(), u8::from(date.month()), date.day()),
        }),
        Due::OnAgreement => json!({ "kind": "ON_AGREEMENT" }),
        Due::After(id) => json!({ "kind": "AFTER_CONTRIBUTION", "contribution": id.0.to_string() }),
    };

    json!({
        "id": contribution.id.0.to_string(),
        "from": contribution.from.as_str(),
        "type": kind,
        "description": contribution.description,
        "quantity": contribution.quantity.as_ref().map(|q| json!({ "amount": q.amount, "unit": q.unit })),
        "due": due,
        "completion_criteria": contribution.completion_criteria,
        "required": contribution.required,
        "amount_minor": amount_minor,
        "settlement": settlement,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Serializes a JSON value as RFC 8785 requires: no whitespace, object keys
/// sorted by UTF-16 code units, minimal string escaping.
///
/// Numbers are limited to whole numbers that every JSON implementation reads
/// back exactly. Anything else is a bug in the caller, not input to tolerate.
fn canonicalize(value: &Value) -> String {
    let mut out = String::new();
    write(value, &mut out);
    out
}

fn write(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => {
            let whole = number
                .as_i64()
                .filter(|n| n.abs() <= MAX_AMOUNT_MINOR)
                .expect("canonical documents hold only whole numbers within the exact range");
            out.push_str(&whole.to_string());
        }
        Value::String(text) => write_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|(a, _), (b, _)| utf16_order(a, b));
            out.push('{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write(item, out);
            }
            out.push('}');
        }
    }
}

/// serde_json escapes exactly what RFC 8785 asks for: the quote, the
/// backslash, and control characters (short forms where they exist, lowercase
/// `\u00xx` otherwise), leaving everything else as UTF-8.
fn write_string(text: &str, out: &mut String) {
    out.push_str(&serde_json::to_string(text).expect("strings always serialize"));
}

fn utf16_order(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::revision::Quantity;
    use crate::domain::revision::tests::fence_job;

    const ZONE: &str = "America/Chicago";

    fn exchange() -> Uuid {
        Uuid::from_u128(0xE)
    }

    #[test]
    fn object_keys_sort_by_utf16_code_units() {
        // The sorting example from RFC 8785 §3.2.3.
        let value = json!({
            "\u{20ac}": "Euro Sign",
            "\r": "Carriage Return",
            "\u{fb33}": "Hebrew Letter Dalet With Dagesh",
            "1": "One",
            "\u{1f600}": "Emoji: Grinning Face",
            "\u{80}": "Control",
            "\u{f6}": "Latin Small Letter O With Diaeresis",
        });
        let text = canonicalize(&value);
        let order: Vec<&str> = [
            "Carriage Return",
            "One",
            "Control",
            "Latin Small Letter O With Diaeresis",
            "Euro Sign",
            "Emoji: Grinning Face",
            "Hebrew Letter Dalet With Dagesh",
        ]
        .into_iter()
        .collect();
        let positions: Vec<usize> = order
            .iter()
            .map(|label| text.find(label).unwrap())
            .collect();
        assert!(positions.is_sorted(), "{text}");
    }

    #[test]
    fn strings_are_escaped_minimally() {
        let value = json!(["a\"b\\c", "\u{8}\t\n\u{c}\r", "\u{1}\u{1f}", "/ñ€😀\u{7f}"]);
        assert_eq!(
            canonicalize(&value),
            r#"["a\"b\\c","\b\t\n\f\r","\u0001\u001f","/ñ€😀"#.to_owned() + "\u{7f}\"]"
        );
    }

    #[test]
    fn output_has_no_whitespace_and_nested_keys_are_sorted_too() {
        let value = json!({ "b": [1, { "z": null, "a": true }], "a": -5 });
        assert_eq!(
            canonicalize(&value),
            r#"{"a":-5,"b":[1,{"a":true,"z":null}]}"#
        );
    }

    #[test]
    #[should_panic(expected = "whole numbers")]
    fn fractions_are_not_canonical() {
        canonicalize(&json!(1.5));
    }

    #[test]
    fn the_hashed_document_is_exactly_this() {
        assert_eq!(
            canonical_document(exchange(), "USD", ZONE, &fence_job()),
            concat!(
                r#"{"attachments":[],"contributions":["#,
                r#"{"amount_minor":null,"completion_criteria":null,"description":"Contribution 1","#,
                r#""due":{"date":"2026-11-01","kind":"DATE"},"from":"A","#,
                r#""id":"00000000-0000-0000-0000-000000000001","quantity":null,"required":true,"#,
                r#""settlement":null,"type":"SERVICE"},"#,
                r#"{"amount_minor":50000,"completion_criteria":null,"description":"Contribution 2","#,
                r#""due":{"contribution":"00000000-0000-0000-0000-000000000001","kind":"AFTER_CONTRIBUTION"},"#,
                r#""from":"B","id":"00000000-0000-0000-0000-000000000002","quantity":null,"#,
                r#""required":true,"settlement":"OFF_PLATFORM","type":"MONEY"}],"#,
                r#""currency":"USD","exchange":"00000000-0000-0000-0000-00000000000e","#,
                r#""parties":{"A":"Ana","B":"Ben"},"terms":"Repair the back fence.","#,
                r#""timezone":"America/Chicago","v":1}"#,
            )
        );
    }

    #[test]
    fn the_note_is_not_part_of_what_is_signed() {
        let mut revision = fence_job();
        let before = content_hash(exchange(), "USD", ZONE, &revision);
        revision.note = Some("Does Saturday work?".into());
        assert_eq!(content_hash(exchange(), "USD", ZONE, &revision), before);
    }

    #[test]
    fn every_term_changes_the_hash() {
        let original = content_hash(exchange(), "USD", ZONE, &fence_job());
        let changes: Vec<fn(&mut Revision)> = vec![
            |r| r.party_a.push('!'),
            |r| r.party_b.push('!'),
            |r| r.terms.push('!'),
            |r| r.attachments.push([7; 32]),
            |r| r.contributions.swap(0, 1),
            |r| r.contributions[0].description.push('!'),
            |r| r.contributions[0].from = r.contributions[0].from.other(),
            |r| r.contributions[0].kind = Kind::Task,
            |r| r.contributions[0].due = Due::OnAgreement,
            |r| r.contributions[0].required = false,
            |r| r.contributions[0].completion_criteria = Some("Gate swings freely".into()),
            |r| {
                r.contributions[0].quantity = Some(Quantity {
                    amount: "2".into(),
                    unit: Some("panels".into()),
                })
            },
            |r| {
                r.contributions[1].kind = Kind::Money {
                    amount_minor: 50_001,
                    settlement: Settlement::OffPlatform,
                }
            },
            |r| {
                r.contributions[1].kind = Kind::Money {
                    amount_minor: 50_000,
                    settlement: Settlement::Processor,
                }
            },
        ];

        for (index, change) in changes.into_iter().enumerate() {
            let mut revision = fence_job();
            change(&mut revision);
            assert_ne!(
                content_hash(exchange(), "USD", ZONE, &revision),
                original,
                "change {index} did not affect the hash"
            );
        }

        assert_ne!(
            content_hash(Uuid::from_u128(0xF), "USD", ZONE, &fence_job()),
            original
        );
        assert_ne!(
            content_hash(exchange(), "CAD", ZONE, &fence_job()),
            original
        );
        // Due dates are read in the timezone, so a different one is a
        // different agreement.
        assert_ne!(
            content_hash(exchange(), "USD", "Asia/Tokyo", &fence_job()),
            original
        );
    }

    #[test]
    fn the_hash_is_the_sha256_of_the_document() {
        // Pinned so an accidental change to the format fails loudly. Verify
        // independently with: printf '%s' '<document>' | shasum -a 256
        assert_eq!(
            hex(&content_hash(exchange(), "USD", ZONE, &fence_job())),
            "2126dd474c35f4c14ad40ad06f737a6d1d3276ce2280b908413ebeaafa3932ab"
        );
    }
}
