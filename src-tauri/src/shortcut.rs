//! Atalho global que abre e fecha o painel. Fica só no Rust: a UI não ganha permissão nenhuma.

use std::error::Error;
use std::sync::mpsc::Sender;

use tauri_plugin_global_shortcut::{
    Builder, Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::dock::Request;

/// Sem o atalho, como quando outro app já o usa, o painel segue abrindo pelo cursor.
pub fn setup(app: &tauri::App, requests: Sender<Request>) {
    let register = || -> Result<(), Box<dyn Error>> {
        let plugin = Builder::new()
            .with_handler(move |_, _, event| {
                if event.state() == ShortcutState::Pressed {
                    // Só falha se a thread do painel morreu.
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
        eprintln!("[atalho] falha ao registrar Ctrl+Alt+Espaço: {error}");
    }
}
