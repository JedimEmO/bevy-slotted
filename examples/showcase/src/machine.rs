//! The machine demo, minus the window and minus the filesystem.
//!
//! Lifted out of `examples/machine/src/lib.rs` (Phase 6 contract section 3.3,
//! `docs/design/showcase-contract.md` section 1) so the windowed example, the
//! headless tests and the showcase's Machine scene drive the identical
//! simulation. What stayed behind in the example is what reads a directory:
//! `load_registries`, `furnace_screen`, `layout`, `assets_dir` and
//! `mods_dir`.
//!
//! A furnace-like screen: input, fuel and output slots, a water tank, an
//! energy bar, two progress arrows, and two side tabs (redstone mode, side
//! configuration). [`machine_sim`] advances the machine on virtual time through
//! `SetProperty` and `SetSlot`, so the same code runs in the window and under
//! `slotted_test::UiHarness`. The `sorter` mod in `mods/` injects a sort
//! button at `title_end` through the wildcard target `slotted:any`.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::prelude::*;
use slotted::theme::Role;
use slotted::ui::{
    IconButtonStateDef, IconDef, Layout, LayoutDirection, LocKey, SLOT_SIZE, SpawnCtx, Tags,
    UiNodeDef, Widget, WidgetRegistry,
};
use slotted_model::{Inventory, MenuDef, PropertyDef, PropertyId, SlotBehaviour};
use slotted_registry::{FrozenRegistries, Value};

/// The compiled-in `machine:furnace` screen.
///
/// The windowed example still reads `screens/furnace.screen.ron` off disk so
/// it hot-reloads; this parses the same bytes out of the binary, which is what
/// a browser tab and a scene switch need.
pub fn screen() -> ScreenDef {
    crate::screens::parse("furnace", crate::screens::FURNACE_SCREEN_RON)
}

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

/// The mod id the shared demo content is loaded under: `assets/data/demo/`.
pub const DEMO_MOD: &str = "demo";

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

/// One seeded stack: which inventory, which slot, what, how many.
struct Row {
    inventory: usize,
    slot: usize,
    item: &'static str,
    count: u32,
}

const fn row(inventory: usize, slot: usize, item: &'static str, count: u32) -> Row {
    Row {
        inventory,
        slot,
        item,
        count,
    }
}

/// Raw material in the input, coal in the fuel slot, and a player inventory
/// left deliberately unsorted for the `sorter` mod to tidy.
const CONTENTS: &[Row] = &[
    row(0, 0, "minecraft:cobblestone", 32),
    row(0, 1, "minecraft:coal", 12),
    row(1, 0, "minecraft:iron_ingot", 5),
    row(1, 4, "minecraft:cobblestone", 9),
    row(1, 11, "minecraft:iron_ingot", 17),
    row(1, 20, "minecraft:oak_planks", 40),
    row(2, 0, "minecraft:iron_pickaxe", 1),
    row(2, 2, "minecraft:diamond", 3),
];

/// The demo's inventories, built against the registries the data stage
/// produced.
///
/// A row naming an id the data files did not register is skipped with a
/// warning rather than a panic: in a browser tab a panic is a blank canvas and
/// no way to find out why.
pub fn inventories(registries: &FrozenRegistries) -> Vec<Inventory> {
    let mut out: Vec<Inventory> = menu_def()
        .inventory_sizes()
        .into_iter()
        .map(Inventory::new)
        .collect();
    for row in CONTENTS {
        let name = slotted_model::Namespaced::parse(row.item)
            .unwrap_or_else(|e| panic!("bad id {:?} in the machine table: {e}", row.item));
        let Some(id) = registries.item_id(&name) else {
            warn!("{name} is not in assets/data/demo/items; skipping that stack");
            continue;
        };
        out[row.inventory].set(row.slot, Some(slotted_model::ItemStack::new(id, row.count)));
    }
    out
}

/// Whether the machine's redstone signal is on. `R` toggles it.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Redstone(pub bool);

/// Whether the furnace simulation advances.
///
/// A test that wants the screen frozen at the property values
/// [`menu_def`] declares sets `paused` before opening the screen: with the
/// simulation off, every readout on the screen is the value in the fixture
/// and a tree snapshot does not depend on how many frames `settle()` ran.
/// The windowed example leaves it running.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MachineSim {
    /// `true` stops [`machine_sim`] before it reads the clock.
    pub paused: bool,
}

/// The menu the simulation drives. Set by [`track_machine_menu`] as soon as a
/// `machine:furnace` screen appears, whoever opened it: the window's startup
/// system or a test harness.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineMenu(pub Entity);

