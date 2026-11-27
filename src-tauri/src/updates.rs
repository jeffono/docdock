//! Native-only update download and a fail-closed save handshake before either kind of exit.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
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

const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Phase {
    Disabled,
    NotChecked,
    Checking,
    UpToDate,
    Downloading,
    Ready,
    CheckFailed,
    DownloadFailed,
    InstallFailed,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckState {
    phase: Phase,
    last_success: Option<u64>,
}

impl CheckState {
    fn new(enabled: bool) -> Self {
        Self {
            phase: if enabled {
                Phase::NotChecked
            } else {
                Phase::Disabled
            },
            last_success: None,
        }
    }

    fn begin(&mut self) -> bool {
        if matches!(
            self.phase,
            Phase::Disabled
                | Phase::Checking
                | Phase::Downloading
                | Phase::Ready
                | Phase::InstallFailed
        ) {
            return false;
        }
        self.phase = Phase::Checking;
        true
    }

    fn checked(&mut self, available: bool, now: Option<u64>) {
        self.last_success = now.or(self.last_success);
        self.phase = if available {
            Phase::Downloading
        } else {
            Phase::UpToDate
        };
    }

    fn failed(&mut self) {
        self.phase = if self.phase == Phase::Downloading {
            Phase::DownloadFailed
        } else {
            Phase::CheckFailed
        };
    }

    fn downloaded(&mut self) {
        self.phase = Phase::Ready;
    }

    fn install_failed(&mut self) {
        self.phase = Phase::InstallFailed;
    }
}

#[derive(Clone, Serialize)]
pub struct UpdateStatus {
    version: String,
    #[serde(flatten)]
    check: CheckState,
}

struct State {
    exit: ExitGate,
    check: CheckState,
    downloaded: Option<(Update, Vec<u8>)>,
}

fn ready_label(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Update available — restart",
        Locale::PtBr => "Atualização disponível — reiniciar",
    }
}

pub fn menu_item(app: &tauri::App, locale: Locale) -> tauri::Result<MenuItem<Wry>> {
    let item = MenuItem::with_id(app, "update", ready_label(locale), false, None::<&str>)?;
    app.manage(UpdateItem(item.clone()));
    Ok(item)
}

pub fn start(app: &AppHandle, window: isize) {
    // Development runs, including Tauri dev in release mode, cannot replace the installed app.
    let enabled = !cfg!(debug_assertions) && !tauri::is_dev();
    app.manage(Mutex::new(State {
        exit: ExitGate::default(),
        check: CheckState::new(enabled),
        downloaded: None,
    }));
    if !enabled {
        return;
    }
    let resume = app.clone();
    if let Err(error) = crate::platform::power::on_resume(window, move || begin_check(&resume)) {
        eprintln!("[updater] {error}");
    }
    begin_check(app);
    let periodic = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(CHECK_INTERVAL);
            begin_check(&periodic);
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    status(&app, locked(&app).check)
}

#[tauri::command]
pub fn update_check(app: AppHandle) {
    begin_check(&app);
}

#[tauri::command]
pub fn update_restart(app: AppHandle) {
    request_exit(&app, true);
}

fn status(app: &AppHandle, check: CheckState) -> UpdateStatus {
    UpdateStatus {
        version: app.package_info().version.to_string(),
        check,
    }
}

/// Call while holding the state lock so concurrent changes are emitted in order.
fn publish(app: &AppHandle, check: CheckState) {
    if let Err(error) = app.emit_to("main", "update-state", status(app, check)) {
        eprintln!("[updater] {error}");
    }
}

fn begin_check(app: &AppHandle) {
    {
        let mut state = locked(app);
        if !state.check.begin() {
            return;
        }
        publish(app, state.check);
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = check_and_download(&app).await {
            eprintln!("[updater] {error}");
            let mut state = locked(&app);
            state.check.failed();
            publish(&app, state.check);
        }
    });
}

async fn check_and_download(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let update = updater.check().await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_millis()).ok());
    {
        let mut state = locked(app);
        state.check.checked(update.is_some(), now);
        publish(app, state.check);
    }
    if let Some(mut update) = update {
        update.timeout = Some(Duration::from_secs(10 * 60));
        let bytes = update.download(|_, _| {}, || {}).await?;
        {
            let mut state = locked(app);
            state.downloaded = Some((update, bytes));
            state.check.downloaded();
            publish(app, state.check);
        }
        if let Err(error) = app.state::<UpdateItem>().0.set_enabled(true) {
            eprintln!("[updater] {error}");
        }
        crate::tray::set_update_tooltip(app, ready_label(*app.state::<Locale>()));
    }
    Ok(())
}

