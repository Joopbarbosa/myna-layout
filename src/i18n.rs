//! User-facing text in Brazilian Portuguese and English (DG-005).

use crate::core::{Message, OneTimeNotice};

/// A supported user interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Brazilian Portuguese.
    PtBr,
    /// English.
    En,
}

impl Language {
    /// Select a language from a locale identifier.
    pub fn from_locale(locale: &str) -> Self {
        if locale.to_ascii_lowercase().starts_with("pt") {
            Self::PtBr
        } else {
            Self::En
        }
    }

    /// Select a language from the standard locale environment variables.
    pub fn from_env() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|locale| !locale.is_empty())
            .map_or(Self::En, |locale| Self::from_locale(&locale))
    }
}

/// Render a domain message for the requested language.
pub fn text(message: &Message, language: Language) -> String {
    match message {
        Message::Loading => fixed(language, "Carregando", "Loading"),
        Message::Listening => fixed(language, "Ouvindo", "Listening"),
        Message::Finishing => fixed(language, "Finalizando", "Finishing"),
        Message::Notice(reason) => reason_text(reason, language, false),
        Message::Error(reason) => reason_text(reason, language, true),
        Message::MynaUnavailable => fixed(language, "Myna indisponível", "Myna unavailable"),
        Message::MynaIncompatible => fixed(language, "Myna incompatível", "Myna incompatible"),
    }
}

/// Render a one-time Myna availability notice.
pub fn one_time_text(notice: OneTimeNotice, language: Language) -> String {
    match notice {
        OneTimeNotice::MynaUnavailable => fixed(language, "Myna indisponível", "Myna unavailable"),
        OneTimeNotice::MynaIncompatible => {
            fixed(language, "Myna incompatível", "Myna incompatible")
        }
    }
}

/// Return the application name shown to the user.
pub fn app_name() -> &'static str {
    "Myna Layout"
}

fn fixed(language: Language, pt_br: &str, en: &str) -> String {
    match language {
        Language::PtBr => pt_br.to_owned(),
        Language::En => en.to_owned(),
    }
}

fn reason_text(reason: &str, language: Language, is_error: bool) -> String {
    let candidate = reason.strip_prefix("Error: ").unwrap_or(reason).trim();
    if candidate.is_empty() {
        return match (language, is_error) {
            (Language::PtBr, true) => "Erro no ditado".to_owned(),
            (Language::En, true) => "Dictation error".to_owned(),
            (Language::PtBr, false) => "Aviso do ditado".to_owned(),
            (Language::En, false) => "Dictation notice".to_owned(),
        };
    }

    const REASONS: [(&str, &str); 8] = [
        ("No text field focused", "Nenhum campo de texto em foco"),
        ("Focus lost", "O foco saiu do campo"),
        ("Password field skipped", "Campo de senha ignorado"),
        ("Text field closed", "Campo de texto fechado"),
        ("Typing failed", "Falha ao digitar"),
        ("Typing unavailable", "Digitação indisponível"),
        ("Background noise is high", "Ruído de fundo alto"),
        ("No speech detected", "Nenhuma fala detectada"),
    ];

    if let Some((english, portuguese)) = REASONS
        .iter()
        .find(|(english, _)| candidate.eq_ignore_ascii_case(english))
    {
        return match language {
            Language::PtBr => (*portuguese).to_owned(),
            Language::En => (*english).to_owned(),
        };
    }

    reason.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{Language, app_name, one_time_text, text};
    use crate::core::{Message, OneTimeNotice};

    #[test]
    fn fixed_messages_are_translated() {
        let cases = [
            (Message::Loading, "Carregando", "Loading"),
            (Message::Listening, "Ouvindo", "Listening"),
            (Message::Finishing, "Finalizando", "Finishing"),
            (
                Message::MynaUnavailable,
                "Myna indisponível",
                "Myna unavailable",
            ),
            (
                Message::MynaIncompatible,
                "Myna incompatível",
                "Myna incompatible",
            ),
        ];
        for (message, pt, en) in cases {
            assert_eq!(text(&message, Language::PtBr), pt);
            assert_eq!(text(&message, Language::En), en);
        }
    }

    #[test]
    fn known_reasons_are_translated_to_brazilian_portuguese() {
        let cases = [
            ("No text field focused", "Nenhum campo de texto em foco"),
            ("Focus lost", "O foco saiu do campo"),
            ("Password field skipped", "Campo de senha ignorado"),
            ("Text field closed", "Campo de texto fechado"),
            ("Typing failed", "Falha ao digitar"),
            ("Typing unavailable", "Digitação indisponível"),
            ("Background noise is high", "Ruído de fundo alto"),
            ("No speech detected", "Nenhuma fala detectada"),
        ];
        for (reason, expected) in cases {
            assert_eq!(
                text(&Message::Error(reason.into()), Language::PtBr),
                expected
            );
            assert_eq!(
                text(&Message::Notice(reason.into()), Language::PtBr),
                expected
            );
        }
    }

    #[test]
    fn known_reason_is_returned_in_english() {
        assert_eq!(
            text(
                &Message::Error("No text field focused".into()),
                Language::En
            ),
            "No text field focused"
        );
    }

    #[test]
    fn unknown_reasons_are_returned_exactly_as_received() {
        for language in [Language::PtBr, Language::En] {
            assert_eq!(
                text(&Message::Error("Disk quota exceeded".into()), language),
                "Disk quota exceeded"
            );
            assert_eq!(
                text(&Message::Notice("Disk quota exceeded".into()), language),
                "Disk quota exceeded"
            );
        }
    }

    #[test]
    fn error_prefix_and_reason_comparison_are_normalized() {
        assert_eq!(
            text(
                &Message::Error("Error: No text field focused".into()),
                Language::PtBr
            ),
            "Nenhum campo de texto em foco"
        );
        assert_eq!(
            text(&Message::Notice("  fOcUs LoSt  ".into()), Language::PtBr),
            "O foco saiu do campo"
        );
    }

    #[test]
    fn empty_reasons_use_generic_messages() {
        assert_eq!(
            text(&Message::Error(String::new()), Language::PtBr),
            "Erro no ditado"
        );
        assert_eq!(
            text(&Message::Error("  ".into()), Language::En),
            "Dictation error"
        );
        assert_eq!(
            text(&Message::Notice(String::new()), Language::PtBr),
            "Aviso do ditado"
        );
        assert_eq!(
            text(&Message::Notice("  ".into()), Language::En),
            "Dictation notice"
        );
    }

    #[test]
    fn one_time_notices_are_translated() {
        for (notice, pt, en) in [
            (
                OneTimeNotice::MynaUnavailable,
                "Myna indisponível",
                "Myna unavailable",
            ),
            (
                OneTimeNotice::MynaIncompatible,
                "Myna incompatível",
                "Myna incompatible",
            ),
        ] {
            assert_eq!(one_time_text(notice, Language::PtBr), pt);
            assert_eq!(one_time_text(notice, Language::En), en);
        }
    }

    #[test]
    fn locales_select_expected_language() {
        for locale in ["pt_BR.UTF-8", "pt"] {
            assert_eq!(Language::from_locale(locale), Language::PtBr);
        }
        for locale in ["en_US.UTF-8", "C", "", "de_DE"] {
            assert_eq!(Language::from_locale(locale), Language::En);
        }
    }

    #[test]
    fn app_name_is_stable() {
        assert_eq!(app_name(), "Myna Layout");
    }
}
