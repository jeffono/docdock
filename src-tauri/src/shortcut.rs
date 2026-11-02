//! Global shortcut that opens and closes the panel. It stays in Rust, granting no permission to the UI.

use std::error::Error;
use std::sync::mpsc::Sender;

use tauri_plugin_global_shortcut::{
    Builder, Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::dock::Request;

/// If the shortcut is unavailable, such as when another app uses it, the cursor can still open the panel.
pub fn setup(app: &tauri::App, requests: Sender<Request>) {
    let register = || -> Result<(), Box<dyn Error>> {
        let plugin = Builder::new()
            .with_handler(move |_, _, event| {
                if event.state() == ShortcutState::Pressed {
                    // This only fails if the panel thread has stopped.
                    let _ = requests.send(Request::Shortcut);
                }
            })
            .build();
        app.handle().plugin(plugin)?;
        let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
        app.global_shortcut().register(shortcut)?;
        Ok(())
    };
    if let Err(error) = register() {
        eprintln!("[shortcut] failed to register Ctrl+Alt+Space: {error}");
    }
}
