//! Código específico de sistema operacional. Hoje, só Windows.

mod windows;

pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, foreground_window, primary_monitor,
    restore_focus, slide_windows, wait_frame,
};
