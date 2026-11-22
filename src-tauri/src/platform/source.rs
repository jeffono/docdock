//! The window something pasted came from: the one that put it on the clipboard, or the one the
//! panel opened over, by its title and program, so a new note can be named after it. Titles are
//! untrusted text from other apps.

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::DataExchange::GetClipboardOwner;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::{BOOL, PWSTR};

/// A window's title and its program's file name, such as `chrome.exe`, in lowercase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWindow {
    pub title: String,
    pub program: String,
}

/// The front window of the program that last put something on the clipboard. The owner itself is
/// often a hidden window, as a browser's, so its program's topmost visible titled window stands in.
pub fn clipboard_owner() -> Option<SourceWindow> {
    let owner = unsafe { GetClipboardOwner() }.ok()?;
    let process = process_of(owner)?;
    let front = front_window_of(process)?;
    Some(SourceWindow {
        title: title_of(front)?,
        program: program_of(process)?,
    })
}

/// The top-level window `raw`, such as the one the panel opened over, if it still exists.
pub fn window(raw: isize) -> Option<SourceWindow> {
    let hwnd = HWND(raw as *mut _);
    let process = process_of(hwnd)?;
    Some(SourceWindow {
        title: title_of(hwnd)?,
        program: program_of(process)?,
    })
}

fn process_of(hwnd: HWND) -> Option<u32> {
    let mut process = 0;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
    (thread != 0 && process != 0).then_some(process)
}

fn title_of(hwnd: HWND) -> Option<String> {
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    (copied > 0).then(|| String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn program_of(process: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) }.ok()?;
    let mut buffer = vec![0u16; 1024];
    let mut length = buffer.len() as u32;
    let read = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    read.ok()?;
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    let name = path.rsplit(['\\', '/']).next()?.to_lowercase();
    (!name.is_empty()).then_some(name)
}

/// The first visible, unowned, titled top-level window of `process`, in z-order from the top.
fn front_window_of(process: u32) -> Option<HWND> {
    struct Search {
        process: u32,
        found: Option<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        let search = unsafe { &mut *(data.0 as *mut Search) };
        let candidate = unsafe { IsWindowVisible(hwnd) }.as_bool()
            && unsafe { GetWindow(hwnd, GW_OWNER) }.is_err()
            && unsafe { GetWindowTextLengthW(hwnd) } > 0
            && process_of(hwnd) == Some(search.process);
        if candidate {
            search.found = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut search = Search {
        process,
        found: None,
    };
    // Stopping early makes EnumWindows answer an error with no code: the window is in `found`.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}
