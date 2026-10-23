//! Loads and validates the user's configuration (RN-NOVA-22).

use std::path::{Path, PathBuf};

use crate::core::{Config, HorizontalAnchor, Mode, Position, Size, VerticalAnchor};
use toml::Value;

/// A complete configuration and non-fatal diagnostics for the log.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub config: Config,
    pub warnings: Vec<String>,
}

/// Returns the user's standard configuration file path, if a home location is known.
pub fn default_path() -> Option<PathBuf> {
    default_path_from(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

fn default_path_from(
    xdg_config_home: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(path) = xdg_config_home.filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path).join("myna-layout/config.toml"));
    }
    home.filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(path).join(".config/myna-layout/config.toml"))
}

/// Loads the default file. A missing file is the normal first-run case.
pub fn load() -> Loaded {
    match default_path() {
        Some(path) => load_from_path(&path),
        None => defaults(),
    }
}

/// Loads a file, falling back to defaults when it cannot be read.
pub fn load_from_path(path: &Path) -> Loaded {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => defaults(),
        Err(error) => Loaded {
            config: Config::default(),
            warnings: vec![format!(
                "Could not read configuration file '{}': {error}",
                path.display()
            )],
        },
    }
}

/// Parses TOML and returns defaults plus diagnostics for malformed or invalid values.
pub fn parse(text: &str) -> Loaded {
    let root = match toml::from_str::<toml::Table>(text) {
        Ok(root) => root,
        Err(error) => {
            return Loaded {
                config: Config::default(),
                warnings: vec![format!("Invalid configuration TOML: {error}")],
            };
        }
    };

    let mut loaded = Loaded {
        config: Config::default(),
        warnings: Vec::new(),
    };
    for key in root.keys() {
        if ![
            "mode",
            "position",
            "size",
            "meter_scale",
            "opacity",
            "reduced_motion",
        ]
        .contains(&key.as_str())
        {
            loaded.warnings.push(format!(
                "Unknown configuration key '{key}' with value {}",
                root[key]
            ));
        }
    }

    if let Some(value) = root.get("mode") {
        loaded.config.mode = match value.as_str() {
            Some("both") => Mode::Both,
            Some("bar") => Mode::BarOnly,
            Some("notification") => Mode::NotificationOnly,
            _ => {
                warning(&mut loaded, "mode", value);
                Config::default().mode
            }
        };
    }

    if let Some(value) = root.get("position") {
        if let Some(table) = value.as_table() {
            for key in table.keys() {
                if !["horizontal", "vertical", "margin"].contains(&key.as_str()) {
                    loaded.warnings.push(format!(
                        "Unknown configuration key 'position.{key}' with value {}",
                        table[key]
                    ));
                }
            }
            loaded.config.position = Position {
                horizontal: table.get("horizontal").map_or(
                    loaded.config.position.horizontal,
                    |value| match value.as_str() {
                        Some("center") => HorizontalAnchor::Center,
                        Some("left") => HorizontalAnchor::Left,
                        Some("right") => HorizontalAnchor::Right,
                        _ => {
                            warning(&mut loaded, "position.horizontal", value);
                            Config::default().position.horizontal
                        }
                    },
                ),
                vertical: table
                    .get("vertical")
                    .map_or(loaded.config.position.vertical, |value| {
                        match value.as_str() {
                            Some("bottom") => VerticalAnchor::Bottom,
                            Some("center") => VerticalAnchor::Center,
                            Some("top") => VerticalAnchor::Top,
                            _ => {
                                warning(&mut loaded, "position.vertical", value);
                                Config::default().position.vertical
                            }
                        }
                    }),
                margin: table
                    .get("margin")
                    .map_or(loaded.config.position.margin, |value| {
                        match value.as_integer().filter(|v| (0..=1000).contains(v)) {
                            Some(value) => value as u32,
                            None => {
                                warning(&mut loaded, "position.margin", value);
                                Config::default().position.margin
                            }
                        }
                    }),
            };
        } else {
            warning(&mut loaded, "position", value);
        }
    }

    if let Some(value) = root.get("size") {
        if let Some(table) = value.as_table() {
            for key in table.keys() {
                if !["width", "height"].contains(&key.as_str()) {
                    loaded.warnings.push(format!(
                        "Unknown configuration key 'size.{key}' with value {}",
                        table[key]
                    ));
                }
            }
            let default_size = Config::default().size;
            let width = parse_dimension(
                &mut loaded,
                table.get("width"),
                "size.width",
                default_size.width,
            );
            let height = parse_dimension(
                &mut loaded,
                table.get("height"),
                "size.height",
                default_size.height,
            );
            loaded.config.size = Size { width, height };
        } else {
            warning(&mut loaded, "size", value);
        }
    }

    if let Some(value) = root.get("meter_scale") {
        loaded.config.meter_scale = match value
            .as_float()
            .or_else(|| value.as_integer().map(|v| v as f64))
            .filter(|v| *v > 0.0 && *v <= 10.0)
        {
            Some(value) => value as f32,
            None => {
                warning(&mut loaded, "meter_scale", value);
                Config::default().meter_scale
            }
        };
    }
    if let Some(value) = root.get("opacity") {
        loaded.config.opacity = match value
            .as_float()
            .or_else(|| value.as_integer().map(|v| v as f64))
            .filter(|v| (0.0..=1.0).contains(v))
        {
            Some(value) => value as f32,
            None => {
                warning(&mut loaded, "opacity", value);
                Config::default().opacity
            }
        };
    }
    if let Some(value) = root.get("reduced_motion") {
        loaded.config.reduced_motion = match value.as_bool() {
            Some(value) => value,
            None => {
                warning(&mut loaded, "reduced_motion", value);
                Config::default().reduced_motion
            }
        };
    }

    loaded
}

