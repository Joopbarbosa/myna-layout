//! Reads and validates the configuration file in the user profile (RN-NOVA-22).
//!
//! The configuration task fills it in. It returns a complete
//! [`crate::core::Config`] plus warnings for the log, and never fails fatally.