/// A panic while holding the state leaves it consistent enough to keep the exit handshake working.
fn locked(app: &AppHandle) -> MutexGuard<'_, State> {
    app.state::<Mutex<State>>()
        .inner()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn exit_allowed(app: &AppHandle) -> bool {
    locked(app).exit.allowed
}

pub fn request_exit(app: &AppHandle, update: bool) {
    let mut state = locked(app);
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
    // The floating windows' last moves, before the gate lets any exit through. Off the main
    // thread, which their worker may be waiting for.
    if saved {
        crate::floating::flush();
    }
    let action = {
        let mut state = locked(&app);
        state.exit.finish(id, saved)
    };
    let update = match action {
        Some(true) => locked(&app).downloaded.take(),
        Some(false) => {
            app.exit(0);
            return;
        }
        None => {
            // No exit: the edits `floating_finish_edits` held may begin again.
            crate::floating::release_edits();
            if !saved {
                let _ = app
                    .state::<std::sync::mpsc::Sender<crate::dock::Request>>()
                    .send(crate::dock::Request::Activate);
            }
            return;
        }
    };
    // On Windows install launches NSIS, which restarts the app, then exits this process.
    if let Some((update, bytes)) = &update
        && let Err(error) = update.install(bytes)
    {
        eprintln!("[updater] install failed: {error}");
        let message = match *app.state::<Locale>() {
            Locale::En => {
                "The update could not be installed. Try restarting again from the tray or Preferences."
            }
            Locale::PtBr => {
                "Não foi possível instalar a atualização. Tente reiniciar de novo pela bandeja ou pelas Preferências."
            }
        };
        // It follows the user's own restart, so a native message is expected; its own thread
        // lets the exit state below be restored while it is open.
        std::thread::spawn(move || {
            rfd::MessageDialog::new()
                .set_title("DocDock")
                .set_description(message)
                .show()
        });
    }
    crate::floating::release_edits();
    {
        let mut state = locked(&app);
        state.exit.allowed = false;
        state.downloaded = update;
        state.check.install_failed();
        publish(&app, state.check);
    }
    let _ = app.emit_to("main", "exit-cancelled", ());
}

#[cfg(test)]
mod tests {
    use super::{CheckState, ExitGate, Phase};

    #[test]
    fn development_refuses_even_manual_checks() {
        let mut state = CheckState::new(false);
        assert!(!state.begin());
        assert_eq!(state.phase, Phase::Disabled);
    }

    #[test]
    fn another_request_joins_the_check_in_progress() {
        let mut state = CheckState::new(true);
        assert!(state.begin());
        assert!(!state.begin());
        assert_eq!(state.phase, Phase::Checking);
    }

    #[test]
    fn another_request_joins_the_download_in_progress() {
        let mut state = CheckState::new(true);
        state.begin();
        state.checked(true, Some(42));
        assert!(!state.begin());
        assert_eq!(state.phase, Phase::Downloading);
    }

    #[test]
    fn a_downloaded_update_waits_for_restart_without_checking_again() {
        let mut state = CheckState::new(true);
        state.begin();
        state.checked(true, Some(42));
        state.downloaded();
        assert!(!state.begin());
        assert_eq!(state.phase, Phase::Ready);
    }

    #[test]
    fn a_failed_install_does_not_redownload_the_verified_update() {
        let mut state = CheckState::new(true);
        state.begin();
        state.checked(true, Some(42));
        state.downloaded();
        state.install_failed();
        assert!(!state.begin());
        assert_eq!(state.phase, Phase::InstallFailed);
    }

    #[test]
    fn a_failed_check_keeps_the_last_success_and_can_be_retried() {
        let mut state = CheckState::new(true);
        state.begin();
        state.checked(false, Some(42));
        state.begin();
        state.failed();
        assert_eq!(state.phase, Phase::CheckFailed);
        assert_eq!(state.last_success, Some(42));
        assert!(state.begin());
    }

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
