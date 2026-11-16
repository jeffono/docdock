//! Draws the edge handle's tab: a bulge of the panel, whose inner-edge line (or focus band) runs
//! around it, with the user's wordmark along it. Pure: it returns premultiplied RGBA pixels that
//! the platform layer shows with per-pixel alpha.

use std::sync::OnceLock;

use serde::Deserialize;
use tiny_skia::{
    Color, FillRule, LineCap, Mask, Paint, Path, PathBuilder, Pixmap, Stroke, Transform,
};

use crate::dock::Side;
use crate::handle::Shape;

/// The user's drawing, used only as a shape: its own fill is replaced by a theme color.
const WORDMARK_SVG: &str = include_str!("../icons/source/wordmark.svg");

/// Colors resolved from the UI's tokens for the current theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Look {
    pub background: [u8; 3],
    pub line: [u8; 3],
    pub accent: [u8; 3],
    pub text: [u8; 3],
}

/// What the tab shows besides its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub dpi: u32,
    pub side: Side,
    /// The panel has focus: the outline is the accent band instead of the line.
    pub focused: bool,
}

/// The tab's outer boundary for a panel on the right: down the panel's edge, out along an S-curve
/// to the tab's outer side, and back. It starts and ends `pad` beyond the window so its stroke
/// shows no caps, and leaves the panel's edge with a vertical tangent, so the line flows on.
fn boundary(shape: &Shape, pad: f32) -> PathBuilder {
    let edge = shape.depth as f32;
    let (curve, height) = (shape.curve, shape.height as f32);
    let mut path = PathBuilder::new();
    path.move_to(edge, -pad);
    path.line_to(edge, 0.0);
    path.cubic_to(edge, curve / 2.0, 0.0, curve / 2.0, 0.0, curve);
    path.line_to(0.0, height - curve);
    path.cubic_to(
        0.0,
        height - curve / 2.0,
        edge,
        height - curve / 2.0,
        edge,
        height,
    );
    path.line_to(edge, height + pad);
    path
}

/// The tab and the covered strip of the panel: the boundary closed beyond the window's panel side.
fn area(shape: &Shape, pad: f32) -> Option<Path> {
    let mut path = boundary(shape, pad);
    let far = shape.width() as f32 + pad;
    path.line_to(far, shape.height as f32 + pad);
    path.line_to(far, -pad);
    path.close();
    path.finish()
}

