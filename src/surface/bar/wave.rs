//! The animated wave: translucent glowing ribbons that follow the voice, in the
//! spirit of a phone assistant (RN-NOVA-14, RN-NOVA-15, RN-NOVA-17).

use std::time::Duration;

use tiny_skia::{BlendMode, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

use super::{
    render::{BarView, Renderer},
    text::TextPainter,
};
use crate::{
    core::BarKind,
    i18n::{self, Language},
    theme::{self, Rgb},
};

/// Map the voice level to a wave amplitude from 0 to 1: a curve that lifts the low
/// levels so ordinary speech (0.01 to 0.15) moves the wave visibly (RN-NOVA-15).
pub fn meter(level: f32, scale: f32) -> f32 {
    let scaled = (level.max(0.0) * scale.max(0.0) / theme::METER_FULL_LEVEL).clamp(0.0, 1.0);
    scaled.powf(theme::METER_CURVE)
}

/// Move `current` towards `target`: quickly when it rises, slowly when it falls, so
/// the wave follows the voice without trembling (RN-NOVA-15).
pub fn smooth(current: f32, target: f32, step: Duration) -> f32 {
    let time_constant = if target > current {
        theme::METER_ATTACK
    } else {
        theme::METER_RELEASE
    }
    .as_secs_f32();
    let blend = 1.0 - (-step.as_secs_f32() / time_constant).exp();
    current + (target - current) * blend
}

/// Draws the wave with software rendering into the shared-memory canvas.
#[derive(Debug)]
pub struct WaveRenderer {
    reduced_motion: bool,
    language: Language,
    text: TextPainter,
    meter: f32,
    shrink: f32,
    last: Option<Duration>,
}

impl WaveRenderer {
    /// A renderer; `reduced_motion` replaces the moving waves by a glow that only
    /// changes intensity (RN-NOVA-17).
    pub fn new(reduced_motion: bool, language: Language) -> Self {
        Self {
            reduced_motion,
            language,
            text: TextPainter::new(),
            meter: 0.0,
            shrink: 0.0,
            last: None,
        }
    }

    /// Time since the previous frame, so the smoothing does not depend on the frame rate.
    fn step(&mut self, elapsed: Duration) -> Duration {
        let step = elapsed.saturating_sub(self.last.unwrap_or(elapsed));
        self.last = Some(elapsed);
        step.min(Duration::from_millis(100))
    }

    /// The wave amplitude (0 to 1) and the intensity (0 to 1) for this frame.
    fn shape(&mut self, view: &BarView, step: Duration, seconds: f32) -> (f32, f32) {
        let target_shrink = if view.look.kind == BarKind::Finishing {
            1.0
        } else {
            0.0
        };
        self.shrink = smooth_linear(self.shrink, target_shrink, step, theme::SHRINK);
        match view.look.kind {
            BarKind::Recording => {
                self.meter = smooth(self.meter, meter(view.level, 1.0), step);
                let amplitude = theme::REST_AMPLITUDE + self.meter * theme::MAX_AMPLITUDE;
                (amplitude, 0.55 + 0.45 * self.meter)
            }
            BarKind::Loading | BarKind::Neutral => {
                self.meter = 0.0;
                let pulse = pulse(seconds, theme::LOADING_PULSE);
                (
                    theme::REST_AMPLITUDE + theme::LOADING_AMPLITUDE * pulse,
                    0.35 + 0.5 * pulse,
                )
            }
            BarKind::Finishing => {
                let amplitude = self.meter.max(0.0) * (1.0 - self.shrink) + theme::REST_AMPLITUDE;
                self.meter = smooth(self.meter, 0.0, step);
                (amplitude, 0.5)
            }
            BarKind::Notice => {
                self.meter = 0.0;
                (
                    theme::REST_AMPLITUDE + 0.28,
                    0.6 + 0.4 * pulse(seconds, theme::NOTICE_PULSE),
                )
            }
            BarKind::Error => {
                self.meter = 0.0;
                (
                    theme::REST_AMPLITUDE + 0.35,
                    0.7 + 0.3 * pulse(seconds, theme::NOTICE_PULSE),
                )
            }
        }
    }
}

/// A smooth 0 to 1 pulse with the given period.
fn pulse(seconds: f32, period: Duration) -> f32 {
    0.5 - 0.5 * (seconds / period.as_secs_f32() * std::f32::consts::TAU).cos()
}

fn smooth_linear(current: f32, target: f32, step: Duration, time: Duration) -> f32 {
    let blend = 1.0 - (-step.as_secs_f32() / time.as_secs_f32()).exp();
    current + (target - current) * blend
}

fn ribbons(kind: BarKind) -> [Rgb; 3] {
    match kind {
        BarKind::Notice => theme::NOTICE_RIBBONS,
        BarKind::Error => theme::ERROR_RIBBONS,
        _ => theme::RIBBONS,
    }
}

impl Renderer for WaveRenderer {
    fn render(&mut self, canvas: &mut [u8], width: u32, height: u32, view: &BarView) {
        let step = self.step(view.elapsed);
        let seconds = if self.reduced_motion {
            0.0
        } else {
            view.elapsed.as_secs_f32()
        };
        let (amplitude, intensity) = self.shape(view, step, view.elapsed.as_secs_f32());
        let Some(mut pixmap) = Pixmap::new(width, height) else {
            return;
        };
        let text = view
            .look
            .message
            .as_ref()
            .map(|message| i18n::text(message, self.language));
        let has_text = text.is_some() && self.text.has_font();
        let text_row = if has_text {
            theme::TEXT_ROW as f32
        } else {
            0.0
        };
        let (w, h) = (width as f32, height as f32 - text_row);
        let middle = h / 2.0;
        let colours = ribbons(view.look.kind);

        for (index, colour) in colours.iter().enumerate() {
            let cycles = theme::RIBBON_CYCLES[index];
            let phase = seconds * theme::RIBBON_SPEED[index] * std::f32::consts::TAU;
            let mut builder = PathBuilder::new();
            let samples = (width / 2).max(2);
            for sample in 0..=samples {
                let x = w * sample as f32 / samples as f32;
                let along = sample as f32 / samples as f32;
                // The wave fades to a flat line at both ends of the bar.
                let envelope = (std::f32::consts::PI * along).sin().powi(2);
                let wave = if self.reduced_motion { 0.0 } else { 1.0 };
                let offset = (cycles * std::f32::consts::TAU * along + phase).sin()
                    * amplitude.min(1.0)
                    * (middle * 0.9)
                    * envelope
                    * wave;
                let y = middle + offset;
                if sample == 0 {
                    builder.move_to(x, y);
                } else {
                    builder.line_to(x, y);
                }
            }
            let Some(path) = builder.finish() else {
                continue;
            };
            for (stroke_width, alpha) in theme::GLOW_STROKES {
                let mut paint = Paint::default();
                paint.set_color_rgba8(
                    colour.0,
                    colour.1,
                    colour.2,
                    (alpha * intensity * 255.0) as u8,
                );
                paint.blend_mode = BlendMode::Plus;
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: (stroke_width * h).max(1.0),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }

        if let (true, Some(text)) = (has_text, &text) {
            self.text.draw(&mut pixmap, text, h, text_row, 1.0);
        }

        // tiny-skia gives premultiplied RGBA; the surface wants premultiplied
        // little-endian ARGB, that is bytes B, G, R, A. Opacity scales every channel.
        let opacity = view.opacity.clamp(0.0, 1.0);
        let (target, _) = canvas.as_chunks_mut::<4>();
        let (source, _) = pixmap.data().as_chunks::<4>();
        for (out, pixel) in target.iter_mut().zip(source) {
            let scale = |channel: u8| (f32::from(channel) * opacity).round() as u8;
            *out = [
                scale(pixel[2]),
                scale(pixel[1]),
                scale(pixel[0]),
                scale(pixel[3]),
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{
        core::{BarLook, Message},
        i18n::Language,
    };

    fn view(kind: BarKind, level: f32, millis: u64) -> BarView {
        BarView {
            look: BarLook {
                kind,
                message: None,
            },
            level,
            opacity: 1.0,
            elapsed: Duration::from_millis(millis),
        }
    }

    fn render(renderer: &mut WaveRenderer, view: &BarView) -> Vec<u8> {
        let mut canvas = vec![0u8; 160 * 40 * 4];
        renderer.render(&mut canvas, 160, 40, view);
        canvas
    }

    fn ink(canvas: &[u8]) -> u64 {
        canvas
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| u64::from(pixel[3]))
            .sum()
    }

    #[test]
    fn the_meter_lifts_ordinary_speech_so_it_moves_visibly() {
        let quiet = meter(0.01, 1.0);
        let normal = meter(0.15, 1.0);
        assert!(quiet > 0.15, "even 0.01 must register, got {quiet}");
        assert!(
            normal - quiet > 0.4,
            "0.01 to 0.15 must differ clearly: {quiet} to {normal}"
        );
        assert_eq!(meter(0.0, 1.0), 0.0);
        assert_eq!(meter(5.0, 1.0), 1.0);
        assert!(
            meter(0.05, 2.0) > meter(0.05, 1.0),
            "the scale amplifies the meter"
        );
    }

    #[test]
    fn the_meter_rises_fast_and_falls_slowly() {
        let step = Duration::from_millis(16);
        let up = smooth(0.0, 1.0, step) - 0.0;
        let down = 1.0 - smooth(1.0, 0.0, step);
        assert!(
            up > down * 3.0,
            "attack {up} must be much faster than release {down}"
        );
        let mut level = 1.0;
        for _ in 0..200 {
            level = smooth(level, 0.0, step);
        }
        assert!(
            level < 0.01,
            "the meter must reach the quiet level, got {level}"
        );
    }

    #[test]
    fn recording_draws_more_ink_with_a_louder_voice() {
        let mut quiet = WaveRenderer::new(false, Language::En);
        let mut loud = WaveRenderer::new(false, Language::En);
        let mut quiet_ink = 0;
        let mut loud_ink = 0;
        for frame in 0..30 {
            let time = frame * 16;
            quiet_ink = ink(&render(&mut quiet, &view(BarKind::Recording, 0.01, time)));
            loud_ink = ink(&render(&mut loud, &view(BarKind::Recording, 0.25, time)));
        }
        assert!(
            loud_ink > quiet_ink,
            "loud {loud_ink} must exceed quiet {quiet_ink}"
        );
        assert!(quiet_ink > 0);
    }

    #[test]
    fn finishing_shrinks_the_wave_to_a_thin_line() {
        let mut renderer = WaveRenderer::new(false, Language::En);
        let mut recording = 0;
        for frame in 0..40 {
            recording = ink(&render(
                &mut renderer,
                &view(BarKind::Recording, 0.25, frame * 16),
            ));
        }
        let mut finishing = recording;
        for frame in 40..120 {
            finishing = ink(&render(
                &mut renderer,
                &view(BarKind::Finishing, 0.0, frame * 16),
            ));
        }
        assert!(
            finishing < recording / 2,
            "finishing {finishing} must shrink from {recording}"
        );
    }

    #[test]
    fn reduced_motion_only_changes_intensity_and_never_moves() {
        let mut renderer = WaveRenderer::new(true, Language::En);
        let first = render(&mut renderer, &view(BarKind::Recording, 0.2, 0));
        let mut later = Vec::new();
        for frame in 1..40 {
            later = render(&mut renderer, &view(BarKind::Recording, 0.2, frame * 250));
        }
        // The shape is a flat glow and the level is steady, so the picture settles
        // instead of travelling: two frames far apart in time are identical.
        let next = render(&mut renderer, &view(BarKind::Recording, 0.2, 40 * 250));
        assert_eq!(later, next);
        let mut louder = WaveRenderer::new(true, Language::En);
        for frame in 0..40 {
            render(&mut louder, &view(BarKind::Recording, 0.28, frame * 16));
        }
        let bright = ink(&render(&mut louder, &view(BarKind::Recording, 0.28, 700)));
        assert!(bright > ink(&first), "a louder voice must glow more");
    }

    #[test]
    fn opacity_scales_the_whole_frame() {
        let mut full = WaveRenderer::new(false, Language::En);
        let mut half = WaveRenderer::new(false, Language::En);
        let mut opaque = view(BarKind::Notice, 0.0, 300);
        let full_ink = ink(&render(&mut full, &opaque));
        opaque.opacity = 0.5;
        let half_ink = ink(&render(&mut half, &opaque));
        assert!(
            half_ink * 10 < full_ink * 6 && half_ink * 10 > full_ink * 4,
            "{half_ink} vs {full_ink}"
        );
    }

    #[test]
    fn every_kind_draws_something_and_stays_inside_the_canvas() {
        for kind in [
            BarKind::Loading,
            BarKind::Recording,
            BarKind::Finishing,
            BarKind::Notice,
            BarKind::Error,
            BarKind::Neutral,
        ] {
            let mut renderer = WaveRenderer::new(false, Language::En);
            let canvas = render(&mut renderer, &view(kind, 0.1, 400));
            assert!(ink(&canvas) > 0, "{kind:?} drew nothing");
        }
    }

    #[test]
    fn a_message_adds_text_under_the_wave_and_the_wave_keeps_its_area() {
        let mut plain = WaveRenderer::new(false, Language::En);
        if !plain.text.has_font() {
            eprintln!("skipped: this machine has no sans font");
            return;
        }
        let mut with_text = WaveRenderer::new(false, Language::En);
        let without = view(BarKind::Recording, 0.2, 300);
        let mut with = without.clone();
        with.look.message = Some(Message::Listening);
        let plain_canvas = render(&mut plain, &without);
        let text_canvas = render(&mut with_text, &with);
        assert!(
            ink(&text_canvas) != ink(&plain_canvas),
            "the text must change the picture"
        );
        // The text sits in the last rows of the canvas, which stay empty without a message.
        let row = 160 * 4;
        let bottom_rows = |canvas: &[u8]| ink(&canvas[canvas.len() - 14 * row..]);
        assert!(bottom_rows(&text_canvas) > bottom_rows(&plain_canvas));
    }
}
