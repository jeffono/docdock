//! Composição do app: registra módulos e comandos, sem regras de negócio.

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_version])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
