//! The rules of the indicator, as a pure state function.
//!
//! The `core` receives [`Event`] values and returns [`Command`] values, both
//! immutable and defined here. It knows nothing about D-Bus or Wayland (DG-001,
//! DG-003): the other modules translate between the outside world and these types
//! and talk to each other only through messages (DG-009).
//!
//! The contract is deliberately small:
//!
//! - [`Event`]: everything that can happen to the indicator.
//! - [`Command`]: everything the indicator can ask the other modules to do.
//! - [`State`] and [`decide`]: the state function. The rules task implements it.
//! - [`Notifier`], [`Bar`] and [`MynaClient`]: the interfaces the other modules
//!   implement. They report results back as [`Event`] values, never as return
//!   values.

mod command;
mod config;
mod decide;
mod event;
mod message;
mod ports;
mod state;

pub use command::{BarKind, BarLook, Command, OneTimeNotice, Urgency};
pub use config::{Config, HorizontalAnchor, Mode, Position, Size, VerticalAnchor};
pub use decide::{State, decide};
pub use event::{Event, EventSink, Surface};
pub use message::Message;
pub use ports::{Bar, MynaClient, Notifier};
pub use state::{MynaReading, MynaState};
