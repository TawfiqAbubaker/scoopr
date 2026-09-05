use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, Read, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use norm::{
    fzf::{FzfParser, FzfV2},
    Metric,
};
use serde::Deserialize;
use serde_json::Value;

const PLUGIN_ID: &str = "scoopr";
const HORIZONTAL_PAN_STEP: usize = 8;
const ALL_AVAILABLE_PANE_LINES: &str = "4294967295";
const KIND_WORD: u8 = 1 << 0;
const KIND_LINE: u8 = 1 << 1;
const KIND_PATH: u8 = 1 << 2;
const KIND_URL: u8 = 1 << 3;
const KIND_HASH: u8 = 1 << 4;
const KIND_QUOTE: u8 = 1 << 5;
const DEFAULT_KEYBINDING: &str = "prefix+shift+c";
const SETUP_START: &str = "# >>> scoopr keybinding >>>";
const SETUP_END: &str = "# <<< scoopr keybinding <<<";
const DEFAULT_PLUGIN_CONFIG: &str = r#"# Scoopr settings. Remove a setting to use its default.

[behavior]
default_scope = "space"
default_filter = "all"

[keys]
copy = "tab"
insert = "enter"
cycle_scope = "ctrl+s"
open_filter = "ctrl+f"
cancel = "esc"

[popup]
width = "80%"
height = "80%"
"#;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
struct Config {
    behavior: BehaviorConfig,
    keys: KeysConfig,
    popup: PopupConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
struct BehaviorConfig {
    default_scope: String,
    default_filter: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
struct KeysConfig {
    copy: String,
    insert: String,
    cycle_scope: String,
    open_filter: String,
    cancel: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
struct PopupConfig {
    width: String,
    height: String,
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
            default_scope: "space".into(),
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
enum Shortcut {
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    text: String,
    kinds: u8,
}

impl Candidate {
    fn appears_in(&self, filter: Filter) -> bool {
        filter.kind().map_or(true, |kind| self.kinds & kind != 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    All,
    Word,
    Line,
    Path,
    Url,
    Hash,
    Quote,
}

impl Filter {
    fn from_key(key: char) -> Option<Self> {
        match key.to_ascii_lowercase() {
            'a' => Some(Self::All),
            'w' => Some(Self::Word),
            'l' => Some(Self::Line),
            'p' => Some(Self::Path),
            'u' => Some(Self::Url),
            'h' => Some(Self::Hash),
            'q' => Some(Self::Quote),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Word => "word",
            Self::Line => "line",
            Self::Path => "path",
            Self::Url => "url",
            Self::Hash => "hash",
            Self::Quote => "quote",
        }
    }

    fn kind(self) -> Option<u8> {
        match self {
            Self::All => None,
            Self::Word => Some(KIND_WORD),
            Self::Line => Some(KIND_LINE),
            Self::Path => Some(KIND_PATH),
            Self::Url => Some(KIND_URL),
            Self::Hash => Some(KIND_HASH),
            Self::Quote => Some(KIND_QUOTE),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Tab,
    Space,
    Server,
}

impl Scope {
    fn next(self, skip_tab: bool) -> Self {
        match self {
            Self::Space if skip_tab => Self::Server,
            Self::Space => Self::Tab,
            Self::Tab => Self::Server,
            Self::Server => Self::Space,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Space => "space",
            Self::Server => "server",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Tab => 0,
            Self::Space => 1,
            Self::Server => 2,
        }
    }
}

impl FromStr for Scope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "tab" => Ok(Self::Tab),
            "space" | "workspace" => Ok(Self::Space),
            "server" => Ok(Self::Server),
            _ => Err(format!("unknown scope `{value}`")),
        }
    }
}

impl FromStr for Filter {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "all" => Ok(Self::All),
            "word" => Ok(Self::Word),
            "line" => Ok(Self::Line),
            "path" => Ok(Self::Path),
            "url" => Ok(Self::Url),
            "hash" => Ok(Self::Hash),
            "quote" => Ok(Self::Quote),
            _ => Err(format!("unknown filter `{value}`")),
        }
    }
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
                .map(Shortcut::Ctrl)
                .ok_or_else(|| format!("invalid control shortcut `{value}`"));
        }
        if modifier("alt") {
            return key
                .chars()
                .next()
                .filter(|_| key.chars().count() == 1)
                .map(Shortcut::Alt)
                .ok_or_else(|| format!("invalid alt shortcut `{value}`"));
        }
        if modifier("shift") {
            return key
                .chars()
                .next()
                .filter(|_| key.chars().count() == 1)
                .map(Shortcut::Shift)
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

fn matches_shortcut(code: KeyCode, modifiers: KeyModifiers, shortcut: Shortcut) -> bool {
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

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let Some(directory) = env::var_os("HERDR_PLUGIN_CONFIG_DIR") else {
        return Ok(Config::default());
    };
    let path = PathBuf::from(directory).join("config.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

fn parse_configured_picker(config: &Config) -> Result<PickerConfig, Box<dyn std::error::Error>> {
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

struct PickerConfig {
    scope: Scope,
    filter: Filter,
    copy: Shortcut,
    insert: Shortcut,
    cycle_scope: Shortcut,
    open_filter: Shortcut,
    cancel: Shortcut,
}

fn main() {
    let result = match env::args().nth(1).as_deref() {
        Some("open") => open_picker(),
        Some("picker") => run_picker(),
        Some("extract") => extract_stdin(),
        Some("setup") => setup_config(),
        Some("remove-setup") => remove_setup(),
        _ => {
            eprintln!("usage: scoopr <open|picker|extract|setup|remove-setup>");
            Err("unknown command".into())
        }
    };

    if let Err(error) = result {
        eprintln!("scoopr: {error}");
        std::process::exit(1);
    }
}

fn setup_config() -> Result<(), Box<dyn std::error::Error>> {
    let path = herdr_config_path()?;
    let original = read_config(&path)?;
    let plugin_config_created = ensure_plugin_config()?;

    if original.contains(SETUP_START) || active_command(&original, "scoopr.open") {
        println!("Scoopr is already configured in {}", path.display());
        if plugin_config_created {
            println!("Created Scoopr settings at the Herdr plugin config directory.");
        }
        return Ok(());
    }

    if active_keybinding(&original, DEFAULT_KEYBINDING) {
        return Err(format!(
            "cannot add Scoopr's {DEFAULT_KEYBINDING} binding: that key is already configured in {}",
            path.display()
        )
        .into());
    }

    let mut updated = original.clone();
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&format!(
        "\n{SETUP_START}\n[[keys.command]]\nkey = \"{DEFAULT_KEYBINDING}\"\ntype = \"plugin_action\"\ncommand = \"scoopr.open\"\ndescription = \"Scoop text from current tab\"\n{SETUP_END}\n"
    ));

    write_config(&path, &original, &updated)?;
    println!(
        "Added Scoopr's {DEFAULT_KEYBINDING} binding to {}. Reload Herdr with `herdr server reload-config`.",
        path.display()
    );
    if plugin_config_created {
        println!("Created Scoopr settings at the Herdr plugin config directory.");
    }
    Ok(())
}

fn ensure_plugin_config() -> Result<bool, Box<dyn std::error::Error>> {
    let Some(directory) = env::var_os("HERDR_PLUGIN_CONFIG_DIR") else {
        return Ok(false);
    };
    let directory = PathBuf::from(directory);
    let path = directory.join("config.toml");
    if path.exists() {
        return Ok(false);
    }
    fs::create_dir_all(directory)?;
    fs::write(path, DEFAULT_PLUGIN_CONFIG)?;
    Ok(true)
}

fn remove_setup() -> Result<(), Box<dyn std::error::Error>> {
    let path = herdr_config_path()?;
    let original = read_config(&path)?;
    let Some(updated) = remove_setup_block(&original) else {
        println!(
            "Scoopr's managed keybinding is not present in {}",
            path.display()
        );
        return Ok(());
    };

    write_config(&path, &original, &updated)?;
    println!(
        "Removed Scoopr's managed keybinding from {}. Reload Herdr with `herdr server reload-config`.",
        path.display()
    );
    Ok(())
}

fn herdr_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = env::var_os("HERDR_CONFIG_PATH") {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME").ok_or("could not determine the home directory")?;
    Ok(PathBuf::from(home).join(".config/herdr/config.toml"))
}

fn read_config(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if path.exists() {
        Ok(fs::read_to_string(path)?)
    } else {
        Ok(String::new())
    }
}

fn active_keybinding(config: &str, key: &str) -> bool {
    config.lines().any(|line| {
        let line = line.trim();
        if line.starts_with('#') {
            return false;
        }
        line.strip_prefix("key = ")
            .and_then(|value| value.strip_prefix('"'))
            .and_then(|value| value.strip_suffix('"'))
            == Some(key)
    })
}

fn active_command(config: &str, command: &str) -> bool {
    let expected = format!("command = \"{command}\"");
    config.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#') && line == expected
    })
}

fn remove_setup_block(config: &str) -> Option<String> {
    let mut removing = false;
    let mut found = false;
    let mut kept = Vec::new();

    for line in config.lines() {
        if line == SETUP_START {
            removing = true;
            found = true;
            continue;
        }
        if removing {
            if line == SETUP_END {
                removing = false;
            }
            continue;
        }
        kept.push(line);
    }

    if !found || removing {
        return None;
    }

    let mut result = kept.join("\n");
    if config.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    Some(result)
}

fn write_config(
    path: &Path,
    original: &str,
    updated: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let backup = PathBuf::from(format!("{}.scoopr.bak", path.display()));
        fs::copy(path, &backup)?;
        eprintln!("Backed up {} to {}", path.display(), backup.display());
    }

    let temporary = PathBuf::from(format!("{}.scoopr.tmp", path.display()));
    fs::write(&temporary, updated)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }

