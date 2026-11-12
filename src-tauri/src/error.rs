//! Errors the UI receives. Each serializes as `{ "code": "<variant>", ...fields }` and the UI
//! translates it by code; `detail` carries the operating system's message, already in its language.

use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Error {
    /// A failure without a specific code, such as an operating system error.
    Unexpected {
        detail: String,
    },
    NoFolder,
    /// The folder's volume, share or a folder above it is gone: a locked vault, an unplugged drive.
    FolderUnavailable,
    /// The folder above the notes folder is there, but the notes folder is not.
    FolderMissing,
    /// A folder is at the path but it is not the one chosen: its identity marker is absent or another.
    FolderUnrecognized,
    InvalidPath,
    InvalidVaultFile,
    VaultFileMissing,
    OutsideFolder,
    TooLarge {
        size_mb: u64,
        limit_mb: u64,
    },
    NotUtf8,
    UnsupportedImage,
    ImageTooLarge {
        limit_mb: u64,
    },
    AttachmentFolderUnsafe,
    SaveFailed {
        detail: String,
    },
    /// The save failed and the previous version could not return to the note's name.
    PreviousKept {
        path: String,
        detail: String,
    },
    /// The note changed outside the app and its external version could not return to the note's name.
    ExternalKept {
        path: String,
        detail: String,
    },
    /// As `ExternalKept`, and the saved text also stayed on disk under another name.
    ExternalAndOursKept {
        external: String,
        ours: String,
        detail: String,
    },
    /// The note changed outside the app while saving; the external version is back at the note and the
    /// saved text stayed under another name.
    OursKept {
        path: String,
    },
    /// `_pinned.md` kept changing outside the app while an edit was applied; nothing was written.
    PinnedChanged,
    /// The new name is taken, or the note changed since it was read; nothing moved.
    RenameConflict,
    /// A folder name Windows would not keep as typed, a hidden one, or one that leaves the folder.
    InvalidFolderName,
    /// A file, not a folder, already has the name `name`.
    FolderNameTaken {
        name: String,
    },
    /// The note was renamed or moved to `path`, but its pinned items still point at the old path.
    PinnedNotRenamed {
        path: String,
    },
    /// The note's volume has no Recycle Bin, or it would not take the note; nothing was deleted. The
    /// UI may ask to delete it for good once the panel closes (`binless`).
    NotRecyclable,
    DeleteFailed {
        detail: String,
    },
    /// The note or folder is deleted, but its pinned items still point at it. `undo`: Undo can bring
    /// it back.
    PinnedNotRemoved {
        undo: bool,
    },
    /// The panel closed since the delete, or it was already undone.
    UndoUnavailable,
    /// A file took the deleted note's or folder's name; nothing moved and Undo stays possible.
    RestoreTaken {
        path: String,
    },
    /// The note is back, but its pinned items are not.
    PinnedNotRestored,
    InvalidUrl {
        detail: String,
    },
    UrlNotAllowed {
        url: String,
    },
    LinkNotAllowed,
    /// A shortcut key needs Ctrl, Alt or Win.
    ShortcutModifierRequired,
    /// Ctrl+Alt with a character key is AltGr on layouts such as ABNT2.
    ShortcutAltGr,
    /// A key a shortcut cannot use, such as one whose name depends on the keyboard layout.
    ShortcutKeyUnsupported,
    /// Registration failed, usually because another app holds `shortcut`; the previous one stays.
    ShortcutTaken {
        shortcut: String,
    },
}

impl Error {
    pub fn unexpected(detail: impl std::fmt::Display) -> Self {
        Self::Unexpected {
            detail: detail.to_string(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::unexpected(error)
    }
}

impl From<tauri::Error> for Error {
    fn from(error: tauri::Error) -> Self {
        Self::unexpected(error)
    }
}
