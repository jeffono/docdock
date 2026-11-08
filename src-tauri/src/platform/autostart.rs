//! Starting with Windows through the current user's `Run` key, with the executable path quoted.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
/// Where Task Manager and Settings record that the user turned a `Run` entry off.
const APPROVED: PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
/// The value name, the same one older builds wrote through `tauri-plugin-autostart`.
const NAME: PCWSTR = w!("DocDock");

/// Whether the app starts with Windows: an entry exists and Task Manager has not turned it off.
pub fn is_enabled() -> io::Result<bool> {
    if read(RUN, RRF_RT_REG_SZ)?.is_none() {
        return Ok(false);
    }
    Ok(approved(read(APPROVED, RRF_RT_REG_BINARY)?.as_deref()))
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    if enabled {
        write(&command(&std::env::current_exe()?))?;
    } else {
        delete(RUN)?;
    }
    // A choice made in the tray overrides an earlier one in Task Manager. Without this value,
    // Windows treats the entry as on.
    delete(APPROVED)
}

/// Points an existing entry at this executable when older builds left it unquoted or the
/// executable it names no longer exists. An entry for another existing copy is left alone.
pub fn repair() -> io::Result<()> {
    let Some(stored) = read(RUN, RRF_RT_REG_SZ)? else {
        return Ok(());
    };
    let exe = std::env::current_exe()?;
    if needs_repair(&utf16(&stored), &exe, Path::exists) {
        write(&command(&exe))?;
    }
    Ok(())
}

/// The `Run` command for `exe`. Unquoted, Windows splits a path with spaces and may start
/// `C:\Program.exe` instead.
fn command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// The executable a `Run` command starts. Older builds wrote it unquoted, followed by a space.
fn command_exe(command: &str) -> &str {
    let command = command.trim();
    match command.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or(quoted),
        None => command,
    }
}

fn needs_repair(stored: &str, exe: &Path, exists: impl Fn(&Path) -> bool) -> bool {
    if stored == command(exe) {
        return false;
    }
    let target = command_exe(stored);
    // Windows paths ignore case.
    target.to_lowercase() == exe.display().to_string().to_lowercase() || !exists(Path::new(target))
}

/// Task Manager writes 12 bytes starting with 2 when the entry is on, and with 3 followed by the
/// time when the user turns it off. No value means on.
fn approved(value: Option<&[u8]>) -> bool {
    value
        .and_then(|bytes| bytes.first())
        .is_none_or(|flag| flag & 1 == 0)
}

/// The value `NAME` under `key` of the current user, or `None` if the key or value is missing.
fn read(key: PCWSTR, kind: REG_ROUTINE_FLAGS) -> io::Result<Option<Vec<u8>>> {
    let query = |data: Option<&mut [u8]>, size: &mut u32| {
        let data = data.map(|data| data.as_mut_ptr().cast());
        unsafe { RegGetValueW(HKEY_CURRENT_USER, key, NAME, kind, None, data, Some(size)) }
    };
    let mut size = 0;
    let status = query(None, &mut size);
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check(status)?;
    let mut data = vec![0; size as usize];
    let status = query(Some(&mut data), &mut size);
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check(status)?;
    data.truncate(size as usize);
    Ok(Some(data))
}

/// A `REG_SZ` value as text, without its terminating null.
fn utf16(data: &[u8]) -> String {
    let wide: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    let end = wide
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

fn write(command: &str) -> io::Result<()> {
    let wide: Vec<u16> = command.encode_utf16().chain([0]).collect();
    let size = (wide.len() * 2) as u32;
    check(unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN,
            NAME,
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            size,
        )
    })
}

fn delete(key: PCWSTR) -> io::Result<()> {
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key, NAME) };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(status)
}

fn check(status: WIN32_ERROR) -> io::Result<()> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status.0 as i32))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{approved, command, command_exe, needs_repair};

    const EXE: &str = r"C:\Program Files\DocDock\docdock.exe";

    #[test]
    fn a_path_with_spaces_is_quoted_whole() {
        let command = command(Path::new(EXE));
        assert_eq!(command, format!("\"{EXE}\""));
        assert_eq!(command_exe(&command), EXE);
    }

    #[test]
    fn the_current_entry_is_kept() {
        let current = format!("\"{EXE}\"");
        assert!(!needs_repair(&current, Path::new(EXE), |_| true));
    }

    #[test]
    fn an_unquoted_entry_from_older_builds_is_rewritten() {
        assert!(needs_repair(&format!("{EXE} "), Path::new(EXE), |_| true));
    }

    #[test]
    fn an_entry_for_a_moved_executable_is_rewritten() {
        let old = r#""C:\Users\me\Old Place\docdock.exe""#;
        assert!(needs_repair(old, Path::new(EXE), |_| false));
    }

    #[test]
    fn an_entry_for_another_existing_copy_is_kept() {
        let other = r#""D:\dev\docdock\target\release\docdock.exe""#;
        assert!(!needs_repair(other, Path::new(EXE), |_| true));
    }

    #[test]
    fn task_manager_turning_the_entry_off_disables_it() {
        let off = [3, 0, 0, 0, 0x21, 0xAA, 0xF0, 0x92, 0xEE, 0x79, 0xDC, 0x01];
        let on = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(!approved(Some(&off)));
        assert!(approved(Some(&on)));
        assert!(approved(None));
    }
}
