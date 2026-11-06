//! What the untrusted page can reach without `on_navigation`, which WebView2 raises only for the main
//! frame: iframes navigate on their own and any frame can ask Windows to launch an external scheme.
use std::sync::mpsc;

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_18;
use webview2_com::{
    CoTaskMemPWSTR, LaunchingExternalUriSchemeEventHandler, NavigationStartingEventHandler,
};
use windows::core::{Interface, PWSTR};

use crate::error::Error;

/// Cancels subframe navigations that `frame_allowed` rejects, and every external scheme launch
/// (`mailto:`, `ms-settings:`, any registered app) before WebView2 offers to open the app.
/// Waits for the WebView2 UI thread, so never call it from that thread.
pub fn guard(window: &WebviewWindow, frame_allowed: fn(&str) -> bool) -> Result<(), Error> {
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let install = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let frames = NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    if !frame_allowed(&CoTaskMemPWSTR::from(uri).to_string()) {
                        args.SetCancel(true)?;
                    }
                }
                Ok(())
            }));
            let external = LaunchingExternalUriSchemeEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetCancel(true)?;
                }
                Ok(())
            }));
            // WebView2 owns handlers until the webview is destroyed.
            let mut token = 0;
            page.add_FrameNavigationStarting(&frames, &mut token)?;
            page.cast::<ICoreWebView2_18>()?
                .add_LaunchingExternalUriScheme(&external, &mut token)
        };
        let _ = sender.send(install());
    })?;
    match receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Error::unexpected(format!("page guard: {error}"))),
        Err(_) => Err(Error::unexpected("page guard not installed")),
    }
}
