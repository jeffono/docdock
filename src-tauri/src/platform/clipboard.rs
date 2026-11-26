//! Windows clipboard read/write: private copies excluded from clipboard history and cloud sync,
//! plus an STA worker that reads Windows Clipboard History (`Win+V`) and watches changes without
//! requiring window focus.
//! Formats from "Cloud Clipboard and Clipboard History Formats":
//! <https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats>.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use windows::ApplicationModel::DataTransfer::{
    Clipboard, ClipboardHistoryChangedEventArgs, ClipboardHistoryItemsResultStatus,
    SetHistoryItemAsContentStatus, StandardDataFormats,
};
use windows::Foundation::EventHandler;
use windows::Win32::Foundation::{
    E_ACCESSDENIED, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, CountClipboardFormats, EmptyClipboard,
    GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    KillTimer, MSG, PM_REMOVE, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetTimer, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WM_QUIT,
    WM_TIMER, WNDCLASSW,
};
use windows::core::{Error, PCWSTR, Result, w};

/// `CF_UNICODETEXT`, defined in `Win32_System_Ole`, which nothing else needs.
const CF_UNICODETEXT: u32 = 13;
const MAX_HISTORY_ITEMS: usize = 50;
const MAX_ITEM_CHARS: usize = 200_000;
/// Difference in 100 ns intervals between Windows FILETIME (1601-01-01) and Unix epoch (1970-01-01).
const WINDOWS_TO_UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_MS: i64 = 10_000;
const ASYNC_TIMEOUT: Duration = Duration::from_secs(2);
const JOB_TIMEOUT: Duration = Duration::from_secs(4);
const NOTIFY_DEBOUNCE_MS: u32 = 150;
const TIMER_NOTIFY: usize = 1;
const WM_JOB: u32 = WM_APP + 1;
const WM_HISTORY_CHANGED: u32 = WM_APP + 2;
const WM_STOP_WORKER: u32 = WM_APP + 3;

/// Each holds a serialized DWORD 0. The first excludes every format from history and sync for any
/// data; the others say the same to readers that check only history or only sync.
const EXCLUSIONS: [PCWSTR; 3] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("CanIncludeInClipboardHistory"),
    w!("CanUploadToCloudClipboard"),
];

/// Another app, often a clipboard monitor reacting to the previous copy, may hold the clipboard open
/// for a moment.
const OPEN_ATTEMPTS: u32 = 10;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub id: String,
    pub text: String,
    pub truncated: bool,
    pub timestamp_ms: Option<i64>,
    pub deletable: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardSnapshot {
    pub history_enabled: bool,
    pub access_denied: bool,
    pub current_non_text: bool,
    pub items: Vec<ClipboardEntry>,
}

enum Job {
    Snapshot(Sender<std::result::Result<ClipboardSnapshot, String>>),
    Restore {
        id: String,
        reply: Sender<std::result::Result<bool, String>>,
    },
    Delete {
        id: String,
        reply: Sender<std::result::Result<bool, String>>,
    },
}

pub struct ClipboardHandle {
    hwnd: isize,
    queue: Arc<Mutex<VecDeque<Job>>>,
}

impl ClipboardHandle {
    pub fn snapshot(&self) -> std::result::Result<ClipboardSnapshot, String> {
        let (reply, receiver) = mpsc::channel();
        self.enqueue(Job::Snapshot(reply))?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "clipboard worker did not answer snapshot in time".to_owned())?
    }

    pub fn restore_item(&self, id: String) -> std::result::Result<bool, String> {
        let (reply, receiver) = mpsc::channel();
        self.enqueue(Job::Restore { id, reply })?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "clipboard worker did not answer restore in time".to_owned())?
    }

    pub fn delete_item(&self, id: String) -> std::result::Result<bool, String> {
        let (reply, receiver) = mpsc::channel();
        self.enqueue(Job::Delete { id, reply })?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "clipboard worker did not answer delete in time".to_owned())?
    }

    fn enqueue(&self, job: Job) -> std::result::Result<(), String> {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(job);
        let hwnd = HWND(self.hwnd as *mut _);
        unsafe { PostMessageW(Some(hwnd), WM_JOB, WPARAM(0), LPARAM(0)) }
            .map_err(|error| error.to_string())
    }
}

