//! The meaning of a text shown to the user, never the text itself.

/// A user-facing message, as a meaning. The `i18n` module turns it into a string
/// for a language (DG-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// "Loading": the model is loading.
    Loading,
    /// "Listening": Myna is recording.
    Listening,
    /// "Finishing": transcribing or finalizing.
    Finishing,
    /// A recoverable notice, with the text Myna published.
    Notice(String),
    /// An error, with the reason Myna published (the `i18n` module translates the
    /// known ones and keeps the unknown ones as they came).
    Error(String),
    /// Myna is not available (RN-NOVA-3).
    MynaUnavailable,
    /// Myna's interface is not the one the indicator expects (RN-NOVA-7).
    MynaIncompatible,
}
