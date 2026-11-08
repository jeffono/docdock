//! Native-only update download and a fail-closed save handshake before either kind of exit.

use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::i18n::Locale;

#[derive(Default)]
struct ExitGate {
    sequence: u64,
    pending: Option<(u64, bool)>,
    allowed: bool,
}

impl ExitGate {
    fn request(&mut self, update: bool) -> Option<u64> {
        if self.allowed {
            return None;
        }
        if let Some((id, _)) = self.pending {
            return Some(id);
        }
        self.sequence += 1;
        self.pending = Some((self.sequence, update));
        Some(self.sequence)
    }

    fn finish(&mut self, id: u64, saved: bool) -> Option<bool> {
        let (pending, update) = self.pending?;
        if pending != id {
            return None;
        }
        self.pending = None;
        if !saved {
            return None;
        }
        self.allowed = true;
        Some(update)
    }
}

struct UpdateItem(MenuItem<Wry>);

#[derive(Default)]
struct State {
    exit: ExitGate,
    downloaded: Option<(Update, Vec<u8>)>,
}

pub fn menu_item(app: &tauri::App, locale: Locale) -> tauri::Result<MenuItem<Wry>> {
    let label = match locale {
        Locale::En => "Update available — restart",
        Locale::PtBr => "Atualização disponível — reiniciar",
    };
    let item = MenuItem::with_id(app, "update", label, false, None::<&str>)?;
    app.manage(UpdateItem(item.clone()));
    Ok(item)
}

pub fn start(app: &AppHandle) {
    app.manage(Mutex::new(State::default()));
    // A development run must never replace the installed app.
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let updater = app
                .updater_builder()
                .timeout(Duration::from_secs(30))
                .build()?;
            if let Some(update) = updater.check().await? {
                let bytes = update.download(|_, _| {}, || {}).await?;
                app.state::<Mutex<State>>().lock().unwrap().downloaded = Some((update, bytes));
                app.state::<UpdateItem>().0.set_enabled(true)?;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;
        if let Err(error) = result {
            // Offline, missing first release and bad signatures leave the app usable.
            eprintln!("[updater] {error}");
        }
    });
}

pub fn exit_allowed(app: &AppHandle) -> bool {
    app.state::<Mutex<State>>().lock().unwrap().exit.allowed
}

pub fn request_exit(app: &AppHandle, update: bool) {
    let state = app.state::<Mutex<State>>();
    let mut state = state.lock().unwrap();
    if update && state.downloaded.is_none() {
        return;
    }
    if app.get_webview_window("main").is_none() {
        // Without the UI there is nothing left to save, and no one to acknowledge.
        state.exit.allowed = true;
        drop(state);
        app.exit(0);
        return;
    }
    if let Some(id) = state.exit.request(update) {
        // Only the trusted main UI can acknowledge; no timeout ever assumes saves succeeded.
        if let Err(error) = app.emit_to("main", "exit-requested", id) {
            state.exit.pending = None;
            eprintln!("[exit] {error}");
        }
    }
}

#[tauri::command]
pub async fn exit_ready(app: AppHandle, id: u64, saved: bool) {
    let action = {
        let state = app.state::<Mutex<State>>();
        let mut state = state.lock().unwrap();
        state.exit.finish(id, saved)
    };
    let update = match action {
        Some(true) => app
            .state::<Mutex<State>>()
            .lock()
            .unwrap()
            .downloaded
            .take(),
        Some(false) => {
            app.exit(0);
            return;
        }
        None => {
            if !saved {
                let _ = app
                    .state::<std::sync::mpsc::Sender<crate::dock::Request>>()
                    .send(crate::dock::Request::Activate);
            }
            return;
        }
    };
    if let Some((update, bytes)) = update {
        // On Windows install launches NSIS, which restarts the app, then exits this process.
        if let Err(error) = update.install(bytes) {
            eprintln!("[updater] install failed: {error}");
            let message = match *app.state::<Locale>() {
                Locale::En => {
                    "The update could not be installed. Quit and reopen DocDock to download it again."
                }
                Locale::PtBr => {
                    "Não foi possível instalar a atualização. Saia e abra o DocDock para baixá-la novamente."
                }
            };
            app.dialog().message(message).title("DocDock").show(|_| {});
        }
    }
    app.state::<Mutex<State>>().lock().unwrap().exit.allowed = false;
    let _ = app.state::<UpdateItem>().0.set_enabled(false);
    let _ = app.emit_to("main", "exit-cancelled", ());
}

#[cfg(test)]
mod tests {
    use super::ExitGate;

    #[test]
    fn failed_save_never_allows_exit() {
        let mut gate = ExitGate::default();
        let id = gate.request(true).unwrap();
        assert_eq!(gate.finish(id, false), None);
        assert!(!gate.allowed);
        assert!(gate.request(true).is_some());
    }

    #[test]
    fn stale_acknowledgement_cannot_install() {
        let mut gate = ExitGate::default();
        let old = gate.request(true).unwrap();
        gate.finish(old, false);
        gate.request(false);
        assert_eq!(gate.finish(old, true), None);
        assert!(!gate.allowed);
    }

    #[test]
    fn repeated_request_cannot_change_quit_into_install() {
        let mut gate = ExitGate::default();
        let id = gate.request(false).unwrap();
        assert_eq!(gate.request(true), Some(id));
        assert_eq!(gate.finish(id, true), Some(false));
        assert_eq!(gate.finish(id, true), None);
    }
}
