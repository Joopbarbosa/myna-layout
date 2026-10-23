//! The short text next to the wave (RN-NOVA-14). The drawing library has no text
//! support, so glyphs are rasterised here with the system's sans font.

use std::process::Command;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use tiny_skia::Pixmap;

use crate::theme;

/// Draws centred text into a premultiplied RGBA pixmap. Without a usable font it
/// draws nothing (the bar still works) and says so once in the log.
pub struct TextPainter {
    font: Option<FontVec>,
}

impl std::fmt::Debug for TextPainter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextPainter")
            .field("has_font", &self.font.is_some())
            .finish()
    }
}

impl TextPainter {
    /// Load the system sans font: the one `fc-match` names, else the first fallback
    /// file that exists.
    pub fn new() -> Self {
        let font = find_font_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| FontVec::try_from_vec(bytes).ok());
        if font.is_none() {
            tracing::warn!("no usable font found: the bar will not show text");
        }
        Self { font }
    }

    /// Whether a font was found.
    pub fn has_font(&self) -> bool {
        self.font.is_some()
    }

    /// Draw `text` centred horizontally in the row starting at `top`, `row_height` pixels
    /// tall, with `alpha` from 0 to 1 on top of the pixmap.
    pub fn draw(&self, pixmap: &mut Pixmap, text: &str, top: f32, row_height: f32, alpha: f32) {
        let Some(font) = &self.font else { return };
        let scaled = font.as_scaled(PxScale::from(theme::TEXT_SIZE));
        let advance: f32 = text
            .chars()
            .map(|c| scaled.h_advance(scaled.glyph_id(c)))
            .sum();
        let width = pixmap.width() as f32;
        let mut caret = ((width - advance) / 2.0).max(0.0);
        let baseline = top + (row_height + scaled.ascent() + scaled.descent()) / 2.0;
        let (red, green, blue) = theme::TEXT_COLOUR;
        let data_width = pixmap.width() as i32;
        let data_height = pixmap.height() as i32;
        for character in text.chars() {
            let id = scaled.glyph_id(character);
            let glyph = id.with_scale_and_position(scaled.scale(), point(caret, baseline));
            caret += scaled.h_advance(id);
            let Some(outline) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            outline.draw(|x, y, coverage| {
                let px = bounds.min.x as i32 + x as i32;
                let py = bounds.min.y as i32 + y as i32;
                if px < 0 || py < 0 || px >= data_width || py >= data_height {
                    return;
                }
                let weight = coverage * alpha * theme::TEXT_ALPHA;
                let offset = ((py * data_width + px) * 4) as usize;
                let pixel = &mut pixmap.data_mut()[offset..offset + 4];
                let over = |source: u8, current: u8| {
                    (f32::from(source) * weight + f32::from(current) * (1.0 - weight)).round() as u8
                };
                pixel[0] = over(red, pixel[0]);
                pixel[1] = over(green, pixel[1]);
                pixel[2] = over(blue, pixel[2]);
                pixel[3] = over(255, pixel[3]);
            });
        }
    }
}

impl Default for TextPainter {
    fn default() -> Self {
        Self::new()
    }
}

fn find_font_path() -> Option<String> {
    let named = Command::new("fc-match")
        .args(["-f", "%{file}", "sans"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .filter(|path| std::path::Path::new(path).is_file());
    named.or_else(|| {
        theme::FALLBACK_FONTS
            .iter()
            .find(|path| std::path::Path::new(path).is_file())
            .map(|path| (*path).to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(pixmap: &Pixmap) -> u64 {
        pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| u64::from(pixel[3]))
            .sum()
    }

    #[test]
    fn text_is_drawn_inside_its_row_and_nothing_elsewhere() {
        let painter = TextPainter::new();
        if !painter.has_font() {
            eprintln!("skipped: this machine has no sans font");
            return;
        }
        let mut pixmap = Pixmap::new(200, 60).unwrap();
        painter.draw(&mut pixmap, "Ouvindo", 40.0, 20.0, 1.0);
        assert!(ink(&pixmap) > 0, "the text must leave ink");
        let above_the_row: u64 = pixmap.data()[..(38 * 200 * 4)]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| u64::from(pixel[3]))
            .sum();
        assert_eq!(above_the_row, 0, "nothing may be drawn above the text row");
    }

    #[test]
    fn longer_text_leaves_more_ink_and_zero_alpha_leaves_none() {
        let painter = TextPainter::new();
        if !painter.has_font() {
            return;
        }
        let mut short = Pixmap::new(200, 30).unwrap();
        let mut long = Pixmap::new(200, 30).unwrap();
        let mut faded = Pixmap::new(200, 30).unwrap();
        painter.draw(&mut short, "Ok", 5.0, 20.0, 1.0);
        painter.draw(&mut long, "Finalizando agora", 5.0, 20.0, 1.0);
        painter.draw(&mut faded, "Finalizando agora", 5.0, 20.0, 0.0);
        assert!(ink(&long) > ink(&short));
        assert_eq!(ink(&faded), 0);
    }
}
