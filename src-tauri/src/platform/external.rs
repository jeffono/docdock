//! Associated-app launch through the same opener used for external links.
use crate::error::Error;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Caller must validate the saved Cryptomator file before entering this effect layer.
pub fn open_file(app: &AppHandle, path: &str) -> Result<(), Error> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(Error::unexpected)
}