impl Drop for ClipboardHandle {
    fn drop(&mut self) {
        let hwnd = HWND(self.hwnd as *mut _);
        let _ = unsafe { PostMessageW(Some(hwnd), WM_STOP_WORKER, WPARAM(0), LPARAM(0)) };
    }
}

/// Replaces the clipboard with `text`, excluded from history and cloud clipboard in the same
/// transaction, so no monitor sees the text without the exclusion. `owner` is a window of this
/// process: with no owner, `SetClipboardData` fails. If a later step fails, the previous content
/// is already gone.
pub fn write_private_text(owner: isize, text: &str) -> Result<()> {
    open(Some(HWND(owner as *mut _)))?;
    let written = unsafe { EmptyClipboard() }.and_then(|()| {
        for name in EXCLUSIONS {
            let format = unsafe { RegisterClipboardFormatW(name) };
            if format == 0 {
                return Err(Error::from_thread());
            }
            set(format, &0u32.to_ne_bytes())?;
        }
        let units: Vec<u8> = windows_newlines(text)
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_ne_bytes)
            .collect();
        set(CF_UNICODETEXT, &units)
    });
    let closed = unsafe { CloseClipboard() };
    written.and(closed)
}

fn open(owner: Option<HWND>) -> Result<()> {
    let mut attempt = 1;
    loop {
        match unsafe { OpenClipboard(owner) } {
            Ok(()) => return Ok(()),
            Err(error) if attempt == OPEN_ATTEMPTS => return Err(error),
            Err(_) => attempt += 1,
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The system owns the memory once `SetClipboardData` succeeds; until then it is ours to free.
fn set(format: u32, data: &[u8]) -> Result<()> {
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, data.len()) }?;
    let target = unsafe { GlobalLock(memory) };
    let placed = if target.is_null() {
        Err(Error::from_thread())
    } else {
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), target.cast(), data.len());
            // Fails by design once the lock count reaches zero.
            let _ = GlobalUnlock(memory);
        }
        unsafe { SetClipboardData(format, Some(HANDLE(memory.0))) }.map(|_| ())
    };
    if placed.is_err() {
        let _ = unsafe { GlobalFree(Some(memory)) };
    }
    placed
}

/// Clipboard text ends each line with CR-LF ("Standard Clipboard Formats"); the editor uses LF.
fn windows_newlines(text: &str) -> String {
    let mut converted = String::with_capacity(text.len());
    let mut previous = None;
    for c in text.chars() {
        if c == '\n' && previous != Some('\r') {
            converted.push('\r');
        }
        converted.push(c);
        previous = Some(c);
    }
    converted
}

pub fn filetime_to_unix_ms(ticks: i64) -> Option<i64> {
    (ticks >= WINDOWS_TO_UNIX_EPOCH_TICKS)
        .then(|| (ticks - WINDOWS_TO_UNIX_EPOCH_TICKS) / TICKS_PER_MS)
}

pub fn is_clipboard_excluded(
    monitor_excluded_present: bool,
    include_in_history: Option<u32>,
) -> bool {
    monitor_excluded_present || matches!(include_in_history, Some(0))
}

/// Decodes UTF-16 units in one pass, normalizes `\r\n` and `\r` to `\n`, and stops at `max_chars`
/// without allocating a full decoded copy of oversized clipboard buffers.
pub fn normalize_and_cap_utf16(units: &[u16], max_chars: usize) -> (String, bool) {
    let mut out = String::with_capacity(units.len().min(max_chars.saturating_mul(2)));
    let mut count = 0usize;
    let mut truncated = false;
    let mut prev_cr = false;

    for decoded in char::decode_utf16(units.iter().copied()) {
        let ch = decoded.unwrap_or(char::REPLACEMENT_CHARACTER);
        if ch == '\n' && prev_cr {
            prev_cr = false;
            continue;
        }
        prev_cr = ch == '\r';
        let normalized = if ch == '\r' { '\n' } else { ch };
        if count == max_chars {
            truncated = true;
            break;
        }
        out.push(normalized);
        count += 1;
    }

    (out, truncated)
}

