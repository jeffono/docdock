//! Edge features foreign to the app. WebView2 brings Edge's accelerator keys and context menu
//! (downloads on Ctrl+J, print, save, history, view source, DevTools), which open dialogs or popup
//! windows: some cannot be closed in a panel, and any of them takes focus and closes the panel.
//! They are filtered natively, before any script runs, so the page needs no channel.
use tauri::{Emitter, Manager, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
    COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
    COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, ICoreWebView2_11,
    ICoreWebView2AcceleratorKeyPressedEventArgs2,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CoTaskMemPWSTR, ContextMenuRequestedEventHandler,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_ADD, VK_BROWSER_BACK, VK_BROWSER_FORWARD, VK_CONTROL, VK_F5,
    VK_F12, VK_LEFT, VK_MENU, VK_NUMPAD0, VK_OEM_MINUS, VK_OEM_PLUS, VK_RIGHT, VK_SHIFT,
    VK_SUBTRACT, VK_TAB,
};
use windows::core::{Interface, PWSTR};

use crate::error::Error;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Owner {
    /// The app's own UI: no browser accelerator applies.
    Ui,
    /// The untrusted page: reload, zoom and history still work as in a browser. Find is blocked:
    /// its bar is a separate popup window, so the panel loses focus and closes as it opens.
    Page,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Keys {
    key: VIRTUAL_KEY,
    ctrl: bool,
    shift: bool,
    alt: bool,
}

#[derive(PartialEq, Debug)]
enum Decision {
    /// Let WebView2 run its accelerator, if the key has one.
    Browser,
    /// The key still reaches the content as a plain key event, so the UI's editor sees Ctrl+J.
    Content,
    /// An app shortcut typed while the page has focus; the page never sees it.
    ToggleMode,
}

fn decide(owner: Owner, keys: Keys) -> Decision {
    let Keys {
        key,
        ctrl,
        shift,
        alt,
    } = keys;
    if owner == Owner::Page && key == VK_TAB && ctrl && !shift && !alt {
        return Decision::ToggleMode;
    }
    let digit_zero = VIRTUAL_KEY(u16::from(b'0'));
    let letter = |c: u8| key == VIRTUAL_KEY(u16::from(c));
    let allowed = match owner {
        // DevTools stay reachable while developing the UI.
        Owner::Ui => {
            cfg!(debug_assertions)
                && (key == VK_F12 || (ctrl && shift && !alt && (letter(b'I') || letter(b'C'))))
        }
        Owner::Page => {
            let combination = match (ctrl, shift, alt) {
                (false, _, false) => key == VK_F5,
                (true, _, false) => {
                    letter(b'R')
                        || [
                            VK_OEM_PLUS,
                            VK_OEM_MINUS,
                            VK_ADD,
                            VK_SUBTRACT,
                            VK_NUMPAD0,
                            digit_zero,
                        ]
                        .contains(&key)
                }
                (false, false, true) => key == VK_LEFT || key == VK_RIGHT,
                _ => false,
            };
            combination || key == VK_BROWSER_BACK || key == VK_BROWSER_FORWARD
        }
    };
    if allowed {
        Decision::Browser
    } else {
        Decision::Content
    }
}

/// Context menu items kept: editing everywhere, plus history and reload on the page (reloading the
/// UI would discard its state). Everything else (save, print, share, emoji, more tools, inspect,
/// open in a new window) is dropped, including names added later.
const PAGE_MENU: [&str; 3] = ["back", "forward", "reload"];
const EDIT_MENU: [&str; 9] = [
    "undo",
    "redo",
    "cut",
    "copy",
    "paste",
    "pasteAndMatchStyle",
    "selectAll",
    "copyLinkLocation",
    "copyImage",
];

/// Indices to remove, highest first so earlier removals keep later indices valid. `None` is a
/// separator: kept only between two kept items.
fn menu_removals(owner: Owner, items: &[Option<String>]) -> Vec<usize> {
    let kept = |name: &str| {
        EDIT_MENU.contains(&name)
            || match owner {
                Owner::Page => PAGE_MENU.contains(&name),
                // DevTools stay reachable while developing the UI.
                Owner::Ui => cfg!(debug_assertions) && name == "inspectElement",
            }
    };
    let mut keep: Vec<bool> = items
        .iter()
        .map(|item| item.as_deref().is_none_or(kept))
        .collect();
    let mut previous_command = false;
    for (index, item) in items.iter().enumerate() {
        if !keep[index] {
            continue;
        }
        if item.is_some() {
            previous_command = true;
        } else {
            keep[index] = previous_command;
            previous_command = false;
        }
    }
    // A separator left last, after the final command was dropped.
    if let Some(last) = (0..items.len()).rev().find(|&index| keep[index])
        && items[last].is_none()
    {
        keep[last] = false;
    }
    (0..items.len())
        .rev()
        .filter(|&index| !keep[index])
        .collect()
}

fn down(key: VIRTUAL_KEY) -> bool {
    unsafe { GetKeyState(i32::from(key.0)) < 0 }
}

/// Installs the filter. For the page, waits for it before the first navigation, so call it off
/// the WebView2 UI thread; for the UI, which loads only trusted content, it returns at once.
pub fn filter(window: &WebviewWindow, owner: Owner) -> Result<(), Error> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let app = window.app_handle().clone();
    window.with_webview(move |view| unsafe {
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            args.KeyEventKind(&mut kind)?;
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }
            let mut key = 0;
            args.VirtualKey(&mut key)?;
            let keys = Keys {
                key: VIRTUAL_KEY(key as u16),
                ctrl: down(VK_CONTROL),
                shift: down(VK_SHIFT),
                alt: down(VK_MENU),
            };
            match decide(owner, keys) {
                Decision::Browser => Ok(()),
                Decision::Content => args
                    .cast::<ICoreWebView2AcceleratorKeyPressedEventArgs2>()?
                    .SetIsBrowserAcceleratorKeyEnabled(false),
                Decision::ToggleMode => {
                    args.SetHandled(true)?;
                    let _ = app.emit_to("main", "panel-toggle-mode", ());
                    Ok(())
                }
            }
        }));
        let menu = ContextMenuRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let items = args.MenuItems()?;
            let mut count = 0;
            items.Count(&mut count)?;
            let mut described = Vec::new();
            for index in 0..count {
                let item = items.GetValueAtIndex(index)?;
                let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
                item.Kind(&mut kind)?;
                described.push(if kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR {
                    None
                } else {
                    let mut name = PWSTR::null();
                    item.Name(&mut name)?;
                    Some(CoTaskMemPWSTR::from(name).to_string())
                });
            }
            for index in menu_removals(owner, &described) {
                items.RemoveValueAtIndex(index as u32)?;
            }
            Ok(())
        }));
        // WebView2 owns handlers until the webview is destroyed.
        let install = || -> windows::core::Result<()> {
            let mut token = 0;
            view.controller()
                .add_AcceleratorKeyPressed(&handler, &mut token)?;
            view.controller()
                .CoreWebView2()?
                .cast::<ICoreWebView2_11>()?
                .add_ContextMenuRequested(&menu, &mut token)
        };
        let _ = sender.send(install());
    })?;
    if owner == Owner::Ui {
        return Ok(());
    }
    match receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Error::unexpected(format!("key filter: {error}"))),
        Err(_) => Err(Error::unexpected("key filter not installed")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(c: u8) -> Keys {
        Keys {
            key: VIRTUAL_KEY(u16::from(c)),
            ctrl: true,
            shift: false,
            alt: false,
        }
    }

    // Catches the unclosable Downloads dialog, and the find bar popup that closed the panel.
    #[test]
    fn downloads_history_print_and_save_never_open() {
        for owner in [Owner::Ui, Owner::Page] {
            for c in *b"JHPSOUNFG" {
                assert_eq!(
                    decide(owner, ctrl(c)),
                    Decision::Content,
                    "{owner:?} {}",
                    c as char
                );
            }
        }
    }

    #[test]
    fn the_page_keeps_reload_and_zoom_but_the_ui_does_not() {
        for c in *b"R0" {
            assert_eq!(decide(Owner::Page, ctrl(c)), Decision::Browser);
            assert_eq!(decide(Owner::Ui, ctrl(c)), Decision::Content);
        }
    }

    #[test]
    fn page_devtools_stay_closed() {
        let keys = Keys {
            shift: true,
            ..ctrl(b'I')
        };
        assert_eq!(decide(Owner::Page, keys), Decision::Content);
        let f12 = Keys {
            key: VK_F12,
            ctrl: false,
            ..ctrl(b'I')
        };
        assert_eq!(decide(Owner::Page, f12), Decision::Content);
    }

    fn menu(items: &[Option<&str>]) -> Vec<usize> {
        let items: Vec<_> = items.iter().map(|item| item.map(String::from)).collect();
        menu_removals(Owner::Page, &items)
    }

    // Catches Save as, Print and Inspect reaching the same dialogs the keys no longer open.
    #[test]
    fn the_context_menu_keeps_only_history_reload_and_editing() {
        let got = menu(&[
            Some("back"),
            Some("reload"),
            None,
            Some("saveAs"),
            Some("print"),
            None,
            Some("other"),
            None,
            Some("inspectElement"),
        ]);
        assert_eq!(got, [8, 7, 6, 5, 4, 3, 2]);
    }

    // Catches Reload in the UI menu discarding unsaved UI state.
    #[test]
    fn the_ui_menu_has_no_history_or_reload() {
        let items =
            [Some("back"), Some("reload"), None, Some("copy")].map(|item| item.map(String::from));
        assert_eq!(menu_removals(Owner::Ui, &items), [2, 1, 0]);
    }

    #[test]
    fn a_separator_stays_between_kept_groups() {
        assert_eq!(
            menu(&[Some("copy"), None, Some("print"), None, Some("paste")]),
            [3, 2]
        );
        assert_eq!(menu(&[None, Some("print"), Some("copy")]), [1, 0]);
    }

    // Catches the mode shortcut working only while the UI, not the page, has focus.
    #[test]
    fn ctrl_tab_on_the_page_switches_mode() {
        let tab = Keys {
            key: VK_TAB,
            ..ctrl(0)
        };
        assert_eq!(decide(Owner::Page, tab), Decision::ToggleMode);
        let back = Keys { shift: true, ..tab };
        assert_eq!(decide(Owner::Page, back), Decision::Content);
    }
}
