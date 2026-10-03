//! The HTML part of an email (DESIGN.md §12).
//!
//! Every email also goes as HTML, beside its plain text and saying the same
//! thing: the wording decides what is said, this file only how it looks. The
//! pieces here take text that is already in the reader's language, and
//! [`super::wording`] puts them together.
//!
//! What the markup is held to, since email clients are a long way behind
//! browsers:
//!
//! * a table layout with every style inline, at most 560 pixels wide, in the
//!   reader's system font: no web font, no script, no image, nothing fetched
//!   when the message is opened, so nothing can tell anyone it was read;
//! * the product's mark drawn in text and colour, a blue tile with a `Y`
//!   beside the product name, which shows the same with images blocked (the
//!   default in many clients) and needs nothing hosted;
//! * a light palette with a dark one for clients that honour
//!   `prefers-color-scheme`. That one rule cannot be written inline, so it is
//!   the only `<style>`; a client that drops it shows the light palette,
//!   which still reads, and one that darkens messages by itself has no image
//!   to spoil;
//! * every value escaped, and a link written as a link only when it is a web
//!   address.

/// The reader's system font, everywhere.
const FONT: &str = "-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif";
const MONO: &str = "ui-monospace,SFMono-Regular,Menlo,Consolas,'Liberation Mono',monospace";

/// The brand blue (`apps/web/public/favicon.svg`). White on it is above 7:1.
const ACCENT: &str = "#0b57b0";
const PAGE: &str = "#f2f4f7";
const CARD: &str = "#ffffff";
const BORDER: &str = "#dde2e8";
const TEXT: &str = "#1a1d21";
/// Small print: above 4.5:1 on both the card and the page.
const MUTED: &str = "#545c66";

/// The dark palette, for clients that read it. Each class names the role an
/// element plays; its light colours are inline on the element itself.
const DARK: &str = "@media (prefers-color-scheme: dark) {\n\
     .y-page { background-color: #0f1216 !important; }\n\
     .y-card { background-color: #1a1e24 !important; border-color: #2f363f !important; }\n\
     .y-text { color: #e8eaed !important; }\n\
     .y-muted { color: #aab3bd !important; }\n\
     .y-link { color: #8ab4f8 !important; }\n\
     .y-brand { color: #8ab4f8 !important; }\n\
     .y-code { background-color: #0f1216 !important; border-color: #2f363f !important; color: #e8eaed !important; }\n\
     }";

/// The letter on the mark's tile. Decoration, hidden from screen readers,
/// and the only text in a message that its plain text does not have.
pub const MARK_LETTER: &str = "Y";

/// `text` safe to put between tags or inside a quoted attribute.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Whether `url` may be the target of a link: a web address. The service
/// only ever gives addresses under its own web origin; anything else, such
/// as a `javascript:` URL, is shown as text and not linked.
fn linkable(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// An address as a link whose text is the address itself.
pub fn link(url: &str) -> String {
    let shown = escape(url);
    if !linkable(url) {
        return shown;
    }
    format!(
        "<a class=\"y-link\" href=\"{shown}\" dir=\"ltr\" \
         style=\"color:{ACCENT};text-decoration:underline;word-break:break-all;\">{shown}</a>"
    )
}

/// Text set apart in a sentence, such as a one-time code.
pub fn strong(text: &str) -> String {
    format!("<strong dir=\"ltr\">{}</strong>", escape(text))
}

/// A paragraph of the message. `inner` is markup already escaped.
pub fn paragraph(inner: &str) -> String {
    format!("<p class=\"y-text\" style=\"margin:0 0 16px;color:{TEXT};\">{inner}</p>\n")
}

/// A paragraph of the small print under the message.
pub fn small_print(inner: &str) -> String {
    format!("<p class=\"y-muted\" style=\"margin:0 0 12px;color:{MUTED};\">{inner}</p>\n")
}

/// The message's way in: a button, and the same address beneath it as text
/// for a client that shows no button or a reader who would rather see where
/// it goes. `label` is plain text.
pub fn button(label: &str, url: &str) -> String {
    if !linkable(url) {
        return paragraph(&format!("{} {}", escape(label), escape(url)));
    }
    let href = escape(url);
    let label = escape(label);
    format!(
        "<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" \
         style=\"margin:8px 0 12px;border-collapse:separate;\"><tr>\
         <td bgcolor=\"{ACCENT}\" style=\"border-radius:8px;background-color:{ACCENT};\">\
         <a href=\"{href}\" style=\"display:inline-block;padding:12px 24px;border-radius:8px;\
         font-family:{FONT};font-size:16px;line-height:20px;font-weight:600;color:#ffffff;\
         text-decoration:none;\">{label}</a></td></tr></table>\n\
         <p class=\"y-muted\" style=\"margin:0 0 16px;font-size:14px;line-height:20px;\
         color:{MUTED};word-break:break-all;\">{}</p>\n",
        link(url)
    )
}

/// A one-time code, large, on its own, and easy to select whole.
pub fn code(code: &str) -> String {
    format!(
        "<p style=\"margin:4px 0 20px;\"><span class=\"y-code\" dir=\"ltr\" \
         style=\"display:inline-block;padding:12px 20px;border:1px solid {BORDER};\
         border-radius:8px;background-color:{PAGE};color:{TEXT};font-family:{MONO};\
         font-size:32px;line-height:40px;font-weight:700;letter-spacing:6px;\
         -webkit-user-select:all;user-select:all;\">{}</span></p>\n",
        escape(code)
    )
}

/// What a whole message needs besides its paragraphs.
pub struct Page<'a> {
    /// The language the message is in, as a tag, and its direction.
    pub language: &'a str,
    pub direction: &'a str,
    pub product: &'a str,
    /// The heading, and the page title: the subject. Plain text.
    pub heading: &'a str,
    /// The message, as markup.
    pub main: &'a str,
    /// The small print under it, as markup. May be empty.
    pub small_print: &'a str,
}

