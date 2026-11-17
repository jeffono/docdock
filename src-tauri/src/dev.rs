//! Development builds only: the tweak window and the commands behind it. `lib.rs` declares this
//! module under `cfg(debug_assertions)`, so release builds contain none of it.
//!
//! The tweak window edits; the panel applies. The window sends its whole state to the panel with the
//! `tweak-state` event, and the panel asks for it with `tweak-request` after a reload. The window
//! reaches no app command: `main_only` rejects them, and its capability only grants events. So the
//! panel calls every command here, including `dev_fonts`, whose list it forwards to the window.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use tauri::ipc::CapabilityBuilder;
use tauri::webview::NewWindowResponse;
use tauri::{App, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::handle::Logical;

const TWEAK: &str = "tweak";

/// Lets the tweak window exchange events with the panel, and nothing else.
pub fn setup(app: &App) -> tauri::Result<()> {
    app.add_capability(
        CapabilityBuilder::new(TWEAK)
            .window(TWEAK)
            .permission("core:event:allow-listen")
            .permission("core:event:allow-unlisten")
            .permission("core:event:allow-emit-to"),
    )
}

/// The panel applies the tweak window's slide motions, from the next slide on.
#[tauri::command]
pub fn dev_motion(motions: crate::motion::Motions) {
    crate::motion::tune(motions);
}

static ACTIVATION: AtomicU32 = AtomicU32::new(crate::handle::TUNING.activation);
static REVEAL: AtomicU32 = AtomicU32::new(crate::handle::TUNING.reveal);
static RUSH: AtomicU32 = AtomicU32::new(crate::handle::TUNING.rush);
/// One lock, so a reader never sees a depth, height and curve from different changes.
static TAB: Mutex<Logical> = Mutex::new(crate::handle::LOGICAL);

/// The tuned edge handle values.
pub fn handle_tuning() -> crate::handle::Tuning {
    crate::handle::Tuning {
        activation: ACTIVATION.load(Ordering::Relaxed),
        reveal: REVEAL.load(Ordering::Relaxed),
        rush: RUSH.load(Ordering::Relaxed),
        tab: *TAB.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
    }
}

/// The panel applies the tweak window's edge handle values, from the next cursor poll on.
#[tauri::command]
pub fn dev_handle(
    activation_percent: u32,
    reveal_percent: u32,
    rush_speed: u32,
) -> Result<(), String> {
    if !(100..=1000).contains(&activation_percent)
        || !(10..=1000).contains(&reveal_percent)
        || !(500..=20000).contains(&rush_speed)
    {
        return Err("Handle values out of range".into());
    }
    ACTIVATION.store(activation_percent, Ordering::Relaxed);
    REVEAL.store(reveal_percent, Ordering::Relaxed);
    RUSH.store(rush_speed, Ordering::Relaxed);
    Ok(())
}

/// Whether the tweak window's tab can be drawn and shaped: within the ranges it offers, its two
/// curves fitting in its height.
fn valid_tab(tab: Logical) -> bool {
    (4..=64).contains(&tab.depth) && (40..=400).contains(&tab.height) && tab.curve <= tab.height / 2
}

/// The panel takes the tweak window's tab shape at its next cursor poll: it resizes its window,
/// reshapes its region and sends the shape to the UI, which redraws the tab.
#[tauri::command]
pub fn dev_tab_shape(depth: u32, height: u32, curve: u32) -> Result<(), String> {
    let tab = Logical {
        depth,
        height,
        curve,
    };
    if !valid_tab(tab) {
        return Err("Tab shape out of range".into());
    }
    *TAB.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = tab;
    Ok(())
}

/// Installed font families, for the tweak window's font picker.
#[tauri::command]
pub fn dev_fonts() -> Result<Vec<String>, String> {
    crate::platform::fonts::families()
}

/// Opens the tweak window, an ordinary resizable window, or brings it forward if already open.
/// Asynchronous because creating a window in a synchronous command hangs on Windows.
#[tauri::command]
pub async fn dev_tweak_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(TWEAK) {
        let _ = window.unminimize();
        return window.set_focus().map_err(|error| error.to_string());
    }
    let ui = crate::main_window::ui_url(app.config().build.dev_url.as_ref());
    WebviewWindowBuilder::new(&app, TWEAK, WebviewUrl::App("index.html".into()))
        .title("DocDock tweaks")
        .inner_size(880.0, 860.0)
        .min_inner_size(320.0, 320.0)
        .theme(crate::notes::theme(&app).native())
        .on_navigation(move |url| crate::main_window::stays_in_ui(url, &ui))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches the release shape falling outside what the tweak window may set, which would
    /// refuse a preset of the defaults.
    #[test]
    fn the_release_tab_is_valid() {
        assert!(valid_tab(crate::handle::LOGICAL));
    }
}
