//! WebView2 memory use while nobody sees the panel.
use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    ICoreWebView2_19,
};
use windows::core::Interface;

/// Asks WebView2 to trim memory, best effort and without waiting. Scripts keep running, so autosave
/// and the file watcher stay live. `TrySuspend` would pause them, and it requires a hidden
/// controller, while the parked main window stays visible off-screen to draw the edge tab.
pub fn set_memory_low(window: &WebviewWindow, low: bool) {
    let level = if low {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let label = window.label().to_owned();
    let queued = window.with_webview(move |view| unsafe {
        let result = view
            .controller()
            .CoreWebView2()
            .and_then(|page| page.cast::<ICoreWebView2_19>())
            .and_then(|page| page.SetMemoryUsageTargetLevel(level));
        if let Err(error) = result {
            eprintln!("[memory] {label}: {error}");
        }
    });
    if let Err(error) = queued {
        eprintln!("[memory] {error}");
    }
}
