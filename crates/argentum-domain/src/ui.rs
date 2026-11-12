//! Presentation preferences for the native shell.
//!
//! This is presentation state only. Nothing here can grant authority, and the
//! same validated [`UiPatch`] path serves the settings page, the CLI, and the
//! agent's `ui_update` tool. Values are bounded and checked for legibility, so a
//! patch from any source cannot make the interface unreadable. Approval prompts
//! use fixed colors in the Slint source and are out of reach of this module.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const UI_PREFERENCES_VERSION: u16 = 1;
pub const DEFAULT_UI_PRESET: &str = "argentum";

pub const UI_FONT_PERCENT_RANGE: (u16, u16) = (80, 140);
pub const UI_RADIUS_PERCENT_RANGE: (u16, u16) = (0, 200);
pub const UI_SIDEBAR_WIDTH_RANGE: (u16, u16) = (200, 420);
pub const UI_PANEL_WIDTH_RANGE: (u16, u16) = (320, 860);
pub const UI_CHAT_WIDTH_RANGE: (u16, u16) = (520, 1200);
const MAX_FONT_NAME_BYTES: usize = 64;
const MAX_COLOR_OVERRIDES: usize = 64;

const MIN_BODY_CONTRAST: f64 = 4.5;
const MIN_QUIET_CONTRAST: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    Compact,
    Comfortable,
    Spacious,
}

