//! The machine demo, minus the window. Phase 6 contract section 3.3.
//!
//! A furnace-like screen: input, fuel and output slots, a water tank, an
//! energy bar, two progress arrows, and two side tabs (redstone mode, side
//! configuration). [`MachineSim`] advances the machine on virtual time through
//! `SetProperty` and `SetSlot`, so the same code runs in the window and under
//! `slotted_test::UiHarness`. The `sorter` mod in `mods/` injects a sort
//! button at `title_end` through the wildcard target `slotted:any`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::*;
use slotted::ui::{SpawnCtx, UiNodeDef, Widget, WidgetRegistry};
use slotted_model::{Inventory, MenuDef, PropertyDef, PropertyId, SlotBehaviour};
use slotted_packs::PackLayout;
use slotted_registry::{FrozenRegistries, Value};

/// The screen this example opens.
pub const FURNACE: &str = "machine:furnace";

/// The mod id the demo content is loaded under: `assets/data/machine/`.
pub const MACHINE_MOD: &str = "machine";

/// Property ids, matching `screens/furnace.screen.ron`.
pub mod props {
    use slotted_model::PropertyId;

    /// Fuel remaining.
    pub const BURN: PropertyId = PropertyId(0);
    /// Fuel per item.
    pub const BURN_MAX: PropertyId = PropertyId(1);
    /// Cooking progress.
    pub const COOK: PropertyId = PropertyId(2);
    /// Ticks per item.
    pub const COOK_MAX: PropertyId = PropertyId(3);
    /// Stored energy.
    pub const ENERGY: PropertyId = PropertyId(4);
    /// Energy capacity.
    pub const ENERGY_MAX: PropertyId = PropertyId(5);
    /// Tank amount in mB.
    pub const TANK: PropertyId = PropertyId(6);
    /// Tank capacity in mB.
    pub const TANK_CAP: PropertyId = PropertyId(7);
    /// Frozen fluid id in the tank.
    pub const TANK_FLUID: PropertyId = PropertyId(8);
    /// `0` ignore, `1` low, `2` high.
    pub const REDSTONE_MODE: PropertyId = PropertyId(9);
    /// First of six face properties: `0` none, `1` input, `2` output.
    pub const FACES: PropertyId = PropertyId(10);
}

/// Slot ids.
pub mod slots {
    use slotted_model::SlotIx;

    /// Input.
    pub const INPUT: SlotIx = SlotIx(0);
    /// Fuel.
    pub const FUEL: SlotIx = SlotIx(1);
    /// Output.
    pub const OUTPUT: SlotIx = SlotIx(2);
    /// First player main slot.
    pub const PLAYER_FIRST: u16 = 3;
    /// First hotbar slot.
    pub const HOTBAR_FIRST: u16 = 30;
}

/// The workspace's shared `assets/` directory.
pub fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// `examples/machine/mods/`.
pub fn mods_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("mods")
}

/// The pack layout: workspace assets plus this example's mods.
///
/// # Panics
///
/// When a manifest is malformed: a broken checkout.
pub fn layout(mods_dir: &Path) -> PackLayout {
    PackLayout::new(assets_dir())
        .with_mods(mods_dir)
        .unwrap_or_else(|e| panic!("discovering {}: {e}", mods_dir.display()))
}

