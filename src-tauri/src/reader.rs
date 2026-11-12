//! Reader view: the article of the shown page, extracted in Rust from the page's own DOM and
//! returned as Markdown, which the UI renders with its allowlisted model like any note. The page
//! gets no channel: Rust reads it through the same isolated-world evaluation as the selection.

use std::time::Duration;

use dom_smoothie::{Config, Readability};

use crate::error::Error;

/// Bounds the HTML read from the page, in UTF-16 units; the script's budget and slice use this
/// number.
const HTML_MAX_CHARS: usize = 1_500_000;
/// At most six JSON bytes per UTF-16 unit, plus the title.
const READER_MAX_JSON: usize = 6 * HTML_MAX_CHARS + 64 * 1024;
/// The script copies at most this many elements; a page with more is cut there.
const COPIED_ELEMENTS: usize = 100_000;
/// Parsing the copy can add elements, such as `tbody`; a page with more is refused before scoring.
const MAX_ELEMENTS: usize = 2 * COPIED_ELEMENTS;
/// Deeper element nesting is refused, by the script and again here: parsing it takes quadratic
/// time and extraction walks the tree recursively. Chromium's parser stops nesting at 512, so only
/// a tree built by script goes deeper.
const MAX_DEPTH: usize = 512;
/// Half the note size limit, so the saved note always fits.
const MARKDOWN_MAX_BYTES: usize = 1024 * 1024;
/// Images are dropped: the UI's CSP blocks remote images. Media and embedded documents have no
/// text to keep; scripts and styles are never text.
const SKIPPED: &[&str] = &[
    "script", "style", "meta", "head", "img", "picture", "svg", "video", "audio", "iframe",
    "object", "embed", "canvas", "noscript", "template",
];
/// Fixed: no data is ever interpolated. Runs in the isolated world, so the builtins are the
/// browser's, and no page code runs while it walks. It copies the page node by node into a
/// document without a browsing context: no custom element constructor, script or load of the page
/// runs for the copy, and shadow roots stay behind. What extraction drops anyway is never copied;
/// JSON-LD stays for metadata. Every length is counted before its copy is made: the copy stops at
/// `COPIED_ELEMENTS` elements or `HTML_MAX_CHARS` units of markup and text, so a huge page costs
/// neither memory nor a full walk. Escaping can still lengthen the HTML, so the slice stays. A
/// tree deeper than `MAX_DEPTH` answers empty: parsing deep nesting again takes quadratic time.
const READER_SCRIPT: &str = r#"(() => {
  const source = document.documentElement;
  if (!source) return ["", ""];
  const copy = document.implementation.createHTMLDocument("");
  const dropped = new Set(["style", "link", "noscript", "template", "svg", "iframe", "object",
    "embed", "canvas", "video", "audio", "picture", "img", "source"]);
  let elements = 100000;
  let chars = 1500000;
  // The copy of `node`, or null to leave it and its children out. A negative `chars` ends the walk.
  const take = (node) => {
    chars--;
    if (node.nodeType === 3) {
      const length = Math.min(node.length, chars);
      chars -= node.length;
      return length > 0 ? copy.createTextNode(node.substringData(0, length)) : null;
    }
    if (node.nodeType !== 1) return null;
    const name = node.localName;
    if (dropped.has(name) ||
        (name === "script" && node.getAttribute("type") !== "application/ld+json")) return null;
    if (--elements < 0) {
      chars = -1;
      return null;
    }
    // Chromium finds each attribute by a linear search, so only the first 100 are read.
    const attributes = Array.prototype.slice.call(node.attributes, 0, 100);
    chars -= 2 * name.length + 5;
    for (const attribute of attributes) {
      if (chars < 0) return null;
      chars -= attribute.name.length + attribute.value.length + 4;
    }
    if (chars < 0) return null;
    let element;
    try {
      element = copy.createElementNS(node.namespaceURI,
        node.prefix ? node.prefix + ":" + name : name);
    } catch {
      return null;
    }
    for (const attribute of attributes) {
      try {
        element.setAttributeNS(attribute.namespaceURI, attribute.name, attribute.value);
      } catch {}
    }
    return element;
  };
  const root = take(source);
  if (!root) return ["", ""];
  copy.replaceChild(root, copy.documentElement);
  // `parent` is always the copy of `node`'s parent; only copied elements are entered.
  let node = source;
  let target = root;
  let parent = copy;
  let depth = 0;
  while (chars >= 0) {
    if (target && target.nodeType === 1 && node.firstChild) {
      if (++depth > 512) return ["", ""];
      parent = target;
      node = node.firstChild;
    } else {
      while (node !== source && !node.nextSibling) {
        node = node.parentNode;
        parent = parent.parentNode;
        depth--;
      }
      if (node === source) break;
      node = node.nextSibling;
    }
    target = take(node);
    if (target) parent.appendChild(target);
  }
  return [copy.title.slice(0, 4096), root.outerHTML.slice(0, 1500000)]
    .map((text) => text.toWellFormed());
})()"#;
/// Copying a large page takes a moment; a hostile or hung page delays the command at most this.
const READER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(serde::Serialize, Debug)]
pub struct Article {
    url: String,
    title: String,
    /// Untrusted: the UI renders it only through its allowlisted model.
    markdown: String,
}

