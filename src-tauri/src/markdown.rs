//! Markdown não confiável para HTML seguro. Regras em `docs/decisions/0004-conteudo-nao-confiavel.md`.

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use pulldown_cmark::{Options, Parser, html};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

const SEGMENT: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
    .remove(b'.')
    .remove(b'-')
    .remove(b'_')
    .remove(b'~');

fn image_url(src: &str, note: &str) -> Option<String> {
    let decoded = percent_decode_str(src).decode_utf8().ok()?;
    let src = decoded.as_ref();
    if src.is_empty() || src.starts_with('/') || src.contains([':', '\\', '?', '#']) {
        return None;
    }
    let mut parts: Vec<&str> = note.split('/').collect();
    parts.pop();
    for part in src.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(format!(
        "http://note-asset.localhost/{}",
        parts
            .into_iter()
            .map(|s| utf8_percent_encode(s, SEGMENT).to_string())
            .collect::<Vec<_>>()
            .join("/")
    ))
}

pub fn render(text: &str, note: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut html = String::new();
    html::push_html(&mut html, Parser::new_ext(text, options));
    let tags: HashSet<_> = [
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "p",
        "br",
        "hr",
        "ul",
        "ol",
        "li",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "pre",
        "code",
        "blockquote",
        "em",
        "strong",
        "del",
        "a",
        "img",
        "input",
        "sup",
        "div",
    ]
    .into_iter()
    .collect();
    let attrs: HashMap<_, HashSet<_>> = [
        ("a", ["href", "title"].into_iter().collect()),
        ("img", ["src", "alt", "title"].into_iter().collect()),
        ("input", ["checked"].into_iter().collect()),
        ("div", ["id"].into_iter().collect()),
        ("sup", ["id"].into_iter().collect()),
    ]
    .into_iter()
    .collect();
    let note = note.to_owned();
    let mut builder = ammonia::Builder::default();
    builder
        .tags(tags)
        .tag_attributes(attrs)
        .generic_attributes(HashSet::new())
        .url_schemes(["http", "https", "mailto"].into_iter().collect())
        .link_rel(None)
        .id_prefix(Some("note-"))
        .set_tag_attribute_value("input", "type", "checkbox")
        .set_tag_attribute_value("input", "disabled", "");
    builder.attribute_filter(move |element, attr, value| {
        if element == "img" && attr == "src" {
            return image_url(value, &note).map(Cow::Owned);
        }
        if element == "input" && attr == "type" && value != "checkbox" {
            return None;
        }
        Some(Cow::Borrowed(value))
    });
    builder.clean(&html).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remove_codigo_perigoso() {
        let h = render(
            "<script>alert(1)</script><a href=\"javascript:alert(1)\" onclick=\"alert(1)\">x</a>",
            "a.md",
        );
        assert!(!h.contains("script"));
        assert!(!h.contains("onclick"));
        assert!(!h.contains("javascript:"));
    }
    #[test]
    fn tabela() {
        assert!(render("| a | b |\n|---|---|\n| c | d |", "a.md").contains("<table>"));
    }
    #[test]
    fn imagem_relativa() {
        assert!(
            render("![x](imagens/minha%20foto.png)", "sub/nota.md")
                .contains("http://note-asset.localhost/sub/imagens/minha%20foto.png")
        );
    }
    #[test]
    fn imagens_inseguras() {
        for src in [
            "../../fora.png",
            "https://example.com/a.png",
            "data:image/png;base64,abc",
        ] {
            let h = render(&format!("![texto]({src})"), "sub/nota.md");
            assert!(!h.contains("src="), "{h}");
            assert!(h.contains("texto"));
        }
    }
    #[test]
    fn imagem_html_cru() {
        let h = render(
            "<img src=\"foto.png\" alt=\"foto\"><img src=\"https://example.com/a.png\" alt=\"remota\">",
            "sub/nota.md",
        );
        assert!(h.contains("http://note-asset.localhost/sub/foto.png"));
        assert!(!h.contains("example.com"));
    }
}
