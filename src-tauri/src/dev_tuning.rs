//! Development-only handle controls. Never compiled or registered in release builds.
use crate::handle::{ACTIVATION_PERCENT, REVEAL_DISTANCE};
use std::sync::atomic::{AtomicU32, Ordering};

static ACTIVATION: AtomicU32 = AtomicU32::new(ACTIVATION_PERCENT);
static REVEAL: AtomicU32 = AtomicU32::new(REVEAL_DISTANCE);

pub fn activation_percent() -> u32 {
    ACTIVATION.load(Ordering::Relaxed)
}
pub fn reveal_distance() -> u32 {
    REVEAL.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn dev_handle(activation_percent: u32, reveal_distance: u32) -> Result<(), String> {
    if !(100..=1000).contains(&activation_percent) || !(1..=500).contains(&reveal_distance) {
        return Err("Handle values out of range".into());
    }
    ACTIVATION.store(activation_percent, Ordering::Relaxed);
    REVEAL.store(reveal_distance, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn dev_fonts() -> Result<Vec<String>, String> {
    crate::platform::fonts::families()
}
