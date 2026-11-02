//! Thin Win32 file layer for replacing notes without losing a concurrent external write.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows::Win32::Foundation::{ERROR_SHARING_VIOLATION, HANDLE};
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_INFO, FILE_GENERIC_READ, FileDispositionInfo, MOVEFILE_WRITE_THROUGH,
    MoveFileExW, REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
    SetFileInformationByHandle,
};
use windows::core::HSTRING;

/// Win32 failures arrive as `HRESULT_FROM_WIN32`; the low word is the Win32 code.
fn os_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xFFFF)
}

/// `ReplaceFileW`: `replacement` takes the name, attributes, ACL and creation time of `target`, and
/// the file that was at `target` moves to `backup`, which must not exist.
pub fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
    let flags = REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS;
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