impl UiDensity {
    pub const fn scale(self) -> f32 {
        match self {
            Self::Compact => 0.86,
            Self::Comfortable => 1.0,
            Self::Spacious => 1.16,
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
            Self::Spacious => "spacious",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiPanelSide {
    Left,
    Right,
}

/// A color token the interface exposes for customization.
pub struct UiColorToken {
    pub id: &'static str,
    pub role: &'static str,
}

pub const UI_COLOR_TOKENS: &[UiColorToken] = &[
    UiColorToken {
        id: "canvas",
        role: "Main window background",
    },
    UiColorToken {
        id: "sidebar",
        role: "Sidebar and status bar background",
    },
    UiColorToken {
        id: "surface",
        role: "Composer and card background",
    },
    UiColorToken {
        id: "raised",
        role: "User message and raised element background",
    },
    UiColorToken {
        id: "overlay",
        role: "Popover and dialog background",
    },
    UiColorToken {
        id: "code-bg",
        role: "Code block and input background",
    },
    UiColorToken {
        id: "hover",
        role: "Hover wash, usually translucent",
    },
    UiColorToken {
        id: "pressed",
        role: "Pressed wash, usually translucent",
    },
    UiColorToken {
        id: "border",
        role: "Default hairline",
    },
    UiColorToken {
        id: "border-strong",
        role: "Emphasized hairline",
    },
    UiColorToken {
        id: "text",
        role: "Primary text",
    },
    UiColorToken {
        id: "text-2",
        role: "Secondary text",
    },
    UiColorToken {
        id: "text-3",
        role: "Quiet text such as hints and timestamps",
    },
    UiColorToken {
        id: "silver",
        role: "Neutral icon and meter highlight",
    },
    UiColorToken {
        id: "accent",
        role: "Primary action and live state",
    },
    UiColorToken {
        id: "accent-hover",
        role: "Primary action hover",
    },
    UiColorToken {
        id: "accent-text",
        role: "Text on the accent color",
    },
    UiColorToken {
        id: "accent-ink",
        role: "Accent used as text or icon on dark surfaces",
    },
    UiColorToken {
        id: "ok",
        role: "Success state",
    },
    UiColorToken {
        id: "warn",
        role: "Warning state",
    },
    UiColorToken {
        id: "danger",
        role: "Error state",
    },
    UiColorToken {
        id: "diff-add-text",
        role: "Added line text",
    },
    UiColorToken {
        id: "diff-add-bg",
        role: "Added line background",
    },
    UiColorToken {
        id: "diff-del-text",
        role: "Removed line text",
    },
    UiColorToken {
        id: "diff-del-bg",
        role: "Removed line background",
    },
];

type PresetColors = &'static [(&'static str, &'static str)];

pub struct UiPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub colors: PresetColors,
}

const ARGENTUM_COLORS: PresetColors = &[
    ("canvas", "#0B0B0D"),
    ("sidebar", "#070708"),
    ("surface", "#131316"),
    ("raised", "#1A1A1E"),
    ("overlay", "#1E1E23"),
    ("code-bg", "#0E0E11"),
    ("hover", "#FFFFFF0F"),
    ("pressed", "#FFFFFF1A"),
    ("border", "#FFFFFF14"),
    ("border-strong", "#FFFFFF2B"),
    ("text", "#EDEDEF"),
    ("text-2", "#A6A6AD"),
    ("text-3", "#75757D"),
    ("silver", "#C9CCD1"),
    ("accent", "#D2202E"),
    ("accent-hover", "#E8323F"),
    ("accent-text", "#FFFFFF"),
    ("accent-ink", "#F0646E"),
    ("ok", "#8FB996"),
    ("warn", "#D9B36A"),
    ("danger", "#F0646E"),
    ("diff-add-text", "#A9D0AF"),
    ("diff-add-bg", "#8FB9961A"),
    ("diff-del-text", "#F0949B"),
    ("diff-del-bg", "#D2202E1F"),
];

const OBSIDIAN_COLORS: PresetColors = &[
    ("canvas", "#000000"),
    ("sidebar", "#000000"),
    ("surface", "#0C0C0E"),
    ("raised", "#141417"),
    ("overlay", "#17171B"),
    ("code-bg", "#08080A"),
    ("hover", "#FFFFFF0F"),
    ("pressed", "#FFFFFF1A"),
    ("border", "#FFFFFF17"),
    ("border-strong", "#FFFFFF30"),
    ("text", "#EFEFF1"),
    ("text-2", "#A6A6AD"),
    ("text-3", "#74747C"),
    ("silver", "#C9CCD1"),
    ("accent", "#D2202E"),
    ("accent-hover", "#E8323F"),
    ("accent-text", "#FFFFFF"),
    ("accent-ink", "#F0646E"),
    ("ok", "#8FB996"),
    ("warn", "#D9B36A"),
    ("danger", "#F0646E"),
    ("diff-add-text", "#A9D0AF"),
    ("diff-add-bg", "#8FB9961A"),
    ("diff-del-text", "#F0949B"),
    ("diff-del-bg", "#D2202E1F"),
];

const GRAPHITE_COLORS: PresetColors = &[
    ("canvas", "#18181B"),
    ("sidebar", "#121214"),
    ("surface", "#222226"),
    ("raised", "#2A2A2F"),
    ("overlay", "#2D2D33"),
    ("code-bg", "#141416"),
    ("hover", "#FFFFFF12"),
    ("pressed", "#FFFFFF21"),
    ("border", "#FFFFFF1A"),
    ("border-strong", "#FFFFFF33"),
    ("text", "#F0F0F2"),
    ("text-2", "#B4B4BB"),
    ("text-3", "#8A8A92"),
    ("silver", "#D4D6DA"),
    ("accent", "#C91F2C"),
    ("accent-hover", "#DE2F3C"),
    ("accent-text", "#FFFFFF"),
    ("accent-ink", "#F4767F"),
    ("ok", "#9CC4A3"),
    ("warn", "#DFBB75"),
    ("danger", "#F4767F"),
    ("diff-add-text", "#B3D7B9"),
    ("diff-add-bg", "#9CC4A31F"),
    ("diff-del-text", "#F59DA3"),
    ("diff-del-bg", "#C91F2C29"),
];

const EMBER_COLORS: PresetColors = &[
    ("canvas", "#0E0A0A"),
    ("sidebar", "#090606"),
    ("surface", "#181111"),
    ("raised", "#211718"),
    ("overlay", "#261A1B"),
    ("code-bg", "#120C0C"),
    ("hover", "#FFE6E612"),
    ("pressed", "#FFE6E61F"),
    ("border", "#FFE6E617"),
    ("border-strong", "#FFE6E630"),
    ("text", "#F2ECEC"),
    ("text-2", "#B5A9A9"),
    ("text-3", "#857878"),
    ("silver", "#D6CCCC"),
    ("accent", "#E0202C"),
    ("accent-hover", "#F53845"),
    ("accent-text", "#FFFFFF"),
    ("accent-ink", "#FF7A82"),
    ("ok", "#9CBF9B"),
    ("warn", "#E0B56A"),
    ("danger", "#FF7A82"),
    ("diff-add-text", "#B3D0B0"),
    ("diff-add-bg", "#9CBF9B1A"),
    ("diff-del-text", "#FFA0A6"),
    ("diff-del-bg", "#E0202C26"),
];

const SILVER_COLORS: PresetColors = &[
    ("canvas", "#F6F6F7"),
    ("sidebar", "#ECECEE"),
    ("surface", "#FFFFFF"),
    ("raised", "#E7E7EA"),
    ("overlay", "#FFFFFF"),
    ("code-bg", "#F0F0F2"),
    ("hover", "#0000000D"),
    ("pressed", "#00000017"),
    ("border", "#00000017"),
    ("border-strong", "#00000030"),
    ("text", "#17171A"),
    ("text-2", "#55555C"),
    ("text-3", "#74747C"),
    ("silver", "#5A5C62"),
    ("accent", "#C41C29"),
    ("accent-hover", "#AD1722"),
    ("accent-text", "#FFFFFF"),
    ("accent-ink", "#B01824"),
    ("ok", "#2F7D3D"),
    ("warn", "#8A6312"),
    ("danger", "#B01824"),
    ("diff-add-text", "#1F6B2C"),
    ("diff-add-bg", "#2F7D3D1F"),
    ("diff-del-text", "#A0141F"),
    ("diff-del-bg", "#C41C291A"),
];

pub const UI_PRESETS: &[UiPreset] = &[
    UiPreset {
        id: DEFAULT_UI_PRESET,
        label: "Argentum",
        detail: "Near-black with the logotype's red.",
        colors: ARGENTUM_COLORS,
    },
    UiPreset {
        id: "obsidian",
        label: "Obsidian",
        detail: "True black for OLED screens.",
        colors: OBSIDIAN_COLORS,
    },
    UiPreset {
        id: "graphite",
        label: "Graphite",
        detail: "Softer dark gray.",
        colors: GRAPHITE_COLORS,
    },
    UiPreset {
        id: "ember",
        label: "Ember",
        detail: "Warm black with a brighter red.",
        colors: EMBER_COLORS,
    },
    UiPreset {
        id: "silver",
        label: "Silver",
        detail: "Light theme.",
        colors: SILVER_COLORS,
    },
];

/// The persisted appearance document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiPreferences {
    pub thinking: ThinkingMode,
    pub version: u16,
    pub preset: String,
    pub colors: BTreeMap<String, String>,
    pub font_percent: u16,
    pub radius_percent: u16,
    pub density: UiDensity,
    pub font_ui: String,
    pub font_mono: String,
    pub sidebar_width: u16,
    pub panel_width: u16,
    pub chat_width: u16,
    pub panel_side: UiPanelSide,
    pub user_bubbles: bool,
    pub reduced_motion: bool,
    pub first_run_complete: bool,
}

/// How much reasoning to request from a provider that supports it.
///
/// `Off` turns thinking off. The others are effort levels a model may report as
/// supported. DeepSeek documents `low`, `high` and `max` at
/// <https://api-docs.deepseek.com/api/create-chat-completion>.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingMode {
    Off,
    Low,
    #[default]
    High,
    Max,
}

