//! Convert untrusted Markdown to safe HTML. Rules: `docs/decisions/0004-untrusted-content.md`.

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

/// The supported YAML subset treats an unquoted `#` after whitespace as a comment.
fn scalar(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with(['\'', '"']) {
        let quote = value.chars().next()?;
        let end = value[1..].find(quote)? + 1;
        let rest = value[end + 1..].trim();
        let result = &value[1..end];
        return ((rest.is_empty() || rest.starts_with('#'))
            && !result.is_empty()
            && !result.contains(['\'', '"', '\\']))
        .then(|| result.to_owned());
    }
    let end = value
        .char_indices()
        .find(|(i, c)| *c == '#' && (*i == 0 || value[..*i].ends_with(char::is_whitespace)))
        .map_or(value.len(), |(i, _)| i);
    let value = value[..end].trim();
    if value.is_empty()
        || value.starts_with(['[', '{', '|', '>', '-', '&', '*', '!', '%', '@', '`'])
        || value.contains(['\'', '"', '[', ']', '{', '}', ','])
        || value.contains(": ")
    {
        return None;
    }
    Some(value.to_owned())
}

fn frontmatter_block(text: &str) -> Option<&str> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let start = first.len();
    let mut end = start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some(&text[start..end]);
        }
        end += line.len();
    }
    None
}

/// Reads only title and tag scalar/list forms; unsupported YAML is ignored.
pub fn frontmatter(text: &str) -> (Option<String>, Vec<String>) {
    let Some(block) = frontmatter_block(text) else {
        return (None, Vec::new());
    };
    let mut title = None;
    let mut tags = Vec::new();
    let mut in_tags = false;
    for line in block.lines() {
        if line.starts_with([' ', '\t']) {
            if in_tags && let Some(item) = line.trim().strip_prefix("- ").and_then(scalar) {
                tags.push(item);
            }
            continue;
        }
        in_tags = false;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "title" => title = scalar(value),
            "tag" | "tags" => {
                tags.clear();
                if value.is_empty() {
                    in_tags = true;
                } else if let Some(list) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']'))
                {
                    let mut start = 0;
                    let mut quote = None;
                    for (i, c) in list.char_indices() {
                        if c == '\'' || c == '"' {
                            if quote == Some(c) {
                                quote = None;
                            } else if quote.is_none() {
                                quote = Some(c);
                            }
                        } else if c == ',' && quote.is_none() {
                            if let Some(item) = scalar(&list[start..i]) {
                                tags.push(item);
                            }
                            start = i + 1;
                        }
                    }
                    if quote.is_none()
                        && let Some(item) = scalar(&list[start..])
                    {
                        tags.push(item);
                    }
                } else if let Some(item) = scalar(value) {
                    tags.push(item);
                }
            }
            _ => {}
        }
    }
    (title, tags)
}

pub fn render(text: &str, note: &str) -> String {
    let options = Options::ENABLE_GFM
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | if frontmatter_block(text).is_some() {
            Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        } else {
            Options::empty()
        };
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
        .allowed_classes(HashMap::from([(
            "blockquote",
            HashSet::from([
                "markdown-alert-note",
                "markdown-alert-tip",
                "markdown-alert-important",
                "markdown-alert-warning",
                "markdown-alert-caution",
            ]),
        )]))
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
    fn frontmatter_is_extracted_and_hidden_only_when_closed_at_start() {
        let text = "---\r\ntitle: \"A # title\"\r\ntags: [one, 'two, three', a#b, four # comment]\r\n---\r\nBody";
        assert_eq!(
            frontmatter(text),
            (
                Some("A # title".into()),
                vec!["one", "two, three", "a#b", "four"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            )
        );
        let html = render(text, "a.md");
        assert!(!html.contains("title"));
        assert!(html.contains("Body"));
        assert_eq!(
            frontmatter("---\ntags:\n  - alpha\n  - 'beta # two'\n---\n").1,
            vec!["alpha", "beta # two"]
        );
        assert_eq!(
            frontmatter("---\ntag: single # comment\n---\n").1,
            vec!["single"]
        );
        for text in ["before\n---\ntitle: hidden\n---", "---\ntitle: visible\n"] {
            assert_eq!(frontmatter(text), (None, vec![]));
            assert!(render(text, "a.md").contains("title:"));
        }
    }
    #[test]
    fn removes_dangerous_code() {
        let h = render(
            "<script>alert(1)</script><a href=\"javascript:alert(1)\" onclick=\"alert(1)\">x</a>",
            "a.md",
        );
        assert!(!h.contains("script"));
        assert!(!h.contains("onclick"));
        assert!(!h.contains("javascript:"));
    }
    #[test]
    fn images_only_load_from_inside_folder() {
        assert!(
            render("![x](imagens/minha%20foto.png)", "sub/nota.md")
                .contains("http://note-asset.localhost/sub/imagens/minha%20foto.png")
        );
        for src in [
            "../../fora.png",
            "https://example.com/a.png",
            "data:image/png;base64,abc",
        ] {
            let h = render(&format!("![texto]({src})"), "sub/nota.md");
            assert!(!h.contains("src="), "{h}");
            assert!(h.contains("texto"));
        }
        let h = render(
            "<img src=\"foto.png\" alt=\"foto\"><img src=\"https://example.com/a.png\" alt=\"remota\">",
            "sub/nota.md",
        );
        assert!(h.contains("http://note-asset.localhost/sub/foto.png"));
        assert!(!h.contains("example.com"));
    }
    #[test]
    fn alert_classes_are_allowlisted() {
        for kind in ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"] {
            let html = render(&format!("> [!{kind}]\n> Safe"), "a.md");
            assert!(
                html.contains(&format!(
                    r#"class="markdown-alert-{}""#,
                    kind.to_lowercase()
                )),
                "{html}"
            );
        }
        let html = render(
            r#"<blockquote class="arbitrary markdown-alert-note">Safe</blockquote><p class="markdown-alert-note">Body</p>"#,
            "a.md",
        );
        assert!(!html.contains("arbitrary"));
        assert_eq!(html.matches("class=").count(), 1);
    }
}
