//! The only access to Myna's D-Bus interface (DG-003).
//!
//! The Myna client task implements [`crate::core::MynaClient`] here and turns
//! D-Bus messages into [`crate::core::Event`] values. No D-Bus type may leave this
//! module.
