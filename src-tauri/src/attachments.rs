//! Conservative attachment reference checks, independent of the disk and the editor.

use std::collections::HashSet;

use dom_query::Document;
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, Parser, Tag};

/// Which of `candidates` the supplied notes reference. All paths are relative to the notes
/// folder, with `/` separators. Pass a trashed note's original path and omit any note whose
/// references should be ignored.
pub fn referenced<'a, 'n>(
    candidates: &'a [String],
    notes: impl IntoIterator<Item = (&'n str, &'n str)>,
) -> HashSet<&'a str> {
    let mut remaining: Vec<_> = candidates
        .iter()
        .map(|path| {
            let folded = path.to_lowercase();
            let name = folded.rsplit('/').next().unwrap_or_default().to_owned();
            (path.as_str(), folded, name)
        })
        .collect();
    let mut used = HashSet::new();
    for (note, source) in notes {
        if remaining.is_empty() {
            break;
        }
        let raw = source.to_lowercase();
        let decoded = source.contains('%').then(|| {
            percent_decode_str(source)
                .decode_utf8_lossy()
                .to_lowercase()
        });
        remaining.retain(|(path, _, name)| {
            let mentions = raw.contains(name) || decoded.as_ref().is_some_and(|s| s.contains(name));
            if mentions {
                used.insert(*path);
            }
            !mentions
        });
        if remaining.is_empty() {
            break;
        }
        let links = destinations(note, source);
        remaining.retain(|(path, folded, _)| {
            let linked = links.contains(folded);
            if linked {
                used.insert(*path);
            }
            !linked
        });
    }
    used
}

/// Which of `candidates` are among `paths`, ignoring case as Windows does: images a floating window
/// shows, which count as in use.
pub fn listed<'a>(candidates: &'a [String], paths: &[String]) -> HashSet<&'a str> {
    let paths: HashSet<String> = paths.iter().map(|path| path.to_lowercase()).collect();
    candidates
        .iter()
        .filter(|candidate| paths.contains(&candidate.to_lowercase()))
        .map(String::as_str)
        .collect()
}

fn destinations(note: &str, source: &str) -> HashSet<String> {
    let mut paths = HashSet::new();
    let mut collect = |url: &str| {
        if let Some(path) = resolve(note, url) {
            paths.insert(path);
        }
    };
    for event in Parser::new(source) {
        match event {
            Event::Start(Tag::Image { dest_url, .. } | Tag::Link { dest_url, .. }) => {
                collect(&dest_url)
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let document = Document::fragment(html.as_ref());
                for node in document.select("[href], [src], [poster]").nodes() {
                    for attribute in ["href", "src", "poster"] {
                        if let Some(value) = node.attr(attribute) {
                            collect(&value);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    paths
}

fn resolve(note: &str, url: &str) -> Option<String> {
    let url = url.trim().split(['?', '#']).next()?;
    let path = percent_decode_str(url)
        .decode_utf8()
        .ok()?
        .replace('\\', "/");
    if path.is_empty() || path.starts_with('/') || path.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = note.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/").to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(note: &str, text: &str, candidates: &[&str]) -> Vec<String> {
        let candidates: Vec<_> = candidates.iter().map(|path| (*path).to_owned()).collect();
        let mut paths: Vec<_> = referenced(&candidates, [(note, text)])
            .into_iter()
            .map(str::to_owned)
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn markdown_link_resolves_from_subfolder_after_unescaping() {
        let a = "topics/attachments/map(2).png";
        let b = "attachments/map(2).png";
        assert_eq!(
            found(
                "topics/2025/notes.md",
                r"![](../attachments/map\(2\).png)",
                &[a, b]
            ),
            [a]
        );
    }

    #[test]
    fn html_link_matches_case_insensitively_after_decoding_entities() {
        let path = "attachments/A&B.jpg";
        assert_eq!(
            found("note.md", r#"<img src="ATTACHMENTS/a&amp;b.JPG">"#, &[path]),
            [path]
        );
    }

    #[test]
    fn percent_encoded_filename_matches_within_other_syntax() {
        let path = "attachments/Férias #1.png";
        assert_eq!(
            found("note.md", "[[f%C3%A9rias%20%231.PNG]]", &[path]),
            [path]
        );
    }

    #[test]
    fn bare_name_substring_protects_every_copy() {
        let a = "a/attachments/Report.png";
        let b = "b/attachments/report.png";
        assert_eq!(found("note.md", "archive-REPORT.PNG-old", &[a, b]), [a, b]);
    }
}
