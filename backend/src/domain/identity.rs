//! Email addresses and phone numbers as proof of who someone is (DESIGN.md §8).

/// A normalized identifier: what is stored, compared and sent to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Identifier {
    /// Lower-case, without surrounding space.
    Email(String),
    /// E.164: a plus sign, then country code and number, digits only.
    Phone(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a usable email address or phone number")]
pub struct InvalidIdentifier;

impl Identifier {
    /// Reads what someone typed. Anything containing `@` is treated as an
    /// email address; anything else must be a phone number in international
    /// form. Formatting characters in phone numbers are dropped.
    ///
    /// This checks shape only. Whether the address or number exists is proved
    /// by the one-time code.
    pub fn parse(input: &str) -> Result<Self, InvalidIdentifier> {
        let input = input.trim();
        if input.contains('@') {
            Self::email(input)
        } else {
            Self::phone(input)
        }
    }

    fn email(input: &str) -> Result<Self, InvalidIdentifier> {
        let email = input.to_lowercase();
        let (local, domain) = email.rsplit_once('@').ok_or(InvalidIdentifier)?;

        // ASCII only, for now. Outside it, lowercasing is not one agreed
        // function: this code and the database can disagree about a letter,
        // and then the same address is two addresses.
        let valid = email.is_ascii()
            && email.len() <= 254
            && !local.is_empty()
            && !local.contains('@')
            && domain.contains('.')
            && domain.split('.').all(|label| !label.is_empty())
            && !email.chars().any(|c| c.is_whitespace() || c.is_control());

        valid.then_some(Self::Email(email)).ok_or(InvalidIdentifier)
    }

    fn phone(input: &str) -> Result<Self, InvalidIdentifier> {
        let rest = input.strip_prefix('+').ok_or(InvalidIdentifier)?;
        if !rest
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '(' | ')' | '.'))
        {
            return Err(InvalidIdentifier);
        }
        let digits: String = rest.chars().filter(char::is_ascii_digit).collect();

        let valid = (7..=15).contains(&digits.len()) && !digits.starts_with('0');
        valid
            .then(|| Self::Phone(format!("+{digits}")))
            .ok_or(InvalidIdentifier)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Identifier::Email(value) | Identifier::Phone(value) => value,
        }
    }

    /// For showing someone which identifier was used without revealing it in
    /// full: `a•••@example.com`, `+1•••••••42`.
    pub fn masked(&self) -> String {
        match self {
            Identifier::Email(email) => {
                let (local, domain) = email.rsplit_once('@').expect("validated at parse");
                let first = local.chars().next().expect("validated at parse");
                format!("{first}•••@{domain}")
            }
            Identifier::Phone(phone) => {
                let digits = &phone[1..];
                let hidden = "•".repeat(digits.len() - 3);
                format!("+{}{}{}", &digits[..1], hidden, &digits[digits.len() - 2..])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_addresses_are_trimmed_and_lowercased() {
        assert_eq!(
            Identifier::parse("  Ana.Ruiz@Example.COM "),
            Ok(Identifier::Email("ana.ruiz@example.com".into()))
        );
    }

    #[test]
    fn malformed_email_addresses_are_refused() {
        for bad in [
            "@example.com",
            "ana@",
            "ana@example",
            "ana@.com",
            "ana@example..com",
            "ana@@example.com",
            "ana ruiz@example.com",
            "ana@exam ple.com",
            "aña@example.com",
            "ana@exämple.com",
            "\u{38d}na@example.com",
        ] {
            assert_eq!(Identifier::parse(bad), Err(InvalidIdentifier), "{bad}");
        }
    }

    #[test]
    fn phone_numbers_are_reduced_to_e164() {
        for input in ["+12025550142", "+1 (202) 555-0142", " +1.202.555.0142 "] {
            assert_eq!(
                Identifier::parse(input),
                Ok(Identifier::Phone("+12025550142".into())),
                "{input}"
            );
        }
    }

    #[test]
    fn phone_numbers_need_a_country_code_and_a_sane_length() {
        for bad in [
            "2025550142",
            "+0123456789",
            "+123456",
            "+1234567890123456",
            "+1 202 555 01x2",
            "",
        ] {
            assert_eq!(Identifier::parse(bad), Err(InvalidIdentifier), "{bad}");
        }
    }

    #[test]
    fn masking_hides_most_of_the_identifier() {
        assert_eq!(
            Identifier::parse("ana.ruiz@example.com").unwrap().masked(),
            "a•••@example.com"
        );
        assert_eq!(
            Identifier::parse("+12025550142").unwrap().masked(),
            "+1••••••••42"
        );
    }
}