impl ThinkingMode {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    /// The short name the composer shows.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Low => "Low",
            Self::High => "High",
            Self::Max => "Max",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "off" => Some(Self::Off),
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    /// An effort level a provider reports for a model. `none` and unknown values
    /// are not effort levels, so they map to nothing.
    pub fn from_effort_level(level: &str) -> Option<Self> {
        match level {
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    /// The mode after `self` when the owner taps the composer chip: Off, then each
    /// supported level in order, then Off again. A mode the model does not support
    /// moves to the next one it does.
    pub fn next_for(self, levels: &[Self]) -> Self {
        let mut choices = levels
            .iter()
            .copied()
            .filter(|level| *level != Self::Off)
            .collect::<Vec<_>>();
        choices.sort();
        choices.dedup();
        choices
            .into_iter()
            .find(|level| *level > self)
            .unwrap_or(Self::Off)
    }
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            thinking: ThinkingMode::default(),
            version: UI_PREFERENCES_VERSION,
            preset: DEFAULT_UI_PRESET.to_owned(),
            colors: BTreeMap::new(),
            font_percent: 100,
            radius_percent: 100,
            density: UiDensity::Comfortable,
            font_ui: "Segoe UI Variable".to_owned(),
            font_mono: "Cascadia Mono".to_owned(),
            sidebar_width: 264,
            panel_width: 460,
            chat_width: 760,
            panel_side: UiPanelSide::Right,
            user_bubbles: true,
            reduced_motion: false,
            first_run_complete: false,
        }
    }
}

