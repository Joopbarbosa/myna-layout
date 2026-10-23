//! Composition root: wires the modules together without holding business rules.
//!
//! The event loop, the D-Bus task and the registration order are wired in a later
//! task (event loop and registration); this entry point only starts logging.

fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("myna-layout {} starting", env!("CARGO_PKG_VERSION"));
}
