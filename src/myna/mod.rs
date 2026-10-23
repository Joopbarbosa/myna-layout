//! The only access to Myna's D-Bus interface (DG-003).
//!
//! [`DbusMynaClient`] implements [`crate::core::MynaClient`]: it watches Myna's bus
//! name, reads and watches its properties and turns them into [`crate::core::Event`]
//! values, and registers and unregisters the indicator on request. No D-Bus type
//! leaves this module.

mod client;
pub mod protocol;

pub use client::{DbusMynaClient, MynaError};
