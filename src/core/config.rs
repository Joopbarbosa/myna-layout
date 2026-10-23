//! The configuration, with the defaults of RN-NOVA-22.

/// Which surfaces the indicator uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Bar and notification (the default).
    #[default]
    Both,
    /// Only the bar.
    BarOnly,
    /// Only the notification.
    NotificationOnly,
}

/// Horizontal placement of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HorizontalAnchor {
    /// Centred (the default).
    #[default]
    Center,
    /// Left edge.
    Left,
    /// Right edge.
    Right,
}

/// Vertical placement of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAnchor {
    /// Bottom (the default).
    #[default]
    Bottom,
    /// Middle of the screen.
    Center,
    /// Top edge.
    Top,
}

/// Where the bar sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Horizontal anchor.
    pub horizontal: HorizontalAnchor,
    /// Vertical anchor.
    pub vertical: VerticalAnchor,
    /// Distance from the anchored edge or the dock, in pixels.
    pub margin: u32,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            horizontal: HorizontalAnchor::default(),
            vertical: VerticalAnchor::default(),
            margin: 28,
        }
    }
}

/// The size of the bar, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Default for Size {
    fn default() -> Self {
        Self {
            width: 320,
            height: 80,
        }
    }
}

/// The complete configuration. Every field has a default, so a missing or invalid
/// file never stops the indicator (RN-NOVA-22).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// Which surfaces to use.
    pub mode: Mode,
    /// Where the bar sits.
    pub position: Position,
    /// The size of the bar.
    pub size: Size,
    /// Scale applied to the level meter (1.0 means no scaling).
    pub meter_scale: f32,
    /// Opacity of the bar, from 0 to 1.
    pub opacity: f32,
    /// Replace the moving waves by a glow that only changes intensity.
    pub reduced_motion: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            position: Position::default(),
            size: Size::default(),
            meter_scale: 1.0,
            opacity: 0.9,
            reduced_motion: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_configuration_rule() {
        let config = Config::default();
        assert_eq!(config.mode, Mode::Both);
        assert_eq!(config.position.horizontal, HorizontalAnchor::Center);
        assert_eq!(config.position.vertical, VerticalAnchor::Bottom);
        assert!((24..=32).contains(&config.position.margin));
        assert_eq!(
            config.size,
            Size {
                width: 320,
                height: 80
            }
        );
        assert_eq!(config.opacity, 0.9);
        assert_eq!(config.meter_scale, 1.0);
        assert!(!config.reduced_motion);
    }
}
