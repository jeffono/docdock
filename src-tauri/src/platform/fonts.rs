//! Installed font discovery, only compiled for the development tweak panel.
use std::collections::BTreeSet;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Graphics::Gdi::{
    DEFAULT_CHARSET, EnumFontFamiliesExW, GetDC, LOGFONTW, ReleaseDC, TEXTMETRICW,
    TRUETYPE_FONTTYPE,
};

unsafe extern "system" fn collect(
    font: *const LOGFONTW,
    _: *const TEXTMETRICW,
    kind: u32,
    data: LPARAM,
) -> i32 {
    // Raster and vector faces (Modern, MS Sans Serif) are GDI-only: the WebView cannot use them.
    if kind & TRUETYPE_FONTTYPE == 0 {
        return 1;
    }
    // GDI calls synchronously with a LOGFONTW prefix and our live collection pointer.
    let font = unsafe { &*font };
    let names = unsafe { &mut *(data.0 as *mut BTreeSet<String>) };
    let end = font
        .lfFaceName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(font.lfFaceName.len());
    let name = String::from_utf16_lossy(&font.lfFaceName[..end]);
    if !name.is_empty() && !name.starts_with('@') {
        names.insert(name);
    }
    1
}

pub fn families() -> Result<Vec<String>, String> {
    let mut names = BTreeSet::new();
    unsafe {
        let dc = GetDC(None);
        if dc.0.is_null() {
            return Err("Could not enumerate system fonts".into());
        }
        let font = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        EnumFontFamiliesExW(
            dc,
            &font,
            Some(collect),
            LPARAM(&mut names as *mut BTreeSet<String> as isize),
            0,
        );
        ReleaseDC(None, dc);
    }
    Ok(names.into_iter().collect())
}
