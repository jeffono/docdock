//! Armazenamento e validação das notas locais.
use crate::dock::Request;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::mpsc::Sender,
};
use tauri::http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue, X_CONTENT_TYPE_OPTIONS};
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

/// No Windows, as imagens ficam em `http://note-asset.localhost/<caminho relativo>`.
pub const IMAGE_SCHEME: &str = "note-asset";

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
/// `allowed` vale para o pedido e para o destino canônico: um nome curto 8.3 ou um symlink dentro da
/// pasta pode trocar a extensão pedida pela real.
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
pub fn open(app: &AppHandle, path: &str) -> Result<String, String> {
    fs::read_to_string(resolve(&root(app)?, path, markdown)?)
        .map_err(|e| format!("Não foi possível ler a nota como UTF-8: {e}"))
}
/// SVG fica de fora porque pode conter script.
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
/// A janela `web` também tem o protocolo, mas seu conteúdo é remoto. `no-store` evita que o perfil do
/// WebView2, compartilhado entre as janelas, guarde a imagem.
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
    fn nao_segue_junction_para_fora_da_pasta() {
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
    fn imagem_so_sai_da_pasta_para_a_janela_principal() {
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

        // Symlink de arquivo exige o modo de desenvolvedor do Windows: sem ele, o caso não se aplica.
        if mklink(&[], &notas.join("falso.png"), &notas.join("nota.md")) {
            assert_eq!(pedir("main", "/falso.png").status(), StatusCode::NOT_FOUND);
        }
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
