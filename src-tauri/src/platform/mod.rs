//! Código específico de sistema operacional. Hoje, só Windows.

mod windows;

pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, escape_pressed, foreground_window,
    monitors, restore_focus, slide_windows, wait_frame,
};