/// WinRT `IAsyncOperation::join()` deadlocks on an unpumped STA thread when OLE clipboard objects
/// marshal calls back to the caller's apartment; pump Windows messages while polling completion.
macro_rules! pump_async {
    ($op:expr, $hwnd:expr, $pending_notify:expr, $stopping:expr, $deadline:expr) => {{
        let op = $op;
        let start = Instant::now();
        loop {
            if *$stopping {
                let _ = op.Cancel();
                break Err("clipboard worker stopping".to_owned());
            }
            match op.Status() {
                Ok(s) if s.0 == 0 => {
                    if Instant::now() >= $deadline || start.elapsed() > ASYNC_TIMEOUT {
                        let _ = op.Cancel();
                        break Err("clipboard operation timed out".to_owned());
                    }
                    let mut msg = MSG::default();
                    let mut aborted = None;
                    while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                        match msg.message {
                            WM_STOP_WORKER | WM_QUIT => {
                                *$stopping = true;
                                let _ = op.Cancel();
                                aborted = Some("clipboard worker stopping".to_owned());
                                break;
                            }
                            WM_JOB => {}
                            WM_HISTORY_CHANGED => {
                                let _ = unsafe {
                                    SetTimer(Some($hwnd), TIMER_NOTIFY, NOTIFY_DEBOUNCE_MS, None)
                                };
                            }
                            WM_TIMER if msg.wParam.0 == TIMER_NOTIFY => {
                                let _ = unsafe { KillTimer(Some($hwnd), TIMER_NOTIFY) };
                                *$pending_notify = true;
                            }
                            _ => {
                                let _ = unsafe { TranslateMessage(&msg) };
                                unsafe { DispatchMessageW(&msg) };
                            }
                        }
                    }
                    if let Some(error) = aborted {
                        break Err(error);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Ok(_) => break op.GetResults().map_err(|error| error.to_string()),
                Err(error) => break Err(error.to_string()),
            }
        }
    }};
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLIPBOARDUPDATE => {
            let _ = unsafe { SetTimer(Some(hwnd), TIMER_NOTIFY, NOTIFY_DEBOUNCE_MS, None) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub fn start_worker(
    on_change: impl Fn() + Send + 'static,
) -> std::result::Result<ClipboardHandle, String> {
    let queue = Arc::new(Mutex::new(VecDeque::new()));
    let worker_queue = Arc::clone(&queue);
    let (ready_tx, ready_rx) = mpsc::channel::<std::result::Result<isize, String>>();
    thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            let class_name = w!("DocDockClipboardWorker");
            let wnd_class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                lpszClassName: class_name,
                ..Default::default()
            };
            unsafe { RegisterClassW(&wnd_class) };
            let hwnd = match unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class_name,
                    w!(""),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            } {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    if com.is_ok() {
                        unsafe { CoUninitialize() };
                    }
                    return;
                }
            };

            let listener_added = unsafe { AddClipboardFormatListener(hwnd) }.is_ok();
            let raw_hwnd = hwnd.0 as isize;
            let history_token = Clipboard::HistoryChanged(&EventHandler::<
                ClipboardHistoryChangedEventArgs,
            >::new(move |_, _| {
                let target = HWND(raw_hwnd as *mut _);
                let _ =
                    unsafe { PostMessageW(Some(target), WM_HISTORY_CHANGED, WPARAM(0), LPARAM(0)) };
                Ok(())
            }))
            .ok();

            if ready_tx.send(Ok(raw_hwnd)).is_err() {
                if let Some(tok) = history_token {
                    let _ = Clipboard::RemoveHistoryChanged(tok);
                }
                if listener_added {
                    let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
                }
                let _ = unsafe { DestroyWindow(hwnd) };
                if com.is_ok() {
                    unsafe { CoUninitialize() };
                }
                return;
            }

            let mut pending_notify = false;
            let mut stopping = false;
            let mut msg = MSG::default();
            while !stopping && unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                match msg.message {
                    WM_STOP_WORKER => {
                        unsafe { PostQuitMessage(0) };
                        break;
                    }
                    WM_JOB => drain_jobs(&worker_queue, hwnd, &mut pending_notify, &mut stopping),
                    WM_HISTORY_CHANGED => {
                        let _ =
                            unsafe { SetTimer(Some(hwnd), TIMER_NOTIFY, NOTIFY_DEBOUNCE_MS, None) };
                    }
                    WM_TIMER if msg.wParam.0 == TIMER_NOTIFY => {
                        let _ = unsafe { KillTimer(Some(hwnd), TIMER_NOTIFY) };
                        pending_notify = true;
                    }
                    _ => {
                        let _ = unsafe { TranslateMessage(&msg) };
                        unsafe { DispatchMessageW(&msg) };
                    }
                }
                if stopping {
                    unsafe { PostQuitMessage(0) };
                    break;
                }
                drain_jobs(&worker_queue, hwnd, &mut pending_notify, &mut stopping);
                if stopping {
                    unsafe { PostQuitMessage(0) };
                    break;
                }
                if pending_notify {
                    pending_notify = false;
                    on_change();
                }
            }

            if let Some(tok) = history_token {
                let _ = Clipboard::RemoveHistoryChanged(tok);
            }
            if listener_added {
                let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
            }
            let _ = unsafe { DestroyWindow(hwnd) };
            if com.is_ok() {
                unsafe { CoUninitialize() };
            }
        })
        .map_err(|error| error.to_string())?;

    let hwnd = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "clipboard worker failed to start".to_owned())??;
    Ok(ClipboardHandle { hwnd, queue })
}

