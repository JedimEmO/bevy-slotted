//! What an item looks like: [`IconDef`], the `icon` field of an
//! [`ItemDef`](crate::defs::ItemDef).
//!
//! Three shapes, in the order a data file is most likely to use them:
//!
//! - `Image`, a plain asset path. This is the Phase 2 form and it still
//!   parses: `icon: Some("icons/sword.png")`.
//! - `Shape`, a lit primitive baked into the icon atlas. This is what the
//!   demo data uses, so the shipped examples have real 3D icons without
//!   shipping a single texture:
//!   `icon: Some((shape: "ingot", color: "#c98a4b", metallic: 0.9))`.
//! - `Model`, a glTF path, optionally with the shape fields beside it:
//!   `icon: Some((model: "models/pickaxe.gltf", shape: "rod", color: "#6b4c33"))`.
//!   The GPU bake loads the glTF scene and lights it under the same rig; the
//!   shape fields describe the stand-in the CPU bake draws when there is no
//!   renderer, and they also decide the view angle and the cell fill the
//!   model is rendered at, so the two bakes agree about how the item sits.
//!
//! The three are told apart by shape rather than by a tag, so RON, JSON and a
//! Lua table all write the same thing: a string is an image, a map with a
//! `shape` key is a shape, a map with a `model` key is a model.

use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A colour in the icon format: sRGB components with alpha, each `0..=1`.
///
/// Written as `"#rrggbb"` or `"#rrggbbaa"`. It is deliberately not a Bevy
/// `Color`: this crate has no Bevy dependency, and the bake converts once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconColor(pub [f32; 4]);

impl IconColor {
    /// Opaque, from 8-bit sRGB channels.
    #[must_use]
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self([
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ])
    }

    /// The four components as 8-bit sRGB.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn to_u8(self) -> [u8; 4] {
        std::array::from_fn(|i| (self.0[i].clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// `#rrggbb`, or `#rrggbbaa` when the colour is not opaque.
    #[must_use]
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.to_u8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa`, with or without the `#`.
    ///
    /// # Errors
    ///
    /// When the string is not one of those three lengths of hex digits.
    pub fn parse(raw: &str) -> Result<Self, IconColorError> {
        let hex = raw.strip_prefix('#').unwrap_or(raw);
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(IconColorError(raw.to_owned()));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        match hex.len() {
            3 => {
                let nibble = |i: usize| {
                    let v = u8::from_str_radix(&hex[i..=i], 16).unwrap_or(0);
                    v * 17
                };
                Ok(Self::rgb(nibble(0), nibble(1), nibble(2)))
            }
            6 => Ok(Self::rgb(byte(0), byte(2), byte(4))),
            8 => {
                let mut color = Self::rgb(byte(0), byte(2), byte(4));
                color.0[3] = f32::from(byte(6)) / 255.0;
                Ok(color)
            }
            _ => Err(IconColorError(raw.to_owned())),
        }
    }
}

/// A colour string that is not `#rgb`, `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a colour: expected #rgb, #rrggbb or #rrggbbaa")]
pub struct IconColorError(pub String);

impl Serialize for IconColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for IconColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// The primitives the icon bake can render. Chosen to cover a Minecraft-ish
/// item list: blocks, slabs, bars, gems, sticks and pearls.
/// Serialised as a lowercase string (`"ingot"`) rather than a RON enum
/// variant, for the same reason [`Rarity`](crate::defs::Rarity) is: it has to
/// survive the [`Value`](crate::Value) round trip the patch stage does, and a
/// Lua table can only write a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
#[serde(into = "&'static str")]
pub enum ShapeKind {
    /// A full block. Cobblestone, dirt, a chest.
    #[default]
    Cube,
    /// A half-height block. Planks, slabs, plates.
    Slab,
    /// A flat, tapered bar. Ingots.
    Ingot,
    /// An octahedron. Diamonds and other gems.
    Gem,
    /// A thin shaft, optionally with an `accent`-coloured head. Tools.
    Rod,
    /// A ball. Dusts, pearls, pellets.
    Sphere,
}

impl ShapeKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 6] = [
        Self::Cube,
        Self::Slab,
        Self::Ingot,
        Self::Gem,
        Self::Rod,
        Self::Sphere,
    ];

    /// The name this kind is written under in data.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cube => "cube",
            Self::Slab => "slab",
            Self::Ingot => "ingot",
            Self::Gem => "gem",
            Self::Rod => "rod",
            Self::Sphere => "sphere",
        }
    }
}

/// The string in a data file was not one of the six shapes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown icon shape `{0}`, expected one of cube, slab, ingot, gem, rod, sphere")]
pub struct UnknownShape(pub String);

impl std::str::FromStr for ShapeKind {
    type Err = UnknownShape;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| UnknownShape(s.to_owned()))
    }
}

impl fmt::Display for ShapeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ShapeKind> for &'static str {
    fn from(kind: ShapeKind) -> Self {
        kind.as_str()
    }
}

