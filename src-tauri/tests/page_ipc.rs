//! The page window shows untrusted remote pages: it must reach no command, from any origin.
//! Secondary app windows reach only their own commands and no core command at all.
//! An integration test because only test targets can get the comctl32 v6 manifest; see `build.rs`.

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[tauri::command]
fn probe() {}

#[tauri::command]
fn secondary_probe() {}

/// Commands that succeed in `main` with these arguments: one app command and the core event
/// commands its capability grants.
const COMMANDS: [(&str, &str); 3] = [
    ("probe", "{}"),
    (
        "plugin:event|emit_to",
        r#"{"target":{"kind":"Any"},"event":"probe"}"#,
    ),
    ("plugin:event|unlisten", r#"{"event":"probe","eventId":0}"#),
];
/// Core commands with valid arguments: `main` reaches the event ones its capability names, and no
/// other window reaches any.
const CORE: [(&str, &str); 6] = [
    (
        "plugin:event|listen",
        r#"{"event":"probe","target":{"kind":"Any"},"handler":0}"#,
    ),
    ("plugin:event|unlisten", r#"{"event":"probe","eventId":0}"#),
    ("plugin:event|emit", r#"{"event":"probe"}"#),
    (
        "plugin:event|emit_to",
        r#"{"target":{"kind":"Any"},"event":"probe"}"#,
    ),
    ("plugin:window|title", "{}"),
    ("plugin:window|start_dragging", "{}"),
];
const UI: &str = "http://tauri.localhost/";
/// Origins Tauri treats as local: the UI, `localhost:1420` (the UI in `tauri dev`) and the
/// registered `note-asset` scheme. A page window that loaded one must still reach nothing.
const LOCAL: [&str; 3] = [UI, "http://localhost:1420/", "http://note-asset.localhost/"];
/// Windows that are neither `main` nor secondary: the page, the development tweak window and
/// floating-window labels nothing registered. Registered ones are tested in `floating.rs`, since
/// only Rust registers them.
const OTHERS: [&str; 4] = ["web", "tweak", "float-9", "float-03"];

type Window = WebviewWindow<MockRuntime>;

/// The app's real capabilities, barrier and secondary-window rule, with `probe` standing in for
/// `main`'s commands and `secondary_probe` for the secondary windows' ones.
fn app() -> App<MockRuntime> {
    mock_builder()
        .invoke_handler(docdock_lib::by_window(
            tauri::generate_handler![probe],
            tauri::generate_handler![secondary_probe],
            docdock_lib::is_secondary_window,
        ))
        // Registered as in production: registered schemes take part in classifying local origins.
        .register_uri_scheme_protocol("note-asset", |_, _| tauri::http::Response::new(Vec::new()))
        .build(tauri::generate_context!())
        .unwrap()
}

fn window(app: &App<MockRuntime>, label: &str) -> Window {
    WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .build()
        .unwrap()
}

fn invoke(window: &Window, cmd: &str, url: &str) -> Result<InvokeResponseBody, serde_json::Value> {
    call(window, cmd, "{}", url)
}

fn call(
    window: &Window,
    cmd: &str,
    args: &str,
    url: &str,
) -> Result<InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: url.parse().unwrap(),
            body: InvokeBody::Json(serde_json::from_str(args).unwrap()),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
}

/// The error text, so a test tells a refusal from arguments the command could not take.
fn refusal(result: Result<InvokeResponseBody, serde_json::Value>) -> String {
    result.map(|_| ()).unwrap_err().to_string()
}

#[test]
fn page_window_reaches_no_command() {
    let app = app();
    let (main, web) = (window(&app, "main"), window(&app, "web"));
    for (cmd, args) in COMMANDS {
        assert!(call(&main, cmd, args, UI).is_ok(), "{cmd} from main");
        assert!(
            call(&web, cmd, args, "https://example.com/").is_err(),
            "{cmd} from a remote page"
        );
        for url in LOCAL {
            assert!(call(&web, cmd, args, url).is_err(), "{cmd} from {url}");
        }
    }
    for url in LOCAL.into_iter().chain(["https://example.com/"]) {
        assert!(
            invoke(&web, "secondary_probe", url).is_err(),
            "secondary_probe from {url}"
        );
    }
}

/// The panel's capability grants only the event commands the UI uses, so a script injected into it
/// still cannot reach the other core commands, such as the window and webview ones.
#[test]
fn main_reaches_no_core_command_the_ui_does_not_use() {
    let app = app();
    let main = window(&app, "main");
    for (cmd, args) in [
        ("plugin:app|version", "{}"),
        ("plugin:window|title", "{}"),
        ("plugin:event|emit", r#"{"event":"probe"}"#),
    ] {
        assert!(call(&main, cmd, args, UI).is_err(), "{cmd}");
    }
}

/// The local origins really pass the ACL, so the page window's rejection comes from `by_window`.
#[test]
fn local_origins_reach_app_commands_only_from_app_windows() {
    let app = app();
    let (main, web) = (window(&app, "main"), window(&app, "web"));
    for url in LOCAL {
        assert!(
            invoke(&main, "probe", url).is_ok(),
            "probe from main at {url}"
        );
        let error = refusal(invoke(&web, "probe", url));
        assert!(
            error.contains("app windows"),
            "probe from web at {url}: {error}"
        );
    }
}

#[test]
fn remote_origins_reach_no_command() {
    let app = app();
    let main = window(&app, "main");
    for (cmd, args) in COMMANDS {
        assert!(
            call(&main, cmd, args, "https://example.com/").is_err(),
            "{cmd}"
        );
    }
}

#[test]
fn image_preview_reaches_only_secondary_commands() {
    let app = app();
    let (main, preview) = (window(&app, "main"), window(&app, "image-preview"));
    assert!(invoke(&preview, "secondary_probe", UI).is_ok());
    let error = refusal(invoke(&preview, "probe", UI));
    assert!(error.contains("not found"), "{error}");
    let error = refusal(invoke(&main, "secondary_probe", UI));
    assert!(error.contains("not found"), "{error}");
}

/// The barrier, not a missing command, refuses every other window, including a `float-*` label no
/// floating window registered.
#[test]
fn other_windows_reach_no_app_command() {
    let app = app();
    for label in OTHERS {
        let other = window(&app, label);
        for cmd in ["probe", "secondary_probe"] {
            let error = refusal(invoke(&other, cmd, UI));
            assert!(error.contains("app windows"), "{cmd} from {label}: {error}");
        }
    }
}

/// Secondary windows have no capability: the ACL refuses every core command with arguments `main`
/// accepts, so they can neither hear other windows' events nor drive a window.
#[test]
fn secondary_and_other_windows_reach_no_core_command() {
    let app = app();
    let main = window(&app, "main");
    for (cmd, args) in &CORE[..2] {
        assert!(call(&main, cmd, args, UI).is_ok(), "{cmd} from main");
    }
    for label in ["image-preview"].into_iter().chain(OTHERS) {
        let other = window(&app, label);
        for (cmd, args) in CORE {
            let error = refusal(call(&other, cmd, args, UI));
            assert!(error.contains("not allowed"), "{cmd} from {label}: {error}");
        }
    }
}
