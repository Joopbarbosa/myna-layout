//! Colours, durations, curves and default sizes used by the drawing code.
//!
//! No colour, time or size may be hard-coded in the drawing code outside this
//! module (DG-004). The values here are the first proposal for the wave animation
//! and are meant to be tuned by looking at the bar.

use std::time::Duration;

/// An opaque colour as `(red, green, blue)`.
pub type Rgb = (u8, u8, u8);

/// The three ribbon colours of the wave (blue, violet, pink), drawn on top of each other.
pub const RIBBONS: [Rgb; 3] = [(0x3D, 0x8B, 0xFF), (0x8E, 0x5B, 0xFF), (0xFF, 0x5B, 0xB8)];
/// The ribbon colours while a notice is shown (amber).
pub const NOTICE_RIBBONS: [Rgb; 3] = [(0xFF, 0xB1, 0x2B), (0xFF, 0x8C, 0x1A), (0xFF, 0xD3, 0x6B)];
/// The ribbon colours while an error is shown (red).
pub const ERROR_RIBBONS: [Rgb; 3] = [(0xFF, 0x4D, 0x4D), (0xE0, 0x2A, 0x2A), (0xFF, 0x7A, 0x7A)];

/// How many times the wave crosses the width of the bar, per ribbon.
pub const RIBBON_CYCLES: [f32; 3] = [1.6, 2.3, 1.1];
/// How fast each ribbon travels, in turns per second.
pub const RIBBON_SPEED: [f32; 3] = [0.35, -0.5, 0.22];

/// The strokes of one ribbon, from the widest and faintest (the glow) to the
/// sharpest: `(width as a fraction of the bar height, alpha from 0 to 1)`.
pub const GLOW_STROKES: [(f32, f32); 3] = [(0.30, 0.10), (0.14, 0.28), (0.05, 0.95)];

/// Amplitude of the wave as a fraction of half the bar height, at rest.
pub const REST_AMPLITUDE: f32 = 0.04;
/// Amplitude of the soft pulse while loading.
pub const LOADING_AMPLITUDE: f32 = 0.16;
/// The largest amplitude, reached at full voice level.
pub const MAX_AMPLITUDE: f32 = 0.95;
/// Period of the loading pulse.
pub const LOADING_PULSE: Duration = Duration::from_millis(1600);
/// Period of the brief notice pulse.
pub const NOTICE_PULSE: Duration = Duration::from_millis(900);
/// How quickly the level meter rises (time to cover most of a jump up).
pub const METER_ATTACK: Duration = Duration::from_millis(60);
/// How slowly the level meter falls (time to cover most of a drop).
pub const METER_RELEASE: Duration = Duration::from_millis(420);
/// How quickly the wave shrinks to a thin line when finishing.
pub const SHRINK: Duration = Duration::from_millis(350);
/// Exponent of the meter curve: below 1 lifts low levels (voice sits at 0.01 to 0.15).
pub const METER_CURVE: f32 = 0.5;
/// The level at which the curve reaches full amplitude.
pub const METER_FULL_LEVEL: f32 = 0.30;

/// Height of the text row under the wave, in pixels, when the bar has a message.
pub const TEXT_ROW: u32 = 20;
/// Size of the message text, in pixels.
pub const TEXT_SIZE: f32 = 14.0;
/// Colour of the message text.
pub const TEXT_COLOUR: Rgb = (0xF2, 0xF4, 0xFA);
/// Opacity of the message text, from 0 to 1.
pub const TEXT_ALPHA: f32 = 0.92;
/// Files tried for the text when the font manager cannot name one.
pub const FALLBACK_FONTS: [&str; 3] = [
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];