impl<'de> Deserialize<'de> for ShapeKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// A lit primitive, baked into the icon atlas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeIcon {
    /// Which primitive.
    #[serde(default)]
    pub shape: ShapeKind,
    /// The body colour.
    pub color: IconColor,
    /// A second colour for the part of the shape that reads as a head, a
    /// binding or an inlay. `Rod` puts it on the head; the others put it on a
    /// band across the front face. `None` means "one colour".
    #[serde(default)]
    pub accent: Option<IconColor>,
    /// 0 is a dielectric, 1 is bare metal.
    #[serde(default)]
    pub metallic: f32,
    /// 0 is a mirror, 1 is chalk.
    #[serde(default = "default_roughness")]
    pub roughness: f32,
}

fn default_roughness() -> f32 {
    0.55
}

impl ShapeIcon {
    /// A shape with default material parameters.
    #[must_use]
    pub fn new(shape: ShapeKind, color: IconColor) -> Self {
        Self {
            shape,
            color,
            accent: None,
            metallic: 0.0,
            roughness: default_roughness(),
        }
    }
}

/// How an item is drawn. The `icon` field of an
/// [`ItemDef`](crate::defs::ItemDef).
#[derive(Debug, Clone, PartialEq)]
pub enum IconDef {
    /// A ready-made image, by asset path.
    Image(String),
    /// A primitive the icon bake lights and renders.
    Shape(ShapeIcon),
    /// A glTF model, by asset path.
    Model {
        /// Asset path of the `.gltf` or `.glb`.
        path: String,
        /// What the CPU bake draws in its place, and the shape whose view
        /// angle and cell fill the GPU bake renders the model at.
        ///
        /// `None` means "a cube in a colour hashed from the path", which is
        /// what [`ShapeIcon`] the icon bake substitutes; a data file that
        /// cares gives the real silhouette so a headless run and a rendered
        /// run read the same way.
        stand_in: Option<ShapeIcon>,
    },
}

impl IconDef {
    /// The asset path an `Image` or `Model` names.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Image(path) | Self::Model { path, .. } => Some(path),
            Self::Shape(_) => None,
        }
    }

    /// The shape, when this is one.
    #[must_use]
    pub const fn shape(&self) -> Option<&ShapeIcon> {
        match self {
            Self::Shape(shape) => Some(shape),
            _ => None,
        }
    }

    /// The shape a `Model` declared as its stand-in, when it declared one.
    #[must_use]
    pub const fn stand_in(&self) -> Option<&ShapeIcon> {
        match self {
            Self::Model { stand_in, .. } => stand_in.as_ref(),
            _ => None,
        }
    }
}

