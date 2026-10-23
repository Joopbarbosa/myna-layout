//! What Myna's D-Bus interface means, as pure functions.
//!
//! The interface is observed, not documented, and Myna is in preview, so everything
//! that interprets it lives here and in the client next to it (DG-003).

use crate::core::{MynaReading, MynaState};

/// Myna's well-known bus name.
pub const BUS_NAME: &str = "com.canonical.Myna.Dictation";
/// Myna's object path.
pub const OBJECT_PATH: &str = "/com/canonical/Myna/Dictation";
/// Myna's interface.
pub const INTERFACE: &str = "com.canonical.Myna.Dictation";

/// The properties the indicator reads, each possibly missing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    /// `State`.
    pub state: Option<String>,
    /// `StatusMessage`.
    pub status_message: Option<String>,
    /// `AudioRms`.
    pub audio_rms: Option<f64>,
    /// `LastError`.
    pub last_error: Option<String>,
}

/// Map the published state name to a [`MynaState`]; an unknown name is kept as it was
/// published.
pub fn map_state(name: &str) -> MynaState {
    match name {
        "idle" => MynaState::Idle,
        "loading" => MynaState::Loading,
        "recording" => MynaState::Recording,
        "transcribing" => MynaState::Transcribing,
        "finalizing" => MynaState::Finalizing,
        "notice" => MynaState::Notice,
        "error" => MynaState::Error,
        other => MynaState::Unknown(other.to_owned()),
    }
}

/// Check that the interface offers what the indicator needs: `State`,
/// `StatusMessage` and `AudioRms`. Returns what is missing otherwise (RN-NOVA-7).
pub fn check_compatible(properties: &Properties) -> Result<(), String> {
    let mut missing = Vec::new();
    if properties.state.is_none() {
        missing.push("State");
    }
    if properties.status_message.is_none() {
        missing.push("StatusMessage");
    }
    if properties.audio_rms.is_none() {
        missing.push("AudioRms");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "missing or unreadable properties: {}",
            missing.join(", ")
        ))
    }
}

/// The longest text the indicator shows from Myna, in characters.
pub const MAX_TEXT_CHARS: usize = 160;