/// Cook units per second. `cook_max` is 200, so an item takes two seconds.
pub const COOK_PER_SECOND: f32 = 100.0;
/// Fuel units per second. `burn_max` is 1600, so one coal lasts eight.
pub const BURN_PER_SECOND: f32 = 200.0;
/// Energy drained per second while cooking.
pub const ENERGY_PER_SECOND: f32 = 120.0;
/// Millibuckets drained per second while cooking.
pub const TANK_PER_SECOND: f32 = 40.0;

/// Sub-unit remainders, so a rate that is not a whole number of units per
/// frame still adds up to that rate over a second. Only the fraction lives
/// here; the whole part is written to the property, which stays the one
/// place the value is kept.
#[derive(Debug, Default, Clone, Copy)]
pub struct SimCarry {
    cook: f32,
    burn: f32,
    energy: f32,
    tank: f32,
}

/// Adds `rate * dt` to `carry` and returns the whole units that came out.
///
/// The cast is exact by construction: `whole` is a non-negative integral
/// `f32` no larger than one frame of one rate, and every rate here is a few
/// hundred units per second.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn tick(carry: &mut f32, rate: f32, dt: f32) -> i32 {
    *carry += rate * dt;
    let whole = carry.floor();
    *carry -= whole;
    whole as i32
}

/// `Startup`-independent binding: the first `machine:furnace` screen to
/// appear names the menu the simulation drives.
pub fn track_machine_menu(roots: Query<&ScreenRoot, Added<ScreenRoot>>, mut commands: Commands) {
    for root in &roots {
        if root.kind == ScreenKind::new(FURNACE)
            && let Some(menu) = root.menu
        {
            commands.insert_resource(MachineMenu(menu));
        }
    }
}