fn drain_jobs(
    queue: &Arc<Mutex<VecDeque<Job>>>,
    hwnd: HWND,
    pending_notify: &mut bool,
    stopping: &mut bool,
) {
    while !*stopping {
        let (job, coalesced_snapshots) = {
            let mut guard = queue.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(job) = guard.pop_front() else {
                return;
            };
            let mut extras = Vec::new();
            if matches!(job, Job::Snapshot(_)) {
                while matches!(guard.front(), Some(Job::Snapshot(_))) {
                    if let Some(Job::Snapshot(reply)) = guard.pop_front() {
                        extras.push(reply);
                    }
                }
            }
            (job, extras)
        };
        let deadline = Instant::now() + JOB_TIMEOUT;
        match job {
            Job::Snapshot(reply) => {
                let result = read_snapshot(hwnd, pending_notify, stopping, deadline);
                let _ = reply.send(result.clone());
                for extra in coalesced_snapshots {
                    let _ = extra.send(result.clone());
                }
            }
            Job::Restore { id, reply } => {
                let _ = reply.send(restore_item(hwnd, pending_notify, stopping, deadline, &id));
            }
            Job::Delete { id, reply } => {
                let _ = reply.send(delete_item(hwnd, pending_notify, stopping, deadline, &id));
            }
        }
    }
}

fn read_snapshot(
    hwnd: HWND,
    pending_notify: &mut bool,
    stopping: &mut bool,
    deadline: Instant,
) -> std::result::Result<ClipboardSnapshot, String> {
    let history_enabled = Clipboard::IsHistoryEnabled().unwrap_or(false);
    let current_non_text = unsafe {
        CountClipboardFormats() > 0 && IsClipboardFormatAvailable(CF_UNICODETEXT).is_err()
    };

    if history_enabled {
        let op = Clipboard::GetHistoryItemsAsync().map_err(|error| error.to_string())?;
        let res = pump_async!(op, hwnd, pending_notify, stopping, deadline)?;
        match res.Status().map_err(|error| error.to_string())? {
            ClipboardHistoryItemsResultStatus::Success => {
                let list = res.Items().map_err(|error| error.to_string())?;
                let count = list.Size().map_err(|error| error.to_string())?;
                let text_format = StandardDataFormats::Text().ok();
                let mut items = Vec::new();
                for idx in 0..(count as usize).min(MAX_HISTORY_ITEMS) {
                    if *stopping {
                        return Err("clipboard worker stopping".to_owned());
                    }
                    if Instant::now() >= deadline {
                        break;
                    }
                    let Ok(item) = list.GetAt(idx as u32) else {
                        continue;
                    };
                    let Ok(content) = item.Content() else {
                        continue;
                    };
                    let Some(ref fmt) = text_format else {
                        continue;
                    };
                    if !content.Contains(fmt).unwrap_or(false) {
                        continue;
                    }
                    let Ok(text_op) = content.GetTextAsync() else {
                        continue;
                    };
                    let raw = match pump_async!(text_op, hwnd, pending_notify, stopping, deadline) {
                        Ok(value) => value,
                        Err(_) if *stopping => return Err("clipboard worker stopping".to_owned()),
                        Err(_) if Instant::now() >= deadline => break,
                        Err(_) => continue,
                    };
                    let (text, truncated) = normalize_and_cap_utf16(&raw, MAX_ITEM_CHARS);
                    let id = item.Id().map(|s| s.to_string_lossy()).unwrap_or_default();
                    let timestamp_ms = item
                        .Timestamp()
                        .ok()
                        .and_then(|ts| filetime_to_unix_ms(ts.UniversalTime));
                    items.push(ClipboardEntry {
                        id,
                        text,
                        truncated,
                        timestamp_ms,
                        deletable: true,
                    });
                }
                return Ok(ClipboardSnapshot {
                    history_enabled: true,
                    access_denied: false,
                    current_non_text,
                    items,
                });
            }
            ClipboardHistoryItemsResultStatus::AccessDenied => {
                return Ok(ClipboardSnapshot {
                    history_enabled: true,
                    access_denied: true,
                    current_non_text,
                    items: Vec::new(),
                });
            }
            ClipboardHistoryItemsResultStatus::ClipboardHistoryDisabled => {}
            other => {
                return Err(format!("unexpected clipboard history status: {}", other.0));
            }
        }
    }

    match read_current_win32_text() {
        Ok(Some((text, truncated))) => {
            let items = if text.is_empty() {
                Vec::new()
            } else {
                vec![ClipboardEntry {
                    id: "current".to_owned(),
                    text,
                    truncated,
                    timestamp_ms: None,
                    deletable: false,
                }]
            };
            Ok(ClipboardSnapshot {
                history_enabled: false,
                access_denied: false,
                current_non_text,
                items,
            })
        }
        Ok(None) => Ok(ClipboardSnapshot {
            history_enabled: false,
            access_denied: false,
            current_non_text,
            items: Vec::new(),
        }),
        Err(error) if error.code() == E_ACCESSDENIED => Ok(ClipboardSnapshot {
            history_enabled: false,
            access_denied: true,
            current_non_text,
            items: Vec::new(),
        }),
        Err(error) => Err(error.to_string()),
    }
}

