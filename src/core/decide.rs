//! The state function: `decide(state, event) -> (state, commands)`.

use super::{Command, Config, Event};

/// The state of the indicator. Fields are private to the `core`; the rules task
/// decides what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    #[allow(dead_code)]
    config: Config,
}

impl State {
    /// The state at start-up, with the given configuration.
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

/// Apply an event to the state and return the new state and the commands to run.
///
/// The rules task (RN-NOVA-1 to 7, 10, 11, 13, 18, 19) implements this; until then
/// it changes nothing and asks for nothing, so the wiring can be built against the
/// final signature.
pub fn decide(state: State, event: Event) -> (State, Vec<Command>) {
    let _ = event;
    (state, Vec::new())
}