/// Mirrors the drawing for a panel on the left, whose tab grows right.
fn mirror(shape: &Shape, side: Side) -> Transform {
    match side {
        Side::Right => Transform::identity(),
        Side::Left => Transform::from_row(-1.0, 0.0, 0.0, 1.0, shape.width() as f32, 0.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Command(char),
    Number(f32),
}

fn tokenize(data: &str) -> Option<Vec<Token>> {
    fn flush(number: &mut String, tokens: &mut Vec<Token>) -> Option<()> {
        if !number.is_empty() {
            tokens.push(Token::Number(number.parse().ok()?));
            number.clear();
        }
        Some(())
    }
    let mut tokens = Vec::new();
    let mut number = String::new();
    for c in data.chars() {
        // A minus sign starts a number unless it belongs to an exponent.
        if c == '-' && !number.ends_with(['e', 'E']) {
            flush(&mut number, &mut tokens)?;
        }
        let exponent = matches!(c, 'e' | 'E') && !number.is_empty();
        if c.is_ascii_digit() || matches!(c, '.' | '-' | '+') || exponent {
            number.push(c);
        } else {
            flush(&mut number, &mut tokens)?;
            if c.is_ascii_alphabetic() {
                tokens.push(Token::Command(c));
            }
        }
    }
    flush(&mut number, &mut tokens)?;
    Some(tokens)
}

/// Adds SVG path data made of the absolute `M`, `L`, `C` and `Z` commands that the wordmark uses.
/// `None` for anything else, which a test catches if the drawing changes.
fn parse_path(data: &str, path: &mut PathBuilder) -> Option<()> {
    let mut tokens = tokenize(data)?.into_iter().peekable();
    while let Some(token) = tokens.next() {
        let Token::Command(command) = token else {
            return None;
        };
        let count = match command {
            'Z' => {
                path.close();
                continue;
            }
            'M' | 'L' => 2,
            'C' => 6,
            _ => return None,
        };
        // A command repeats while numbers follow it; after `M` they are lines.
        let mut first = true;
        loop {
            let mut values = [0.0; 6];
            for value in &mut values[..count] {
                let Some(Token::Number(number)) = tokens.next() else {
                    return None;
                };
                *value = number;
            }
            let [a, b, c, d, e, f] = values;
            match command {
                'M' if first => path.move_to(a, b),
                'C' => path.cubic_to(a, b, c, d, e, f),
                _ => path.line_to(a, b),
            }
            first = false;
            if !matches!(tokens.peek(), Some(Token::Number(_))) {
                break;
            }
        }
    }
    Some(())
}

/// The `d` attributes of the drawing's paths.
fn path_data(svg: &str) -> impl Iterator<Item = &str> {
    svg.match_indices("d=\"")
        .filter(|&(index, _)| svg[..index].ends_with(char::is_whitespace))
        .filter_map(|(index, start)| {
            let rest = &svg[index + start.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
}

fn wordmark() -> Option<&'static Path> {
    static PATH: OnceLock<Option<Path>> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut path = PathBuilder::new();
        for data in path_data(WORDMARK_SVG) {
            parse_path(data, &mut path)?;
        }
        path.finish()
    })
    .as_ref()
}

/// Places the wordmark, of tight bounds `bounds`, along the tab: a quarter turn counterclockwise,
/// so it reads from bottom to top, centered between the band and the panel's edge so focus does
/// not move it.
fn wordmark_transform(shape: &Shape, side: Side, bounds: tiny_skia::Rect) -> Transform {
    let scale = shape.wordmark / bounds.width();
    let across = (shape.depth + shape.band) as f32 / 2.0;
    let x = match side {
        Side::Right => across,
        Side::Left => shape.width() as f32 - across,
    };
    let y = shape.height as f32 / 2.0;
    let (u, v) = (
        bounds.left() + bounds.width() / 2.0,
        bounds.top() + bounds.height() / 2.0,
    );
    // (u, v) in the drawing goes to (x + scale·(v − center), y − scale·(u − center)).
    Transform::from_row(0.0, -scale, scale, 0.0, x - scale * v, y + scale * u)
}

fn color([red, green, blue]: [u8; 3]) -> Color {
    Color::from_rgba8(red, green, blue, 255)
}

/// The tab's pixels, premultiplied RGBA, sized as [`Shape`] says: transparent outside the outline.
pub fn draw(look: &Look, frame: Frame) -> Option<Pixmap> {
    let shape = Shape::new(frame.dpi);
    let mut pixmap = Pixmap::new(shape.width() as u32, shape.height as u32)?;
    let mirror = mirror(&shape, frame.side);
    let width = if frame.focused {
        shape.band
    } else {
        shape.line
    } as f32;
    let pad = 2.0 * width + 2.0;
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    pixmap.fill(color(look.background));
    // Centered on the boundary, twice as wide: the half outside is cut below, leaving a band as
    // wide as the panel's own.
    paint.set_color(color(if frame.focused {
        look.accent
    } else {
        look.line
    }));
    let stroke = Stroke {
        width: 2.0 * width,
        line_cap: LineCap::Butt,
        ..Stroke::default()
    };
    pixmap.stroke_path(
        &boundary(&shape, pad).finish()?,
        &paint,
        &stroke,
        mirror,
        None,
    );
    if let Some(wordmark) = wordmark() {
        paint.set_color(color(look.text));
        let place = wordmark_transform(&shape, frame.side, wordmark.bounds());
        pixmap.fill_path(wordmark, &paint, FillRule::Winding, place, None);
    }
    // Cut to the outline with antialiased edges. Scaling every premultiplied channel by the
    // coverage keeps the band's color right up to the edge.
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    mask.fill_path(&area(&shape, pad)?, FillRule::Winding, true, mirror);
    let pixels = pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut();
    for (pixel, &coverage) in pixels.zip(mask.data()) {
        for channel in pixel {
            *channel = ((u16::from(*channel) * u16::from(coverage) + 127) / 255) as u8;
        }
    }
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOK: Look = Look {
        background: [23, 25, 29],
        line: [44, 48, 56],
        accent: [106, 165, 255],
        text: [232, 234, 237],
    };
    /// 100%, 125%, 150%, 175% and 200%.
    const DPIS: [u32; 5] = [96, 120, 144, 168, 192];

    fn frames() -> impl Iterator<Item = Frame> {
        DPIS.into_iter().flat_map(|dpi| {
            [
                (Side::Right, false),
                (Side::Right, true),
                (Side::Left, false),
                (Side::Left, true),
            ]
            .map(|(side, focused)| Frame { dpi, side, focused })
        })
    }

    /// The pixel at `x` counted from the window's outer side, which is its right for a left panel.
    fn pixel(pixmap: &Pixmap, side: Side, x: i32, y: i32) -> [u8; 4] {
        let x = match side {
            Side::Right => x,
            Side::Left => pixmap.width() as i32 - 1 - x,
        };
        let p = pixmap.pixel(x as u32, y as u32).unwrap();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    fn opaque([red, green, blue]: [u8; 3]) -> [u8; 4] {
        [red, green, blue, 255]
    }

    /// Catches a seam where the tab meets the panel: at the window's top and bottom rows the outline
    /// must be exactly the panel's line or band, on the panel's own columns, with the panel's
    /// background after it and nothing before it.
    #[test]
    fn the_outline_continues_the_panel_edge_without_a_seam() {
        for frame in frames() {
            let shape = Shape::new(frame.dpi);
            let pixmap = draw(&LOOK, frame).unwrap();
            let (band, color) = if frame.focused {
                (shape.band, LOOK.accent)
            } else {
                (shape.line, LOOK.line)
            };
            assert!(
                shape.overlap > band,
                "{frame:?}: the window must cover the band"
            );
            for y in [0, shape.height - 1] {
                for x in 0..shape.depth - 1 {
                    assert!(
                        pixel(&pixmap, frame.side, x, y)[3] < 16,
                        "{frame:?} x={x} y={y}"
                    );
                }
                for x in shape.depth..shape.width() {
                    let expected = if x < shape.depth + band {
                        color
                    } else {
                        LOOK.background
                    };
                    assert_eq!(
                        pixel(&pixmap, frame.side, x, y),
                        opaque(expected),
                        "{frame:?} x={x} y={y}"
                    );
                }
            }
        }
    }

    /// Catches the band thinning, blurring or vanishing around the tab, as a half-pixel offset or
    /// an unmasked stroke would.
    #[test]
    fn the_outline_runs_crisp_along_the_outer_side() {
        for frame in frames() {
            let shape = Shape::new(frame.dpi);
            let pixmap = draw(&LOOK, frame).unwrap();
            let (band, color) = if frame.focused {
                (shape.band, LOOK.accent)
            } else {
                (shape.line, LOOK.line)
            };
            let y = shape.height / 2;
            for x in 0..band {
                assert_eq!(pixel(&pixmap, frame.side, x, y), opaque(color), "{frame:?}");
            }
            assert_eq!(
                pixel(&pixmap, frame.side, band, y),
                opaque(LOOK.background),
                "{frame:?}"
            );
        }
    }

    /// Catches a change to the drawing that this parser no longer reads, or a wordmark that spills
    /// over the band, the curves or the panel.
    #[test]
    fn the_wordmark_fits_inside_the_tab() {
        let wordmark = wordmark().expect("wordmark.svg uses only M, L, C and Z");
        let bounds = wordmark.bounds();
        assert!(
            bounds.left() >= 0.0 && bounds.right() <= 264.0,
            "{bounds:?}"
        );
        assert!(bounds.top() >= 0.0 && bounds.bottom() <= 50.0, "{bounds:?}");
        for dpi in DPIS {
            let shape = Shape::new(dpi);
            let place = wordmark_transform(&shape, Side::Right, bounds);
            let placed = wordmark.clone().transform(place).unwrap().bounds();
            assert!(placed.left() >= shape.band as f32, "{dpi}: {placed:?}");
            assert!(placed.right() <= shape.depth as f32, "{dpi}: {placed:?}");
            assert!(placed.top() >= shape.curve, "{dpi}: {placed:?}");
            assert!(
                placed.bottom() <= shape.height as f32 - shape.curve,
                "{dpi}: {placed:?}"
            );
        }
    }
}
