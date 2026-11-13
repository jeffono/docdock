//! The page window shows untrusted remote pages: it must reach no command, from any origin.
//! An integration test because only test targets can get the comctl32 v6 manifest; see `build.rs`.

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[tauri::command]
fn probe() {}

/// Commands that succeed in `main` without arguments: one app command and core plugin commands.
const COMMANDS: [&str; 3] = ["probe", "plugin:app|version", "plugin:app|name"];
const UI: &str = "http://tauri.localhost/";
/// Origins Tauri treats as local: the UI, `localhost:1420` (the UI in `tauri dev`) and the
/// registered `note-asset` scheme. A page window that loaded one must still reach nothing.
const LOCAL: [&str; 3] = [UI, "http://localhost:1420/", "http://note-asset.localhost/"];

type Window = WebviewWindow<MockRuntime>;

/// The app's real capabilities and barrier, with `probe` standing in for its commands.
fn app() -> (App<MockRuntime>, Window, Window) {
    let app = mock_builder()
        .invoke_handler(docdock_lib::main_only(tauri::generate_handler![probe]))
        // Registered as in production: registered schemes take part in classifying local origins.
        .register_uri_scheme_protocol("note-asset", |_, _| tauri::http::Response::new(Vec::new()))
        .build(tauri::generate_context!())
        .unwrap();
    let window = |label| {
        WebviewWindowBuilder::new(&app, label, WebviewUrl::default())
            .build()
            .unwrap()
    };
    let (main, web) = (window("main"), window("web"));
    (app, main, web)
}

fn invoke(window: &Window, cmd: &str, url: &str) -> Result<InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: url.parse().unwrap(),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
}

#[test]
fn page_window_reaches_no_command() {
    let (_app, main, web) = app();
    for cmd in COMMANDS {
        assert!(invoke(&main, cmd, UI).is_ok(), "{cmd} from main");
        assert!(
            invoke(&web, cmd, "https://example.com/").is_err(),
            "{cmd} from a remote page"
        );
        for url in LOCAL {
            assert!(invoke(&web, cmd, url).is_err(), "{cmd} from {url}");
        }
    }
}

/// The local origins really pass the ACL, so the page window's rejection comes from `main_only`.
#[test]
fn local_origins_reach_app_commands_only_from_main() {
    let (_app, main, web) = app();
    for url in LOCAL {
        assert!(
            invoke(&main, "probe", url).is_ok(),
            "probe from main at {url}"
        );
        let error = invoke(&web, "probe", url).unwrap_err().to_string();
        assert!(
            error.contains("main window"),
            "probe from web at {url}: {error}"
        );
    }
}

#[test]
fn remote_origins_reach_no_command() {
    let (_app, main, _) = app();
    for cmd in COMMANDS {
        assert!(invoke(&main, cmd, "https://example.com/").is_err(), "{cmd}");
    }
}
