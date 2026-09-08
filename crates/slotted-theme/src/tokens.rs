//! The token table: numbers a theme is built from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::material::ThemeColor;
use crate::motion::{Easing, MotionPreset};

/// Spacing scale, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spacing {
    /// 2-4px: between an icon and its count.
    pub xs: f32,
    /// Slot gap.
    pub sm: f32,
    /// Panel padding.
    pub md: f32,
    /// Between sections.
    pub lg: f32,
    /// Between panels.
    pub xl: f32,
}

/// Corner radii, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Radii {
    /// Slots, small buttons.
    pub sm: f32,
    /// Buttons, tooltips.
    pub md: f32,
    /// Panels.
    pub lg: f32,
}

/// One shadow level. Becomes a `BoxShadow`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Elevation {
    /// Horizontal offset. Zero for a light from straight above; the paper
    /// theme's hard ink shadows sit down and to the right.
    #[serde(default)]
    pub x: f32,
    /// Vertical offset.
    pub y: f32,
    /// Blur radius.
    pub blur: f32,
    /// Spread.
    pub spread: f32,
    /// Shadow colour (usually black with alpha).
    pub color: ThemeColor,
}

/// Motion durations, in milliseconds, before [`Motion`](crate::Motion) scaling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Durations {
    /// Hover, press.
    pub fast: u32,
    /// Drop squash, fades, fly-to-slot.
    pub normal: u32,
    /// Stagger total.
    pub slow: u32,
    /// How long a slot must be hovered before its compact tooltip is
    /// composed. A token rather than a multiple of [`normal`](Self::normal):
    /// a tooltip that waits for a whole fade tier reads as lag, and the two
    /// numbers move for different reasons.
    #[serde(default = "default_hover_delay")]
    pub hover_delay: u32,
}

/// The hover delay a theme gets when it names none: long enough that a
/// pointer sweeping a grid does not trail tooltips, short enough that a
/// deliberate hover feels answered.
const fn default_hover_delay() -> u32 {
    60
}

/// Where a text role's glyphs come from.
///
/// `family` is the name the direction is designed around; `path` is the TTF
/// or OTF asset that honours it. With a `path` the apply system loads it.
/// With none and `system` set, the family name goes to Bevy's
/// `FontSource::Family`, which resolves against the fonts the OS has when
/// the game enables Bevy's `system_font_discovery` feature. Otherwise Bevy's
/// default font stays, and the family name still tells a game which OFL file
/// to drop into `assets/fonts/` to complete the look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontToken {
    /// Family name (`IBM Plex Mono`, `Rajdhani`).
    pub family: String,
    /// Asset path of a TTF or OTF.
    #[serde(default)]
    pub path: Option<String>,
    /// Ask the OS for `family` when there is no `path`.
    #[serde(default)]
    pub system: bool,
}

impl FontToken {
    /// A family with no file: the name is documentation, the glyphs are Bevy's.
    pub fn family(name: &str) -> Self {
        Self {
            family: name.to_owned(),
            path: None,
            system: false,
        }
    }
}

/// One motion preset's timing, overriding the tier in [`Durations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionSpec {
    /// Duration in milliseconds before [`Motion`](crate::Motion) scaling.
    pub duration: u32,
    /// Easing for this preset; `None` uses [`MotionTokens::easing`].
    #[serde(default)]
    pub easing: Option<Easing>,
}

/// Per-preset motion. A theme that only sets [`Durations`] gets the three
/// tiers and the standard ease-out; a direction with its own feel (paper's
/// stamp, neon's snap) names the presets it wants to differ.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MotionTokens {
    /// Easing for every preset without its own.
    #[serde(default)]
    pub easing: Easing,
    /// Per-preset duration and easing overrides.
    #[serde(default)]
    pub presets: BTreeMap<MotionPreset, MotionSpec>,
}

/// Backdrop blur settings for `Material::Glass`. Read only with the `blur`
/// feature; harmless otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Blur {
    /// Blur radius in backdrop texels.
    pub radius: f32,
    /// Backdrop image is the viewport divided by this on each axis.
    pub backdrop_divisor: u32,
}

