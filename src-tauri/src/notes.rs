//! Armazenamento e validação das notas locais.
use crate::dock::Request;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::mpsc::Sender,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize, Deserialize)]
struct Settings {
    folder: PathBuf,
}
#[derive(Serialize)]
pub struct Folder {
    path: String,
    available: bool,
}
#[derive(Serialize)]
pub struct Note {
    path: String,
    name: String,
}
fn config(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("settings.json"))
        .map_err(|e| e.to_string())
}
fn read_settings(path: &Path) -> Option<PathBuf> {
    serde_json::from_slice::<Settings>(&fs::read(path).ok()?)
        .ok()
        .map(|s| s.folder)
}
pub fn folder(app: &AppHandle) -> Result<Option<Folder>, String> {
    Ok(read_settings(&config(app)?).map(|p| Folder {
        path: p.to_string_lossy().into_owned(),
        available: p.is_dir(),
    }))
}
fn save(path: &Path, folder: PathBuf) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Pasta de configuração inválida")?)
        .map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_vec(&Settings { folder }).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}
struct DialogGuard(Sender<Request>);
impl Drop for DialogGuard {
    fn drop(&mut self) {
        let _ = self.0.send(Request::SetDialog(false));
    }
}
pub fn choose(app: &AppHandle, requests: &Sender<Request>) -> Result<Option<Folder>, String> {
    requests
        .send(Request::SetDialog(true))
        .map_err(|e| e.to_string())?;
    let _guard = DialogGuard(requests.clone());
    let window = app
        .get_webview_window("main")
        .ok_or("janela principal ausente")?;
    // Possuído pelo painel, o seletor fica acima dele e não abre um segundo seletor.
    let dialog = app.dialog().file().set_parent(&window);
    if let Some(path) = dialog.blocking_pick_folder() {
        let path = path.into_path().map_err(|e| e.to_string())?;
        save(&config(app)?, path)?;
    }
    folder(app)
}
fn root(app: &AppHandle) -> Result<PathBuf, String> {
    let path = read_settings(&config(app)?).ok_or("Escolha uma pasta de notas")?;
    if !path.is_dir() {
        return Err("A pasta de notas não está disponível".into());
    }
    Ok(path)
}
fn markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}
/// `is_dir` e `is_file` são falsos para symlink e junction, que por isso nunca são seguidos.
fn collect(root: &Path, dir: &Path, notes: &mut Vec<Note>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        if ty.is_dir() {
            if !entry.file_name().to_string_lossy().starts_with('.') {
                collect(root, &path, notes)?;
            }
        } else if ty.is_file() && markdown(&path) {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            notes.push(Note {
                path: relative,
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            });
        }
    }
    Ok(())
}
fn list(root: &Path) -> Result<Vec<Note>, String> {
    let mut notes = Vec::new();
    collect(root, root, &mut notes)?;
    notes.sort_by(|a, b| {
        a.path
            .to_lowercase()
            .cmp(&b.path.to_lowercase())
            .then(a.path.cmp(&b.path))
    });
    Ok(notes)
}
pub fn list_for(app: &AppHandle) -> Result<Vec<Note>, String> {
    list(&root(app)?)
}
fn resolve(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.contains(':')
        || relative.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !markdown(path)
    {
        return Err("Caminho de nota inválido".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let target = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(&root) || !target.is_file() {
        return Err("Nota fora da pasta escolhida".into());
    }
    Ok(target)
}
pub fn open(app: &AppHandle, path: &str) -> Result<String, String> {
    fs::read_to_string(resolve(&root(app)?, path)?)
        .map_err(|e| format!("Não foi possível ler a nota como UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "docdock-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn configuracao_corrompida_e_ausente() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        assert!(read_settings(&p).is_none());
        fs::write(&p, "{").unwrap();
        assert!(read_settings(&p).is_none());
    }
    #[test]
    fn lista_filtra_e_ordena() {
        let t = Temp::new();
        fs::create_dir(t.0.join(".oculta")).unwrap();
        fs::create_dir(t.0.join("sub")).unwrap();
        for p in ["z.MD", "A.md", "x.txt", ".oculta/segredo.md", "sub/b.md"] {
            fs::write(t.0.join(p), "").unwrap();
        }
        let paths: Vec<_> = list(&t.0).unwrap().into_iter().map(|n| n.path).collect();
        assert_eq!(paths, ["A.md", "sub/b.md", "z.MD"]);
    }
    #[test]
    fn rejeita_caminhos_inseguros() {
        let t = Temp::new();
        fs::write(t.0.join("ok.md"), "ok").unwrap();
        assert!(resolve(&t.0, "ok.md").is_ok());
        for p in [
            "../ok.md",
            "/ok.md",
            "C:/ok.md",
            "//servidor/ok.md",
            "ok.md:oculto",
            "ok.txt",
            "sub\\ok.md",
        ] {
            assert!(resolve(&t.0, p).is_err(), "{p}");
        }
    }
    #[test]
    fn nao_segue_junction_para_fora_da_pasta() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora"));
        fs::create_dir(&notas).unwrap();
        fs::create_dir(&fora).unwrap();
        fs::write(fora.join("segredo.md"), "x").unwrap();
        let link = notas.join("atalho");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&fora)
            .output()
            .unwrap();
        assert!(status.status.success());
        assert!(list(&notas).unwrap().is_empty());
        assert!(resolve(&notas, "atalho/segredo.md").is_err());
    }
    #[test]
    fn substitui_configuracao() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        save(&p, t.0.join("a")).unwrap();
        save(&p, t.0.join("b")).unwrap();
        assert_eq!(read_settings(&p), Some(t.0.join("b")));
    }
}
