//! Comandos expostos à UI. Mantenha-os finos: a lógica fica nos módulos de domínio.

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
