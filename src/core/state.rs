//! What the indicator reads from Myna.

/// The state Myna publishes, plus [`MynaState::Unknown`] for a state this version
/// does not know.
///
/// An unknown state is shown as "neutral active" (the loading look, without text)
/// and logged, and must never stop the indicator (RN-NOVA-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MynaState {
    /// Nothing is happening; nothing is drawn.
    Idle,
    /// The model is loading.
    Loading,
    /// Myna is listening.
    Recording,
    /// The speech is being decoded.
    Transcribing,
    /// Recording stopped; the text is about to be typed.
    Finalizing,
    /// A recoverable, non-blocking problem (for example, no speech captured).
    Notice,
    /// A critical failure.
    Error,
    /// A state published by Myna that this version does not know, as published.
    Unknown(String),
}

/// One reading of Myna's published properties.
#[derive(Debug, Clone, PartialEq)]
pub struct MynaReading {
    /// The state.
    pub state: MynaState,
    /// The text of the notice or the reason of the error, as Myna published it.
    pub text: Option<String>,
    /// The audio level as Myna published it. The `core` only uses it while the state
    /// is [`MynaState::Recording`] and clamps it to 0 to 1 (RN-NOVA-2).
    pub level: f32,
}
