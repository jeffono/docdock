//! Thin Win32 file layer for replacing notes without losing a concurrent external write.

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows::Win32::Foundation::{ERROR_SHARING_VIOLATION, HANDLE};
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_INFO, FILE_GENERIC_READ, FileDispositionInfo, MOVEFILE_WRITE_THROUGH,
    MoveFileExW, REPLACE_FILE_FLAGS, ReplaceFileW, SetFileInformationByHandle,
};
use windows::core::HSTRING;

/// Rejects links and prevents directory replacement until the returned handle is dropped. Hold every
/// ancestor too, from the volume root down. Opened without following reparse points, `is_dir` is false
/// only for name-surrogate tags (junctions, symlinks, mount points): cloud placeholders such as
/// OneDrive folders are reparse points too and must keep working.
pub fn hold_directory(path: &Path) -> io::Result<File> {
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let file = OpenOptions::new()
        // Metadata-only access (0) does not participate in Windows sharing checks.
        .access_mode(FILE_GENERIC_READ.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
        .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Attachment folder cannot be a link",
        ));
    }
    Ok(file)
}

/// Opens a folder through links and mount points, so the handle is on the volume that really holds
/// it, and keeps it from being renamed or deleted while the handle lives. Reading access includes
/// listing: a folder whose ACL refuses listing fails to open.
pub fn open_folder(path: &Path) -> io::Result<File> {
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    OpenOptions::new()
        .access_mode(FILE_GENERIC_READ.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
}

/// Rejects links and prevents directory swaps in `dir` and every folder above it while the handles live.
pub fn hold_ancestry(dir: &Path) -> io::Result<Vec<File>> {
    hold_from_top(dir, hold_directory)
}

/// Some network file systems, such as a Cryptomator vault mounted through WinFsp, expose a share whose
/// top levels cannot be opened (`\\cryptomator-vault\<id>\` fails with ERROR_BAD_NETPATH). Those levels
/// are skipped: no local process can rename a network share root. Only the topmost levels may be
/// skipped, and never `dir` itself.
fn hold_from_top<T>(dir: &Path, hold: impl Fn(&Path) -> io::Result<T>) -> io::Result<Vec<T>> {
    use windows::Win32::Foundation::{ERROR_BAD_NET_NAME, ERROR_BAD_NETPATH};
    let mut held = Vec::new();
    for folder in dir.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match hold(folder) {
            Ok(handle) => held.push(handle),
            Err(error)
                if held.is_empty()
                    && folder != dir
                    && [ERROR_BAD_NETPATH.0, ERROR_BAD_NET_NAME.0]
                        .map(|code| Some(code as i32))
                        .contains(&error.raw_os_error()) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(held)
}

/// The longest file name Windows accepts, in UTF-16 units.
pub const MAX_NAME: usize = 255;

/// `name` cut to at most `units` UTF-16 units, never between the halves of a surrogate pair.
pub fn fit_name(name: &OsStr, units: usize) -> OsString {
    let mut wide: Vec<u16> = name.encode_wide().collect();
    if wide.len() > units {
        let low = (0xDC00..0xE000).contains(&wide[units]);
        wide.truncate(if low && units > 0 { units - 1 } else { units });
    }
    OsString::from_wide(&wide)
}

pub fn paste_timestamp() -> String {
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}

/// Win32 failures arrive as `HRESULT_FROM_WIN32`; the low word is the Win32 code.
fn os_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xFFFF)
}

/// `ReplaceFileW`: `replacement` takes the name, attributes, ACL and creation time of `target`, and
/// the file that was at `target` moves to `backup`, which must not exist. Metadata errors are not
/// ignored: a note whose ACL cannot be carried over must not become more open; it fails unchanged.
pub fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
    let flags = REPLACE_FILE_FLAGS(0);
    let (target, replacement, backup) = (
        HSTRING::from(target),
        HSTRING::from(replacement),
        HSTRING::from(backup),
    );
    unsafe { ReplaceFileW(&target, &replacement, &backup, flags, None, None) }.map_err(os_error)
}

/// Renames `from` to `to`; fails if `to` exists.
pub fn move_new(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (HSTRING::from(from), HSTRING::from(to));
    unsafe { MoveFileExW(&from, &to, MOVEFILE_WRITE_THROUGH) }.map_err(os_error)
}

/// Opens `path` for reading and deletion without sharing: no other handle was open, and none can be
/// opened while the file lives. `None` when another handle is open.
pub fn open_exclusive(path: &Path) -> io::Result<Option<File>> {
    let opened = OpenOptions::new()
        .access_mode((FILE_GENERIC_READ | DELETE).0)
        .share_mode(0)
        .open(path);
    match opened {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION.0 as i32) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Deletes a file from `open_exclusive` when it is closed.
pub fn delete_on_close(file: &File) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&raw const info).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(os_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VAULT: &str = r"\\?\UNC\cryptomator-vault\id\vault\Notes";

    fn holding(unreachable: &'static [&'static str]) -> impl Fn(&Path) -> io::Result<String> {
        move |path| {
            let path = path.to_string_lossy().into_owned();
            match unreachable.contains(&path.as_str()) {
                true => Err(io::Error::from_raw_os_error(53)),
                false => Ok(path),
            }
        }
    }

    #[test]
    fn an_unreachable_share_root_is_skipped() {
        let held = hold_from_top(
            Path::new(VAULT),
            holding(&[r"\\?\UNC\cryptomator-vault\id\"]),
        );
        assert_eq!(
            held.unwrap(),
            [r"\\?\UNC\cryptomator-vault\id\vault", VAULT]
        );
    }

    #[test]
    fn an_unreachable_folder_below_a_held_one_fails() {
        let held = hold_from_top(
            Path::new(VAULT),
            holding(&[r"\\?\UNC\cryptomator-vault\id\vault"]),
        );
        assert!(held.is_err());
    }

    #[test]
    fn an_unreachable_folder_itself_fails() {
        let held = hold_from_top(
            Path::new(VAULT),
            holding(&[
                r"\\?\UNC\cryptomator-vault\id\",
                r"\\?\UNC\cryptomator-vault\id\vault",
                VAULT,
            ]),
        );
        assert!(held.is_err());
    }
}
