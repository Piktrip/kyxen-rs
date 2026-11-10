//! Configuration files, compatible with Python Kyxen:
//!
//! ```text
//! ~/.config/kyxen/config.toml           active profile, M-key slots, layout override
//! ~/.config/kyxen/profiles/<name>.toml  G-key macros and lighting per profile
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::Rgb;
use crate::colour::Colour;

pub const GKEY_COUNT: usize = 5;
const DEFAULT_COLOUR: Colour = Colour(Rgb(0x00, 0xcc, 0x44));

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{path}: {source}")]
    Serialize {
        path: PathBuf,
        source: toml::ser::Error,
    },
}

/// `~/.config/kyxen` (honours `$XDG_CONFIG_HOME`).
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("kyxen")
}

// ── global config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    pub active_profile: String,
    /// `m1` / `m2` / `m3` → profile name.
    pub profile_slots: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard_layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard_variant: Option<String>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            active_profile: "default".into(),
            profile_slots: BTreeMap::new(),
            keyboard_layout: None,
            keyboard_variant: None,
        }
    }
}

impl GlobalConfig {
    /// Profile assigned to M-key `slot` (1-based).
    pub fn slot(&self, slot: u8) -> Option<&str> {
        self.profile_slots
            .get(&format!("m{slot}"))
            .map(String::as_str)
    }

    /// M-key slot (1-based) that `profile` is assigned to.
    pub fn slot_of(&self, profile: &str) -> Option<u8> {
        self.profile_slots
            .iter()
            .filter(|(_, p)| *p == profile)
            .find_map(|(k, _)| k.strip_prefix('m')?.parse().ok())
    }
}

