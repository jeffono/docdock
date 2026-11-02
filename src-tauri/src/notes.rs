//! Local note storage and validation.
use crate::dock::Request;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    hash::{DefaultHasher, Hasher},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, mpsc::Sender},
};
use tauri::http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue, X_CONTENT_TYPE_OPTIONS};
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

/// On Windows, images are at `http://note-asset.localhost/<relative path>`.
pub const IMAGE_SCHEME: &str = "note-asset";

/// A third-party note must not block the UI: conversion and inserting HTML into the DOM are costly.
const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;

/// Appended to the replaced file's name. The extension is no longer `.md`, so `collect` skips it.
const TEMP_SUFFIX: &str = ".docdock-tmp";

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
#[derive(Serialize)]
pub struct OpenNote {
    html: String,
    text: String,
    version: String,
}
#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Saved {
    Saved { version: String },
    Conflict,
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
    write_atomic(
        path,
        &serde_json::to_vec(&Settings { folder }).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
/// Writes a complete temporary file next to `path` and renames it over `path`. Any failure leaves the
/// previous file intact. `fs::rename` replaces in one step; `ReplaceFileW` keeps more metadata but has
/// failure modes that leave nothing under the original name, so only the creation time is carried over.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut name = path
        .file_name()
        .ok_or(io::ErrorKind::InvalidInput)?
        .to_os_string();
    name.push(TEMP_SUFFIX);
    let temp = path.with_file_name(name);
    // A leftover could be a planted link; `create_new` does not open through it.
    let _ = fs::remove_file(&temp);
    let result = write_temp(&temp, path, bytes).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
#[cfg_attr(not(windows), allow(unused_variables))]
fn write_temp(temp: &Path, original: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    file.write_all(bytes)?;
    #[cfg(windows)]
    if let Ok(created) = fs::metadata(original).and_then(|m| m.created()) {
        use std::os::windows::fs::FileTimesExt;
        file.set_times(fs::FileTimes::new().set_created(created))?;
    }
    file.sync_all()
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
    // Owned by the panel, the picker stays above it and does not open a second picker.
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
/// `is_dir` and `is_file` are false for symlinks and junctions, so they are never followed.
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
/// `allowed` applies to both the request and canonical target: an 8.3 short name or symlink inside
/// the folder can change the requested extension to a different real extension.
fn resolve(root: &Path, relative: &str, allowed: fn(&Path) -> bool) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.contains(':')
        || relative.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !allowed(path)
    {
        return Err("Caminho inválido".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let target = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(&root) || !target.is_file() || !allowed(&target) {
        return Err("Arquivo fora da pasta escolhida".into());
    }
    Ok(target)
}
fn too_large(size: u64) -> Result<(), String> {
    if size > MAX_NOTE_BYTES {
        return Err(format!(
            "A nota tem {} MB; o limite é {} MB",
            size / 1_048_576,
            MAX_NOTE_BYTES / 1_048_576
        ));
    }
    Ok(())
}
fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    too_large(fs::metadata(path).map_err(|e| e.to_string())?.len())?;
    fs::read(path).map_err(|e| e.to_string())
}
fn read_note(path: &Path) -> Result<String, String> {
    String::from_utf8(read_bytes(path)?)
        .map_err(|e| format!("Não foi possível ler a nota como UTF-8: {e}"))
}
/// Identifies the exact bytes the editor loaded. Hashing the content, rather than using mtime and size,
/// also catches same-size edits and tools that preserve mtime. Tokens are never persisted, so the
/// unspecified `DefaultHasher` algorithm only needs to be stable within one run.
fn version(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    format!("{:016x}", hasher.finish())
}
pub fn open(app: &AppHandle, path: &str) -> Result<OpenNote, String> {
    let text = read_note(&resolve(&root(app)?, path, markdown)?)?;
    Ok(OpenNote {
        html: crate::markdown::render(&text, path),
        version: version(text.as_bytes()),
        text,
    })
}
/// Serializes saves so the conflict check and the write are not interleaved within the app.
static SAVING: Mutex<()> = Mutex::new(());
/// Writes `content` only if the file still holds the version identified by `base`. An external writer
/// can still slip in between the check and the rename; the window is a few milliseconds.
fn save_note_in(root: &Path, relative: &str, content: &str, base: &str) -> Result<Saved, String> {
    too_large(content.len() as u64)?;
    let target = resolve(root, relative, markdown)?;
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let current = read_bytes(&target)?;
    let saved = Saved::Saved {
        version: version(content.as_bytes()),
    };
    if current == content.as_bytes() {
        return Ok(saved);
    }
    if version(&current) != base {
        return Ok(Saved::Conflict);
    }
    write_atomic(&target, content.as_bytes())
        .map_err(|e| format!("Não foi possível salvar a nota: {e}"))?;
    Ok(saved)
}
pub fn save_note(app: &AppHandle, path: &str, content: &str, base: &str) -> Result<Saved, String> {
    save_note_in(&root(app)?, path, content, base)
}
/// SVG is excluded because it can contain scripts.
fn image_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}
fn is_image(path: &Path) -> bool {
    image_type(path).is_some()
}
fn read_image(root: &Path, uri_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let relative = percent_decode_str(uri_path.strip_prefix('/')?)
        .decode_utf8()
        .ok()?;
    let target = resolve(root, &relative, is_image).ok()?;
    let content_type = image_type(&target)?;
    Some((fs::read(target).ok()?, content_type))
}
fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = status;
    response
}
/// The `web` window also has the protocol, but its content is remote. `no-store` prevents the
/// WebView2 profile shared between windows from caching the image.
fn image_response(webview: &str, root: Option<PathBuf>, uri_path: &str) -> Response<Vec<u8>> {
    if webview != "main" {
        return empty_response(StatusCode::FORBIDDEN);
    }
    let Some((body, content_type)) = root.and_then(|root| read_image(&root, uri_path)) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
pub fn image_for(app: &AppHandle, webview: &str, uri_path: &str) -> Response<Vec<u8>> {
    image_response(webview, root(app).ok(), uri_path)
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
    fn mklink(flags: &[&str], link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd")
            .args(["/C", "mklink"])
            .args(flags)
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status
            .success()
    }
    #[test]
    fn corrupt_and_missing_configuration() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        assert!(read_settings(&p).is_none());
        fs::write(&p, "{").unwrap();
        assert!(read_settings(&p).is_none());
    }
    #[test]
    fn list_filters_and_sorts() {
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
    fn rejects_unsafe_paths() {
        let t = Temp::new();
        fs::write(t.0.join("ok.md"), "ok").unwrap();
        assert!(resolve(&t.0, "ok.md", markdown).is_ok());
        for p in [
            "../ok.md",
            "/ok.md",
            "C:/ok.md",
            "//servidor/ok.md",
            "ok.md:oculto",
            "ok.txt",
            "sub\\ok.md",
        ] {
            assert!(resolve(&t.0, p, markdown).is_err(), "{p}");
        }
    }
    #[test]
    fn does_not_follow_junction_outside_folder() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora"));
        fs::create_dir(&notas).unwrap();
        fs::create_dir(&fora).unwrap();
        fs::write(fora.join("segredo.md"), "x").unwrap();
        assert!(mklink(&["/J"], &notas.join("atalho"), &fora));
        assert!(list(&notas).unwrap().is_empty());
        assert!(resolve(&notas, "atalho/segredo.md", markdown).is_err());
    }
    #[test]
    fn image_only_leaves_folder_for_main_window() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora"));
        fs::create_dir_all(notas.join("com espaco")).unwrap();
        fs::create_dir(&fora).unwrap();
        fs::write(fora.join("segredo.png"), "x").unwrap();
        for p in ["com espaco/foto.PNG", "nota.md", "vetor.svg", "fragil.SVG"] {
            fs::write(notas.join(p), "imagem").unwrap();
        }
        assert!(mklink(&["/J"], &notas.join("atalho"), &fora));
        let pedir = |webview, uri| image_response(webview, Some(notas.clone()), uri);

        let ok = pedir("main", "/com%20espaco/foto.PNG");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.body(), b"imagem");
        assert_eq!(ok.headers()[CONTENT_TYPE], "image/png");
        assert_eq!(ok.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");

        let web = pedir("web", "/com%20espaco/foto.PNG");
        assert_eq!(web.status(), StatusCode::FORBIDDEN);
        assert!(web.body().is_empty());

        for uri in [
            "/../fora/segredo.png",
            "/%2e%2e/fora/segredo.png",
            "/..%2ffora%2fsegredo.png",
            "/..%5cfora%5csegredo.png",
            "/atalho/segredo.png",
            "/vetor.svg",
            "/fragil.SVG",
            "/nota.md",
            "/com%20espaco/foto.PNG%3Aoculto",
            "/%FF.png",
            "/",
        ] {
            assert_eq!(pedir("main", uri).status(), StatusCode::NOT_FOUND, "{uri}");
        }
        let sem_pasta = image_response("main", None, "/com%20espaco/foto.PNG");
        assert_eq!(sem_pasta.status(), StatusCode::NOT_FOUND);

        // File symlinks require Windows developer mode; without it, this case does not apply.
        if mklink(&[], &notas.join("falso.png"), &notas.join("nota.md")) {
            assert_eq!(pedir("main", "/falso.png").status(), StatusCode::NOT_FOUND);
        }
    }
    #[test]
    fn rejects_oversized_note() {
        let t = Temp::new();
        let p = t.0.join("grande.md");
        fs::write(&p, vec![b'x'; MAX_NOTE_BYTES as usize]).unwrap();
        assert!(read_note(&p).is_ok());
        fs::write(&p, vec![b'x'; MAX_NOTE_BYTES as usize + 1]).unwrap();
        assert!(read_note(&p).is_err());
    }
    #[test]
    fn save_refuses_conflicts_and_paths_outside_folder() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora.md"));
        fs::create_dir(&notas).unwrap();
        fs::write(&fora, "fora").unwrap();
        let p = notas.join("a.md");
        fs::write(&p, "um\r\n").unwrap();
        let base = version(b"um\r\n");
        fs::write(&p, "externo").unwrap();
        let conflict = save_note_in(&notas, "a.md", "meu", &base);
        assert_eq!(conflict, Ok(Saved::Conflict));
        assert_eq!(fs::read_to_string(&p).unwrap(), "externo");

        let saved = save_note_in(&notas, "a.md", "dois\r\n", &version(b"externo"));
        let version_dois = version(b"dois\r\n");
        assert_eq!(fs::read_to_string(&p).unwrap(), "dois\r\n");
        assert_eq!(
            saved,
            Ok(Saved::Saved {
                version: version_dois.clone()
            })
        );

        let grande = "x".repeat(MAX_NOTE_BYTES as usize + 1);
        assert!(save_note_in(&notas, "a.md", &grande, &version_dois).is_err());
        assert!(save_note_in(&notas, "../fora.md", "x", &version(b"fora")).is_err());
        assert_eq!(fs::read_to_string(&fora).unwrap(), "fora");
    }
    #[test]
    fn failed_save_keeps_original_and_removes_temp() {
        use std::os::windows::fs::OpenOptionsExt;
        let t = Temp::new();
        let p = t.0.join("a.md");
        fs::write(&p, "original").unwrap();
        // Another app holding the note without delete sharing makes the rename fail.
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ
            .open(&p)
            .unwrap();
        let error = save_note_in(&t.0, "a.md", "novo", &version(b"original")).unwrap_err();
        assert!(error.contains("salvar"), "{error}");
        drop(lock);
        assert_eq!(fs::read_to_string(&p).unwrap(), "original");
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
        // A temp left by a crash is never listed as a note.
        fs::write(t.0.join(format!("a.md{TEMP_SUFFIX}")), "novo").unwrap();
        let paths: Vec<_> = list(&t.0).unwrap().into_iter().map(|n| n.path).collect();
        assert_eq!(paths, ["a.md"]);
    }
    #[test]
    fn replaces_configuration() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        save(&p, t.0.join("a")).unwrap();
        save(&p, t.0.join("b")).unwrap();
        assert_eq!(read_settings(&p), Some(t.0.join("b")));
    }
}