/// A partial change. `None` leaves a field alone. A color value of `null` removes
/// that override and returns the token to the preset's value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_percent: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_percent: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<UiDensity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_ui: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_mono: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel_width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel_side: Option<UiPanelSide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_bubbles: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduced_motion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_run_complete: Option<bool>,
    /// Optional modules to turn on or off, by stable surface ID.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, bool>,
    /// A named module set such as `focused`, `standard`, `review`, `trace`, or `full`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_profile: Option<String>,
    /// Return every appearance value to its default before applying the rest.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reset: bool,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum UiPreferencesError {
    #[error("unknown theme preset '{0}'")]
    UnknownPreset(String),
    #[error("unknown color token '{0}'")]
    UnknownToken(String),
    #[error("color for '{0}' must be #RRGGBB or #RRGGBBAA")]
    InvalidColor(String),
    #[error("too many color overrides")]
    TooManyColors,
    #[error("{field} must be between {min} and {max}")]
    OutOfRange {
        field: &'static str,
        min: u16,
        max: u16,
    },
    #[error("font name for {0} must be 1 to 64 printable characters")]
    InvalidFont(&'static str),
    #[error(
        "'{foreground}' on '{background}' has contrast {actual:.2}:1, below the required {required:.1}:1"
    )]
    Illegible {
        foreground: &'static str,
        background: &'static str,
        actual: f64,
        required: f64,
    },
}

/// Parsed sRGB color with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl UiColor {
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#')?;
        if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |start: usize| u8::from_str_radix(&hex[start..start + 2], 16).ok();
        Some(Self {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
            a: if hex.len() == 8 { channel(6)? } else { 255 },
        })
    }

    fn over(self, base: Self) -> Self {
        let alpha = f64::from(self.a) / 255.0;
        let mix = |top: u8, bottom: u8| {
            (f64::from(top) * alpha + f64::from(bottom) * (1.0 - alpha)).round() as u8
        };
        Self {
            r: mix(self.r, base.r),
            g: mix(self.g, base.g),
            b: mix(self.b, base.b),
            a: 255,
        }
    }

    fn luminance(self) -> f64 {
        let linear = |channel: u8| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    pub fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

/// Foreground and background pairs that must stay legible.
const LEGIBILITY_RULES: &[(&str, &str, f64)] = &[
    ("text", "canvas", MIN_BODY_CONTRAST),
    ("text", "sidebar", MIN_BODY_CONTRAST),
    ("text", "surface", MIN_BODY_CONTRAST),
    ("text", "raised", MIN_BODY_CONTRAST),
    ("text", "overlay", MIN_BODY_CONTRAST),
    ("text", "code-bg", MIN_BODY_CONTRAST),
    ("text-2", "canvas", MIN_BODY_CONTRAST),
    ("text-2", "surface", MIN_BODY_CONTRAST),
    ("text-2", "overlay", MIN_BODY_CONTRAST),
    ("text-3", "canvas", MIN_QUIET_CONTRAST),
    ("text-3", "sidebar", MIN_QUIET_CONTRAST),
    ("text-3", "surface", MIN_QUIET_CONTRAST),
    ("accent-text", "accent", MIN_BODY_CONTRAST),
    ("accent", "canvas", MIN_QUIET_CONTRAST),
    ("accent-ink", "canvas", MIN_QUIET_CONTRAST),
    ("danger", "canvas", MIN_QUIET_CONTRAST),
    ("diff-add-text", "canvas", MIN_BODY_CONTRAST),
    ("diff-del-text", "canvas", MIN_BODY_CONTRAST),
];

fn check_range(
    field: &'static str,
    value: u16,
    (min, max): (u16, u16),
) -> Result<(), UiPreferencesError> {
    if value < min || value > max {
        return Err(UiPreferencesError::OutOfRange { field, min, max });
    }
    Ok(())
}

fn check_font(field: &'static str, value: &str) -> Result<(), UiPreferencesError> {
    if value.trim().is_empty()
        || value.len() > MAX_FONT_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(UiPreferencesError::InvalidFont(field));
    }
    Ok(())
}

