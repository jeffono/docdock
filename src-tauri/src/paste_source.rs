//! The title a note made from something pasted takes from the window it came from.

use crate::platform::SourceWindow;

/// Programs whose windows say nothing about what they put on the clipboard: a screenshot's
/// subject is the window the panel opened over instead.
const CAPTURE_TOOLS: [&str; 6] = [
    "screenclippinghost.exe",
    "snippingtool.exe",
    "screensketch.exe",
    "sharex.exe",
    "greenshot.exe",
    "lightshot.exe",
];
const SEPARATORS: [&str; 3] = [" - ", " — ", " – "];

/// A title for a note made from the clipboard, from the window that put it there (`owner`) or,
/// when that is a capture tool or this app (`own`, its program), the one the panel opened over
/// (`previous`). Without the program's own name at the end, as in "Page - Google Chrome".
pub fn note_title(
    owner: Option<&SourceWindow>,
    previous: Option<&SourceWindow>,
    own: &str,
) -> Option<String> {
    let telling = |window: &&SourceWindow| {
        window.program != own && !CAPTURE_TOOLS.contains(&window.program.as_str())
    };
    owner
        .filter(telling)
        .or(previous.filter(telling))
        .and_then(|window| {
            let title = without_program(window).trim().to_owned();
            (!title.is_empty()).then_some(title)
        })
}

/// The title without its last part, the program's name, as in "Page - Google Chrome", when it
/// has parts; a plain title, such as a folder's, stays whole. Edge also names its profile there.
fn without_program(window: &SourceWindow) -> &str {
    let title = window.title.trim();
    let mut cut = title;
    let parts = if window.program == "msedge.exe" { 2 } else { 1 };
    for _ in 0..parts {
        match SEPARATORS.iter().filter_map(|s| cut.rfind(s)).max() {
            Some(at) if at > 0 => cut = &cut[..at],
            _ => break,
        }
    }
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(title: &str, program: &str) -> SourceWindow {
        SourceWindow {
            title: title.to_owned(),
            program: program.to_owned(),
        }
    }

    // Catches a page's note named after the browser instead of the page.
    #[test]
    fn a_browser_page_keeps_its_own_title() {
        let chrome = window("Receitas de pão - Google Chrome", "chrome.exe");
        assert_eq!(
            note_title(Some(&chrome), None, "docdock.exe").as_deref(),
            Some("Receitas de pão")
        );
        let firefox = window("Mapa — Mozilla Firefox", "firefox.exe");
        assert_eq!(
            note_title(Some(&firefox), None, "docdock.exe").as_deref(),
            Some("Mapa")
        );
        let edge = window("Mapa - Pessoal - Microsoft\u{200b} Edge", "msedge.exe");
        assert_eq!(
            note_title(Some(&edge), None, "docdock.exe").as_deref(),
            Some("Mapa")
        );
    }

    // Catches a screenshot's note named "Snipping Tool" instead of after what it shows.
    #[test]
    fn a_capture_tool_defers_to_the_window_the_panel_opened_over() {
        let tool = window("Ferramenta de Captura", "snippingtool.exe");
        let page = window("Planilha de custos - Excel", "excel.exe");
        assert_eq!(
            note_title(Some(&tool), Some(&page), "docdock.exe").as_deref(),
            Some("Planilha de custos")
        );
        assert_eq!(note_title(Some(&tool), None, "docdock.exe"), None);
    }

    #[test]
    fn this_app_names_nothing() {
        let own = window("DocDock", "docdock.exe");
        assert_eq!(note_title(Some(&own), Some(&own), "docdock.exe"), None);
    }

    #[test]
    fn a_title_without_parts_stays_whole() {
        let folder = window("Fotos", "explorer.exe");
        assert_eq!(
            note_title(Some(&folder), None, "docdock.exe").as_deref(),
            Some("Fotos")
        );
    }
}
