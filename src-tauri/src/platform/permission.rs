//! Site permission requests of the page window, answered by the app instead of WebView2's prompt,
//! which takes focus and appears in a corner of the desktop. WebView2 keeps no choice of its own:
//! one it saved would answer later requests without asking the app.
use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
    COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ, COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION,
    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    COREWEBVIEW2_PERMISSION_STATE_DEFAULT, COREWEBVIEW2_PERMISSION_STATE_DENY, ICoreWebView2,
    ICoreWebView2_13, ICoreWebView2Deferral, ICoreWebView2PermissionRequestedEventArgs,
    ICoreWebView2PermissionRequestedEventArgs2, ICoreWebView2PermissionRequestedEventArgs3,
    ICoreWebView2Profile4,
};
use webview2_com::{
    CoTaskMemPWSTR, GetNonDefaultPermissionSettingsCompletedHandler,
    NavigationStartingEventHandler, PermissionRequestedEventHandler,
    SetPermissionStateCompletedHandler,
};
use windows::core::{HSTRING, Interface, PWSTR};

use crate::error::Error;
use crate::permissions::Kind;

/// A request as the app sees it.
pub struct Asked {
    /// `None` for a kind the app never asks about.
    pub kind: Option<Kind>,
    /// The asking frame's URL.
    pub origin: String,
    /// The page's main-frame URL.
    pub top: String,
    /// The main-frame document that asked; None while a navigation is in flight, when the request
    /// may come from the document leaving while `top` already names the next one.
    pub document: Option<u64>,
}

/// A request WebView2 waits on until it is settled. Dropped unsettled, as when its page window is
/// gone, it is denied.
pub struct Held(
    Option<(
        ICoreWebView2PermissionRequestedEventArgs,
        ICoreWebView2Deferral,
    )>,
);

impl Held {
    pub fn settle(mut self, allow: bool) {
        self.complete(allow);
    }

