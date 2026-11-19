//! Rules for screenshots of the page window, which the browser engine renders through the DevTools
//! protocol. The page is untrusted: it controls its own size and what the image holds, so every
//! dimension and byte count is bounded before the image reaches the notes folder.

use std::borrow::Cow;

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
/// Longest `Page.captureScreenshot` response worth reading, in characters: the base64 of
/// `MAX_BYTES` and its JSON wrapper. A longer one is over the byte limit and is never copied.
pub const MAX_RESPONSE: usize = encoded_len(MAX_BYTES) + 64;
/// Longest `Page.getLayoutMetrics` response, in characters: a few dozen numbers.
pub const MAX_METRICS: usize = 4096;
/// Base64 characters decoded to find the dimensions: a PNG's IHDR comes first, and a JPEG's
/// start-of-frame follows its JFIF header and quantization tables, a few hundred bytes in.
const HEAD_CHARS: usize = 4096;

/// The page area to draw from the document's top left corner, in DIPs as the `clip` of
/// `Page.captureScreenshot` takes it. `clipped`: the page is larger and only its start fits.
#[derive(Debug, PartialEq)]
pub struct Clip {
    pub width: f64,
    pub height: f64,
    pub clipped: bool,
}

/// The whole page from a `Page.getLayoutMetrics` response, cut so that the bitmap the engine draws
/// stays within `MAX_SIDE` along each side and then, in height, within `MAX_PIXELS`. None for
/// metrics without a positive size, a device scale factor between 0.25 and 8 or a page zoom
/// between 0.1 and 10.
///
/// Units, from Chromium's `InspectorPageAgent::getLayoutMetrics` and
/// `PageHandler::CaptureScreenshot`: `contentSize` is in physical pixels, CSS pixels times the page
/// zoom and the device scale factor; `visualViewport.clientWidth` is physical and
/// `cssVisualViewport.clientWidth` the same width in CSS pixels; `zoom` is the page zoom alone. A
/// clip is in DIPs, CSS pixels times the page zoom, and the engine draws `round(DIPs × device scale
/// factor)` pixels. So the limits apply to physical pixels, and the clip is those divided by the
/// device scale factor.
pub fn full_page(metrics: &Value) -> Option<Clip> {
    let number = |value: &Value| value.as_f64().filter(|n| n.is_finite() && *n > 0.0);
    let content = &metrics["contentSize"];
    let width = number(&content["width"])?.ceil();
    let height = number(&content["height"])?.ceil();
    let css = &metrics["cssVisualViewport"];
    let css_to_physical =
        number(&metrics["visualViewport"]["clientWidth"])? / number(&css["clientWidth"])?;
    let zoom = number(&css["zoom"])?;
    let scale = css_to_physical / zoom;
    if !(0.25..=8.0).contains(&scale) || !(0.1..=10.0).contains(&zoom) {
        return None;
    }
    let kept_width = width.min(f64::from(MAX_SIDE));
    let budget = (MAX_PIXELS as f64 / kept_width).floor();
    let kept_height = height.min(f64::from(MAX_SIDE)).min(budget);
    Some(Clip {
        width: kept_width / scale,
        height: kept_height / scale,
        clipped: kept_width < width || kept_height < height,
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

/// The image of a `Page.captureScreenshot` response, decoded. The response is read in place, and
/// its header must give dimensions within the limits and its size fit `MAX_BYTES` before the
/// whole image is decoded.
pub fn image(response: &str, format: Format) -> Result<Vec<u8>, Refused> {
    #[derive(serde::Deserialize)]
    struct Screenshot<'a> {
        #[serde(borrow)]
        data: Cow<'a, str>,
    }
    if response.len() > MAX_RESPONSE {
        return Err(Refused::TooLarge);
    }
    let data = serde_json::from_str::<Screenshot>(response)
        .map_err(|_| Refused::Invalid)?
        .data;
    let engine = base64::engine::general_purpose::STANDARD;
    let head = engine
        .decode(&data.as_bytes()[..data.len().min(HEAD_CHARS)])
        .map_err(|_| Refused::Invalid)?;
    let size = match format {
        Format::Png => png_size(&head),
        Format::Jpeg => jpeg_size(&head),
    };
    if !size.is_some_and(fits) {
        return Err(Refused::Invalid);
    }
    if data.len() > encoded_len(MAX_BYTES) {
        return Err(Refused::TooLarge);
    }
    let bytes = engine
        .decode(data.as_bytes())
        .map_err(|_| Refused::Invalid)?;
    if bytes.len() > MAX_BYTES {
        return Err(Refused::TooLarge);
    }
    Ok(bytes)
}

fn fits((width, height): (u32, u32)) -> bool {
    (1..=MAX_SIDE).contains(&width)
        && (1..=MAX_SIDE).contains(&height)
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// Width and height from a PNG's signature and IHDR chunk.
fn png_size(head: &[u8]) -> Option<(u32, u32)> {
    let header = head.get(..24)?;
    if !header.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR") {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap_or_default());
    Some((side(16), side(20)))
}

/// Width and height from a JPEG's start-of-frame segment, walking the segments before it within
/// `head` only. None if scan data or the end comes first, or the frame lies beyond `head`.
fn jpeg_size(head: &[u8]) -> Option<(u32, u32)> {
    if !head.starts_with(b"\xff\xd8") {
        return None;
    }
    let mut at = 2;
    loop {
        // Fill bytes may precede a marker.
        while head.get(at..at + 2)? == b"\xff\xff" {
            at += 1;
        }
        let [0xff, marker, high, low] = *head.get(at..at + 4)? else {
            return None;
        };
        match marker {
            // SOF0 to SOF15; C4, C8 and CC are other segments in the same range.
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                // Length, precision, then height and width.
                let frame = head.get(at + 5..at + 9)?;
                let height = u16::from_be_bytes([frame[0], frame[1]]);
                let width = u16::from_be_bytes([frame[2], frame[3]]);
                return Some((width.into(), height.into()));
            }
            0xda | 0xd9 => return None,
            _ => {
                let length = usize::from(u16::from_be_bytes([high, low]));
                if length < 2 {
                    return None;
                }
                at += 2 + length;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A page of `width` × `height` CSS pixels at a page `zoom` on a monitor of `scale`.
    fn metrics(width: f64, height: f64, zoom: f64, scale: f64) -> Value {
        let physical = zoom * scale;
        json!({
            "contentSize": { "x": 0, "y": 0, "width": width * physical, "height": height * physical },
            "visualViewport": { "clientWidth": 400.0 * physical, "zoom": zoom },
            "cssVisualViewport": { "clientWidth": 400.0, "zoom": zoom },
        })
    }

    /// The bitmap the engine draws for `clip` at `scale`, rounded as `CaptureScreenshot` does.
    fn drawn(clip: &Clip, scale: f64) -> (f64, f64) {
        ((clip.width * scale).round(), (clip.height * scale).round())
    }

    fn png(width: u32, height: u32, body: usize) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend(vec![0; body]);
        bytes
    }

    /// A JPEG's start as an encoder writes it: its JFIF header and a quantization table.
    fn jpeg_tables() -> Vec<u8> {
        let mut bytes = b"\xff\xd8\xff\xe0\x00\x10JFIF\0\x01\x01\0\0\x01\0\x01\0\0".to_vec();
        bytes.extend(b"\xff\xdb\x00\x43\x00");
        bytes.extend([1; 64]);
        bytes
    }

    /// `jpeg_tables` and then a baseline frame.
    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = jpeg_tables();
        bytes.extend(b"\xff\xc0\x00\x11\x08");
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend(b"\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01");
        bytes
    }

    fn response(bytes: &[u8]) -> String {
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        json!({ "data": data }).to_string()
    }

    #[test]
    fn a_page_within_the_limits_is_captured_whole() {
        let clip = full_page(&metrics(460.0, 3000.0, 1.0, 1.5)).unwrap();
        assert_eq!(
            (clip.width, clip.height, clip.clipped),
            (460.0, 3000.0, false)
        );
        assert_eq!(drawn(&clip, 1.5), (690.0, 4500.0));
    }

    #[test]
    fn page_zoom_enlarges_the_clip_and_the_device_scale_does_not() {
        // Both draw 2000 × 4000 pixels; the clip is in DIPs, which the page zoom scales.
        let zoomed = full_page(&metrics(1000.0, 2000.0, 2.0, 1.0)).unwrap();
        let dense = full_page(&metrics(1000.0, 2000.0, 1.0, 2.0)).unwrap();
        assert_eq!((zoomed.width, zoomed.height), (2000.0, 4000.0));
        assert_eq!((dense.width, dense.height), (1000.0, 2000.0));
        assert_eq!(drawn(&zoomed, 1.0), drawn(&dense, 2.0));
        assert!(!zoomed.clipped && !dense.clipped);
    }

    #[test]
    fn a_tall_page_is_cut_at_the_side_limit_in_device_pixels() {
        let clip = full_page(&metrics(460.0, 50_000.0, 1.0, 2.0)).unwrap();
        assert_eq!((clip.height, clip.clipped), (8192.0, true));
        assert_eq!(drawn(&clip, 2.0).1, f64::from(MAX_SIDE));
    }

    #[test]
    fn a_page_zoomed_out_is_cut_to_the_pixel_budget_it_draws() {
        // At 50% zoom, 20,000 CSS pixels draw 10,000.
        let clip = full_page(&metrics(20_000.0, 20_000.0, 0.5, 1.0)).unwrap();
        let (width, height) = drawn(&clip, 1.0);
        assert_eq!((width, height, clip.clipped), (10_000.0, 1677.0, true));
        assert!(width * height <= MAX_PIXELS as f64);
    }

    #[test]
    fn metrics_without_a_usable_size_scale_or_zoom_are_refused() {
        assert_eq!(full_page(&metrics(0.0, 100.0, 1.0, 1.0)), None);
        assert_eq!(full_page(&metrics(100.0, 100.0, 1.0, 20.0)), None);
        let mut no_zoom = metrics(100.0, 100.0, 1.0, 1.0);
        no_zoom["cssVisualViewport"]["zoom"] = json!(null);
        assert_eq!(full_page(&no_zoom), None);
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
    fn a_jpeg_within_the_limits_is_decoded() {
        let bytes = jpeg(460, 900);
        assert_eq!(image(&response(&bytes), Format::Jpeg), Ok(bytes));
    }

    #[test]
    fn a_jpeg_without_a_frame_is_refused() {
        let mut bytes = jpeg_tables();
        bytes.extend(b"\xff\xda\x00\x08rest");
        assert_eq!(
            image(&response(&bytes), Format::Jpeg),
            Err(Refused::Invalid)
        );
    }

    #[test]
    fn a_jpeg_beyond_the_dimension_limits_is_refused() {
        let bytes = jpeg(MAX_SIDE as u16, MAX_SIDE as u16);
        assert_eq!(
            image(&response(&bytes), Format::Jpeg),
            Err(Refused::Invalid)
        );
    }

    #[test]
    fn data_of_another_format_is_refused() {
        let jpeg = jpeg(10, 10);
        assert_eq!(image(&response(&jpeg), Format::Png), Err(Refused::Invalid));
        assert_eq!(
            image(&response(&png(1, 1, 0)), Format::Jpeg),
            Err(Refused::Invalid)
        );
    }

    #[test]
    fn a_response_that_is_not_base64_image_data_is_refused() {
        for bad in [r#"{"data":"not base64!"}"#, r#"{"data":42}"#, "[]", ""] {
            assert_eq!(image(bad, Format::Png), Err(Refused::Invalid), "{bad}");
        }
    }
}
