use std::{env, fs, path::PathBuf, str::FromStr};

use crossterm::event::{KeyCode, KeyModifiers};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) behavior: BehaviorConfig,
    pub(crate) keys: KeysConfig,
    pub(crate) popup: PopupConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct BehaviorConfig {
    pub(crate) default_scope: String,
    pub(crate) default_filter: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct KeysConfig {
    pub(crate) copy: String,
    pub(crate) insert: String,
    pub(crate) cycle_scope: String,
    pub(crate) open_filter: String,
    pub(crate) cancel: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct PopupConfig {
    pub(crate) width: String,
    pub(crate) height: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            behavior: BehaviorConfig::default(),
            keys: KeysConfig::default(),
            popup: PopupConfig::default(),
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            default_scope: "tab".into(),
            default_filter: "all".into(),
        }
    }
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            copy: "tab".into(),
            insert: "enter".into(),
            cycle_scope: "ctrl+s".into(),
            open_filter: "ctrl+f".into(),
            cancel: "esc".into(),
        }
    }
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            width: "80%".into(),
            height: "80%".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shortcut {
    Char(char),
    Ctrl(char),
    Alt(char),
    Shift(char),
    Tab,
    Enter,
    Esc,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

impl FromStr for Shortcut {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim().to_ascii_lowercase();
        let parts: Vec<&str> = value.split('+').collect();
        let key = parts.last().copied().unwrap_or_default();
        let modifiers = &parts[..parts.len().saturating_sub(1)];
        let modifier = |name: &str| modifiers.iter().any(|part| *part == name);

        if modifier("ctrl") || modifier("control") {
            return key
                .chars()
                .next()
                .filter(|_| key.chars().count() == 1)
                .map(Self::Ctrl)
                .ok_or_else(|| format!("invalid control shortcut `{value}`"));
        }
        if modifier("alt") {
            return key
                .chars()
                .next()
                .filter(|_| key.chars().count() == 1)
                .map(Self::Alt)
                .ok_or_else(|| format!("invalid alt shortcut `{value}`"));
        }
        if modifier("shift") {
            return key
                .chars()
                .next()
                .filter(|_| key.chars().count() == 1)
                .map(Self::Shift)
                .ok_or_else(|| format!("invalid shift shortcut `{value}`"));
        }

        match key {
            "tab" => Ok(Self::Tab),
            "enter" | "return" => Ok(Self::Enter),
            "esc" | "escape" => Ok(Self::Esc),
            "backspace" => Ok(Self::Backspace),
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ if parts.len() == 1 && key.chars().count() == 1 => {
                Ok(Self::Char(key.chars().next().unwrap()))
            }
            _ => Err(format!("unknown shortcut `{value}`")),
        }
    }
}

pub(crate) fn matches_shortcut(code: KeyCode, modifiers: KeyModifiers, shortcut: Shortcut) -> bool {
    match shortcut {
        Shortcut::Char(key) => code == KeyCode::Char(key) && modifiers.is_empty(),
        Shortcut::Ctrl(key) => code == KeyCode::Char(key) && modifiers == KeyModifiers::CONTROL,
        Shortcut::Alt(key) => code == KeyCode::Char(key) && modifiers == KeyModifiers::ALT,
        Shortcut::Shift(key) => code == KeyCode::Char(key) && modifiers == KeyModifiers::SHIFT,
        Shortcut::Tab => code == KeyCode::Tab && modifiers.is_empty(),
        Shortcut::Enter => code == KeyCode::Enter && modifiers.is_empty(),
        Shortcut::Esc => code == KeyCode::Esc && modifiers.is_empty(),
        Shortcut::Backspace => code == KeyCode::Backspace && modifiers.is_empty(),
        Shortcut::Up => code == KeyCode::Up && modifiers.is_empty(),
        Shortcut::Down => code == KeyCode::Down && modifiers.is_empty(),
        Shortcut::Left => code == KeyCode::Left && modifiers.is_empty(),
        Shortcut::Right => code == KeyCode::Right && modifiers.is_empty(),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PickerConfig {
    pub(crate) scope: crate::models::Scope,
    pub(crate) filter: crate::models::Filter,
    pub(crate) copy: Shortcut,
    pub(crate) insert: Shortcut,
    pub(crate) cycle_scope: Shortcut,
    pub(crate) open_filter: Shortcut,
    pub(crate) cancel: Shortcut,
}

pub(crate) fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let Some(directory) = env::var_os("HERDR_PLUGIN_CONFIG_DIR") else {
        return Ok(Config::default());
    };
    let path = PathBuf::from(directory).join("config.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

pub(crate) fn parse_configured_picker(
    config: &Config,
) -> Result<PickerConfig, Box<dyn std::error::Error>> {
    Ok(PickerConfig {
        scope: config.behavior.default_scope.parse()?,
        filter: config.behavior.default_filter.parse()?,
        copy: config.keys.copy.parse()?,
        insert: config.keys.insert.parse()?,
        cycle_scope: config.keys.cycle_scope.parse()?,
        open_filter: config.keys.open_filter.parse()?,
        cancel: config.keys.cancel.parse()?,
    })
}
