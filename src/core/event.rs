//! Everything that can happen to the indicator.

use super::{Config, MynaReading};

/// A feedback surface of the indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The floating bar.
    Bar,
    /// The system notification.
    Notification,
}

/// Where a module reports [`Event`] values to: the composition root turns it into a
/// message to the event loop.
pub type EventSink = Box<dyn Fn(Event) + Send>;

/// An input of the state function.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Myna published a new reading.
    Myna(MynaReading),
    /// Myna appeared on the session bus (or was there when the indicator started).
    MynaPresent,
    /// Myna disappeared from the session bus.
    MynaAbsent,
    /// Myna is there but does not offer what the indicator expects (RN-NOVA-7).
    MynaIncompatible,
    /// A surface answered [`Command::StartSurface`] and is ready to show feedback.
    SurfaceReady(Surface),
    /// A surface could not start, or stopped working later (RN-NOVA-10).
    SurfaceFailed(Surface),
    /// The configuration was loaded.
    ConfigLoaded(Config),
    /// The session or the package asked the indicator to stop.
    Shutdown,
}
