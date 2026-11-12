//! Text on the clipboard kept out of Windows clipboard history and cloud clipboard, through the
//! registered formats in "Cloud Clipboard and Clipboard History Formats":
//! <https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats>.

use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::core::{Error, PCWSTR, Result, w};

/// `CF_UNICODETEXT`, defined in `Win32_System_Ole`, which nothing else needs.
const CF_UNICODETEXT: u32 = 13;

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

/// Replaces the clipboard with `text`, excluded from history and cloud clipboard in the same
/// transaction, so no monitor sees the text without the exclusion. `owner` is a window of this
/// process: with no owner, `SetClipboardData` fails. If a later step fails, the previous content
/// is already gone.
pub fn write_private_text(owner: isize, text: &str) -> Result<()> {
    open(HWND(owner as *mut _))?;
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

fn open(owner: HWND) -> Result<()> {
    let mut attempt = 1;
    loop {
        match unsafe { OpenClipboard(Some(owner)) } {
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

#[cfg(test)]
mod tests {
    use super::*;

    // Catches doubled carriage returns, which some apps paste as blank lines.
    #[test]
    fn lone_line_feeds_become_crlf_and_crlf_stays() {
        assert_eq!(windows_newlines("a\nb\r\nc\n"), "a\r\nb\r\nc\r\n");
    }
}
