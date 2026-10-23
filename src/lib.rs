//! Myna Layout: a status indicator for the Myna dictation on COSMIC/Wayland.
//!
//! The crate is organised by responsibility (see `DECISOES_GLOBAIS.md`, DG-001):
//!
//! - [`core`]: the rules of the indicator as a pure state function. It depends on
//!   no other module and on no D-Bus or Wayland library.
//! - [`myna`]: the only module that talks to Myna's D-Bus interface.
//! - [`surface`]: the two feedback surfaces, the floating bar and the notification.
//! - [`config`]: reads and validates the user configuration file.
//! - [`runtime`]: carries out the commands of the `core` on the real modules.
//! - [`theme`]: colours, durations, curves and sizes used by the drawing code.
//! - [`i18n`]: every text shown to the user.

pub mod config;
pub mod core;
pub mod i18n;
pub mod myna;
pub mod runtime;
pub mod surface;
pub mod theme;