    debug_assert_ne!(original, updated);
    Ok(())
}

fn open_picker() -> Result<(), Box<dyn std::error::Error>> {
    let herdr = env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let config = load_config()?;
    let target_pane = target_pane();
    let target_tab = target_tab();
    let target_workspace = target_workspace();
    let mut command = Command::new(herdr);
    command.args([
        "plugin",
        "pane",
        "open",
        "--plugin",
        PLUGIN_ID,
        "--entrypoint",
        "picker",
        "--placement",
        "popup",
        "--width",
        &config.popup.width,
        "--height",
        &config.popup.height,
        "--focus",
    ]);

    if let Some(target) = target_pane {
        command.args(["--env", &format!("SCOOPR_TARGET_PANE={target}")]);
    }
    if let Some(tab) = target_tab {
        command.args(["--env", &format!("SCOOPR_TARGET_TAB={tab}")]);
    }
    if let Some(workspace) = target_workspace {
        command.args(["--env", &format!("SCOOPR_TARGET_WORKSPACE={workspace}")]);
    }

    let status = command.status()?;
    if !status.success() {
        return Err(format!("Herdr could not open the picker ({status})").into());
    }
    Ok(())
}

fn run_picker() -> Result<(), Box<dyn std::error::Error>> {
    let picker_config = parse_configured_picker(&load_config()?)?;
    let target = target_pane().ok_or("could not determine the originating pane")?;
    let tab = target_tab().ok_or("could not determine the originating tab")?;
    let workspace = target_workspace().ok_or("could not determine the originating space")?;
    let skip_tab = workspace_tab_count(&workspace)? == 1;
    let scope = picker_config.scope;
    let text = read_scope(scope, &tab, &workspace)?;
    let candidates = extract_candidates(&text);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Show)?;

    let result = picker_loop(
        &mut stdout,
        &target,
        &tab,
        &workspace,
        scope,
        skip_tab,
        candidates,
        picker_config,
    );

