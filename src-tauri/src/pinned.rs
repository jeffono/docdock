//! Pure rules for `_pinned.md` ([0005](../../docs/decisions/0005-pinned-items.md)). Text in, text out:
//! reading and writing the file belongs to `notes.rs`.
//!
//! Every edit works on whole lines and never rewrites the file: lines the app does not own (headings,
//! comments, prose) stay byte for byte, and each line keeps its own terminator, so CRLF, LF, CR and mixed
//! files survive. An item is a single-line list entry that contains a link; continuation lines are not
//! part of it.

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Note,
    Url,
}

/// A pinned item. `target` is a note's path relative to the folder (decoded, with `/`) or an http(s) URL.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Item {
    pub target: String,
    pub title: String,
    pub kind: Kind,
}

/// A line without its terminator, and the terminator (`\r\n`, `\n`, `\r`, or empty on a last line
/// without one).
struct Line<'a> {
    body: &'a str,
    end: &'a str,
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        let end = match bytes[i] {
            b'\n' => 1,
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            _ => {
                i += 1;
                continue;
            }
        };
        found.push(Line {
            body: &text[start..i],
            end: &text[i..i + end],
        });
        i += end;
        start = i;
    }
    if start < bytes.len() {
        found.push(Line {
            body: &text[start..],
            end: "",
        });
    }
    found
}

/// The pinned items in file order.
pub fn parse(text: &str) -> Vec<Item> {
    lines(text).iter().filter_map(|l| item(l.body)).collect()
}

/// Items with the index of their line.
fn items(lines: &[Line]) -> Vec<(usize, Item)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| Some((i, item(l.body)?)))
        .collect()
}

fn item(body: &str) -> Option<Item> {
    let rest = after_marker(body)?;
    let mut from = 0;
    while let Some(open) = rest[from..].find('[').map(|i| i + from) {
        from = open + 1;
        // `![alt](x)` is an image and `\[` is text.
        if rest[..open].ends_with(['!', '\\']) {
            continue;
        }
        let Some(close) = closing_bracket(rest, open) else {
            continue;
        };
        let Some(destination) = destination(&rest[close + 1..]) else {
            continue;
        };
        let (kind, target) = classify(destination)?;
        return Some(Item {
            target,
            title: unescape(&rest[open + 1..close]),
            kind,
        });
    }
    None
}

/// The text after a list marker (`-`, `*`, `+`, `1.` or `1)`), or `None` if the line is not a list item.
fn after_marker(body: &str) -> Option<&str> {
    let s = body.trim_start_matches([' ', '\t']);
    let after = if let Some(rest) = s.strip_prefix(['-', '*', '+']) {
        rest
    } else {
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        s[digits..]
            .strip_prefix(['.', ')'])
            .filter(|_| digits > 0)?
    };
    after
        .starts_with([' ', '\t'])
        .then(|| after.trim_start_matches([' ', '\t']))
}

/// The `]` matching the `[` at `open`, skipping escaped brackets.
fn closing_bracket(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut escaped = false;
    for (i, c) in s[open..].char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The destination of `(destination "optional title")` at the start of `s`.
fn destination(s: &str) -> Option<&str> {
    let inner = s.strip_prefix('(')?.trim_start();
    let (destination, rest) = if let Some(angle) = inner.strip_prefix('<') {
        angle.split_once('>')?
    } else {
        let mut depth = 0;
        let end = inner
            .char_indices()
            .find(|&(_, c)| match c {
                '(' => {
                    depth += 1;
                    false
                }
                ')' if depth > 0 => {
                    depth -= 1;
                    false
                }
                ')' => true,
                c => c.is_whitespace(),
            })
            .map_or(inner.len(), |(i, _)| i);
        inner.split_at(end)
    };
    rest.trim_start()
        .starts_with([')', '"', '\''])
        .then_some(destination)
}

fn classify(destination: &str) -> Option<(Kind, String)> {
    let lower = destination.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some((Kind::Url, destination.to_owned()));
    }
    // Other schemes (`mailto:`, drive letters) and in-page anchors are not notes.
    let scheme = destination.split_once(':').is_some_and(|(s, _)| {
        s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
    });
    let path = destination.split('#').next().unwrap_or_default();
    if scheme || path.is_empty() {
        return None;
    }
    let decoded = percent_decode_str(path)
        .decode_utf8()
        .map_or_else(|_| path.to_owned(), |d| d.into_owned());
    let target = decoded.strip_prefix("./").unwrap_or(&decoded);
    Some((Kind::Note, target.to_owned()))
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(next) if c == '\\' && next.is_ascii_punctuation() => {}
            _ => out.push(c),
        }
    }
    out
}

