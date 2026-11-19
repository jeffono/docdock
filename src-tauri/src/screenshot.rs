//! Rules for screenshots of the page window, which the browser engine renders through the DevTools
//! protocol. The page is untrusted: it controls its own size and what the image holds, so every
//! dimension and byte count is bounded before the image reaches the notes folder.

use base64::Engine;
use serde_json::Value;

/// Longest side in device pixels. Chromium's compositor draws at most this many pixels along a
/// side; captures beyond it come back blank or repeated.
pub const MAX_SIDE: u32 = 16_384;
/// Pixels in one image (4096 × 4096): bounds the bitmap the engine draws and encodes, about 64 MB.
pub const MAX_PIXELS: u64 = 16_777_216;
/// The same limit as pasted images, which captures become.
pub const MAX_BYTES: usize = crate::notes::MAX_ATTACHMENT_BYTES;
/// A PNG over `MAX_BYTES` is captured again as a JPEG of this quality, which photos need.
pub const JPEG_QUALITY: u8 = 85;

/// The page area to draw, in CSS pixels. `clipped`: the page is larger and only its start fits.
#[derive(Debug, PartialEq)]
pub struct Clip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub clipped: bool,
}

/// The whole page from a `Page.getLayoutMetrics` response, cut at `MAX_SIDE` along each side and
/// then in height to stay within `MAX_PIXELS`, both in device pixels. None for metrics without a
/// positive finite size or a device pixel ratio between 0.25 and 8.
pub fn full_page(metrics: &Value) -> Option<Clip> {
    let number = |value: &Value| value.as_f64().filter(|n| n.is_finite());
    let css = &metrics["cssContentSize"];
    let (x, y) = (number(&css["x"])?, number(&css["y"])?);
    let (width, height) = (number(&css["width"])?, number(&css["height"])?);
    if width < 1.0 || height < 1.0 {
        return None;
    }
    // `contentSize` is the same area in device pixels.
    let ratio = number(&metrics["contentSize"]["width"])? / width;
    if !(0.25..=8.0).contains(&ratio) {
        return None;
    }
    let (width, height) = (width.ceil(), height.ceil());
    let side = (f64::from(MAX_SIDE) / ratio).floor();
    let clipped_width = width.min(side);
    let budget = (MAX_PIXELS as f64 / (clipped_width * ratio) / ratio).floor();
    let clipped_height = height.min(side).min(budget);
    Some(Clip {
        x,
        y,
        width: clipped_width,
        height: clipped_height,
        clipped: clipped_width < width || clipped_height < height,
    })
}

#[derive(Debug, PartialEq)]
pub enum Refused {
    /// Valid, but over `MAX_BYTES`.
    TooLarge,
    /// Not an image of the format asked for, or over `MAX_SIDE` or `MAX_PIXELS`.
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Png,
    Jpeg,
}