/// Advances the furnace on `Time<Virtual>`: burns fuel, cooks input into
/// output, drains energy and the tank. Everything goes through
/// `slotted_ecs::SetProperty` and `SetSlot`; the sim is the authority, so it
/// never touches an `Inventory` or a `MenuProperty` itself.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
pub fn machine_sim(
    time: Res<Time<Virtual>>,
    sim: Res<MachineSim>,
    menu: Option<Res<MachineMenu>>,
    redstone: Res<Redstone>,
    menus: Query<&OpenMenu>,
    inventories: Query<&slotted::ecs::Inventory>,
    mut carry: Local<SimCarry>,
    mut commands: Commands,
) {
    if sim.paused {
        return;
    }
    let Some(entity) = menu.map(|m| m.0) else {
        return;
    };
    let Ok(open) = menus.get(entity) else { return };
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let read = |id: PropertyId| -> i32 {
        open.def
            .properties
            .iter()
            .position(|p| p.id == id)
            .and_then(|i| open.state.properties.get(i).copied())
            .unwrap_or(0)
    };
    let stack_of = |slot: slotted_model::SlotIx| -> Option<slotted_model::ItemStack> {
        let def = open.def.slot(slot)?;
        let inventory = *open.inventories.get(def.source.index())?;
        inventories
            .get(inventory)
            .ok()?
            .get(usize::from(def.index))
            .cloned()
    };
    let mut writes: Vec<(PropertyId, i32)> = Vec::new();
    let mut set = |id: PropertyId, value: i32| writes.push((id, value));

    let mut burn = read(props::BURN);
    let mut cook = read(props::COOK);
    let cook_max = read(props::COOK_MAX).max(1);
    let burn_max = read(props::BURN_MAX).max(1);

    // The redstone gate: 0 ignore, 1 run while the signal is off, 2 run while
    // it is on. Anything else is treated as ignore rather than as "off", so a
    // property a mod has not set yet does not stop the machine.
    let gate = match read(props::REDSTONE_MODE) {
        1 => !redstone.0,
        2 => redstone.0,
        _ => true,
    };

    let input = stack_of(slots::INPUT);
    let output = stack_of(slots::OUTPUT);
    let output_has_room = match (&input, &output) {
        (Some(input), Some(output)) => input.same_kind(output) && output.count < 64,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let wants_to_run = gate && input.is_some() && output_has_room;

    // Burning. One fuel item buys `burn_max` units and the item is gone: the
    // machine is the authority over its own slots, so this is a `SetSlot`.
    if burn > 0 {
        burn = (burn - tick(&mut carry.burn, BURN_PER_SECOND, dt)).max(0);
        set(props::BURN, burn);
    } else if wants_to_run && let Some(fuel) = stack_of(slots::FUEL) {
        commands.trigger(SetSlot {
            entity,
            slot: slots::FUEL,
            stack: (fuel.count > 1).then(|| fuel.clone().with_count(fuel.count - 1)),
        });
        burn = burn_max;
        set(props::BURN, burn);
    }

    // Cooking. The result is the input item: a demo furnace with no recipe
    // table still has to conserve items, and the tests assert exactly that.
    if wants_to_run && burn > 0 {
        cook += tick(&mut carry.cook, COOK_PER_SECOND, dt);
        if cook >= cook_max {
            cook = 0;
            if let Some(input) = input {
                commands.trigger(SetSlot {
                    entity,
                    slot: slots::INPUT,
                    stack: (input.count > 1).then(|| input.clone().with_count(input.count - 1)),
                });
                let moved = match output {
                    Some(present) => present.clone().with_count(present.count + 1),
                    None => input.clone().with_count(1),
                };
                commands.trigger(SetSlot {
                    entity,
                    slot: slots::OUTPUT,
                    stack: Some(moved),
                });
            }
        }
        set(props::COOK, cook);

        let energy = (read(props::ENERGY) - tick(&mut carry.energy, ENERGY_PER_SECOND, dt)).max(0);
        set(props::ENERGY, energy);
        let tank = (read(props::TANK) - tick(&mut carry.tank, TANK_PER_SECOND, dt)).max(0);
        set(props::TANK, tank);
    } else if cook > 0 {
        // Progress falls back when the machine stops, as a furnace's does.
        cook = (cook - tick(&mut carry.cook, COOK_PER_SECOND, dt)).max(0);
        set(props::COOK, cook);
    }

    for (id, value) in writes {
        commands.trigger(SetProperty { entity, id, value });
    }
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

/// The six faces, in the order the 3x3 pad shows them: top row up, middle row
/// left / front / right, bottom row down and back.
const FACES: [(&str, usize, usize); 6] = [
    ("up", 0, 1),
    ("left", 1, 0),
    ("front", 1, 1),
    ("right", 1, 2),
    ("down", 2, 1),
    ("back", 2, 2),
];

/// What one face may be set to, in cycle order.
const FACE_MODES: [&str; 3] = ["none", "input", "output"];

impl Widget for FaceConfigWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let params: FaceConfigParams = match params {
            Value::Unit => FaceConfigParams::default(),
            other => slotted_registry::to_model(other)
                .map_err(|e| e.to_string())
                .and_then(|v| slotted_model::from_value(v).map_err(|e| e.to_string()))
                .unwrap_or_else(|e| {
                    tracing::warn!(%e, "machine:face_config params; using the defaults");
                    FaceConfigParams::default()
                }),
        };

        // Three rows of three, with an empty cell wherever no face sits: the
        // pad reads as a block seen from the front, which is the whole reason
        // this widget is not a plain row of buttons.
        let mut rows = Vec::with_capacity(3);
        for row in 0..3usize {
            let mut cells = Vec::with_capacity(3);
            for column in 0..3usize {
                cells.push(
                    match FACES
                        .iter()
                        .enumerate()
                        .find(|(_, (_, r, c))| *r == row && *c == column)
                    {
                        Some((face, (name, _, _))) => face_button(&params, face, name),
                        None => UiNodeDef::Panel {
                            role: Role::new_static("invisible"),
                            layout: Layout {
                                width: Some(SLOT_SIZE),
                                height: Some(SLOT_SIZE),
                                ..Layout::default()
                            },
                            children: Vec::new(),
                            tags: Tags::new(),
                        },
                    },
                );
            }
            rows.push(UiNodeDef::Panel {
                role: Role::new_static("invisible"),
                layout: Layout {
                    direction: LayoutDirection::Row,
                    gap: 0.5,
                    ..Layout::default()
                },
                children: cells,
                tags: Tags::new(),
            });
        }

        let mut tags = Tags::new();
        tags.0.insert("widget".to_owned(), "face_config".to_owned());
        ctx.spawn_child(&UiNodeDef::Panel {
            role: Role::new_static("panel"),
            layout: Layout {
                direction: LayoutDirection::Column,
                gap: 0.5,
                padding: 0.5,
                ..Layout::default()
            },
            children: rows,
            tags,
        })
    }
}

/// One face's icon button, bound to `first_property + face`.
fn face_button(params: &FaceConfigParams, face: usize, name: &str) -> UiNodeDef {
    let mut tags = Tags::new();
    tags.0.insert("test_id".to_owned(), format!("face_{name}"));
    tags.0.insert("face".to_owned(), name.to_owned());
    UiNodeDef::IconButton {
        states: FACE_MODES
            .iter()
            .map(|mode| IconButtonStateDef {
                id: (*mode).to_owned(),
                icon: IconDef::Image(format!("icons/face_{mode}.png")),
                label: LocKey(format!("machine.face.{mode}")),
            })
            .collect(),
        property: Some(PropertyId(
            params.first_property.0 + u16::try_from(face).unwrap_or(0),
        )),
        tags,
    }
}

/// Registers the face widget, the simulation and the `R` binding.
pub struct MachineDemoPlugin;

impl Plugin for MachineDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Redstone>()
            .init_resource::<MachineSim>()
            .add_systems(Startup, register_widgets)
            .add_systems(
                Update,
                (toggle_redstone, track_machine_menu, machine_sim).chain(),
            );
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