    disable_raw_mode()?;
    execute!(stdout, Show, LeaveAlternateScreen)?;
    result
}

fn picker_loop(
    stdout: &mut io::Stdout,
    target: &str,
    tab: &str,
    workspace: &str,
    mut scope: Scope,
    skip_tab: bool,
    mut candidates: Vec<Candidate>,
    config: PickerConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut filter = config.filter;
    let mut query = String::new();
    let mut selected = usize::MAX;
    let mut horizontal_offset = 0usize;
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();
    let mut cached_scopes: [Option<Vec<Candidate>>; 3] = [None, None, None];

    loop {
        let filtered = ranked_matches(&candidates, filter, &query, &mut matcher, &mut parser);
        if filtered.is_empty() {
            selected = 0;
        } else if selected >= filtered.len() {
            selected = filtered.len().saturating_sub(1);
        }

        let (width, height) = size().unwrap_or((80, 20));
        let padding_row = height.saturating_sub(4);
        let instructions_row = height.saturating_sub(3);
        let divider_row = height.saturating_sub(2);
        let prompt_row = height.saturating_sub(1);
        let visible_rows = usize::from(height.saturating_sub(4)).max(1);
        let option_width = usize::from(width.saturating_sub(3));
        let horizontal_page_width = option_width.saturating_sub(1).max(1);
        let max_horizontal_offset = filtered
            .iter()
            .map(|(candidate, _)| {
                candidate
                    .chars()
                    .filter(|character| !character.is_control())
                    .count()
            })
            .max()
            .unwrap_or(0)
            .saturating_sub(horizontal_page_width);
        horizontal_offset = horizontal_offset.min(max_horizontal_offset);
        let first_visible = if filtered.len() <= visible_rows {
            0
        } else if selected >= filtered.len() - visible_rows {
            filtered.len() - visible_rows
        } else {
            selected
        };
        let last_visible = (first_visible + visible_rows).min(filtered.len());
        let visible = if filtered.is_empty() {
            &filtered[0..0]
        } else {
            &filtered[first_visible..last_visible]
        };
        let first_option_row = padding_row.saturating_sub(visible.len() as u16);

        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
        for (offset, (candidate, positions)) in visible.iter().enumerate() {
            let index = first_visible + offset;
            render_option(
                stdout,
                first_option_row + offset as u16,
                width,
                candidate,
                positions,
                index == selected,
                horizontal_offset,
            )?;
        }
        if filtered.is_empty() {
            render_plain(
                stdout,
                padding_row.saturating_sub(1),
                width,
                "  No matches",
                Color::DarkGrey,
            )?;
        }
        render_instructions(stdout, instructions_row, width, scope, filter)?;
        let divider = "─".repeat(usize::from(width.saturating_sub(1)));
        render_plain(stdout, divider_row, width, &divider, Color::DarkGrey)?;
        render_prompt(stdout, prompt_row, width, &query)?;
        stdout.flush()?;

        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            match (code, modifiers) {
                (code, modifiers)
                    if matches_shortcut(code, modifiers, config.cancel)
                        || (code == KeyCode::Char('c') && modifiers == KeyModifiers::CONTROL) =>
                {
                    return Ok(())
                }
                (code, modifiers) if matches_shortcut(code, modifiers, config.cycle_scope) => {
                    drop(filtered);
                    cached_scopes[scope.index()] = Some(candidates);
                    scope = scope.next(skip_tab);
                    candidates = match cached_scopes[scope.index()].take() {
                        Some(cached) => cached,
                        None => extract_candidates(&read_scope(scope, tab, workspace)?),
                    };
                    selected = usize::MAX;
                    horizontal_offset = 0;
                }
                (code, modifiers) if matches_shortcut(code, modifiers, config.open_filter) => {
                    if let Some(new_filter) = choose_filter(stdout, width, height, scope, filter)? {
                        filter = new_filter;
                        selected = usize::MAX;
                        horizontal_offset = 0;
                    }
                }
                (code, modifiers) if code == KeyCode::Up && modifiers.is_empty() => {
                    selected = selected.saturating_sub(1)
                }
                (code, modifiers) if code == KeyCode::Down && modifiers.is_empty() => {
                    if selected + 1 < filtered.len() {
                        selected += 1;
                    }
                }
                (code, modifiers) if code == KeyCode::Left && modifiers.is_empty() => {
                    horizontal_offset = horizontal_offset.saturating_sub(HORIZONTAL_PAN_STEP);
                }
                (code, modifiers) if code == KeyCode::Right && modifiers.is_empty() => {
                    horizontal_offset = horizontal_offset
                        .saturating_add(HORIZONTAL_PAN_STEP)
                        .min(max_horizontal_offset);
                }
                (code, modifiers) if code == KeyCode::Backspace && modifiers.is_empty() => {
                    query.pop();
                    selected = usize::MAX;
                    horizontal_offset = 0;
                }
                (code, modifiers) if matches_shortcut(code, modifiers, config.copy) => {
                    if let Some((value, _)) = filtered.get(selected) {
                        copy_to_clipboard(stdout, value)?;
                        return Ok(());
                    }
                }
                (code, modifiers) if matches_shortcut(code, modifiers, config.insert) => {
                    if let Some((value, _)) = filtered.get(selected) {
                        send_text(target, value)?;
                        return Ok(());
                    }
                }
                (KeyCode::Char(character), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    query.push(character);
                    selected = usize::MAX;
                    horizontal_offset = 0;
                }
                _ => {}
            }
        }
    }
}

