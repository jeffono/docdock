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
    FolderUnavailable,
    InvalidPath,
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
    /// The note was renamed to `path`, but its pinned items still point at the old name.
    PinnedNotRenamed {
        path: String,
    },
    InvalidUrl {
        detail: String,
    },
    UrlNotAllowed {
        url: String,
    },
    LinkNotAllowed,
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
