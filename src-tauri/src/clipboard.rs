//! Windows Clipboard History (`Win+V`) mirror service. Owns no persistent state: reads live
//! system snapshots on demand from a dedicated STA worker and forwards change notifications to `main`.

use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

use crate::error::Error;
use crate::platform::{ClipboardHandle, ClipboardSnapshot, start_clipboard_worker};

pub const EVENT: &str = "clipboard-changed";

pub struct ClipboardService(Option<ClipboardHandle>);

impl ClipboardService {
    pub fn start(app: AppHandle) -> Self {
        let handle = match start_clipboard_worker(move || {
            if let Err(error) = app.emit_to("main", EVENT, ()) {
                eprintln!("[clipboard] failed to notify the UI: {error}");
            }
        }) {
            Ok(handle) => Some(handle),
            Err(error) => {
                eprintln!("[clipboard] worker unavailable: {error}");
                None
            }
        };
        Self(handle)
    }

    pub fn snapshot(&self) -> Result<ClipboardSnapshot, Error> {
        let Some(handle) = &self.0 else {
            return Err(Error::unexpected("clipboard worker unavailable"));
        };
        handle.snapshot().map_err(Error::unexpected)
    }

    pub fn restore_item(&self, id: String) -> Result<bool, Error> {
        let Some(handle) = &self.0 else {
            return Err(Error::unexpected("clipboard worker unavailable"));
        };
        handle.restore_item(id).map_err(Error::unexpected)
    }

    pub fn delete_item(&self, id: String) -> Result<bool, Error> {
        let Some(handle) = &self.0 else {
            return Err(Error::unexpected("clipboard worker unavailable"));
        };
        handle.delete_item(id).map_err(Error::unexpected)
    }
}

pub fn open_settings(app: &AppHandle) -> Result<(), Error> {
    app.opener()
        .open_url("ms-settings:clipboard", None::<&str>)
        .map_err(Error::unexpected)
}