fn render_option(
    stdout: &mut io::Stdout,
    row: u16,
    width: u16,
    candidate: &str,
    matched_positions: &[usize],
    selected: bool,
    horizontal_offset: usize,
) -> io::Result<()> {
    queue!(stdout, MoveTo(0, row))?;
    if selected {
        queue!(
            stdout,
            SetForegroundColor(Color::Cyan),
            Print("▌"),
            ResetColor,
            Print(" ")
        )?;
    } else {
        queue!(stdout, Print("  "))?;
    }

    let max_characters = usize::from(width.saturating_sub(3));
    let candidate_length = candidate
        .chars()
        .filter(|character| !character.is_control())
        .count();
    let has_hidden_left = horizontal_offset > 0;
    let width_after_left = max_characters.saturating_sub(if has_hidden_left { 1 } else { 0 });
    let has_hidden_right = candidate_length.saturating_sub(horizontal_offset) > width_after_left;
    let content_width = width_after_left.saturating_sub(if has_hidden_right { 1 } else { 0 });

    if has_hidden_left {
        queue!(
            stdout,
            SetForegroundColor(Color::DarkGrey),
            Print("‹"),
            ResetColor
        )?;
    }

    let mut rendered_characters = 0usize;
    let mut displayable_index = 0usize;
    for (index, character) in candidate.chars().enumerate() {
        if character.is_control() {
            continue;
        }
        if displayable_index < horizontal_offset {
            displayable_index += 1;
            continue;
        }
        if rendered_characters >= content_width {
            break;
        }
        if matched_positions.contains(&index) {
            queue!(
                stdout,
                SetForegroundColor(Color::Yellow),
                SetAttribute(Attribute::Bold),
                Print(character),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        } else {
            queue!(stdout, Print(character))?;
        }
        rendered_characters += 1;
        displayable_index += 1;
    }
    if has_hidden_right {
        queue!(
            stdout,
            SetForegroundColor(Color::DarkGrey),
            Print("›"),
            ResetColor
        )?;
    }
    Ok(())
}

fn render_plain(
    stdout: &mut io::Stdout,
    row: u16,
    width: u16,
    text: &str,
    color: Color,
) -> io::Result<()> {
    let rendered: String = text
        .chars()
        .filter(|character| !character.is_control() || *character == '\t')
        .map(|character| if character == '\t' { ' ' } else { character })
        .take(usize::from(width.saturating_sub(1)))
        .collect();
    queue!(
        stdout,
        MoveTo(0, row),
        SetForegroundColor(color),
        Print(rendered),
        ResetColor
    )
}

fn render_instructions(
    stdout: &mut io::Stdout,
    row: u16,
    width: u16,
    scope: Scope,
    filter: Filter,
) -> io::Result<()> {
    #[derive(Clone, Copy)]
    enum SegmentColor {
        Normal,
        Accent,
        State,
    }

    let scope_state = format!("[{}]", scope.label());
    let filter_state = format!("[{}]", filter.label());
    let segments = [
        ("  tab", SegmentColor::Normal),
        ("=copy", SegmentColor::Accent),
        (", enter", SegmentColor::Normal),
        ("=insert", SegmentColor::Accent),
        (", ↑/↓", SegmentColor::Normal),
        ("=move", SegmentColor::Accent),
        (", ←/→", SegmentColor::Normal),
        ("=pan", SegmentColor::Accent),
        (", ^s", SegmentColor::Normal),
        ("=scope", SegmentColor::Accent),
        (scope_state.as_str(), SegmentColor::State),
        (", ^f", SegmentColor::Normal),
        ("=filter", SegmentColor::Accent),
        (filter_state.as_str(), SegmentColor::State),
        (", esc", SegmentColor::Normal),
        ("=cancel", SegmentColor::Accent),
    ];
    let max_characters = usize::from(width.saturating_sub(1));
    let mut rendered = 0usize;

    queue!(
        stdout,
        MoveTo(0, row),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    for (text, color) in segments {
        let segment: String = text
            .chars()
            .take(max_characters.saturating_sub(rendered))
            .collect();
        if segment.is_empty() {
            break;
        }
        match color {
            SegmentColor::Normal => queue!(stdout, ResetColor)?,
            SegmentColor::Accent => queue!(stdout, SetForegroundColor(Color::Cyan))?,
            SegmentColor::State => queue!(stdout, SetForegroundColor(Color::Yellow))?,
        }
        rendered += segment.chars().count();
        queue!(stdout, Print(segment))?;
    }
    queue!(stdout, ResetColor)
}

fn choose_filter(
    stdout: &mut io::Stdout,
    terminal_width: u16,
    terminal_height: u16,
    scope: Scope,
    current: Filter,
) -> io::Result<Option<Filter>> {
    const OPTIONS: [(char, &str, Filter); 7] = [
        ('a', "all", Filter::All),
        ('w', "word", Filter::Word),
        ('l', "line", Filter::Line),
        ('p', "path", Filter::Path),
        ('u', "url", Filter::Url),
        ('h', "hash", Filter::Hash),
        ('q', "quote", Filter::Quote),
    ];
    const BOX_WIDTH: u16 = 11;
    const BOX_HEIGHT: u16 = 9;

    let width = BOX_WIDTH.min(terminal_width);
    let height = BOX_HEIGHT.min(terminal_height);
    let before_filter_state = format!(
        "  tab=copy, enter=insert, ↑/↓=move, ←/→=pan, ^s=scope[{}], ^f=filter",
        scope.label()
    );
    let filter_anchor = before_filter_state.chars().count() as u16;
    let filter_state_width = current.label().chars().count() as u16 + 2;
    let filter_state_center = filter_anchor + filter_state_width / 2;
    let left = filter_state_center
        .saturating_sub(width / 2)
        .min(terminal_width.saturating_sub(width));
    let instructions_row = terminal_height.saturating_sub(3);
    let top = instructions_row.saturating_sub(height);
    let inner_width = usize::from(width.saturating_sub(2));
    let horizontal_border = "─".repeat(inner_width);

    queue!(
        stdout,
        Hide,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(Color::DarkGrey),
        MoveTo(left, top),
        Print(format!("┌{horizontal_border}┐"))
    )?;

    for (row, (shortcut, label, filter)) in OPTIONS
        .iter()
        .take(usize::from(height.saturating_sub(2)))
        .enumerate()
    {
        let active = *filter == current;
        let content_padding = inner_width.saturating_sub(3 + label.chars().count());
        queue!(
            stdout,
            MoveTo(left, top + 1 + row as u16),
            SetAttribute(Attribute::Reset),
            SetForegroundColor(Color::DarkGrey),
            Print("│"),
            ResetColor,
            Print(" ")
        )?;
        if active {
            queue!(
                stdout,
                SetAttribute(Attribute::Bold),
                SetForegroundColor(Color::Yellow)
            )?;
        } else {
            queue!(stdout, SetForegroundColor(Color::Cyan))?;
        }
        queue!(
            stdout,
            Print(shortcut),
            ResetColor,
            Print(" "),
            Print(label),
            Print(" ".repeat(content_padding)),
            SetAttribute(Attribute::Reset),
            SetForegroundColor(Color::DarkGrey),
            Print("│")
        )?;
    }

    if height >= 2 {
        queue!(
            stdout,
            MoveTo(left, top + height - 1),
            SetAttribute(Attribute::Reset),
            SetForegroundColor(Color::DarkGrey),
            Print(format!("└{horizontal_border}┘")),
            ResetColor
        )?;
    }
    stdout.flush()?;

    loop {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            let choice = match (code, modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Char('f'), KeyModifiers::CONTROL) => None,
                (KeyCode::Char(character), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if let Some(filter) = Filter::from_key(character) {
                        Some(filter)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            queue!(stdout, Show)?;
            stdout.flush()?;
            return Ok(choice);
        }
    }
}

fn render_prompt(stdout: &mut io::Stdout, row: u16, width: u16, query: &str) -> io::Result<()> {
    let available = usize::from(width.saturating_sub(3));
    let mut tail: Vec<char> = query.chars().rev().take(available).collect();
    tail.reverse();
    let visible_query: String = tail.into_iter().collect();
    queue!(
        stdout,
        MoveTo(0, row),
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        Print("> "),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print(visible_query),
        Show
    )
}

fn read_pane(target: &str) -> Result<String, Box<dyn std::error::Error>> {
    let herdr = env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let output = Command::new(herdr)
        .args([
            "pane",
            "read",
            target,
            "--source",
            "recent-unwrapped",
            "--lines",
            ALL_AVAILABLE_PANE_LINES,
        ])
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn read_scope(
    scope: Scope,
    tab: &str,
    workspace: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let herdr = env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let mut command = Command::new(&herdr);
    command.args(["pane", "list"]);
    if scope != Scope::Server {
        command.args(["--workspace", workspace]);
    }

    let output = command.output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into());
    }

    let response: Value = serde_json::from_slice(&output.stdout)?;
    let pane_ids = pane_ids_in_scope(&response, scope, tab, workspace);
    if pane_ids.is_empty() {
        return Err(format!("could not find any panes in {} scope", scope.label()).into());
    }

    let mut combined = String::new();
    let mut successful_reads = 0usize;
    for pane_id in pane_ids {
        if let Ok(text) = read_pane(&pane_id) {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&text);
            successful_reads += 1;
        }
    }

    if successful_reads == 0 {
        return Err(format!("could not read any panes in {} scope", scope.label()).into());
    }
    Ok(combined)
}

fn workspace_tab_count(workspace: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let herdr = env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let output = Command::new(herdr)
        .args(["pane", "list", "--workspace", workspace])
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into());
    }

    let response: Value = serde_json::from_slice(&output.stdout)?;
    Ok(tab_ids_in_workspace(&response, workspace).len())
}

