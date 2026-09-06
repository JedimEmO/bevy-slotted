//! `tank`: a property-driven fluid well. Phase 6 contract section 1.2.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::{Namespaced, PropertyId};

use crate::def::{Orientation, Tags};
use crate::fluids::FluidId;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticRole, WidgetNode};

/// Current fill of a tank or bar, in the property's own units. The one
/// render-free fact the harness asserts on.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct FillValue {
    /// Current amount.
    pub value: f32,
    /// Capacity or maximum; a zero max reads as an empty fill.
    pub max: f32,
}

impl FillValue {
    /// `value / max` clamped to `0..=1`; `0` when `max <= 0`.
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.value / self.max).clamp(0.0, 1.0)
        }
    }
}

/// Which two `MenuProperty`s feed a node's [`FillValue`]. Present only when
/// the node was spawned inside a menu screen.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyBinding {
    /// The `OpenMenu` entity.
    pub menu: Entity,
    /// Property giving the value.
    pub value: PropertyId,
    /// Property giving the maximum.
    pub max: PropertyId,
}

/// The fluid a tank resolved, if any.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TankFluid(pub Option<FluidId>);

/// How a tank picks its fluid.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TankFluidSource {
    /// Static fluid name.
    pub fluid: Option<Namespaced>,
    /// Property whose value is the frozen fluid id; wins over `fluid`.
    pub fluid_property: Option<PropertyId>,
    /// Unit shown in labels.
    pub unit: String,
}

/// Marks a node whose hover shows a widget tooltip through `Widget::tooltip`.
/// `tooltip_delay` treats it like a slot.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TooltipSource;

/// The fill child of a tank or bar. Purely visual: no `SemanticRole`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FillNode;

/// Parameters of `slotted:tank`, the same fields as `UiNodeDef::Tank`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TankParams {
    /// Amount property.
    pub property: PropertyId,
    /// Capacity property.
    pub capacity: PropertyId,
    /// Fill direction.
    #[serde(default = "vertical")]
    pub orientation: Orientation,
    /// Static fluid.
    #[serde(default)]
    pub fluid: Option<Namespaced>,
    /// Fluid id property.
    #[serde(default)]
    pub fluid_property: Option<PropertyId>,
    /// Unit.
    #[serde(default = "mb")]
    pub unit: String,
}

fn vertical() -> Orientation {
    Orientation::Vertical
}

fn mb() -> String {
    "mB".to_owned()
}

impl Default for TankParams {
    fn default() -> Self {
        Self {
            property: PropertyId(0),
            capacity: PropertyId(1),
            orientation: Orientation::Vertical,
            fluid: None,
            fluid_property: None,
            unit: mb(),
        }
    }
}

/// Spawns a tank. Root components per contract 1.2; the fill child is a
/// [`FillNode`].
pub fn spawn_tank(ctx: &mut SpawnCtx<'_>, params: &TankParams, _tags: &Tags) -> Entity {
    // PHASE6-IMPL: A. Root: Node (SLOT_SIZE x 3*SLOT_SIZE or transposed),
    // Themed(TANK), SemanticRole::Tank, SemanticLabel, FillValue,
    // PropertyBinding when ctx.menu is Some, TankFluid, TankFluidSource,
    // Hovered, Pickable, TooltipSource, on_slot_over observer; child FillNode
    // with Themed(TANK_FILL) anchored to the fill origin.
    let binding = ctx.menu.map(|menu| PropertyBinding {
        menu,
        value: params.property,
        max: params.capacity,
    });
    let entity = ctx.spawn_node((
        Node::default(),
        SemanticRole::Tank,
        WidgetNode(crate::widgets::kinds::tank()),
        FillValue::default(),
        TankFluid::default(),
        TankFluidSource {
            fluid: params.fluid.clone(),
            fluid_property: params.fluid_property,
            unit: params.unit.clone(),
        },
        TooltipSource,
    ));
    if let Some(binding) = binding {
        ctx.world.entity_mut(entity).insert(binding);
    }
    entity
}

/// `SlottedUiSet::Render`: copies the two bound `MenuProperty` values into
/// [`FillValue`] for every `Added<PropertyBinding>`, and resolves
/// [`TankFluid`] from [`TankFluidSource`].
pub fn bind_properties(
    added: Query<(Entity, &PropertyBinding), Added<PropertyBinding>>,
    properties: Query<(&slotted_ecs::MenuProperty, &ChildOf)>,
    mut fills: Query<&mut FillValue>,
) {
    // PHASE6-IMPL: A.
    let _ = (&added, &properties, &mut fills);
}

/// Observer on `slotted_ecs::PropertyChanged`: updates every [`FillValue`]
/// whose [`PropertyBinding`] names the changed property, and every tank
/// whose `fluid_property` it is.
pub fn on_property_changed(
    event: On<slotted_ecs::PropertyChanged>,
    mut bound: Query<(&PropertyBinding, &mut FillValue)>,
) {
    // PHASE6-IMPL: A.
    let _ = (&event, &mut bound);
}

/// `SlottedUiSet::Render`: sizes and tints the [`FillNode`] child from
/// `Changed<FillValue>` / `Changed<TankFluid>` and rewrites the semantic
/// label and any `bar.text` child.
pub fn render_fills(
    changed: Query<
        (Entity, &FillValue, Option<&TankFluid>),
        Or<(Changed<FillValue>, Changed<TankFluid>)>,
    >,
    fluids: Option<Res<crate::fluids::Fluids>>,
    children: Query<&Children>,
    mut nodes: Query<&mut Node, With<FillNode>>,
) {
    // PHASE6-IMPL: A.
    let _ = (&changed, &fluids, &children, &mut nodes);
}
