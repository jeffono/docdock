//! Código específico de sistema operacional. Hoje, só Windows.

mod windows;

pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, escape_pressed, foreground_window,
    monitors, restore_focus, slide_windows, wait_frame,
};

/// Substitui o arquivo de destino atomicamente no mesmo volume.
pub fn replace_file(
    from_path: &std::path::Path,
    to_path: &std::path::Path,
) -> ::windows::core::Result<()> {
    use ::windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    use std::os::windows::ffi::OsStrExt;
    let from: Vec<u16> = from_path.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to_path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
}