/// Make a text published by Myna safe to show: control characters become spaces,
/// runs of whitespace collapse, the ends are trimmed and the text is cut at
/// [`MAX_TEXT_CHARS`] characters with an ellipsis (always on a character boundary).
pub fn sanitize(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.chars().count() <= MAX_TEXT_CHARS {
        return cleaned;
    }
    let mut cut: String = cleaned.chars().take(MAX_TEXT_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// Build a reading from the properties. The text is the reason of the error (the
/// last error if Myna published one, else the status message) or the text of the
/// notice (the status message); other states carry no text.
pub fn reading(properties: &Properties) -> MynaReading {
    let state = map_state(properties.state.as_deref().unwrap_or("idle"));
    let status = properties
        .status_message
        .clone()
        .filter(|text| !text.is_empty());
    let last_error = properties
        .last_error
        .clone()
        .filter(|text| !text.is_empty());
    let text = match state {
        MynaState::Error => last_error.or(status),
        MynaState::Notice => status,
        _ => None,
    }
    .map(|text| sanitize(&text))
    .filter(|text| !text.is_empty());
    MynaReading {
        state,
        text,
        level: properties.audio_rms.unwrap_or(0.0) as f32,
    }
}

/// The version of the installed Myna snap, if it can be read. Best effort: Myna's
/// D-Bus interface has no version property.
pub fn installed_version() -> Option<String> {
    let manifest = std::fs::read_to_string("/snap/myna/current/meta/snap.yaml").ok()?;
    manifest
        .lines()
        .find_map(|line| line.strip_prefix("version:"))
        .map(|version| {
            version
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .to_owned()
        })
        .filter(|version| !version.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(state: &str) -> Properties {
        Properties {
            state: Some(state.to_owned()),
            status_message: Some(String::new()),
            audio_rms: Some(0.0),
            last_error: None,
        }
    }

    #[test]
    fn every_published_state_is_mapped_and_unknown_ones_are_kept() {
        assert_eq!(map_state("idle"), MynaState::Idle);
        assert_eq!(map_state("loading"), MynaState::Loading);
        assert_eq!(map_state("recording"), MynaState::Recording);
        assert_eq!(map_state("transcribing"), MynaState::Transcribing);
        assert_eq!(map_state("finalizing"), MynaState::Finalizing);
        assert_eq!(map_state("notice"), MynaState::Notice);
        assert_eq!(map_state("error"), MynaState::Error);
        assert_eq!(map_state("sleeping"), MynaState::Unknown("sleeping".into()));
    }

    #[test]
    fn compatibility_names_what_is_missing() {
        assert!(check_compatible(&full("idle")).is_ok());
        let error = check_compatible(&Properties::default()).unwrap_err();
        assert!(
            error.contains("State")
                && error.contains("StatusMessage")
                && error.contains("AudioRms")
        );
        let mut partial = full("idle");
        partial.audio_rms = None;
        assert_eq!(
            check_compatible(&partial).unwrap_err(),
            "missing or unreadable properties: AudioRms"
        );
    }

    #[test]
    fn an_error_carries_the_last_error_or_else_the_status_message() {
        let mut properties = full("error");
        properties.status_message = Some("Error: No text field focused".into());
        assert_eq!(
            reading(&properties).text.as_deref(),
            Some("Error: No text field focused")
        );
        properties.last_error = Some("No text field focused".into());
        assert_eq!(
            reading(&properties).text.as_deref(),
            Some("No text field focused")
        );
    }

    #[test]
    fn a_notice_carries_the_status_message_and_other_states_no_text() {
        let mut properties = full("notice");
        properties.status_message = Some("No speech detected".into());
        assert_eq!(
            reading(&properties).text.as_deref(),
            Some("No speech detected")
        );
        let mut recording = full("recording");
        recording.status_message = Some("Listening".into());
        recording.audio_rms = Some(0.12);
        let read = reading(&recording);
        assert_eq!(read.text, None);
        assert!((read.level - 0.12).abs() < 1e-6);
    }

    #[test]
    fn hostile_text_is_cleaned_and_cut() {
        assert_eq!(sanitize("a\nb\tc\u{1b}[31mred"), "a b c [31mred");
        assert_eq!(sanitize("   \n\r\t  "), "");
        let huge = "x".repeat(1_000_000);
        let cut = sanitize(&huge);
        assert_eq!(cut.chars().count(), MAX_TEXT_CHARS);
        assert!(cut.ends_with('…'));
        // A cut in the middle of a multi-byte character must not panic or split it.
        let accents = "ã".repeat(MAX_TEXT_CHARS + 5);
        let cut = sanitize(&accents);
        assert_eq!(cut.chars().count(), MAX_TEXT_CHARS);
        assert!(cut.starts_with('ã'));
        assert_eq!(sanitize("short and fine"), "short and fine");
    }

    #[test]
    fn hostile_properties_never_produce_unusable_readings() {
        let mut properties = full("error");
        properties.last_error = Some("\u{7}\u{0}\n".into());
        properties.status_message = Some("x".repeat(500_000));
        let read = reading(&properties);
        // The last error is only control characters, so it cleans to nothing; the
        // reading carries no text and the interface falls back to the generic message.
        assert_eq!(read.text, None);
        properties.last_error = None;
        let read = reading(&properties);
        assert_eq!(
            read.text.as_deref().map(|t| t.chars().count()),
            Some(MAX_TEXT_CHARS)
        );
        // Non-finite levels pass through unchanged: the core treats them as silence.
        properties.audio_rms = Some(f64::NAN);
        assert!(reading(&properties).level.is_nan());
        properties.audio_rms = Some(f64::INFINITY);
        assert!(reading(&properties).level.is_infinite());
    }

    #[test]
    fn a_missing_state_reads_as_idle() {
        assert_eq!(reading(&Properties::default()).state, MynaState::Idle);
    }
}