// ── macros ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    #[default]
    None,
    Type,
    Command,
    Combo,
    HoldToggle,
    MouseButton,
    /// Unknown action names are kept as no-ops instead of failing the file.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
    Button4,
    Button5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseMode {
    #[default]
    Click,
    DoubleClick,
    Hold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MacroAction {
    pub action: ActionKind,
    pub text: String,
    pub cmd: String,
    pub keys: Vec<String>,
    pub mouse_btn: MouseButton,
    pub mouse_mode: MouseMode,
    pub repeat: bool,
    /// Repeat interval in milliseconds.
    pub repeat_rate: f64,
}

impl Default for MacroAction {
    fn default() -> Self {
        Self {
            action: ActionKind::None,
            text: String::new(),
            cmd: String::new(),
            keys: Vec::new(),
            mouse_btn: MouseButton::Left,
            mouse_mode: MouseMode::Click,
            repeat: false,
            repeat_rate: 100.0,
        }
    }
}

impl MacroAction {
    /// Hold-style actions toggle on press instead of firing.
    pub fn is_hold(&self) -> bool {
        self.action == ActionKind::HoldToggle
            || (self.action == ActionKind::MouseButton && self.mouse_mode == MouseMode::Hold)
    }

    pub fn repeats(&self) -> bool {
        self.repeat
            && !matches!(
                self.action,
                ActionKind::None | ActionKind::HoldToggle | ActionKind::Unknown
            )
    }
}

// ── lighting ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingMode {
    #[default]
    Static,
    Preset,
    Animation,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetName {
    #[default]
    Breathing,
    Wave,
    RainbowWave,
    ColourCycle,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    LeftRight,
    RightLeft,
    TopBottom,
    BottomTop,
    Radial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetConfig {
    pub name: PresetName,
    pub colours: Vec<Colour>,
    /// 0.1 (slow) … 5.0 (fast).
    pub speed: f64,
    pub direction: Direction,
}

impl Default for PresetConfig {
    fn default() -> Self {
        Self {
            name: PresetName::Breathing,
            colours: vec![DEFAULT_COLOUR],
            speed: 1.0,
            direction: Direction::LeftRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    #[default]
    Cut,
    Fade,
    Ease,
    Hsv,
    WipeLeft,
    WipeRight,
    WipeTop,
    WipeBottom,
    Blink,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slide {
    pub keys: BTreeMap<String, Colour>,
    /// Seconds to hold this slide.
    pub hold: f64,
    pub transition: Transition,
    /// Seconds for the transition to the next slide (ignored for `cut`).
    pub transition_duration: f64,
}

impl Default for Slide {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            hold: 0.5,
            transition: Transition::Cut,
            transition_duration: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationConfig {
    #[serde(rename = "loop")]
    pub looped: bool,
    pub slides: Vec<Slide>,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            looped: true,
            slides: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingConfig {
    pub mode: LightingMode,
    /// Older Python profiles stored this as `colour`.
    #[serde(alias = "colour")]
    pub base_colour: Colour,
    /// Per-key overrides (static mode), keyed by names from [`crate::keymap`].
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, Colour>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<PresetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation: Option<AnimationConfig>,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            mode: LightingMode::Static,
            base_colour: DEFAULT_COLOUR,
            keys: BTreeMap::new(),
            preset: None,
            animation: None,
        }
    }
}

// ── profiles ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Triggers {
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub display_name: String,
    pub tray_colour: Colour,
    pub g1: MacroAction,
    pub g2: MacroAction,
    pub g3: MacroAction,
    pub g4: MacroAction,
    pub g5: MacroAction,
    pub triggers: Triggers,
    pub lighting: LightingConfig,
}

impl Default for Profile {
    fn default() -> Self {
        Self::new("default")
    }
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            display_name: String::new(),
            tray_colour: DEFAULT_COLOUR,
            g1: MacroAction::default(),
            g2: MacroAction::default(),
            g3: MacroAction::default(),
            g4: MacroAction::default(),
            g5: MacroAction::default(),
            triggers: Triggers::default(),
            lighting: LightingConfig::default(),
        }
    }

    /// Macro for G-key `n` (1-based).
    pub fn gkey(&self, n: u8) -> Option<&MacroAction> {
        match n {
            1 => Some(&self.g1),
            2 => Some(&self.g2),
            3 => Some(&self.g3),
            4 => Some(&self.g4),
            5 => Some(&self.g5),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.name
        } else {
            &self.display_name
        }
    }
}

// ── store ────────────────────────────────────────────────────────────────────

/// Reads and writes the config directory.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Default for Store {
    fn default() -> Self {
        Self::new(config_dir())
    }
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.dir.join("profiles")
    }

    pub fn global_path(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    pub fn load_global(&self) -> Result<GlobalConfig, ConfigError> {
        let path = self.global_path();
        if !path.exists() {
            return Ok(GlobalConfig::default());
        }
        read_toml(&path)
    }

    pub fn save_global(&self, cfg: &GlobalConfig) -> Result<(), ConfigError> {
        write_toml(&self.global_path(), cfg)
    }

    pub fn load_profile(&self, path: &Path) -> Result<Profile, ConfigError> {
        let mut p: Profile = read_toml(path)?;
        if p.name.is_empty() {
            p.name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        Ok(p)
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<(), ConfigError> {
        write_toml(
            &self.profiles_dir().join(format!("{}.toml", profile.name)),
            profile,
        )
    }

    /// All readable profiles sorted by file name, plus the files that failed.
    pub fn load_profiles(&self) -> (Vec<Profile>, Vec<ConfigError>) {
        let dir = self.profiles_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return (Vec::new(), Vec::new());
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        paths.sort();
        let (mut ok, mut failed) = (Vec::new(), Vec::new());
        for path in paths {
            match self.load_profile(&path) {
                Ok(p) => ok.push(p),
                Err(e) => failed.push(e),
            }
        }
        (ok, failed)
    }

    /// Create `profiles/default.toml` if there is no profile at all.
    pub fn ensure_default_profile(&self) -> Result<Profile, ConfigError> {
        let path = self.profiles_dir().join("default.toml");
        if path.exists() {
            return self.load_profile(&path);
        }
        let mut p = Profile::new("default");
        p.display_name = "Default".into();
        self.save_profile(&p)?;
        Ok(p)
    }

    /// Newest modification time of any profile file (for change detection).
    pub fn profiles_mtime(&self) -> Option<std::time::SystemTime> {
        fs::read_dir(self.profiles_dir())
            .ok()?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .filter_map(|e| e.metadata().ok()?.modified().ok())
            .max()
    }
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.into(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.into(),
        source,
    })
}

/// Write via a temporary file + rename so readers never see half a file.
fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    let text = toml::to_string(value).map_err(|source| ConfigError::Serialize {
        path: path.into(),
        source,
    })?;
    let io_err = |source| ConfigError::Io {
        path: path.into(),
        source,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile as written by Python Kyxen's GUI.
    const PYTHON_PROFILE: &str = r##"
name = "test"
display_name = "Test M1"
tray_colour = "#00CC44"

[g1]
action = "type"
text = "Grüße @ äöü"
cmd = ""
keys = []
mouse_btn = "left"
mouse_mode = "click"
repeat = false
repeat_rate = 100.0

[g2]
action = "combo"
keys = ["ctrl", "z"]

[g3]
action = "mouse_button"
mouse_btn = "button4"
mouse_mode = "hold"

[triggers]
apps = []

[lighting]
mode = "preset"
base_colour = "#000000"

[lighting.keys]
W = "#ffffff"

[lighting.preset]
name = "rainbow_wave"
colours = ["#ff0000", "#0000FF"]
speed = 1.5
direction = "top_bottom"
"##;

    #[test]
    fn reads_python_profile() {
        let p: Profile = toml::from_str(PYTHON_PROFILE).unwrap();
        assert_eq!(p.display_name(), "Test M1");
        assert_eq!(p.g1.action, ActionKind::Type);
        assert_eq!(p.g1.text, "Grüße @ äöü");
        assert_eq!(p.g2.keys, ["ctrl", "z"]);
        assert!(p.g3.is_hold());
        assert_eq!(p.g4.action, ActionKind::None);
        assert_eq!(p.lighting.mode, LightingMode::Preset);
        let preset = p.lighting.preset.as_ref().unwrap();
        assert_eq!(
            (preset.name, preset.speed, preset.direction),
            (PresetName::RainbowWave, 1.5, Direction::TopBottom)
        );
        assert_eq!(preset.colours[1].0, Rgb(0, 0, 255));
        assert_eq!(p.lighting.keys["W"].0, Rgb(255, 255, 255));
    }

    #[test]
    fn legacy_colour_key_and_unknown_values() {
        let p: Profile = toml::from_str(
            "[g1]\naction = \"teleport\"\n[lighting]\nmode = \"disco\"\ncolour = \"#112233\"\n",
        )
        .unwrap();
        assert_eq!(p.g1.action, ActionKind::Unknown);
        assert_eq!(p.lighting.mode, LightingMode::Unknown);
        assert_eq!(p.lighting.base_colour.0, Rgb(0x11, 0x22, 0x33));
    }

    #[test]
    fn round_trips_through_toml() {
        let p: Profile = toml::from_str(PYTHON_PROFILE).unwrap();
        let back: Profile = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn global_config_slots() {
        let g: GlobalConfig = toml::from_str(
            "active_profile = \"test\"\nkeyboard_layout = \"at\"\n[profile_slots]\nm1 = \"test\"\nm3 = \"x\"\n",
        )
        .unwrap();
        assert_eq!(g.slot(1), Some("test"));
        assert_eq!(g.slot(2), None);
        assert_eq!(g.slot_of("x"), Some(3));
        assert_eq!(g.keyboard_layout.as_deref(), Some("at"));
    }

    #[test]
    fn store_round_trip() {
        let dir = std::env::temp_dir().join(format!("kyxen-store-{}", std::process::id()));
        let store = Store::new(&dir);
        let created = store.ensure_default_profile().unwrap();
        let (profiles, failed) = store.load_profiles();
        assert!(failed.is_empty());
        assert_eq!(profiles, [created]);
        fs::write(store.profiles_dir().join("broken.toml"), "g1 = [").unwrap();
        assert_eq!(store.load_profiles().1.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