impl Default for Spacing {
    /// The `glass` theme's scale, so a widget can lay itself out before any
    /// theme asset has loaded.
    fn default() -> Self {
        Self {
            xs: 2.0,
            sm: 6.0,
            md: 14.0,
            lg: 20.0,
            xl: 32.0,
        }
    }
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
        }
    }
}

impl Default for Durations {
    fn default() -> Self {
        Self {
            fast: 90,
            normal: 180,
            slow: 320,
            hover_delay: default_hover_delay(),
        }
    }
}

impl Default for Blur {
    fn default() -> Self {
        Self {
            radius: 4.0,
            backdrop_divisor: 4,
        }
    }
}

/// Widget sizes, in logical pixels at `UiScale` 1.
///
/// A slot's edge, the gap between slots and the item browser's panel
/// geometry were constants in `slotted-ui` and `slotted-browser` until a
/// theme needed to say a slot is bigger than 44 px. The defaults are the
/// numbers those constants held, so a theme that names none looks exactly as
/// it did.
///
/// Every number is a UI unit: `UiScale` multiplies a `Node`'s pixels on its
/// way to the screen, so a widget writes these values unscaled and code that
/// plans a layout outside the node tree (the browser's dock) converts the
/// window into the same units first.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sizes {
    /// Edge length of a slot.
    #[serde(default = "default_slot_size")]
    pub slot_size: f32,
    /// Gap between slots in a grid. `None` follows the spacing scale's `sm`
    /// step, which is where the gap came from before it was a size; read it
    /// through [`Tokens::slot_gap`] rather than directly.
    #[serde(default)]
    pub slot_gap: Option<f32>,
    /// The item browser panel's width where the free strip allows it.
    #[serde(default = "default_panel_width")]
    pub panel_width: f32,
    /// One browser card's width.
    #[serde(default = "default_card_width")]
    pub card_width: f32,
    /// One browser card's height.
    #[serde(default = "default_card_height")]
    pub card_height: f32,
    /// Height the browser panel reserves for everything that is not the card
    /// grid: the search field, the chip row, the bookmark strip and the
    /// status line.
    #[serde(default = "default_chrome_height")]
    pub chrome_height: f32,
    /// Height of a menu control row (button, toggle, slider, select, text
    /// field). Menus M1 contract 0.
    #[serde(default = "default_control_height")]
    pub control_height: f32,
    /// Height of a control used inline or in a list row.
    #[serde(default = "default_control_height_compact")]
    pub control_height_compact: f32,
    /// Edge length of a dialogue portrait (menus M3 contract 4.1).
    #[serde(default = "default_portrait")]
    pub portrait: f32,
}

const fn default_portrait() -> f32 {
    64.0
}

/// The dialogue runner's pacing (menus M3 contract 1.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueTokens {
    /// Typewriter speed; `0` reveals every line at once.
    #[serde(default = "default_chars_per_second")]
    pub chars_per_second: u32,
    /// Milliseconds after a line's reveal before its choices show.
    #[serde(default = "default_choice_delay")]
    pub choice_delay: u32,
}

const fn default_chars_per_second() -> u32 {
    40
}

const fn default_choice_delay() -> u32 {
    150
}

impl Default for DialogueTokens {
    fn default() -> Self {
        Self {
            chars_per_second: default_chars_per_second(),
            choice_delay: default_choice_delay(),
        }
    }
}

const fn default_control_height() -> f32 {
    44.0
}

const fn default_control_height_compact() -> f32 {
    28.0
}

/// One entry of the type scale (menus M1 contract 1.4): what a `$name` text
/// size resolves to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeStyle {
    /// Font size in px.
    pub size: f32,
    /// Name of an entry in `tokens.fonts`.
    #[serde(default)]
    pub font: Option<String>,
    /// Variable-font weight, 100 to 900.
    #[serde(default)]
    pub weight: Option<u16>,
    /// Line height as a multiple of the size.
    #[serde(default)]
    pub line_height: Option<f32>,
}