impl UiPreferences {
    /// The effective color of each token, preset first and overrides on top.
    pub fn resolved_colors(&self) -> BTreeMap<&'static str, UiColor> {
        let preset = UI_PRESETS
            .iter()
            .find(|preset| preset.id == self.preset)
            .unwrap_or(&UI_PRESETS[0]);
        let mut colors = BTreeMap::new();
        for (token, value) in preset.colors {
            if let Some(color) = UiColor::parse(value) {
                colors.insert(*token, color);
            }
        }
        for token in UI_COLOR_TOKENS {
            if let Some(color) = self.colors.get(token.id).and_then(|v| UiColor::parse(v)) {
                colors.insert(token.id, color);
            }
        }
        colors
    }

    pub fn validate(&self) -> Result<(), UiPreferencesError> {
        if !UI_PRESETS.iter().any(|preset| preset.id == self.preset) {
            return Err(UiPreferencesError::UnknownPreset(self.preset.clone()));
        }
        if self.colors.len() > MAX_COLOR_OVERRIDES {
            return Err(UiPreferencesError::TooManyColors);
        }
        for (token, value) in &self.colors {
            if !UI_COLOR_TOKENS.iter().any(|known| known.id == token) {
                return Err(UiPreferencesError::UnknownToken(token.clone()));
            }
            if UiColor::parse(value).is_none() {
                return Err(UiPreferencesError::InvalidColor(token.clone()));
            }
        }
        check_range("font_percent", self.font_percent, UI_FONT_PERCENT_RANGE)?;
        check_range(
            "radius_percent",
            self.radius_percent,
            UI_RADIUS_PERCENT_RANGE,
        )?;
        check_range("sidebar_width", self.sidebar_width, UI_SIDEBAR_WIDTH_RANGE)?;
        check_range("panel_width", self.panel_width, UI_PANEL_WIDTH_RANGE)?;
        check_range("chat_width", self.chat_width, UI_CHAT_WIDTH_RANGE)?;
        check_font("font_ui", &self.font_ui)?;
        check_font("font_mono", &self.font_mono)?;

        let colors = self.resolved_colors();
        let canvas = colors.get("canvas").copied().unwrap_or(UiColor {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        });
        let solid = |token: &str| colors.get(token).copied().unwrap_or(canvas).over(canvas);
        for (foreground, background, required) in LEGIBILITY_RULES {
            let actual = solid(foreground).contrast(solid(background));
            if actual < *required {
                return Err(UiPreferencesError::Illegible {
                    foreground,
                    background,
                    actual,
                    required: *required,
                });
            }
        }
        Ok(())
    }

    /// Apply a patch. The result is returned only when the whole document is valid,
    /// so a rejected patch changes nothing.
    pub fn with_patch(&self, patch: &UiPatch) -> Result<Self, UiPreferencesError> {
        let mut next = if patch.reset {
            Self {
                first_run_complete: self.first_run_complete,
                ..Self::default()
            }
        } else {
            self.clone()
        };
        if let Some(preset) = &patch.preset {
            next.preset = preset.clone();
            // A new preset starts clean, unless this patch also sets overrides.
            next.colors.clear();
        }
        for (token, value) in &patch.colors {
            match value {
                Some(value) => {
                    next.colors.insert(token.clone(), value.clone());
                }
                None => {
                    next.colors.remove(token);
                }
            }
        }
        macro_rules! take {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = &patch.$field { next.$field = value.clone(); })*
            };
        }
        take!(
            font_percent,
            radius_percent,
            density,
            font_ui,
            font_mono,
            sidebar_width,
            panel_width,
            chat_width,
            panel_side,
            user_bubbles,
            reduced_motion,
            first_run_complete,
        );
        next.version = UI_PREFERENCES_VERSION;
        next.validate()?;
        Ok(next)
    }
}