fn read_clipboard_u32(format: u32) -> Option<u32> {
    if format == 0 || unsafe { IsClipboardFormatAvailable(format) }.is_err() {
        return None;
    }
    let Ok(handle) = (unsafe { GetClipboardData(format) }) else {
        return Some(0);
    };
    let hmem = HGLOBAL(handle.0);
    let bytes = unsafe { GlobalSize(hmem) };
    if bytes < std::mem::size_of::<u32>() {
        return Some(0);
    }
    let ptr = unsafe { GlobalLock(hmem) };
    if ptr.is_null() {
        return Some(0);
    }
    let mut buf = [0u8; 4];
    unsafe {
        std::ptr::copy_nonoverlapping(ptr.cast::<u8>(), buf.as_mut_ptr(), 4);
        let _ = GlobalUnlock(hmem);
    }
    Some(u32::from_ne_bytes(buf))
}

fn read_current_win32_text() -> Result<Option<(String, bool)>> {
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_err() {
        return Ok(None);
    }
    let exclude_fmt =
        unsafe { RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")) };
    if exclude_fmt != 0 && unsafe { IsClipboardFormatAvailable(exclude_fmt) }.is_ok() {
        return Ok(None);
    }
    let history_fmt = unsafe { RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")) };
    open(None)?;
    let read = (|| -> Result<Option<(String, bool)>> {
        let monitor_excluded =
            exclude_fmt != 0 && unsafe { IsClipboardFormatAvailable(exclude_fmt) }.is_ok();
        let include_flag = read_clipboard_u32(history_fmt);
        if is_clipboard_excluded(monitor_excluded, include_flag) {
            return Ok(None);
        }
        let Ok(handle) = (unsafe { GetClipboardData(CF_UNICODETEXT) }) else {
            let last = Error::from_thread();
            if last.code() == E_ACCESSDENIED {
                return Err(last);
            }
            return Ok(None);
        };
        let hmem = HGLOBAL(handle.0);
        let bytes = unsafe { GlobalSize(hmem) };
        let ptr = unsafe { GlobalLock(hmem) };
        if ptr.is_null() {
            let err = Error::from_thread();
            return if err.code() == E_ACCESSDENIED {
                Err(err)
            } else {
                Ok(None)
            };
        }
        let max_units = bytes / 2;
        let units = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), max_units) };
        let end = units.iter().position(|&ch| ch == 0).unwrap_or(max_units);
        let result = normalize_and_cap_utf16(&units[..end], MAX_ITEM_CHARS);
        let _ = unsafe { GlobalUnlock(hmem) };
        Ok(Some(result))
    })();
    let _ = unsafe { CloseClipboard() };
    read
}

