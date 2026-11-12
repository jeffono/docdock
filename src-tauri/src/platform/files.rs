//! Thin Win32 file layer for replacing notes without losing a concurrent external write.

use std::fs::{File, OpenOptions};
use std::io;
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

/// The serial number and file system name (`NTFS`, or the name a vault's driver gives) of the volume
/// holding `file`.
pub fn volume_of(file: &File) -> io::Result<(u32, String)> {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationByHandleW;
    let mut serial = 0u32;
    let mut name = [0u16; 261];
    unsafe {
        GetVolumeInformationByHandleW(
            HANDLE(file.as_raw_handle()),
            None,
            Some(&raw mut serial),
            None,
            None,
            Some(&mut name),
        )
    }
    .map_err(os_error)?;
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Ok((serial, String::from_utf16_lossy(&name[..end])))
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