impl Serialize for IconDef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Image(path) => serializer.serialize_str(path),
            Self::Shape(shape) => shape.serialize(serializer),
            Self::Model { path, stand_in } => {
                let mut map =
                    serializer.serialize_map(Some(if stand_in.is_some() { 5 } else { 1 }))?;
                map.serialize_entry("model", path)?;
                // The stand-in is written flat beside `model`, which is how
                // it parses: a model map is a shape map with a path added,
                // not a shape map nested inside one.
                if let Some(shape) = stand_in {
                    map.serialize_entry("shape", &shape.shape)?;
                    map.serialize_entry("color", &shape.color)?;
                    // The `Option` itself, not its contents: the visitor reads
                    // `accent` as an `Option<IconColor>`, and RON without
                    // `implicit_some` writes and expects `Some("#rrggbb")`.
                    map.serialize_entry("accent", &shape.accent)?;
                    map.serialize_entry("metallic", &shape.metallic)?;
                    map.serialize_entry("roughness", &shape.roughness)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for IconDef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IconVisitor;

        impl<'de> Visitor<'de> for IconVisitor {
            type Value = IconDef;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an image path, a shape map with a `shape` key, or `(model: \"..\")`")
            }

            fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<Self::Value, E> {
                Ok(IconDef::Image(raw.to_owned()))
            }

            fn visit_string<E: serde::de::Error>(self, raw: String) -> Result<Self::Value, E> {
                Ok(IconDef::Image(raw))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                use serde::de::Error as _;

                let mut shape: Option<ShapeKind> = None;
                let mut color: Option<IconColor> = None;
                let mut accent: Option<IconColor> = None;
                let mut metallic: Option<f32> = None;
                let mut roughness: Option<f32> = None;
                let mut image: Option<String> = None;
                let mut model: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "shape" | "kind" => shape = Some(map.next_value()?),
                        "color" | "colour" => color = Some(map.next_value()?),
                        "accent" => accent = map.next_value()?,
                        "metallic" => metallic = Some(map.next_value()?),
                        "roughness" => roughness = Some(map.next_value()?),
                        "image" | "path" => image = Some(map.next_value()?),
                        "model" => model = Some(map.next_value()?),
                        other => {
                            return Err(A::Error::unknown_field(
                                other,
                                &["shape", "color", "accent", "metallic", "roughness", "model"],
                            ));
                        }
                    }
                }
                if let Some(model) = model {
                    // A `color` beside `model` is the stand-in; without one
                    // there is nothing to describe and the bake substitutes a
                    // cube in the path's hash colour.
                    return Ok(IconDef::Model {
                        path: model,
                        stand_in: color.map(|color| ShapeIcon {
                            shape: shape.unwrap_or_default(),
                            color,
                            accent,
                            metallic: metallic.unwrap_or(0.0),
                            roughness: roughness.unwrap_or_else(default_roughness),
                        }),
                    });
                }
                if let Some(image) = image {
                    return Ok(IconDef::Image(image));
                }
                let color = color.ok_or_else(|| A::Error::missing_field("color"))?;
                Ok(IconDef::Shape(ShapeIcon {
                    shape: shape.unwrap_or_default(),
                    color,
                    accent,
                    metallic: metallic.unwrap_or(0.0),
                    roughness: roughness.unwrap_or_else(default_roughness),
                }))
            }
        }

        deserializer.deserialize_any(IconVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_bare_string_is_still_an_image_path() {
        let icon: IconDef = ron::from_str(r#""icons/sword.png""#).expect("parses");
        assert_eq!(icon, IconDef::Image("icons/sword.png".to_owned()));
    }

    #[test]
    fn a_shape_map_parses_from_ron() {
        let icon: IconDef = ron::from_str(r##"(shape: "ingot", color: "#c98a4b", metallic: 0.9)"##)
            .expect("parses");
        let shape = icon.shape().expect("a shape");
        assert_eq!(shape.shape, ShapeKind::Ingot);
        assert_eq!(shape.color, IconColor::rgb(0xc9, 0x8a, 0x4b));
        assert!((shape.metallic - 0.9).abs() < f32::EPSILON);
        // The default is a mid roughness, not zero.
        assert!(shape.roughness > 0.0);
    }

    #[test]
    fn a_model_map_parses_and_keeps_its_path() {
        let icon: IconDef = ron::from_str(r#"(model: "models/anvil.gltf")"#).expect("parses");
        assert_eq!(
            icon,
            IconDef::Model {
                path: "models/anvil.gltf".to_owned(),
                stand_in: None,
            }
        );
        assert_eq!(icon.path(), Some("models/anvil.gltf"));
        assert_eq!(icon.stand_in(), None);
    }

    #[test]
    fn a_model_can_carry_the_shape_the_cpu_bake_draws_instead() {
        let icon: IconDef = ron::from_str(
            r##"(model: "models/pickaxe.gltf", shape: "rod", color: "#6b4c33", accent: Some("#d0d6dd"))"##,
        )
        .expect("parses");
        let stand_in = icon.stand_in().expect("a stand-in");
        assert_eq!(stand_in.shape, ShapeKind::Rod);
        assert_eq!(stand_in.color, IconColor::rgb(0x6b, 0x4c, 0x33));
        assert_eq!(stand_in.accent, Some(IconColor::rgb(0xd0, 0xd6, 0xdd)));
        assert_eq!(icon.path(), Some("models/pickaxe.gltf"));
    }

    #[test]
    fn a_model_with_a_stand_in_round_trips_through_ron() {
        let icon = IconDef::Model {
            path: "models/pickaxe.gltf".to_owned(),
            stand_in: Some(ShapeIcon {
                shape: ShapeKind::Rod,
                color: IconColor::rgb(0x6b, 0x4c, 0x33),
                accent: Some(IconColor::rgb(0xd0, 0xd6, 0xdd)),
                metallic: 0.3,
                roughness: 0.4,
            }),
        };
        let text = ron::to_string(&icon).expect("serialises");
        let back: IconDef = ron::from_str(&text).expect("parses back");
        assert_eq!(back, icon);
    }

    #[test]
    fn colours_round_trip_through_hex() {
        let color = IconColor::parse("#8a8f96").expect("parses");
        assert_eq!(color.to_hex(), "#8a8f96");
        assert_eq!(
            IconColor::parse("#abc").expect("short"),
            IconColor::rgb(0xaa, 0xbb, 0xcc)
        );
        assert!(IconColor::parse("crimson").is_err());
        assert!(
            (IconColor::parse("#8a8f9680").expect("alpha").0[3] - 128.0 / 255.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn a_shape_round_trips_through_ron() {
        let icon = IconDef::Shape(ShapeIcon {
            shape: ShapeKind::Rod,
            color: IconColor::rgb(0x6b, 0x4c, 0x33),
            accent: Some(IconColor::rgb(0xd0, 0xd6, 0xdd)),
            metallic: 0.3,
            roughness: 0.4,
        });
        let text = ron::to_string(&icon).expect("serialises");
        let back: IconDef = ron::from_str(&text).expect("parses back");
        assert_eq!(back, icon);
    }
}