/// Windows paths are case-insensitive; URLs are compared exactly.
fn same(a: &Item, target: &str) -> bool {
    match a.kind {
        Kind::Url => a.target == target,
        Kind::Note => a.target.to_lowercase() == target.to_lowercase(),
    }
}

/// Characters that would end or change a markdown link destination. Non-ASCII stays readable.
const PATH: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'(')
    .add(b')')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b']')
    .add(b'\\');
/// A URL is already encoded; only what would end the link needs escaping.
const URL: &AsciiSet = &CONTROLS.add(b' ').add(b'(').add(b')').add(b'<').add(b'>');

fn link_line(kind: Kind, target: &str, title: &str) -> String {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = if title.is_empty() { target } else { &title };
    let mut escaped = String::new();
    for c in title.chars() {
        if matches!(c, '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    let set = if kind == Kind::Url { URL } else { PATH };
    format!("- [{escaped}]({})", utf8_percent_encode(target, set))
}

/// Pins `target` with one new line after the last item, or at the end of the file if there is none. The
/// line uses the file's first line terminator (LF in an empty file), as the editor does for new lines.
/// An item already pinned leaves the text unchanged.
pub fn pin(text: &str, kind: Kind, target: &str, title: &str) -> String {
    let lines = lines(text);
    let items = items(&lines);
    if items.iter().any(|(_, item)| same(item, target)) {
        return text.to_owned();
    }
    let sep = lines
        .iter()
        .map(|l| l.end)
        .find(|end| !end.is_empty())
        .unwrap_or("\n");
    let new = link_line(kind, target, title);
    let at = items.last().map_or(lines.len(), |(i, _)| i + 1);
    let mut out = String::with_capacity(text.len() + new.len() + 2);
    for (k, line) in lines.iter().enumerate() {
        if k == at {
            out.push_str(&new);
            out.push_str(sep);
        }
        out.push_str(line.body);
        out.push_str(line.end);
    }
    if at == lines.len() {
        // The previous last line may have had no terminator.
        if lines.last().is_some_and(|l| l.end.is_empty()) {
            out.push_str(sep);
        }
        out.push_str(&new);
        out.push_str(sep);
    }
    out
}

/// Removes the lines of every item pointing to `target` and nothing else.
pub fn unpin(text: &str, target: &str) -> String {
    lines(text)
        .iter()
        .filter(|l| !item(l.body).is_some_and(|item| same(&item, target)))
        .flat_map(|l| [l.body, l.end])
        .collect()
}

/// Moves the item pointing to `target` to position `to` among the items (clamped to the last). Only the
/// item lines trade places: the lines between them, and each line's terminator, stay where they are.
pub fn move_item(text: &str, target: &str, to: usize) -> String {
    let lines = lines(text);
    let items = items(&lines);
    let Some(from) = items.iter().position(|(_, item)| same(item, target)) else {
        return text.to_owned();
    };
    let mut order: Vec<usize> = (0..items.len()).collect();
    let moved = order.remove(from);
    order.insert(to.min(items.len() - 1), moved);
    let mut bodies: Vec<&str> = lines.iter().map(|l| l.body).collect();
    for (slot, &source) in order.iter().enumerate() {
        bodies[items[slot].0] = lines[items[source].0].body;
    }
    lines
        .iter()
        .zip(bodies)
        .flat_map(|(l, body)| [body, l.end])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(text: &str) -> Vec<(String, Kind)> {
        parse(text)
            .into_iter()
            .map(|i| (i.target, i.kind))
            .collect()
    }

    #[test]
    fn parses_a_relative_link_with_encoded_spaces() {
        let items = parse("- [My idea](sub/my%20idea.md)\n");
        assert_eq!(
            items,
            [Item {
                target: "sub/my idea.md".into(),
                title: "My idea".into(),
                kind: Kind::Note
            }]
        );
    }

    #[test]
    fn parses_urls_and_keeps_file_order() {
        let text = "1. [Page](https://example.com/a?b=1)\n* [Note](./n.md \"hint\")\n+ [Angle](<a b.md>)\n";
        assert_eq!(
            targets(text),
            [
                ("https://example.com/a?b=1".into(), Kind::Url),
                ("n.md".into(), Kind::Note),
                ("a b.md".into(), Kind::Note)
            ]
        );
    }

    #[test]
    fn ignores_lines_that_are_not_items() {
        let text = "# Pinned\n<!-- [x](hidden.md) -->\n[plain](a.md)\n- no link here\n- ![img](a.png)\n- [mail](mailto:a@b.c)\n- [anchor](#top)\n  continuation [c](c.md)\n-[nospace](n.md)\n- [broken(a.md)\n";
        assert!(parse(text).is_empty());
    }

    #[test]
    fn finds_the_link_after_a_task_checkbox() {
        assert_eq!(
            targets("- [ ] [Todo](todo.md)\n"),
            [("todo.md".into(), Kind::Note)]
        );
    }

    #[test]
    fn pin_appends_after_the_last_item_with_the_file_line_ending() {
        let text = "# Pinned\r\n- [A](a.md)\r\n\r\nMy notes\r\n";
        assert_eq!(
            pin(text, Kind::Note, "b c.md", "B"),
            "# Pinned\r\n- [A](a.md)\r\n- [B](b%20c.md)\r\n\r\nMy notes\r\n"
        );
    }

    #[test]
    fn pin_appends_at_the_end_when_there_are_no_items() {
        assert_eq!(
            pin("# Pinned", Kind::Note, "a.md", "A"),
            "# Pinned\n- [A](a.md)\n"
        );
        assert_eq!(
            pin("# Pinned\r", Kind::Note, "a.md", "A"),
            "# Pinned\r- [A](a.md)\r"
        );
        assert_eq!(pin("", Kind::Note, "a.md", "A"), "- [A](a.md)\n");
    }

    #[test]
    fn pin_after_an_unterminated_last_item_keeps_its_content() {
        assert_eq!(
            pin("- [A](a.md)", Kind::Note, "b.md", "B"),
            "- [A](a.md)\n- [B](b.md)\n"
        );
    }

    #[test]
    fn pin_leaves_an_already_pinned_item_alone() {
        let text = "- [A](A.md)\r\n";
        assert_eq!(pin(text, Kind::Note, "a.md", "other"), text);
    }

    #[test]
    fn pin_escapes_what_would_break_the_link() {
        let text = pin("", Kind::Note, "x/a (1)#2.md", "[draft] a\nb");
        assert_eq!(text, "- [\\[draft\\] a b](x/a%20%281%29%232.md)\n");
        assert_eq!(parse(&text)[0].target, "x/a (1)#2.md");
        assert_eq!(parse(&text)[0].title, "[draft] a b");
    }

    #[test]
    fn unpin_removes_only_the_matching_line() {
        let text = "# Pinned\r\n- [A](a.md)\n- [B](b.md)\r\nnote [B](b.md)\r\n- [C](c.md)";
        assert_eq!(
            unpin(text, "b.md"),
            "# Pinned\r\n- [A](a.md)\nnote [B](b.md)\r\n- [C](c.md)"
        );
    }

    #[test]
    fn unpin_of_a_missing_target_changes_nothing() {
        let text = "- [A](a.md)\r\n";
        assert_eq!(unpin(text, "b.md"), text);
    }

    #[test]
    fn move_swaps_only_item_lines() {
        let text = "# Pinned\r\n- [A](a.md)\r\nkeep me\n- [B](b.md)\r\n- [C](c.md)";
        assert_eq!(
            move_item(text, "c.md", 0),
            "# Pinned\r\n- [C](c.md)\r\nkeep me\n- [A](a.md)\r\n- [B](b.md)"
        );
    }

    #[test]
    fn move_to_a_position_past_the_end_goes_last() {
        let text = "- [A](a.md)\n- [B](b.md)\n";
        assert_eq!(move_item(text, "a.md", 9), "- [B](b.md)\n- [A](a.md)\n");
    }

    #[test]
    fn move_of_a_missing_target_changes_nothing() {
        let text = "- [A](a.md)\n- [B](b.md)\n";
        assert_eq!(move_item(text, "z.md", 0), text);
    }
}