/// The shown page's article. None without an http(s) page, or when a navigation started while it
/// was read: the article would cover the page the user went to.
pub fn read(app: &tauri::AppHandle) -> Result<Option<Article>, Error> {
    let navigation = crate::platform::browser::navigation();
    let Some((url, json)) = crate::web::read_page(app, READER_SCRIPT, READER_TIMEOUT)? else {
        return Ok(None);
    };
    let (title, html) = json
        .and_then(|json| crate::web::two_strings(&json, READER_MAX_JSON))
        .ok_or(Error::PageNotRead)?;
    let (title, markdown) = extract(&html, &url, &title).ok_or(Error::NoArticle)?;
    if crate::platform::browser::navigation() != navigation {
        return Ok(None);
    }
    Ok(Some(Article {
        url,
        title,
        markdown,
    }))
}

/// Title and Markdown body of the article in `html`, or None if there is no text to read.
fn extract(html: &str, url: &str, page_title: &str) -> Option<(String, String)> {
    let document = dom_query::Document::from(html);
    if too_deep(&document) {
        return None;
    }
    let config = Config {
        max_elements_to_parse: MAX_ELEMENTS,
        ..Config::default()
    };
    let article = Readability::with_document(document, Some(url), Some(config))
        .ok()?
        .parse()
        .ok()?;
    let content = dom_query::Document::from(article.content);
    keep_safe_links(&content);
    let markdown = content.md(Some(SKIPPED));
    let markdown = bounded(&clean(&markdown));
    if markdown.is_empty() {
        return None;
    }
    let title = Some(crate::web::clean_title(&article.title))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| crate::web::clean_title(page_title));
    Some((title, markdown))
}

/// Links keep only web and email destinations; others become their text. Relative links are
/// already absolute here, and in-page anchors such as footnote markers would point at ids the
/// article no longer has.
fn keep_safe_links(content: &dom_query::Document) {
    for link in content.select("a[href]").nodes() {
        let href = link
            .attr("href")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| href.starts_with(scheme))
        {
            link.remove_attr("href");
        }
    }
}

/// Walks iteratively, so the check itself cannot exhaust the stack. The document node and a
/// text node add two levels to the element depth.
fn too_deep(document: &dom_query::Document) -> bool {
    let mut stack = vec![(document.root(), 0)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH + 2 {
            return true;
        }
        let mut child = node.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            stack.push((next, depth + 1));
        }
    }
    false
}