/// Reads `screens/furnace.screen.ron`.
///
/// # Panics
///
/// If the file is missing or does not parse: a broken checkout.
pub fn furnace_screen() -> ScreenDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("screens/furnace.screen.ron");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    ScreenDef::from_ron(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Runs the data stage over `assets/data/demo/` and `assets/data/machine/`.
pub fn load_registries() -> Arc<FrozenRegistries> {
    // PHASE6-IMPL: C. As `chest::load_registries` plus the machine's fluid.
    Arc::new(
        slotted_registry::Registries::new()
            .freeze()
            .expect("empty registries freeze")
            .0,
    )
}

/// The menu: three machine slots (output is `Output`), player main and
/// hotbar, sixteen properties at their initial values.
pub fn menu_def() -> Arc<MenuDef> {
    let mut def = MenuDef::generic(3);
    def.slots[usize::from(slots::OUTPUT.0)].behaviour = SlotBehaviour::Output;
    let initial = |id: PropertyId, value: i32| PropertyDef { id, initial: value };
    def.properties = vec![
        initial(props::BURN, 0),
        initial(props::BURN_MAX, 1600),
        initial(props::COOK, 0),
        initial(props::COOK_MAX, 200),
        initial(props::ENERGY, 6400),
        initial(props::ENERGY_MAX, 10000),
        initial(props::TANK, 3000),
        initial(props::TANK_CAP, 8000),
        initial(props::TANK_FLUID, 0),
        initial(props::REDSTONE_MODE, 0),
    ];
    for face in 0..6u16 {
        def.properties
            .push(initial(PropertyId(props::FACES.0 + face), 0));
    }
    Arc::new(def)
}

/// The demo's inventories: raw ore in the input, coal in the fuel slot, a
/// deliberately unsorted player inventory for the sorter mod to tidy.
pub fn inventories(registries: &FrozenRegistries) -> Vec<Inventory> {
    // PHASE6-IMPL: C.
    let _ = registries;
    vec![Inventory::new(3), Inventory::new(27), Inventory::new(9)]
}

/// Whether the machine's redstone signal is on. `R` toggles it.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Redstone(pub bool);

/// The menu the simulation drives.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineMenu(pub Entity);

/// Advances the furnace on `Time<Virtual>`: burns fuel, cooks input into
/// output, drains energy and the tank. Everything goes through
/// `slotted_ecs::SetProperty` and `SetSlot`; the sim is the authority.
pub fn machine_sim(
    time: Res<Time<Virtual>>,
    menu: Option<Res<MachineMenu>>,
    redstone: Res<Redstone>,
    menus: Query<&OpenMenu>,
    mut commands: Commands,
) {
    // PHASE6-IMPL: C. Read the properties from `OpenMenu.state`, apply the
    // redstone gate (0 ignore, 1 needs signal off, 2 needs signal on), and
    // write back with `SetProperty` / `SetSlot`.
    let _ = (&time, &menu, &redstone, &menus, &mut commands);
}

/// `machine:face_config`: a 3x3 panel of six icon buttons cycling
/// `none / input / output`, each bound to `first_property + face`. Local to
/// the example because a generic face widget needs block orientation data
/// the ui crate does not have.
#[derive(Debug, Default, Clone, Copy)]
pub struct FaceConfigWidget;

/// Parameters of `machine:face_config`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FaceConfigParams {
    /// Property of face `0`; faces `0..6` follow.
    pub first_property: PropertyId,
}

impl Default for FaceConfigParams {
    fn default() -> Self {
        Self {
            first_property: props::FACES,
        }
    }
}

impl Widget for FaceConfigWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        // PHASE6-IMPL: C. Build a `Panel` grid of `IconButton` defs with
        // `property: Some(first + face)` and spawn it through `ctx.spawn_child`.
        let _ = params;
        ctx.spawn_node((
            Node::default(),
            SemanticRole::Custom("face_config".to_owned()),
        ))
    }
}

/// Registers the face widget, the simulation and the `R` binding.
pub struct MachineDemoPlugin;

impl Plugin for MachineDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Redstone>()
            .add_systems(Startup, register_widgets)
            .add_systems(Update, (toggle_redstone, machine_sim));
    }
}

fn register_widgets(mut registry: ResMut<WidgetRegistry>) {
    registry.register(WidgetKind::new("machine:face_config"), FaceConfigWidget);
}

fn toggle_redstone(keys: Res<ButtonInput<KeyCode>>, mut redstone: ResMut<Redstone>) {
    if keys.just_pressed(KeyCode::KeyR) {
        redstone.0 = !redstone.0;
    }
}
