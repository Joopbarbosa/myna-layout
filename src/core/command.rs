//! Everything the indicator can ask the other modules to do.

use super::{Message, Surface};

/// How loud a notification must be. Myna itself uses these two levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// An ordinary state.
    Normal,
    /// An error.
    Critical,
}

/// A warning shown only once per period of unavailability (RN-NOVA-3, RN-NOVA-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneTimeNotice {
    /// Myna is not there.
    MynaUnavailable,
    /// Myna's interface changed.
    MynaIncompatible,
}

/// The look of the bar, which the rendering code maps to colours and motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    /// Soft pulsing glow, almost still wave.
    Loading,
    /// Waves that follow the voice level.
    Recording,
    /// Waves shrinking to a thin line.
    Finishing,
    /// Brief amber glow.
    Notice,
    /// Red glow.
    Error,
    /// The loading look without text, for an unknown Myna state.
    Neutral,
}

/// What the bar must show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarLook {
    /// The visual treatment.
    pub kind: BarKind,
    /// The short text next to the wave, if any.
    pub message: Option<Message>,
}

/// An output of the state function.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Start a surface. It answers with `Event::SurfaceReady` or `Event::SurfaceFailed`.
    StartSurface(Surface),
    /// Show the bar with this look.
    ShowBar(BarLook),
    /// Update the voice level on the bar, from 0 to 1.
    UpdateLevel(f32),
    /// Hide the bar.
    HideBar,
    /// Show the single notification of the indicator, replacing the previous one.
    ShowNotification {
        /// What to say.
        message: Message,
        /// How loud.
        urgency: Urgency,
    },
    /// Close the notification of the indicator, if any.
    CloseNotification,
    /// Warn once (the notifier shows it on its own, apart from the single notification).
    NoticeOnce(OneTimeNotice),
    /// Register the indicator in Myna, which then hides its own toast.
    Register,
    /// Unregister the indicator from Myna.
    Unregister,
}
