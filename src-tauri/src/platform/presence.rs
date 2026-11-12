//! Whether a process is still running, across the Windows sessions of one computer (console and
//! Remote Desktop): a named mutex in the `Global\` namespace lives exactly as long as a handle to it.

use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, GetLastError};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::core::HSTRING;

fn global(name: &str) -> HSTRING {
    HSTRING::from(format!(r"Global\{name}"))
}

/// Makes `name` present until the handle closes, which the system also does when the process ends.
pub fn announce(name: &str) -> io::Result<OwnedHandle> {
    let handle = unsafe { CreateMutexW(None, false, &global(name)) }
        .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

/// Whether a process still holds `name` from `announce`; an error when that cannot be told.
pub fn present(name: &str) -> io::Result<bool> {
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &global(name)) } {
        Ok(handle) => {
            drop(unsafe { OwnedHandle::from_raw_handle(handle.0) });
            Ok(true)
        }
        Err(_) => match unsafe { GetLastError() } {
            ERROR_FILE_NOT_FOUND => Ok(false),
            // It exists, under rights this process lacks.
            ERROR_ACCESS_DENIED => Ok(true),
            error => Err(io::Error::from_raw_os_error(error.0 as i32)),
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_name_is_present_only_while_announced() {
        let name = format!("DocDock-test-presence-{}", std::process::id());
        assert!(!super::present(&name).unwrap());
        let held = super::announce(&name).unwrap();
        assert!(super::present(&name).unwrap());
        drop(held);
        assert!(!super::present(&name).unwrap());
    }
}