fn tab_ids_in_workspace(value: &Value, workspace: &str) -> HashSet<String> {
    fn visit(value: &Value, workspace: &str, tab_ids: &mut HashSet<String>) {
        match value {
            Value::Object(object) => {
                if object.get("workspace_id").and_then(Value::as_str) == Some(workspace) {
                    if let Some(tab_id) = object.get("tab_id").and_then(Value::as_str) {
                        tab_ids.insert(tab_id.to_string());
                    }
                }
                for nested in object.values() {
                    visit(nested, workspace, tab_ids);
                }
            }
            Value::Array(values) => {
                for nested in values {
                    visit(nested, workspace, tab_ids);
                }
            }
            _ => {}
        }
    }

    let mut tab_ids = HashSet::new();
    visit(value, workspace, &mut tab_ids);
    tab_ids
}

fn pane_ids_in_scope(value: &Value, scope: Scope, tab: &str, workspace: &str) -> Vec<String> {
    fn visit(
        value: &Value,
        scope: Scope,
        tab: &str,
        workspace: &str,
        seen: &mut HashSet<String>,
        panes: &mut Vec<String>,
    ) {
        match value {
            Value::Object(object) => {
                let is_in_scope = match scope {
                    Scope::Tab => object.get("tab_id").and_then(Value::as_str) == Some(tab),
                    Scope::Space => {
                        object.get("workspace_id").and_then(Value::as_str) == Some(workspace)
                    }
                    Scope::Server => true,
                };
                if is_in_scope {
                    if let Some(pane_id) = object.get("pane_id").and_then(Value::as_str) {
                        if seen.insert(pane_id.to_string()) {
                            panes.push(pane_id.to_string());
                        }
                    }
                }
                for nested in object.values() {
                    visit(nested, scope, tab, workspace, seen, panes);
                }
            }
            Value::Array(values) => {
                for nested in values {
                    visit(nested, scope, tab, workspace, seen, panes);
                }
            }
            _ => {}
        }
    }

    let mut seen = HashSet::new();
    let mut panes = Vec::new();
    visit(value, scope, tab, workspace, &mut seen, &mut panes);
    panes
}

fn send_text(target: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let herdr = env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let status = Command::new(herdr)
        .args(["pane", "send-text", target, text])
        .status()?;
    if !status.success() {
        return Err(format!("Herdr could not insert text ({status})").into());
    }
    Ok(())
}

