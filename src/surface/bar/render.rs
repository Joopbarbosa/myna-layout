//! Drawing the bar. The drawing sits behind [`Renderer`] so it can be replaced
//! without touching the surface code (DT-NOVA-2); the bar drawing task adds the
//! animated renderer.

use crate::core::{BarKind, BarLook};

/// What the renderer is asked to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct BarView {
    /// What to show.
    pub look: BarLook,
    /// The voice level from 0 to 1, meaningful while recording.
    pub level: f32,
    /// Overall opacity from 0 to 1.
    pub opacity: f32,
    /// Time since the bar was shown, for the animation.
    pub elapsed: std::time::Duration,
}

/// Draws one frame into an ARGB (premultiplied, little endian) canvas.
pub trait Renderer: Send {
    /// Fill `canvas`, which holds `width * height` four-byte pixels.
    fn render(&mut self, canvas: &mut [u8], width: u32, height: u32, view: &BarView);
}

/// A flat fill with one colour per kind. It proves the surface works and is what
/// the bar shows until the animated renderer exists.
#[derive(Debug, Default)]
pub struct PlainRenderer;

impl PlainRenderer {
    /// The colour of a kind as `(red, green, blue)`.
    pub fn colour(kind: BarKind) -> (u8, u8, u8) {
        match kind {
            BarKind::Loading | BarKind::Neutral => (0x4A, 0x90, 0xE2),
            BarKind::Recording => (0x8E, 0x5B, 0xE8),
            BarKind::Finishing => (0x5B, 0xA8, 0xC9),
            BarKind::Notice => (0xF5, 0xA6, 0x23),
            BarKind::Error => (0xD0, 0x2B, 0x2B),
        }
    }

    /// The premultiplied pixel, in memory order (blue, green, red, alpha).
    pub fn pixel(kind: BarKind, opacity: f32) -> [u8; 4] {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        let (red, green, blue) = Self::colour(kind);
        let premultiply = |channel: u8| ((u32::from(channel) * alpha + 127) / 255) as u8;
        [
            premultiply(blue),
            premultiply(green),
            premultiply(red),
            alpha as u8,
        ]
    }
}

impl Renderer for PlainRenderer {
    fn render(&mut self, canvas: &mut [u8], _width: u32, _height: u32, view: &BarView) {
        let pixel = Self::pixel(view.look.kind, view.opacity);
        let (pixels, _) = canvas.as_chunks_mut::<4>();
        pixels.fill(pixel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Message;

    fn view(kind: BarKind, opacity: f32) -> BarView {
        BarView {
            look: BarLook {
                kind,
                message: Some(Message::Loading),
            },
            level: 0.0,
            opacity,
            elapsed: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn the_plain_renderer_fills_every_pixel_with_the_kind_colour() {
        let mut canvas = vec![0u8; 4 * 4 * 2];
        PlainRenderer.render(&mut canvas, 4, 2, &view(BarKind::Error, 1.0));
        let expected = PlainRenderer::pixel(BarKind::Error, 1.0);
        let (pixels, _) = canvas.as_chunks::<4>();
        assert!(pixels.iter().all(|pixel| *pixel == expected));
        assert_eq!(expected, [0x2B, 0x2B, 0xD0, 0xFF]);
    }

    #[test]
    fn opacity_scales_alpha_and_premultiplies_the_colour() {
        let half = PlainRenderer::pixel(BarKind::Notice, 0.5);
        assert_eq!(half[3], 128);
        assert!(half[2] < 0xF5 && half[2] > 0);
        assert_eq!(PlainRenderer::pixel(BarKind::Notice, 0.0), [0, 0, 0, 0]);
        assert_eq!(PlainRenderer::pixel(BarKind::Notice, 7.0)[3], 255);
    }

    #[test]
    fn every_kind_has_a_distinct_colour_except_the_neutral_loading_look() {
        use std::collections::HashSet;
        let kinds = [
            BarKind::Loading,
            BarKind::Recording,
            BarKind::Finishing,
            BarKind::Notice,
            BarKind::Error,
        ];
        let colours: HashSet<_> = kinds
            .iter()
            .map(|kind| PlainRenderer::colour(*kind))
            .collect();
        assert_eq!(colours.len(), kinds.len());
        assert_eq!(
            PlainRenderer::colour(BarKind::Neutral),
            PlainRenderer::colour(BarKind::Loading)
        );
    }
}