/// Base64 characters for `bytes` bytes, padded.
const fn encoded_len(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

/// The image of a `Page.captureScreenshot` response, decoded. Its size is checked on the encoded
/// text before anything is decoded; a PNG's header must give dimensions within the limits.
pub fn image(response: &str, format: Format) -> Result<Vec<u8>, Refused> {
    #[derive(serde::Deserialize)]
    struct Screenshot {
        data: String,
    }
    // The engine encodes at most `MAX_PIXELS` pixels; anything longer is not its answer.
    if response.len() > encoded_len(MAX_PIXELS as usize * 4 + 1024 * 1024) {
        return Err(Refused::Invalid);
    }
    let data = serde_json::from_str::<Screenshot>(response)
        .map_err(|_| Refused::Invalid)?
        .data;
    let engine = base64::engine::general_purpose::STANDARD;
    // The header alone: 33 bytes, enough for a PNG's signature and IHDR.
    let head = engine
        .decode(&data.as_bytes()[..data.len().min(44)])
        .map_err(|_| Refused::Invalid)?;
    match format {
        Format::Png => png_size(&head).ok_or(Refused::Invalid)?,
        Format::Jpeg if head.starts_with(b"\xff\xd8\xff") => (0, 0),
        Format::Jpeg => return Err(Refused::Invalid),
    };
    if data.len() > encoded_len(MAX_BYTES) {
        return Err(Refused::TooLarge);
    }
    let bytes = engine.decode(&data).map_err(|_| Refused::Invalid)?;
    if bytes.len() > MAX_BYTES {
        return Err(Refused::TooLarge);
    }
    Ok(bytes)
}

/// Width and height from a PNG's signature and IHDR chunk, if within the limits.
fn png_size(head: &[u8]) -> Option<(u32, u32)> {
    let header = head.get(..24)?;
    if !header.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR") {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap_or_default());
    let (width, height) = (side(16), side(20));
    let fits = (1..=MAX_SIDE).contains(&width)
        && (1..=MAX_SIDE).contains(&height)
        && u64::from(width) * u64::from(height) <= MAX_PIXELS;
    fits.then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metrics(width: f64, height: f64, ratio: f64) -> Value {
        json!({
            "cssContentSize": { "x": 0.0, "y": 0.0, "width": width, "height": height },
            "contentSize": { "x": 0.0, "y": 0.0, "width": width * ratio, "height": height * ratio },
        })
    }

    fn png(width: u32, height: u32, body: usize) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend(vec![0; body]);
        bytes
    }

    fn response(bytes: &[u8]) -> String {
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        json!({ "data": data }).to_string()
    }

    #[test]
    fn a_page_within_the_limits_is_captured_whole() {
        let clip = full_page(&metrics(460.0, 3000.5, 1.5)).unwrap();
        assert_eq!(
            (clip.width, clip.height, clip.clipped),
            (460.0, 3001.0, false)
        );
    }

    #[test]
    fn a_tall_page_is_cut_at_the_side_limit_in_device_pixels() {
        let clip = full_page(&metrics(460.0, 50_000.0, 2.0)).unwrap();
        assert_eq!((clip.height, clip.clipped), (8192.0, true));
    }

    #[test]
    fn a_wide_page_is_cut_to_the_pixel_budget() {
        let clip = full_page(&metrics(10_000.0, 10_000.0, 1.0)).unwrap();
        assert_eq!(
            (clip.width, clip.height, clip.clipped),
            (10_000.0, 1677.0, true)
        );
        assert!(clip.width * clip.height <= MAX_PIXELS as f64);
    }

    #[test]
    fn metrics_without_a_usable_size_or_ratio_are_refused() {
        assert_eq!(full_page(&metrics(0.0, 100.0, 1.0)), None);
        assert_eq!(full_page(&metrics(100.0, 100.0, 20.0)), None);
        assert_eq!(
            full_page(&json!({ "cssContentSize": { "width": 1 } })),
            None
        );
    }

    #[test]
    fn a_png_within_the_limits_is_decoded() {
        let bytes = png(460, 900, 100);
        assert_eq!(image(&response(&bytes), Format::Png), Ok(bytes));
    }

    #[test]
    fn a_png_beyond_the_dimension_limits_is_refused() {
        let tall = png(10, MAX_SIDE + 1, 0);
        assert_eq!(image(&response(&tall), Format::Png), Err(Refused::Invalid));
        let crowded = png(MAX_SIDE, MAX_SIDE, 0);
        assert_eq!(
            image(&response(&crowded), Format::Png),
            Err(Refused::Invalid)
        );
    }

    #[test]
    fn a_png_over_the_byte_limit_is_too_large() {
        let bytes = png(460, 900, MAX_BYTES);
        assert_eq!(
            image(&response(&bytes), Format::Png),
            Err(Refused::TooLarge)
        );
    }

    #[test]
    fn data_of_another_format_is_refused() {
        let jpeg = b"\xff\xd8\xff\xe0rest";
        assert_eq!(image(&response(jpeg), Format::Png), Err(Refused::Invalid));
        assert_eq!(
            image(&response(&png(1, 1, 0)), Format::Jpeg),
            Err(Refused::Invalid)
        );
        assert_eq!(image(&response(jpeg), Format::Jpeg), Ok(jpeg.to_vec()));
    }

    #[test]
    fn a_response_that_is_not_base64_image_data_is_refused() {
        for bad in [r#"{"data":"not base64!"}"#, r#"{"data":42}"#, "[]", ""] {
            assert_eq!(image(bad, Format::Png), Err(Refused::Invalid), "{bad}");
        }
    }
}
