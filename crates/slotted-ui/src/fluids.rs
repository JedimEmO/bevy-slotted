//! Fluids a tank can hold (Phase 6 contract section 1.1).
//!
//! `FluidDef` is the typed form of the registry's `fluids` payload; [`Fluids`]
//! is indexed by the frozen id, so a menu property can name a fluid by its
//! dense index.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::Namespaced;

/// A fluid, as data (`data/<ns>/fluids/*.ron` or `slotted.register_fluid`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluidDef {
    /// Registry name.
    pub name: Namespaced,
    /// `#RRGGBB[AA]`. Tints the tank fill (solid or over the tiled texture).
    pub color: String,
    /// Asset path of a tile texture; `None` for a solid fill.
    #[serde(default)]
    pub texture: Option<String>,
    /// Unit for labels and tooltips.
    #[serde(default = "default_unit")]
    pub unit: String,
}

fn default_unit() -> String {
    "mB".to_owned()
}

impl FluidDef {
    /// Types one registry payload, defaulting `name` to the registry name.
    ///
    /// The payload travels untyped, so it reads the same whether it came from
    /// `data/<ns>/fluids/water.ron` or from a mod's `slotted.register_fluid`.
    ///
    /// # Errors
    ///
    /// The first field that did not fit, as a message.
    pub fn from_payload(
        name: &Namespaced,
        payload: &slotted_registry::Value,
    ) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            name: Option<Namespaced>,
            color: String,
            #[serde(default)]
            texture: Option<String>,
            #[serde(default = "default_unit")]
            unit: String,
        }
        let untyped = slotted_registry::to_model(payload).map_err(|e| e.to_string())?;
        let raw: Raw = slotted_model::from_value(untyped).map_err(|e| e.to_string())?;
        Ok(Self {
            name: raw.name.unwrap_or_else(|| name.clone()),
            color: raw.color,
            texture: raw.texture,
            unit: raw.unit,
        })
    }
}

impl FluidDef {
    /// The colour as a Bevy colour; magenta when the literal does not parse.
    ///
    /// Fluid colours are data, not theme roles: a fluid is a game object with
    /// its own identity, so `#3B7DD8B0` in `fluids/water.ron` is the fluid's
    /// colour the way its name is its name. The tank *well* around it is
    /// themed.
    pub fn color(&self) -> Color {
        slotted_theme::ThemeColor::hex(&self.color)
            .parse_hex()
            .unwrap_or(MISSING_FLUID_COLOR)
    }
}

/// What a fluid whose colour literal does not parse is painted with: the same
/// flat magenta a missing icon and a missing palette colour use.
pub const MISSING_FLUID_COLOR: Color = Color::srgb(1.0, 0.0, 1.0);

/// Dense index into [`Fluids`], the frozen registry id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FluidId(pub u32);

/// Every fluid the app knows, in frozen order.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Fluids(pub Vec<FluidDef>);

impl Fluids {
    /// Lookup by dense id.
    pub fn get(&self, id: FluidId) -> Option<&FluidDef> {
        self.0.get(id.0 as usize)
    }

    /// Lookup by name.
    pub fn id_of(&self, name: &Namespaced) -> Option<FluidId> {
        self.0
            .iter()
            .position(|f| &f.name == name)
            .and_then(|i| u32::try_from(i).ok())
            .map(FluidId)
    }

    /// Replaces the table from the registry's `fluids` payloads, in frozen
    /// order. Malformed entries are logged and skipped **without** shifting
    /// later ids (a placeholder magenta fluid keeps the slot).
    pub fn load_from_registry(&mut self, registries: &slotted_registry::FrozenRegistries) {
        let mut out = Vec::with_capacity(registries.fluids.len());
        for (_, name, raw) in registries.fluids.iter() {
            match FluidDef::from_payload(name, &raw.payload) {
                Ok(def) => out.push(def),
                Err(e) => {
                    tracing::warn!(%name, %e, "fluid payload is not a FluidDef");
                    // A malformed entry keeps its slot: `FluidId` is the
                    // registration index, and a property already holding id 3
                    // must not start naming a different fluid because id 2
                    // failed to parse.
                    out.push(FluidDef {
                        name: name.clone(),
                        color: String::new(),
                        texture: None,
                        unit: default_unit(),
                    });
                }
            }
        }
        self.0 = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_registry::registry::Registries;

    fn id(s: &str) -> Namespaced {
        Namespaced::parse(s).expect("well formed")
    }

    fn payload(text: &str) -> slotted_registry::Value {
        ron::from_str(text).expect("payload parses")
    }

    fn frozen(entries: &[(&str, &str)]) -> slotted_registry::FrozenRegistries {
        let mut registries = Registries::new();
        for (name, text) in entries {
            registries
                .fluids
                .insert(
                    id(name),
                    slotted_registry::defs::FluidDef {
                        name: id(name),
                        payload: payload(text),
                    },
                )
                .expect("a fresh name");
        }
        registries.freeze().expect("freezes").0
    }

    #[test]
    fn a_fluid_takes_its_name_from_the_registry_and_its_colour_from_the_payload() {
        let mut fluids = Fluids::default();
        fluids.load_from_registry(&frozen(&[(
            "machine:water",
            r##"(color: "#3B7DD8B0", unit: "mB")"##,
        )]));
        let water = fluids.get(FluidId(0)).expect("one fluid");
        assert_eq!(water.name, id("machine:water"));
        assert_eq!(water.unit, "mB");
        assert_eq!(fluids.id_of(&id("machine:water")), Some(FluidId(0)));
        assert_eq!(water.color(), Color::srgba_u8(0x3B, 0x7D, 0xD8, 0xB0));
    }

    #[test]
    fn a_malformed_fluid_keeps_its_id_so_later_fluids_do_not_shift() {
        let mut fluids = Fluids::default();
        fluids.load_from_registry(&frozen(&[
            ("machine:broken", "(no_colour_here: 1)"),
            ("machine:water", r##"(color: "#3B7DD8B0")"##),
        ]));
        assert_eq!(fluids.0.len(), 2);
        assert_eq!(fluids.id_of(&id("machine:water")), Some(FluidId(1)));
        assert_eq!(
            fluids.get(FluidId(0)).expect("the placeholder").color(),
            MISSING_FLUID_COLOR,
            "a fluid whose colour did not parse paints magenta"
        );
    }

    #[test]
    fn a_texture_less_fluid_defaults_its_unit() {
        let mut fluids = Fluids::default();
        fluids.load_from_registry(&frozen(&[("machine:lava", r##"(color: "#D2601A")"##)]));
        let lava = fluids.get(FluidId(0)).expect("one fluid");
        assert_eq!(lava.unit, "mB");
        assert_eq!(lava.texture, None);
    }
}