fn restore_item(
    hwnd: HWND,
    pending_notify: &mut bool,
    stopping: &mut bool,
    deadline: Instant,
    id: &str,
) -> std::result::Result<bool, String> {
    if id == "current" {
        return Ok(true);
    }
    let op = Clipboard::GetHistoryItemsAsync().map_err(|error| error.to_string())?;
    let res = pump_async!(op, hwnd, pending_notify, stopping, deadline)?;
    if res.Status().map_err(|error| error.to_string())?
        != ClipboardHistoryItemsResultStatus::Success
    {
        return Ok(false);
    }
    let list = res.Items().map_err(|error| error.to_string())?;
    let count = list.Size().map_err(|error| error.to_string())?;
    for idx in 0..count {
        let Ok(item) = list.GetAt(idx) else {
            continue;
        };
        if item
            .Id()
            .map(|value| value.to_string_lossy() == id)
            .unwrap_or(false)
        {
            let status =
                Clipboard::SetHistoryItemAsContent(&item).map_err(|error| error.to_string())?;
            return Ok(status == SetHistoryItemAsContentStatus::Success);
        }
    }
    Ok(false)
}

fn delete_item(
    hwnd: HWND,
    pending_notify: &mut bool,
    stopping: &mut bool,
    deadline: Instant,
    id: &str,
) -> std::result::Result<bool, String> {
    let op = Clipboard::GetHistoryItemsAsync().map_err(|error| error.to_string())?;
    let res = pump_async!(op, hwnd, pending_notify, stopping, deadline)?;
    if res.Status().map_err(|error| error.to_string())?
        != ClipboardHistoryItemsResultStatus::Success
    {
        return Ok(false);
    }
    let list = res.Items().map_err(|error| error.to_string())?;
    let count = list.Size().map_err(|error| error.to_string())?;
    for idx in 0..count {
        let Ok(item) = list.GetAt(idx) else {
            continue;
        };
        if item
            .Id()
            .map(|value| value.to_string_lossy() == id)
            .unwrap_or(false)
        {
            return Clipboard::DeleteItemFromHistory(&item).map_err(|error| error.to_string());
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches doubled carriage returns, which some apps paste as blank lines.
    #[test]
    fn lone_line_feeds_become_crlf_and_crlf_stays() {
        assert_eq!(windows_newlines("a\nb\r\nc\n"), "a\r\nb\r\nc\r\n");
    }

    #[test]
    fn filetime_converts_ticks_to_unix_milliseconds_after_unix_epoch() {
        assert_eq!(filetime_to_unix_ms(0), None);
        assert_eq!(
            filetime_to_unix_ms(WINDOWS_TO_UNIX_EPOCH_TICKS + 1_500 * TICKS_PER_MS),
            Some(1_500)
        );
    }

    #[test]
    fn is_clipboard_excluded_honors_monitor_and_history_flags() {
        assert!(is_clipboard_excluded(true, None));
        assert!(is_clipboard_excluded(true, Some(1)));
        assert!(is_clipboard_excluded(false, Some(0)));
        assert!(!is_clipboard_excluded(false, None));
        assert!(!is_clipboard_excluded(false, Some(1)));
    }

    #[test]
    fn normalize_and_cap_utf16_handles_endings_surrogates_and_bounds() {
        let crlf: Vec<u16> = "a\r\nb\rc".encode_utf16().collect();
        let (norm, truncated) = normalize_and_cap_utf16(&crlf, 10);
        assert_eq!(norm, "a\nb\nc");
        assert!(!truncated);

        let exact: Vec<u16> = "a\r\n".encode_utf16().collect();
        assert_eq!(
            normalize_and_cap_utf16(&exact, 2),
            ("a\n".to_owned(), false)
        );

        let overflow: Vec<u16> = "a\r\nb".encode_utf16().collect();
        assert_eq!(
            normalize_and_cap_utf16(&overflow, 2),
            ("a\n".to_owned(), true)
        );

        let emoji: Vec<u16> = "aé😀z".encode_utf16().collect();
        assert_eq!(
            normalize_and_cap_utf16(&emoji, 3),
            ("aé😀".to_owned(), true)
        );

        let unpaired = [b'a' as u16, 0xD800, b'b' as u16];
        assert_eq!(
            normalize_and_cap_utf16(&unpaired, 10),
            ("a\u{FFFD}b".to_owned(), false)
        );
    }
}