const fn default_slot_size() -> f32 {
    44.0
}
const fn default_panel_width() -> f32 {
    352.0
}
const fn default_card_width() -> f32 {
    75.0
}
const fn default_card_height() -> f32 {
    94.0
}
const fn default_chrome_height() -> f32 {
    190.0
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            slot_size: default_slot_size(),
            slot_gap: None,
            panel_width: default_panel_width(),
            card_width: default_card_width(),
            card_height: default_card_height(),
            chrome_height: default_chrome_height(),
            control_height: default_control_height(),
            control_height_compact: default_control_height_compact(),
            portrait: default_portrait(),
        }
    }
}

/// The whole table.
///
/// [`Tokens::default`] is the `glass` theme's numbers with empty colour maps:
/// what a widget lays itself out with before any theme asset has loaded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    /// Spacing scale.
    pub spacing: Spacing,
    /// Widget sizes. A theme that names none gets the built-in numbers.
    #[serde(default)]
    pub sizes: Sizes,
    /// Corner radii.
    pub radii: Radii,
    /// Named shadow levels (`low`, `mid`, `high` by convention).
    pub elevation: BTreeMap<String, Elevation>,
    /// Motion durations: the three tiers every preset falls back to.
    pub durations: Durations,
    /// Per-preset motion overrides and the theme's easing.
    #[serde(default)]
    pub motion: MotionTokens,
    /// Named fonts that `Material::Text { font }` refers to (`body`, `mono`,
    /// `display` by convention).
    #[serde(default)]
    pub fonts: BTreeMap<String, FontToken>,
    /// The type scale: `display`, `title`, `heading`, `body`, `label`,
    /// `caption` by convention, referenced as `size: "$body"`.
    #[serde(default)]
    pub typography: BTreeMap<String, TypeStyle>,
    /// Backdrop blur.
    pub blur: Blur,
    /// Named colours that materials reference with `$name`.
    pub palette: BTreeMap<String, ThemeColor>,
    /// Rarity ring colours keyed by `slotted_registry::Rarity::as_str()`.
    pub rarity: BTreeMap<String, ThemeColor>,
    /// Dialogue pacing (menus M3).
    #[serde(default)]
    pub dialogue: DialogueTokens,
}

impl Durations {
    /// How long a slot must be hovered before its tooltip is composed.
    pub const fn hover_delay_ms(&self) -> u32 {
        self.hover_delay
    }

    /// The tier a preset falls into when the theme gives it no override.
    pub const fn tier_ms(&self, preset: MotionPreset) -> u32 {
        match preset {
            MotionPreset::Hover | MotionPreset::Press => self.fast,
            MotionPreset::DropSquash
            | MotionPreset::Fade
            | MotionPreset::FlyToSlot
            | MotionPreset::Slide => self.normal,
            MotionPreset::Stagger => self.slow,
        }
    }
}

impl Tokens {
    /// Duration of `preset` in milliseconds: the per-preset override when the
    /// theme has one, else its tier in [`Durations`].
    pub fn duration_ms(&self, preset: MotionPreset) -> u32 {
        self.motion
            .presets
            .get(&preset)
            .map_or_else(|| self.durations.tier_ms(preset), |spec| spec.duration)
    }

    /// Easing of `preset`: its own, else the theme's.
    pub fn easing(&self, preset: MotionPreset) -> Easing {
        self.motion
            .presets
            .get(&preset)
            .and_then(|spec| spec.easing)
            .unwrap_or(self.motion.easing)
    }

    /// The gap between slots in a grid: the theme's [`Sizes::slot_gap`] when
    /// it names one, else the spacing scale's `sm` step.
    pub fn slot_gap(&self) -> f32 {
        self.sizes.slot_gap.unwrap_or(self.spacing.sm)
    }

    /// The font token a text role names, if the theme defines it.
    pub fn font(&self, name: &str) -> Option<&FontToken> {
        self.fonts.get(name)
    }
}
