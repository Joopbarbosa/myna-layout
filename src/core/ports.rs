//! The interfaces the other modules implement.
//!
//! Each implementation is given the sending side of the event channel when it is
//! built, and reports results as [`super::Event`] values: `start` answers with
//! `SurfaceReady` or `SurfaceFailed`, and later failures arrive as `SurfaceFailed`,
//! never as a return value (DT-NOVA-2).

use std::time::Duration;

use super::{BarLook, Message, OneTimeNotice, Urgency};

/// The system notification surface.
pub trait Notifier: Send {
    /// Check that the notification service is available, without showing anything.
    fn start(&mut self);
    /// Show the single notification of the indicator. The implementation decides
    /// whether to replace the previous one in place or to close it and open a new
    /// one (RN-NOVA-9); the caller only asks for "show this".
    fn show(&mut self, message: &Message, urgency: Urgency);
    /// Show a one-time warning, apart from the single notification.
    fn notice_once(&mut self, notice: OneTimeNotice);
    /// Close the notification of the indicator, if any.
    fn close(&mut self);
    /// Block until every pending request has been carried out, for at most `timeout`
    /// (used at shutdown so the last close is not lost).
    fn flush(&mut self, timeout: Duration);
}

/// The floating bar.
pub trait Bar: Send {
    /// Create the surface. It may fail; the result arrives as an event.
    fn start(&mut self);
    /// Show the bar with this look.
    fn show(&mut self, look: &BarLook);
    /// Update the voice level, from 0 to 1.
    fn update_level(&mut self, level: f32);
    /// Hide the bar.
    fn hide(&mut self);
}

/// The client of Myna's D-Bus interface. Observed readings and presence changes
/// arrive as events; only registration is a call.
pub trait MynaClient: Send {
    /// Register the indicator in Myna.
    fn register(&mut self);
    /// Unregister the indicator from Myna.
    fn unregister(&mut self);
    /// The version of Myna being observed, for the log.
    fn version(&self) -> Option<String>;
    /// Block until every pending request has been carried out, for at most `timeout`
    /// (used at shutdown so the unregistration is not lost).
    fn flush(&mut self, timeout: Duration);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interfaces_are_object_safe() {
        fn accepts(_: Option<&dyn Notifier>, _: Option<&dyn Bar>, _: Option<&dyn MynaClient>) {}
        accepts(None, None, None);
    }
}