/// The whole HTML document.
pub fn page(page: &Page<'_>) -> String {
    let language = escape(page.language);
    let direction = if page.direction == "rtl" {
        "rtl"
    } else {
        "ltr"
    };
    let (align, start) = if direction == "rtl" {
        ("right", "right")
    } else {
        ("left", "left")
    };
    let heading = escape(page.heading);
    let product = escape(page.product);
    let main = page.main;
    let small_print = if page.small_print.is_empty() {
        String::new()
    } else {
        format!(
            "<tr><td class=\"y-muted\" dir=\"{direction}\" align=\"{align}\" \
             style=\"padding:16px 8px 0;font-family:{FONT};font-size:13px;line-height:20px;\
             color:{MUTED};text-align:{align};\">\n{}</td></tr>\n",
            page.small_print
        )
    };
    format!(
        "<!DOCTYPE html>\n\
<html lang=\"{language}\" dir=\"{direction}\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"color-scheme\" content=\"light dark\">\n\
<meta name=\"supported-color-schemes\" content=\"light dark\">\n\
<title>{heading}</title>\n\
<style>\n{DARK}\n</style>\n\
</head>\n\
<body class=\"y-page\" style=\"margin:0;padding:0;background-color:{PAGE};\">\n\
<table role=\"presentation\" class=\"y-page\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" \
style=\"width:100%;background-color:{PAGE};\">\n\
<tr><td align=\"center\" style=\"padding:24px 12px;\">\n\
<!--[if mso]><table role=\"presentation\" width=\"560\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\"><tr><td><![endif]-->\n\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" \
style=\"width:100%;max-width:560px;\">\n\
<tr><td dir=\"{direction}\" align=\"{align}\" style=\"padding:0 8px 16px;\">\n\
<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\"><tr>\
<td aria-hidden=\"true\" width=\"28\" height=\"28\" align=\"center\" valign=\"middle\" bgcolor=\"{ACCENT}\" \
style=\"width:28px;height:28px;border-radius:8px;background-color:{ACCENT};color:#ffffff;\
font-family:{FONT};font-size:17px;line-height:28px;font-weight:700;text-align:center;\">{MARK_LETTER}</td>\
<td class=\"y-brand\" style=\"padding-{start}:8px;font-family:{FONT};font-size:18px;line-height:28px;\
font-weight:700;color:{ACCENT};\">{product}</td>\
</tr></table>\n\
</td></tr>\n\
<tr><td class=\"y-card\" dir=\"{direction}\" align=\"{align}\" bgcolor=\"{CARD}\" \
style=\"padding:28px 24px 12px;background-color:{CARD};border:1px solid {BORDER};border-radius:12px;\
font-family:{FONT};font-size:16px;line-height:24px;color:{TEXT};text-align:{align};\">\n\
<h1 class=\"y-text\" style=\"margin:0 0 16px;font-family:{FONT};font-size:22px;line-height:28px;\
font-weight:700;color:{TEXT};\">{heading}</h1>\n\
{main}\
</td></tr>\n\
{small_print}\
</table>\n\
<!--[if mso]></td></tr></table><![endif]-->\n\
</td></tr>\n\
</table>\n\
</body>\n\
</html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_covers_markup_and_both_quotes() {
        assert_eq!(
            escape(r#"<script>alert("a" + 'b') & more</script>"#),
            "&lt;script&gt;alert(&quot;a&quot; + &#39;b&#39;) &amp; more&lt;/script&gt;"
        );
        assert_eq!(escape("José 123"), "José 123");
    }

    #[test]
    fn only_a_web_address_becomes_a_link() {
        assert!(link("https://app.test/x").contains("href=\"https://app.test/x\""));
        let script = link("javascript:alert(1)");
        assert!(!script.contains("href"), "{script}");
        let button = button("Open", "javascript:alert(1)");
        assert!(!button.contains("href"), "{button}");
    }

    #[test]
    fn a_right_to_left_page_says_so_and_aligns_to_the_right() {
        let html = page(&Page {
            language: "ar",
            direction: "rtl",
            product: "P",
            heading: "H",
            main: "",
            small_print: "",
        });
        assert!(html.contains("<html lang=\"ar\" dir=\"rtl\">"), "{html}");
        assert!(html.contains("text-align:right"));
        assert!(!html.contains("text-align:left"));
    }
}