/// LF line endings; no controls other than newline and tab.
fn clean(markdown: &str) -> String {
    markdown
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Cut at the last paragraph break within `MARKDOWN_MAX_BYTES`, so no construct is split.
fn bounded(markdown: &str) -> String {
    if markdown.len() <= MARKDOWN_MAX_BYTES {
        return markdown.to_owned();
    }
    let mut end = MARKDOWN_MAX_BYTES;
    while !markdown.is_char_boundary(end) {
        end -= 1;
    }
    let head = &markdown[..end];
    head[..head.rfind("\n\n").unwrap_or(end)]
        .trim_end()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://example.com/news/story";

    fn page(body: &str) -> String {
        let paragraph = "<p>Readers keep coming back to this paragraph, which holds enough words \
            and commas, so the scorer counts it as the article's real content, not navigation.</p>";
        format!(
            "<html><head><title>Story</title></head><body><nav><a href='/'>Home</a></nav>\
             <article><h1>Story</h1>{paragraph}{body}{paragraph}{paragraph}</article></body></html>"
        )
    }

    fn markdown(body: &str) -> String {
        extract(&page(body), URL, "Story").unwrap().1
    }

    #[test]
    fn the_article_becomes_markdown_with_absolute_links() {
        let text = markdown("<p>See <a href='/docs'>the docs</a> and <strong>this</strong>.</p>");
        assert!(
            text.contains("[the docs](https://example.com/docs)"),
            "{text}"
        );
        assert!(text.contains("**this**"), "{text}");
        assert!(!text.contains("Home"), "{text}");
    }

    #[test]
    fn scripts_styles_and_handlers_never_reach_the_markdown() {
        let text = markdown(
            "<script>alert('script')</script><style>p{color:red}</style>\
             <p onclick=\"alert('handler')\">Text</p>",
        );
        assert!(!text.contains("alert"), "{text}");
        assert!(!text.contains("color:red"), "{text}");
    }

    #[test]
    fn markup_in_page_text_stays_escaped_text() {
        let text = markdown("<p>&lt;img src=x onerror=alert(1)&gt; and ![x](a.png)</p>");
        assert!(!text.contains("![x](a.png)"), "{text}");
        assert!(text.contains("img src=x"), "{text}");
    }

    #[test]
    fn script_and_data_links_lose_their_destination() {
        let text = markdown(
            "<p><a href='javascript:alert(1)'>run</a> <a href='data:text/html,x'>data</a></p>",
        );
        assert!(!text.contains("javascript:"), "{text}");
        assert!(!text.contains("data:"), "{text}");
    }

    #[test]
    fn images_are_dropped() {
        let text = markdown(
            "<p>Before <img src='https://example.com/a.png' alt='photo'> after \
             <a href='/big'><img src='/b.png' alt='thumb'></a></p>",
        );
        assert!(!text.contains("a.png") && !text.contains("b.png"), "{text}");
        assert!(!text.contains("photo") && !text.contains("thumb"), "{text}");
    }

    #[test]
    fn a_page_without_text_has_no_article() {
        assert_eq!(extract("<html><body></body></html>", URL, "Empty"), None);
    }

    #[test]
    fn deep_nesting_is_refused() {
        let depth = MAX_DEPTH + 10;
        let html = format!(
            "<html><body>{}text{}</body></html>",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        );
        assert_eq!(extract(&html, URL, "Deep"), None);
    }

    /// Extraction recurses over the tree; a stack overflow would abort the app.
    #[test]
    fn the_deepest_accepted_page_fits_the_stack() {
        let depth = MAX_DEPTH - 8;
        let html = page(&format!(
            "{}<p>deep</p>{}",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        ));
        assert!(!too_deep(&dom_query::Document::from(html.as_str())));
        let worker = std::thread::Builder::new().stack_size(2 * 1024 * 1024);
        let text = worker
            .spawn(move || extract(&html, URL, "Deep"))
            .unwrap()
            .join()
            .unwrap();
        assert!(text.is_some());
    }

    #[test]
    fn long_articles_are_cut_at_a_paragraph_break() {
        let paragraph = format!("{}\n\n", "word ".repeat(1000));
        let long = paragraph.repeat(MARKDOWN_MAX_BYTES / paragraph.len() + 2);
        let cut = bounded(&long);
        assert!(cut.len() <= MARKDOWN_MAX_BYTES);
        assert!(cut.ends_with("word"));
    }

    #[test]
    fn controls_are_removed_and_line_endings_become_lf() {
        assert_eq!(clean("a\r\nb\rc\u{0}d\u{1b}e\tf"), "a\nb\ncde\tf");
    }

    #[test]
    fn the_page_title_is_used_when_the_article_has_none() {
        let html = page("").replace("<title>Story</title>", "");
        let (title, _) = extract(&html.replace("<h1>Story</h1>", ""), URL, "Fallback").unwrap();
        assert_eq!(title, "Fallback");
    }
}
