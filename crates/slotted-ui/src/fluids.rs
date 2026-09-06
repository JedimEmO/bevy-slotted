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
    /// The colour as a Bevy colour; magenta when the literal does not parse.
    pub fn color(&self) -> Color {
        // PHASE6-IMPL: A. Parse `#RRGGBB[AA]` like `slotted_theme::ThemeColor`.
        let _ = &self.color;
        Color::srgb(1.0, 0.0, 1.0)
    }
}

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
        // PHASE6-IMPL: A. Deserialise each payload through
        // `slotted_registry::to_model` + `slotted_model::from_value`, filling
        // `name` from the registry name when absent.
        let _ = registries;
    }
}