fn defaults() -> Loaded {
    Loaded {
        config: Config::default(),
        warnings: Vec::new(),
    }
}

fn warning(loaded: &mut Loaded, key: &str, value: &Value) {
    loaded
        .warnings
        .push(format!("Invalid configuration value for '{key}': {value}"));
}

fn parse_dimension(loaded: &mut Loaded, value: Option<&Value>, key: &str, default: u32) -> u32 {
    value.map_or(default, |value| {
        match value.as_integer().filter(|v| (1..=4096).contains(v)) {
            Some(value) => value as u32,
            None => {
                warning(loaded, key, value);
                default
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_missing_files_use_defaults_without_warnings() {
        assert_eq!(parse("").config, Config::default());
        assert!(parse("").warnings.is_empty());
        let path = Path::new("/path/that/does/not/exist/myna-layout.toml");
        assert_eq!(load_from_path(path).config, Config::default());
        assert!(load_from_path(path).warnings.is_empty());
    }

    #[test]
    fn parses_complete_configuration() {
        let loaded = parse(
            r#"mode = "notification"
meter_scale = 2
opacity = 1
reduced_motion = true
[position]
horizontal = "right"
vertical = "top"
margin = 1000
[size]
width = 4096
height = 900
"#,
        );
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.config.mode, Mode::NotificationOnly);
        assert_eq!(loaded.config.position.horizontal, HorizontalAnchor::Right);
        assert_eq!(loaded.config.position.vertical, VerticalAnchor::Top);
        assert_eq!(loaded.config.position.margin, 1000);
        assert_eq!(
            loaded.config.size,
            Size {
                width: 4096,
                height: 900
            }
        );
        assert_eq!(loaded.config.meter_scale, 2.0);
        assert_eq!(loaded.config.opacity, 1.0);
        assert!(loaded.config.reduced_motion);
    }

    #[test]
    fn invalid_opacity_type_is_local_and_mentions_value() {
        let loaded = parse("opacity = \"muito alta\"\nmode = \"bar\"\n[ size ]\nwidth = 500");
        assert_eq!(loaded.config.opacity, 0.9);
        assert_eq!(loaded.config.mode, Mode::BarOnly);
        assert_eq!(loaded.config.size.width, 500);
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.contains("opacity") && w.contains("muito alta"))
        );
    }

    #[test]
    fn opacity_bounds_and_unknown_mode_fall_back() {
        for value in ["1.5", "-0.1"] {
            let loaded = parse(&format!("opacity = {value}"));
            assert_eq!(loaded.config.opacity, 0.9);
            assert!(loaded.warnings[0].contains("opacity"));
        }
        let loaded = parse("mode = \"compact\"");
        assert_eq!(loaded.config.mode, Config::default().mode);
        assert!(loaded.warnings[0].contains("mode"));
    }

    #[test]
    fn invalid_margin_and_dimensions_fall_back_per_key() {
        let loaded = parse("[position]\nmargin = 1001\n[size]\nwidth = 0\nheight = 5000");
        assert_eq!(
            loaded.config.position.margin,
            Config::default().position.margin
        );
        assert_eq!(loaded.config.size, Config::default().size);
        assert_eq!(loaded.warnings.len(), 3);
    }

    #[test]
    fn unknown_keys_are_warned_and_ignored() {
        let loaded = parse("mystery = 3\n[position]\ncolor = \"red\"");
        assert_eq!(loaded.warnings.len(), 2);
        assert!(loaded.warnings.iter().any(|w| w.contains("mystery")));
        assert!(loaded.warnings.iter().any(|w| w.contains("position.color")));
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn broken_syntax_returns_all_defaults() {
        let loaded = parse("opacity = [");
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.warnings.len(), 1);
    }

    #[test]
    fn invalid_key_does_not_affect_other_values() {
        let loaded = parse("opacity = 2\nmode = \"bar\"\nreduced_motion = true");
        assert_eq!(loaded.config.opacity, 0.9);
        assert_eq!(loaded.config.mode, Mode::BarOnly);
        assert!(loaded.config.reduced_motion);
    }

    #[test]
    fn default_path_prefers_xdg_then_uses_home() {
        use std::ffi::OsStr;
        assert_eq!(
            default_path_from(
                Some(OsStr::new("/tmp/config")),
                Some(OsStr::new("/home/test"))
            ),
            Some(PathBuf::from("/tmp/config/myna-layout/config.toml"))
        );
        assert_eq!(
            default_path_from(None, Some(OsStr::new("/home/test"))),
            Some(PathBuf::from("/home/test/.config/myna-layout/config.toml"))
        );
        assert_eq!(default_path_from(None, None), None);
    }
}