impl UiPatch {
    /// A plain sentence naming what the patch changes, for approval prompts and
    /// notices. Built from validated field names, never from free model text.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.reset {
            parts.push("reset appearance to defaults".to_owned());
        }
        if let Some(profile) = &self.module_profile {
            parts.push(format!("modules to the {profile} set"));
        }
        for (module, enabled) in &self.modules {
            parts.push(format!(
                "{} {module}",
                if *enabled { "turn on" } else { "turn off" }
            ));
        }
        if let Some(preset) = &self.preset {
            parts.push(format!("theme to {preset}"));
        }
        for (token, value) in &self.colors {
            match value {
                Some(value) => parts.push(format!("{token} to {value}")),
                None => parts.push(format!("{token} back to the theme default")),
            }
        }
        if let Some(value) = self.font_percent {
            parts.push(format!("text size to {value}%"));
        }
        if let Some(value) = self.radius_percent {
            parts.push(format!("corner roundness to {value}%"));
        }
        if let Some(value) = self.density {
            parts.push(format!("density to {}", value.id()));
        }
        if let Some(value) = &self.font_ui {
            parts.push(format!("interface font to {value}"));
        }
        if let Some(value) = &self.font_mono {
            parts.push(format!("code font to {value}"));
        }
        if let Some(value) = self.sidebar_width {
            parts.push(format!("sidebar width to {value}px"));
        }
        if let Some(value) = self.panel_width {
            parts.push(format!("panel width to {value}px"));
        }
        if let Some(value) = self.chat_width {
            parts.push(format!("conversation width to {value}px"));
        }
        if let Some(value) = self.panel_side {
            parts.push(
                match value {
                    UiPanelSide::Left => "side panel to the left",
                    UiPanelSide::Right => "side panel to the right",
                }
                .to_owned(),
            );
        }
        if let Some(value) = self.user_bubbles {
            parts.push(format!(
                "message bubbles {}",
                if value { "on" } else { "off" }
            ));
        }
        if let Some(value) = self.reduced_motion {
            parts.push(format!(
                "reduced motion {}",
                if value { "on" } else { "off" }
            ));
        }
        if parts.is_empty() {
            "no change".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiChangeSource {
    User,
    Agent,
}

/// Token totals reported by one provider profile in this project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsageRow {
    pub profile_id: String,
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Reported tokens for one UTC day and model in this workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageDayRow {
    pub day: String,
    pub model: String,
    pub turns: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
}

/// One project this database knows, for switching and for usage across projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectOverview {
    pub name: String,
    pub workspace_root: String,
    pub sessions: u64,
    /// Unix seconds of the latest session activity, if any.
    pub last_activity: Option<i64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitViewState {
    Ready,
    /// The project's authority policy does not allow an external process.
    Blocked,
    NotRepository,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    /// One letter: M, A, D, R, ?, or U.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatusSnapshot {
    pub state: GitViewState,
    pub message: String,
    pub branch: String,
    pub files: Vec<GitFileStatus>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiffSnapshot {
    pub state: GitViewState,
    pub message: String,
    pub path: String,
    pub diff: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserBlockKind {
    Heading1,
    Heading2,
    Heading3,
    Paragraph,
    ListItem,
    Quote,
    Code,
    Rule,
}

/// One block of a page as reader text. `markdown` carries escaped inline
/// formatting and absolute http(s) links only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserBlock {
    pub kind: BrowserBlockKind,
    pub markdown: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserPageState {
    Ready,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPage {
    pub state: BrowserPageState,
    pub message: String,
    pub url: String,
    pub title: String,
    pub blocks: Vec<BrowserBlock>,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_modes_cycle_through_the_levels_a_model_supports() {
        use ThinkingMode::{High, Low, Max, Off};
        let deepseek = [Low, High, Max];
        assert_eq!(Off.next_for(&deepseek), Low);
        assert_eq!(Low.next_for(&deepseek), High);
        assert_eq!(High.next_for(&deepseek), Max);
        assert_eq!(Max.next_for(&deepseek), Off);
        let narrow = [Max, High, High];
        assert_eq!(Off.next_for(&narrow), High);
        assert_eq!(Low.next_for(&narrow), High);
        assert_eq!(Max.next_for(&narrow), Off);
        assert_eq!(High.next_for(&[]), Off);
        for mode in [Off, Low, High, Max] {
            assert_eq!(ThinkingMode::from_id(mode.id()), Some(mode));
            assert_eq!(
                serde_json::to_value(mode).unwrap(),
                serde_json::json!(mode.id())
            );
        }
        assert_eq!(ThinkingMode::from_effort_level("low"), Some(Low));
        assert_eq!(ThinkingMode::from_effort_level("none"), None);
        assert_eq!(ThinkingMode::from_effort_level("off"), None);
        assert_eq!(High.label(), "High");
    }

    #[test]
    fn every_preset_is_complete_and_legible() {
        for preset in UI_PRESETS {
            for token in UI_COLOR_TOKENS {
                assert!(
                    preset.colors.iter().any(|(id, _)| *id == token.id),
                    "{} is missing {}",
                    preset.id,
                    token.id
                );
            }
            let preferences = UiPreferences {
                preset: preset.id.to_owned(),
                ..UiPreferences::default()
            };
            preferences
                .validate()
                .unwrap_or_else(|error| panic!("{}: {error}", preset.id));
        }
    }

    #[test]
    fn invisible_text_is_rejected_and_changes_nothing() {
        let current = UiPreferences::default();
        let patch = UiPatch {
            colors: BTreeMap::from([("text".to_owned(), Some("#0B0B0D".to_owned()))]),
            ..UiPatch::default()
        };
        assert!(matches!(
            current.with_patch(&patch),
            Err(UiPreferencesError::Illegible {
                foreground: "text",
                ..
            })
        ));
        assert_eq!(current, UiPreferences::default());
    }

    #[test]
    fn translucent_text_cannot_slip_past_the_contrast_check() {
        let patch = UiPatch {
            colors: BTreeMap::from([("text".to_owned(), Some("#FFFFFF08".to_owned()))]),
            ..UiPatch::default()
        };
        assert!(UiPreferences::default().with_patch(&patch).is_err());
    }

    #[test]
    fn unknown_tokens_and_bad_values_are_rejected() {
        let current = UiPreferences::default();
        let unknown = UiPatch {
            colors: BTreeMap::from([("approval".to_owned(), Some("#000000".to_owned()))]),
            ..UiPatch::default()
        };
        assert!(matches!(
            current.with_patch(&unknown),
            Err(UiPreferencesError::UnknownToken(_))
        ));
        let bad_color = UiPatch {
            colors: BTreeMap::from([("accent".to_owned(), Some("red".to_owned()))]),
            ..UiPatch::default()
        };
        assert!(matches!(
            current.with_patch(&bad_color),
            Err(UiPreferencesError::InvalidColor(_))
        ));
        let small = UiPatch {
            font_percent: Some(10),
            ..UiPatch::default()
        };
        assert!(matches!(
            current.with_patch(&small),
            Err(UiPreferencesError::OutOfRange { .. })
        ));
        let font = UiPatch {
            font_ui: Some("bad\nfont".to_owned()),
            ..UiPatch::default()
        };
        assert!(matches!(
            current.with_patch(&font),
            Err(UiPreferencesError::InvalidFont(_))
        ));
    }

    #[test]
    fn a_new_preset_clears_overrides_and_null_removes_one() {
        let themed = UiPreferences::default()
            .with_patch(&UiPatch {
                colors: BTreeMap::from([("accent".to_owned(), Some("#E0202C".to_owned()))]),
                ..UiPatch::default()
            })
            .expect("override");
        assert_eq!(themed.colors.len(), 1);
        let removed = themed
            .with_patch(&UiPatch {
                colors: BTreeMap::from([("accent".to_owned(), None)]),
                ..UiPatch::default()
            })
            .expect("remove");
        assert!(removed.colors.is_empty());
        let switched = themed
            .with_patch(&UiPatch {
                preset: Some("graphite".to_owned()),
                ..UiPatch::default()
            })
            .expect("preset");
        assert!(switched.colors.is_empty());
    }

    #[test]
    fn reset_keeps_first_run_state() {
        let done = UiPreferences {
            first_run_complete: true,
            font_percent: 130,
            ..UiPreferences::default()
        };
        let reset = done
            .with_patch(&UiPatch {
                reset: true,
                ..UiPatch::default()
            })
            .expect("reset");
        assert_eq!(reset.font_percent, 100);
        assert!(reset.first_run_complete);
    }

    #[test]
    fn patch_description_names_changes_without_free_text() {
        let patch = UiPatch {
            preset: Some("ember".to_owned()),
            font_percent: Some(110),
            ..UiPatch::default()
        };
        assert_eq!(patch.describe(), "theme to ember, text size to 110%");
        assert_eq!(UiPatch::default().describe(), "no change");
    }

    #[test]
    fn unknown_patch_fields_are_rejected_at_the_wire() {
        let error = serde_json::from_str::<UiPatch>(r#"{"authority":"grant"}"#);
        assert!(error.is_err());
    }
}
