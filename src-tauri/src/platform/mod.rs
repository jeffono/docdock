//! Código específico de sistema operacional. Hoje, só Windows.

mod windows;

pub use self::windows::{
    Monitor, PanelWindow, Point, Rect, cursor_pos, primary_monitor, restore_focus, wait_frame,
};