fn copy_to_clipboard(stdout: &mut io::Stdout, text: &str) -> io::Result<()> {
    let encoded = encode_base64(text.as_bytes());
    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
    stdout.flush()
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(((input.len() + 2) / 3) * 4);
    let mut index = 0usize;

    while index < input.len() {
        let first = input[index];
        let second = input.get(index + 1).copied().unwrap_or(0);
        let third = input.get(index + 2).copied().unwrap_or(0);

        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
        if index + 1 < input.len() {
            output.push(TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if index + 2 < input.len() {
            output.push(TABLE[(third & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
        index += 3;
    }
    output
}

fn extract_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    for candidate in extract_candidates(&text) {
        println!("{}", candidate.text);
    }
    Ok(())
}

fn extract_candidates(text: &str) -> Vec<Candidate> {
    let mut indices = HashMap::new();
    let mut candidates = Vec::new();

    for line in text.lines().map(str::trim).filter(|line| line.len() >= 3) {
        add_candidate(&mut candidates, &mut indices, line, KIND_LINE);
        for quoted in quoted_values(line) {
            add_candidate(&mut candidates, &mut indices, quoted, KIND_QUOTE);
        }
    }

    for raw_token in text.split_whitespace() {
        let word = clean_token(raw_token);
        add_candidate(&mut candidates, &mut indices, &word, KIND_WORD);

        let structured = clean_structured_token(raw_token);
        if looks_like_url(&structured) {
            add_candidate(&mut candidates, &mut indices, &structured, KIND_URL);
        }
        if looks_like_path(&structured) {
            add_candidate(&mut candidates, &mut indices, &structured, KIND_PATH);
        }
        if looks_like_hash(&structured) {
            add_candidate(&mut candidates, &mut indices, &structured, KIND_HASH);
        }
    }

    candidates
}

fn add_candidate(
    candidates: &mut Vec<Candidate>,
    indices: &mut HashMap<String, usize>,
    text: &str,
    kind: u8,
) {
    let text = text.trim();
    if text.chars().count() < 3 {
        return;
    }

    if let Some(index) = indices.get(text).copied() {
        candidates[index].kinds |= kind;
    } else {
        let index = candidates.len();
        indices.insert(text.to_string(), index);
        candidates.push(Candidate {
            text: text.to_string(),
            kinds: kind,
        });
    }
}

fn clean_token(token: &str) -> String {
    token
        .trim_matches(|character: char| ",;:()[]{}<>\"'‘’“”".contains(character))
        .to_string()
}

fn clean_structured_token(token: &str) -> String {
    token
        .trim_matches(|character: char| ",;()[]{}<>\"'‘’“”".contains(character))
        .trim_end_matches(|character: char| character == '.' || character == ':')
        .to_string()
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "http://", "https://", "git@", "git://", "ssh://", "ftp://", "sftp://", "file:///",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn looks_like_path(value: &str) -> bool {
    if value.len() < 3 || looks_like_url(value) || value.contains("://") {
        return false;
    }

    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() < 2 {
        return false;
    }

    let looks_like_ratio = parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        });
    let looks_like_speed = parts.len() == 2
        && parts[1].eq_ignore_ascii_case("s")
        && parts[0]
            .chars()
            .all(|character| character.is_ascii_digit() || "kmgKMG.".contains(character));

    !looks_like_ratio && !looks_like_speed
}

fn looks_like_hash(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let (hash, prefixed) = ["sha1:", "sha224:", "sha256:", "sha384:", "sha512:"]
        .iter()
        .find_map(|prefix| lower.strip_prefix(*prefix).map(|hash| (hash, true)))
        .unwrap_or((lower.as_str(), false));

    (7..=128).contains(&hash.len())
        && hash.chars().all(|character| character.is_ascii_hexdigit())
        && (prefixed
            || hash
                .chars()
                .any(|character| character.is_ascii_alphabetic()))
}

fn quoted_values(line: &str) -> Vec<&str> {
    let mut values = Vec::new();
    let mut active: Option<(char, usize)> = None;
    let mut previous = None;

    for (byte_index, character) in line.char_indices() {
        if let Some((closing, content_start)) = active {
            if character == closing {
                let value = line[content_start..byte_index].trim();
                if value.chars().count() >= 3 {
                    values.push(value);
                }
                active = None;
            }
        } else {
            let closing = match character {
                '"' => Some('"'),
                '\'' if previous.map_or(true, |value: char| !value.is_alphanumeric()) => Some('\''),
                '“' => Some('”'),
                '‘' => Some('’'),
                _ => None,
            };
            if let Some(closing) = closing {
                active = Some((closing, byte_index + character.len_utf8()));
            }
        }
        previous = Some(character);
    }

    values
}

fn ranked_matches<'a>(
    candidates: &'a [Candidate],
    filter: Filter,
    query: &str,
    matcher: &mut FzfV2,
    parser: &mut FzfParser,
) -> Vec<(&'a String, Vec<usize>)> {
    if query.is_empty() {
        return candidates
            .iter()
            .filter(|candidate| candidate.appears_in(filter))
            .map(|candidate| (&candidate.text, Vec::new()))
            .collect();
    }

    let (normalized_query, _) = normalize_for_match(query);
    let query = parser.parse(&normalized_query);
    let mut ranked = candidates
        .iter()
        .filter(|candidate| candidate.appears_in(filter))
        .enumerate()
        .filter_map(|(original_index, candidate)| {
            let (normalized_candidate, original_character_indices) =
                normalize_for_match(&candidate.text);
            let mut ranges = Vec::new();
            let distance = catch_unwind(AssertUnwindSafe(|| {
                matcher.distance_and_ranges(query, &normalized_candidate, &mut ranges)
            }))
            .ok()
            .flatten()?;
            let literal_rank = literal_match_rank(&normalized_candidate, &normalized_query);
            let positions = normalized_candidate
                .char_indices()
                .enumerate()
                .filter_map(|(normalized_character_index, (byte_index, _))| {
                    ranges
                        .iter()
                        .any(|range| range.contains(&byte_index))
                        .then(|| original_character_indices[normalized_character_index])
                })
                .collect();
            Some((
                &candidate.text,
                positions,
                literal_rank,
                distance,
                original_index,
            ))
        })
        .collect::<Vec<_>>();

    // The UI is bottom-up and selects the final row. Match quality comes
    // first; source order only breaks otherwise comparable matches so a
    // newer result wins without displacing a much stronger match.
    ranked.sort_by(|left, right| {
        left.2
            .cmp(&right.2)
            .then_with(|| right.3.cmp(&left.3))
            .then_with(|| left.4.cmp(&right.4))
    });
    ranked
        .into_iter()
        .map(|(candidate, positions, _, _, _)| (candidate, positions))
        .collect()
}

fn normalize_for_match(text: &str) -> (String, Vec<usize>) {
    let mut normalized = String::with_capacity(text.len());
    let mut original_character_indices = Vec::with_capacity(text.chars().count());

    for (character_index, character) in text.chars().enumerate() {
        let character = match character {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' | '\u{ff07}' => '\'',
            '\u{201c}' | '\u{201d}' | '\u{ff02}' => '"',
            _ => character,
        };
        normalized.push(character);
        original_character_indices.push(character_index);
    }

    (normalized, original_character_indices)
}

fn literal_match_rank(candidate: &str, query: &str) -> u8 {
    let candidate = candidate.to_lowercase();
    let query = query.to_lowercase();

    if candidate == query {
        3
    } else if candidate.starts_with(&query) {
        2
    } else if candidate.contains(&query) {
        1
    } else {
        0
    }
}

fn target_pane() -> Option<String> {
    env::var("SCOOPR_TARGET_PANE")
        .ok()
        .or_else(|| env::var("HERDR_PANE_ID").ok())
        .or_else(|| context_string(&["focused_pane_id", "pane_id", "target_pane_id"]))
}

fn target_tab() -> Option<String> {
    env::var("SCOOPR_TARGET_TAB")
        .ok()
        .or_else(|| env::var("HERDR_TAB_ID").ok())
        .or_else(|| context_string(&["tab_id", "focused_tab_id", "target_tab_id"]))
}

fn target_workspace() -> Option<String> {
    env::var("SCOOPR_TARGET_WORKSPACE")
        .ok()
        .or_else(|| env::var("HERDR_WORKSPACE_ID").ok())
        .or_else(|| {
            context_string(&[
                "workspace_id",
                "focused_workspace_id",
                "target_workspace_id",
            ])
        })
}

fn context_string(keys: &[&str]) -> Option<String> {
    let context = env::var("HERDR_PLUGIN_CONTEXT_JSON").ok()?;
    let value: Value = serde_json::from_str(&context).ok()?;
    find_context_string(&value, keys)
}

fn find_context_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(Value::String(value)) = object.get(*key) {
                    return Some(value.clone());
                }
            }
            object
                .values()
                .find_map(|value| find_context_string(value, keys))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_context_string(value, keys)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        encode_base64, extract_candidates, pane_ids_in_scope, ranked_matches, tab_ids_in_workspace,
        Candidate, Filter, Scope, KIND_LINE, KIND_WORD,
    };
    use norm::fzf::{FzfParser, FzfV2};
    use serde_json::json;

    fn word_candidates(values: &[&str]) -> Vec<Candidate> {
        values
            .iter()
            .map(|value| Candidate {
                text: (*value).to_string(),
                kinds: KIND_WORD,
            })
            .collect()
    }

    fn values_for_filter(candidates: &[Candidate], filter: Filter) -> Vec<&str> {
        candidates
            .iter()
            .filter(|candidate| candidate.appears_in(filter))
            .map(|candidate| candidate.text.as_str())
            .collect()
    }

    #[test]
    fn encodes_osc52_payloads() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64("scoop 🥄".as_bytes()), "c2Nvb3Ag8J+lhA==");
    }

    #[test]
    fn ranks_a_contiguous_word_as_the_selected_last_result() {
        let candidates = word_candidates(&[
            "w_o_r_d spread across a weak match",
            "the exact word is here",
            "another wandering odd result, deliberately",
        ]);
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        let ranked = ranked_matches(&candidates, Filter::All, "word", &mut matcher, &mut parser);

        assert_eq!(
            ranked.last().map(|(candidate, _)| candidate.as_str()),
            Some("the exact word is here")
        );
    }

    #[test]
    fn ranks_a_literal_multi_term_prefix_above_reordered_terms() {
        let candidates = word_candidates(&["pane ...... 1", "1 package"]);
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        let ranked = ranked_matches(&candidates, Filter::All, "1 pa", &mut matcher, &mut parser);

        assert_eq!(
            ranked.last().map(|(candidate, _)| candidate.as_str()),
            Some("1 package")
        );
    }

    #[test]
    fn prefers_the_newer_matching_terminal_line() {
        let candidates = vec![
            Candidate {
                text: "git add one".into(),
                kinds: KIND_LINE,
            },
            Candidate {
                text: "git add two".into(),
                kinds: KIND_LINE,
            },
        ];
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        let ranked = ranked_matches(
            &candidates,
            Filter::All,
            "git add",
            &mut matcher,
            &mut parser,
        );

        assert_eq!(
            ranked.last().map(|(candidate, _)| candidate.as_str()),
            Some("git add two")
        );
    }

    #[test]
    fn prefers_a_strong_match_over_a_newer_weak_match() {
        let candidates = vec![
            Candidate {
                text: "git push".into(),
                kinds: KIND_LINE,
            },
            Candidate {
                text: "scoopr git:(main)".into(),
                kinds: KIND_LINE,
            },
        ];
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        let ranked = ranked_matches(&candidates, Filter::All, "git p", &mut matcher, &mut parser);

        assert_eq!(
            ranked.last().map(|(candidate, _)| candidate.as_str()),
            Some("git push")
        );
    }

    #[test]
    fn handles_a_single_h_search_query() {
        let candidates = extract_candidates(
            "https://example.com\ncommit deadbeef\nherdr plugin action invoke scoopr.open",
        );
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        let ranked = ranked_matches(&candidates, Filter::All, "h", &mut matcher, &mut parser);

        assert!(!ranked.is_empty());
    }

    #[test]
    fn handles_single_letter_search_queries() {
        let candidates = extract_candidates(
            "alpha bravo charlie delta echo foxtrot golf hotel\n\
             herdr plugin action invoke scoopr.open\n\
             https://example.com/path",
        );
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        for letter in 'a'..='z' {
            let query = letter.to_string();
            let _ = ranked_matches(&candidates, Filter::All, &query, &mut matcher, &mut parser);
        }
    }

    #[test]
    fn handles_repeated_queries_and_clearing() {
        let candidates = extract_candidates(
            "alpha bravo charlie delta echo foxtrot golf hotel\n\
             herdr plugin action invoke scoopr.open\n\
             https://example.com/path",
        );
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        for query in ["a", "aa", "a", "", "a", "a", "", "a"] {
            let _ = ranked_matches(&candidates, Filter::All, query, &mut matcher, &mut parser);
        }
    }

    #[test]
    fn treats_straight_and_typographic_apostrophes_as_equivalent() {
        let candidates = word_candidates(&["Earlier you wrote I’m here"]);
        let mut matcher = FzfV2::new();
        let mut parser = FzfParser::new();

        let ranked = ranked_matches(&candidates, Filter::All, "I'm", &mut matcher, &mut parser);

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].0, "Earlier you wrote I’m here");
        assert_eq!(ranked[0].1, [18, 19, 20]);
    }

    #[test]
    fn cycles_through_all_scopes() {
        assert_eq!(Scope::Space.next(false), Scope::Tab);
        assert_eq!(Scope::Tab.next(false), Scope::Server);
        assert_eq!(Scope::Server.next(false), Scope::Space);
        assert_eq!(Scope::Space.next(true), Scope::Server);
        assert_eq!(Scope::Server.next(true), Scope::Space);
        assert_eq!(Scope::Tab.index(), 0);
        assert_eq!(Scope::Space.index(), 1);
        assert_eq!(Scope::Server.index(), 2);
    }

    #[test]
    fn selects_filters_by_shortcut() {
        assert_eq!(Filter::from_key('a'), Some(Filter::All));
        assert_eq!(Filter::from_key('W'), Some(Filter::Word));
        assert_eq!(Filter::from_key('l'), Some(Filter::Line));
        assert_eq!(Filter::from_key('p'), Some(Filter::Path));
        assert_eq!(Filter::from_key('u'), Some(Filter::Url));
        assert_eq!(Filter::from_key('h'), Some(Filter::Hash));
        assert_eq!(Filter::from_key('q'), Some(Filter::Quote));
        assert_eq!(Filter::from_key('x'), None);
    }

    #[test]
    fn tags_structured_candidates_for_filtering() {
        let candidates = extract_candidates(
            "open /tmp/report.txt at https://example.com/a\n\
             commit deadbeef says \"hello world\"",
        );

        assert!(values_for_filter(&candidates, Filter::Path).contains(&"/tmp/report.txt"));
        assert!(values_for_filter(&candidates, Filter::Url).contains(&"https://example.com/a"));
        assert!(values_for_filter(&candidates, Filter::Hash).contains(&"deadbeef"));
        assert!(values_for_filter(&candidates, Filter::Quote).contains(&"hello world"));
    }

    #[test]
    fn collects_panes_for_each_scope() {
        let response = json!({
            "id": "cli:pane",
            "result": {
                "panes": [
                    { "pane_id": "w1:p1", "tab_id": "w1:t1", "workspace_id": "w1" },
                    { "pane_id": "w1:p2", "tab_id": "w1:t1", "workspace_id": "w1" },
                    { "pane_id": "w1:p3", "tab_id": "w1:t2", "workspace_id": "w1" },
                    { "pane_id": "w2:p1", "tab_id": "w2:t1", "workspace_id": "w2" }
                ]
            }
        });

        assert_eq!(
            pane_ids_in_scope(&response, Scope::Tab, "w1:t1", "w1"),
            ["w1:p1".to_string(), "w1:p2".to_string()]
        );
        assert_eq!(
            pane_ids_in_scope(&response, Scope::Space, "w1:t1", "w1"),
            [
                "w1:p1".to_string(),
                "w1:p2".to_string(),
                "w1:p3".to_string()
            ]
        );
        assert_eq!(
            pane_ids_in_scope(&response, Scope::Server, "w1:t1", "w1"),
            [
                "w1:p1".to_string(),
                "w1:p2".to_string(),
                "w1:p3".to_string(),
                "w2:p1".to_string()
            ]
        );
    }

    #[test]
    fn counts_distinct_tabs_in_a_workspace() {
        let response = json!({
            "panes": [
                { "pane_id": "w1:p1", "tab_id": "w1:t1", "workspace_id": "w1" },
                { "pane_id": "w1:p2", "tab_id": "w1:t1", "workspace_id": "w1" },
                { "pane_id": "w1:p3", "tab_id": "w1:t2", "workspace_id": "w1" },
                { "pane_id": "w2:p1", "tab_id": "w2:t1", "workspace_id": "w2" }
            ]
        });

        assert_eq!(tab_ids_in_workspace(&response, "w1").len(), 2);
        assert_eq!(tab_ids_in_workspace(&response, "w2").len(), 1);
    }

    #[test]
    fn detects_only_active_matching_keybindings() {
        assert!(!super::active_keybinding(
            "# key = \"prefix+shift+c\"\n",
            super::DEFAULT_KEYBINDING
        ));
        assert!(super::active_keybinding(
            "key = \"prefix+shift+c\"\n",
            super::DEFAULT_KEYBINDING
        ));
    }

    #[test]
    fn removes_only_scoopr_managed_block() {
        let config = "[keys]\n\n# >>> scoopr keybinding >>>\n[[keys.command]]\nkey = \"prefix+shift+c\"\ntype = \"plugin_action\"\ncommand = \"scoopr.open\"\ndescription = \"Scoop text from current tab\"\n# <<< scoopr keybinding <<<\n";

        assert_eq!(
            super::remove_setup_block(config),
            Some("[keys]\n\n".to_string())
        );
    }
}