    fn complete(&mut self, allow: bool) {
        let Some((args, deferral)) = self.0.take() else {
            return;
        };
        let state = if allow {
            COREWEBVIEW2_PERMISSION_STATE_ALLOW
        } else {
            COREWEBVIEW2_PERMISSION_STATE_DENY
        };
        // Never left at the default state: WebView2 would then show its own prompt.
        unsafe {
            if let Err(error) = args.SetState(state) {
                eprintln!("[permissions] state: {error}");
            }
            if let Err(error) = deferral.Complete() {
                eprintln!("[permissions] complete: {error}");
            }
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.complete(false);
    }
}

/// The kinds the panel asks about. Notifications would show outside the panel from a page that is
/// hidden most of the time; sensors, local fonts and MIDI serve fingerprinting more than a reference
/// panel; autoplay starts sound by itself; automatic downloads, file access and window management
/// need features the page window does not offer. Unknown and future kinds are denied too.
fn kind(kind: COREWEBVIEW2_PERMISSION_KIND) -> Option<Kind> {
    match kind {
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => Some(Kind::Location),
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => Some(Kind::Camera),
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => Some(Kind::Microphone),
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => Some(Kind::ClipboardRead),
        _ => None,
    }
}

fn text(
    read: impl FnOnce(&mut PWSTR) -> windows::core::Result<()>,
) -> windows::core::Result<String> {
    let mut value = PWSTR::null();
    read(&mut value)?;
    Ok(CoTaskMemPWSTR::from(value).to_string())
}

/// How long the page window waits for the profile's saved choices to be reset.
const RESET_WAIT: Duration = Duration::from_secs(5);

/// Resets what the page profile saved before the app answered requests itself, such as an "Allow"
/// given to WebView2's own prompt: a saved state answers a request without raising
/// `PermissionRequested`, so it would never reach the app. Sends `Ok` once every reset completed,
/// or the first failure. UI thread.
fn forget_saved(
    page: &ICoreWebView2,
    reply: Sender<windows::core::Result<()>>,
) -> windows::core::Result<()> {
    let profile =
        unsafe { page.cast::<ICoreWebView2_13>()?.Profile()? }.cast::<ICoreWebView2Profile4>()?;
    let target = profile.clone();
    let listed = GetNonDefaultPermissionSettingsCompletedHandler::create(Box::new(
        move |result, settings| {
            let reset_all = || -> windows::core::Result<u32> {
                result?;
                let Some(settings) = settings else {
                    return Ok(0);
                };
                let mut count = 0;
                unsafe { settings.Count(&mut count)? };
                let left = Rc::new(Cell::new(count));
                for index in 0..count {
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                    let origin = unsafe {
                        let setting = settings.GetValueAtIndex(index)?;
                        setting.PermissionKind(&mut kind)?;
                        text(|value| setting.PermissionOrigin(value))?
                    };
                    let (left, reply) = (left.clone(), reply.clone());
                    let done =
                        SetPermissionStateCompletedHandler::create(Box::new(move |result| {
                            match result {
                                Err(error) => {
                                    let _ = reply.send(Err(error));
                                }
                                Ok(()) => {
                                    left.set(left.get() - 1);
                                    if left.get() == 0 {
                                        let _ = reply.send(Ok(()));
                                    }
                                }
                            }
                            Ok(())
                        }));
                    unsafe {
                        target.SetPermissionState(
                            kind,
                            &HSTRING::from(origin),
                            COREWEBVIEW2_PERMISSION_STATE_DEFAULT,
                            &done,
                        )?;
                    }
                }
                Ok(count)
            };
            match reset_all() {
                Ok(0) => {
                    let _ = reply.send(Ok(()));
                }
                Ok(_) => {}
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            }
            Ok(())
        },
    ));
    unsafe { profile.GetNonDefaultPermissionSettings(&listed) }
}

/// Hands every permission request of the window, from any frame, to `requested` with a deferral,
/// so WebView2 shows no prompt and waits for `Held::settle`. `leaving` runs when a main-frame
/// navigation starts: the document whose requests wait may be about to go. Both run on the WebView2
/// UI thread, so they must return at once. Only for the page window, before its first navigation:
/// returns once the profile's saved choices are reset, and fails when they could not be, since one
/// left in place would answer without the app. Waits for the WebView2 UI thread, so never call it
/// from that thread.
pub fn install(
    window: &WebviewWindow,
    requested: impl Fn(Asked, Held) + Send + 'static,
    leaving: impl Fn() + Send + 'static,
) -> Result<(), Error> {
    if window.label() == "main" {
        return Err(Error::unexpected(
            "permission requests are for the page window",
        ));
    }
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let reply = sender.clone();
        let install = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let asked = PermissionRequestedEventHandler::create(Box::new(move |page, args| {
                let (Some(page), Some(args)) = (page, args) else {
                    return Ok(());
                };
                let mut raw = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut raw)?;
                let asked = Asked {
                    kind: kind(raw),
                    origin: text(|value| args.Uri(value))?,
                    top: text(|value| page.Source(value))?,
                    document: super::browser::document(),
                };
                // The answer covers this request only; the app remembers choices itself.
                if let Ok(args) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                    args.SetSavesInProfile(false)?;
                }
                if let Ok(args) = args.cast::<ICoreWebView2PermissionRequestedEventArgs2>() {
                    args.SetHandled(true)?;
                }
                let deferral = args.GetDeferral()?;
                requested(asked, Held(Some((args, deferral))));
                Ok(())
            }));
            let starting = NavigationStartingEventHandler::create(Box::new(move |_, _| {
                leaving();
                Ok(())
            }));
            // WebView2 owns handlers until the webview is destroyed.
            let mut token = 0;
            page.add_PermissionRequested(&asked, &mut token)?;
            page.add_NavigationStarting(&starting, &mut token)?;
            forget_saved(&page, sender)
        };
        if let Err(error) = install() {
            let _ = reply.send(Err(error));
        }
    })?;
    match receiver.recv_timeout(RESET_WAIT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Error::unexpected(format!("page permissions: {error}"))),
        Err(_) => Err(Error::unexpected("page permissions not reset")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND_AUTOPLAY, COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE,
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS, COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS,
        COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION,
    };

    // A kind mapped by mistake would be asked about, and could be allowed, under another name.
    #[test]
    fn only_the_asked_kinds_reach_the_panel() {
        assert_eq!(
            kind(COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION),
            Some(Kind::Location)
        );
        for other in [
            COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS,
            COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS,
            COREWEBVIEW2_PERMISSION_KIND_AUTOPLAY,
            COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE,
            COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION,
            COREWEBVIEW2_PERMISSION_KIND(99),
        ] {
            assert_eq!(kind(other), None, "{other:?}");
        }
    }
}
