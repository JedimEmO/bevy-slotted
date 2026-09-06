//! `tank`: a property-driven fluid well. Phase 6 contract section 1.2.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use serde::{Deserialize, Serialize};
use slotted_model::{Namespaced, PropertyId};
use slotted_theme::{Themed, roles};

use crate::def::{Direction, Orientation, Tags};
use crate::fluids::FluidId;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::{BORDER_WIDTH, SLOT_SIZE};

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

/// Which edge the [`FillNode`] child grows from, on the tank or bar root.
///
/// A bar's [`BarState`](crate::widgets::bar::BarState) carries the authored
/// direction because the contract puts it there; this is the same fact in the
/// one shape [`render_fills`] needs for both widgets, since a tank's
/// [`Orientation`] is not a [`Direction`]. `spawn_tank` and `spawn_bar` are
/// the only writers and they keep the two in step.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillOrigin(pub Direction);

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

/// The unknown fluid a tank has already complained about.
///
/// A tank renders every frame, so warning from the render path without this
/// would put one line per frame on the console for as long as the screen is
/// open. `None` records a `fluid` name that is in no registry; `Some(id)` a
/// property value that names no registered fluid.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownFluidWarned(pub Option<FluidId>);

/// The fill child of a tank or bar. Purely visual: no `SemanticRole`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FillNode;

/// The well of a tank: the bordered, clipped box the fluid sits in. It is a
/// child of the tank root rather than the root itself, so a narrow tank can
/// hang its readout underneath without the clip eating it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TankWell;

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

/// The `"<value> / <max> <unit>"` line a tank and a bar label and tooltip
/// with. An empty `unit` drops the trailing space.
pub fn fill_label(fill: &FillValue, unit: &str) -> String {
    let text = format!("{} / {}", amount(fill.value), amount(fill.max));
    if unit.is_empty() {
        text
    } else {
        format!("{text} {unit}")
    }
}

/// The same reading stacked over three lines, which is what fits inside a
/// tank one slot wide. The unit line is dropped when there is no unit.
pub fn tank_label(fill: &FillValue, unit: &str) -> String {
    let mut text = format!("{}\n/ {}", amount(fill.value), amount(fill.max));
    if !unit.is_empty() {
        text.push('\n');
        text.push_str(unit);
    }
    text
}

/// Formats one amount. Property values are whole numbers, so a fill that came
/// from a property reads `1200`, not `1200.0`.
fn amount(v: f32) -> String {
    if (v - v.round()).abs() < f32::EPSILON {
        format!("{:.0}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// The `Node` of a tank or bar fill child: absolutely positioned against the
/// edge it grows from, sized in percent of the well.
pub fn fill_node(origin: Direction, fraction: f32) -> Node {
    let along = percent(100.0 * fraction.clamp(0.0, 1.0));
    let mut node = Node {
        position_type: PositionType::Absolute,
        ..default()
    };
    match origin {
        Direction::Right | Direction::Left => {
            node.width = along;
            node.height = percent(100);
            if origin == Direction::Right {
                node.left = px(0);
            } else {
                node.right = px(0);
            }
        }
        Direction::Up | Direction::Down => {
            node.width = percent(100);
            node.height = along;
            if origin == Direction::Up {
                node.bottom = px(0);
            } else {
                node.top = px(0);
            }
        }
    }
    node
}

/// Spawns a tank. Root components per contract 1.2; the fill child is a
/// [`FillNode`].
pub fn spawn_tank(ctx: &mut SpawnCtx<'_>, params: &TankParams, _tags: &Tags) -> Entity {
    let binding = ctx.menu.map(|menu| PropertyBinding {
        menu,
        value: params.property,
        max: params.capacity,
    });
    let (width, height, origin) = match params.orientation {
        Orientation::Vertical => (SLOT_SIZE, 3.0 * SLOT_SIZE, Direction::Up),
        Orientation::Horizontal => (3.0 * SLOT_SIZE, SLOT_SIZE, Direction::Right),
    };
    // A well one slot wide has no room for a reading on top of the fluid, so
    // a vertical tank puts its readout under the well and grows to fit it.
    // Phase 6 left it overlapping; `docs/FOLLOWUPS.md` called it.
    let readout_below = width < 2.0 * SLOT_SIZE;
    let radius = ctx.tokens().radii.sm;
    let fill = FillValue::default();
    // The root is the widget: it carries the state, the semantics and the
    // tooltip. What it paints is the well, which is its first child, so the
    // readout can sit outside the fluid without leaving the widget.
    let entity = ctx.spawn_node((
        Node {
            width: px(width),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2.0),
            position_type: PositionType::Relative,
            ..default()
        },
        SemanticRole::Tank,
        SemanticLabel(fill_label(&fill, &params.unit)),
        WidgetNode(crate::widgets::kinds::tank()),
        fill,
        FillOrigin(origin),
        TankFluid::default(),
        TankFluidSource {
            fluid: params.fluid.clone(),
            fluid_property: params.fluid_property,
            unit: params.unit.clone(),
        },
        Hovered::default(),
        Pickable::default(),
        TooltipSource,
    ));
    let well = ctx
        .world
        .spawn((
            Node {
                width: px(width),
                height: px(height),
                border: UiRect::all(px(BORDER_WIDTH)),
                position_type: PositionType::Relative,
                overflow: Overflow::clip(),
                border_radius: BorderRadius::all(px(radius)),
                ..default()
            },
            Themed(roles::TANK),
            TankWell,
            Pickable::IGNORE,
            ChildOf(entity),
        ))
        .id();
    ctx.world.spawn((
        fill_node(origin, 0.0),
        Themed(roles::TANK_FILL),
        FillNode,
        Pickable::IGNORE,
        ChildOf(well),
    ));
    // The reading is a `BarText`, which is what `render_fills` already knows
    // how to write. Below the well when the well is narrow, centred inside it
    // when the tank is wide enough to carry it.
    let text_parent = if readout_below { entity } else { well };
    let text_node = if readout_below {
        Node {
            justify_content: JustifyContent::Center,
            ..default()
        }
    } else {
        Node {
            position_type: PositionType::Absolute,
            top: px(2),
            left: px(0),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        }
    };
    ctx.world.spawn((
        text_node,
        Text::new(tank_label(&fill, &params.unit)),
        TextLayout::justify(Justify::Center),
        Themed(roles::BAR_TEXT),
        crate::widgets::bar::BarText,
        Pickable::IGNORE,
        ChildOf(text_parent),
    ));
    if let Some(binding) = binding {
        ctx.world.entity_mut(entity).insert(binding);
    }
    ctx.world
        .entity_mut(entity)
        .observe(crate::tooltip::on_slot_over);
    entity
}

/// `SlottedUiSet::Render`: copies the two bound `MenuProperty` values into
/// [`FillValue`] for every `Added<PropertyBinding>`, and resolves
/// [`TankFluid`] from [`TankFluidSource`].
pub fn bind_properties(
    mut added: Query<
        (
            Option<&PropertyBinding>,
            &mut FillValue,
            Option<&TankFluidSource>,
            Option<&mut TankFluid>,
        ),
        Or<(Added<PropertyBinding>, Added<TankFluidSource>)>,
    >,
    properties: Query<(&slotted_ecs::MenuProperty, &ChildOf)>,
    fluids: Option<Res<crate::fluids::Fluids>>,
) {
    let value_of = |menu: Entity, id: PropertyId| {
        properties
            .iter()
            .find(|(p, child_of)| child_of.parent() == menu && p.id == id)
            .map(|(p, _)| p.value)
    };
    for (binding, mut fill, source, tank) in &mut added {
        if let Some(binding) = binding {
            let next = FillValue {
                value: as_f32(value_of(binding.menu, binding.value).unwrap_or(0)),
                max: as_f32(value_of(binding.menu, binding.max).unwrap_or(0)),
            };
            if *fill != next {
                *fill = next;
            }
        }
        let (Some(source), Some(mut tank)) = (source, tank) else {
            continue;
        };
        let resolved = match (source.fluid_property, binding) {
            // A `fluid_property` wins over the static name, per contract 1.1.
            (Some(id), Some(binding)) => value_of(binding.menu, id).and_then(fluid_id),
            _ => source
                .fluid
                .as_ref()
                .zip(fluids.as_deref())
                .and_then(|(name, fluids)| fluids.id_of(name)),
        };
        if tank.0 != resolved {
            tank.0 = resolved;
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn as_f32(value: i32) -> f32 {
    value as f32
}

/// A property value read as a frozen fluid id. A negative value means "no
/// fluid", the way an empty tank reports itself.
fn fluid_id(value: i32) -> Option<FluidId> {
    u32::try_from(value).ok().map(FluidId)
}

/// Observer on `slotted_ecs::PropertyChanged`: updates every [`FillValue`]
/// whose [`PropertyBinding`] names the changed property, and every tank
/// whose `fluid_property` it is.
pub fn on_property_changed(
    event: On<slotted_ecs::PropertyChanged>,
    mut bound: Query<(&PropertyBinding, &mut FillValue)>,
    mut tanks: Query<(&PropertyBinding, &TankFluidSource, &mut TankFluid)>,
) {
    let slotted_ecs::PropertyChanged {
        menu, id, value, ..
    } = *event;
    let value_f = as_f32(value);
    for (binding, mut fill) in &mut bound {
        if binding.menu != menu {
            continue;
        }
        // A property is an `i32`, so the two sides are the same integer or a
        // different one; this is not an approximate comparison.
        if binding.value == id {
            fill.value = value_f;
        }
        if binding.max == id {
            fill.max = value_f;
        }
    }
    for (binding, source, mut tank) in &mut tanks {
        if binding.menu != menu || source.fluid_property != Some(id) {
            continue;
        }
        let resolved = fluid_id(value);
        if tank.0 != resolved {
            tank.0 = resolved;
        }
    }
}

/// `SlottedUiSet::Render`: sizes the [`FillNode`] child from
/// `Changed<FillValue>` / `Changed<TankFluid>`, rewrites the semantic label
/// and any `bar.text` child, and paints a tank's fill with its fluid.
///
/// The fluid tint is re-asserted every frame, and only when it differs:
/// `apply_theme` repaints the fill node's role whenever the theme asset
/// changes, and the fluid's own colour has to win over the `tank.fill` role
/// again afterwards.
#[allow(clippy::too_many_arguments)]
pub fn render_fills(
    changed: Query<
        (
            Entity,
            &FillValue,
            &FillOrigin,
            Option<&TankFluidSource>,
            Option<&crate::widgets::bar::BarState>,
        ),
        Or<(Changed<FillValue>, Changed<TankFluid>)>,
    >,
    tanks: Query<(Entity, &TankFluid)>,
    fluids: Option<Res<crate::fluids::Fluids>>,
    assets: Option<Res<AssetServer>>,
    children: Query<&Children>,
    mut fills: Query<
        (
            &mut Node,
            &mut Themed,
            Option<&mut BackgroundColor>,
            Option<&mut ImageNode>,
        ),
        With<FillNode>,
    >,
    mut texts: Query<&mut Text, With<crate::widgets::bar::BarText>>,
    mut labels: Query<&mut SemanticLabel>,
    warned: Query<&UnknownFluidWarned>,
    mut commands: Commands,
) {
    for (entity, fill, origin, source, bar) in &changed {
        let unit = source.map_or("", |s| s.unit.as_str());
        let label = fill_label(fill, unit);
        if let Ok(mut semantic) = labels.get_mut(entity)
            && semantic.0 != label
        {
            semantic.0.clone_from(&label);
        }
        for child in child_entities(&children, entity) {
            if let Ok((mut node, ..)) = fills.get_mut(child) {
                let next = fill_node(origin.0, fill.fraction());
                if *node != next {
                    *node = next;
                }
            }
            // A bar's readout is the one-line label; a tank's is stacked, so
            // it fits a well one slot wide.
            let wanted = if source.is_some() {
                Some(tank_label(fill, unit))
            } else if bar.is_some_and(|b| b.text) {
                Some(label.clone())
            } else {
                None
            };
            if let Some(wanted) = wanted
                && let Ok(mut text) = texts.get_mut(child)
                && text.0 != wanted
            {
                text.0 = wanted;
            }
        }
    }

    for (entity, tank) in &tanks {
        let def = tank
            .0
            .and_then(|id| fluids.as_deref().and_then(|f| f.get(id)));
        // A tank pointed at a fluid nobody registered keeps the `tank.fill`
        // role, which is a real colour and not a blank well, and says so
        // once. Anything more would be a line per frame.
        if def.is_none()
            && let Some(id) = tank.0
            && warned.get(entity) != Ok(&UnknownFluidWarned(Some(id)))
        {
            tracing::warn!(
                ?entity,
                fluid = id.0,
                "tank names a fluid that is in no registry; falling back to the tank.fill role"
            );
            commands.entity(entity).insert(UnknownFluidWarned(Some(id)));
        }
        for child in child_entities(&children, entity) {
            let Ok((_, mut themed, background, image)) = fills.get_mut(child) else {
                continue;
            };
            let Some(def) = def else {
                // No fluid: hand the node back to the `tank.fill` role.
                if image.is_some() {
                    commands.entity(child).remove::<ImageNode>();
                    themed.set_changed();
                }
                continue;
            };
            let color = def.color();
            match (&def.texture, image) {
                (Some(path), Some(mut image)) => {
                    let handle = assets.as_ref().map(|a| a.load(path)).unwrap_or_default();
                    if image.color != color {
                        image.color = color;
                    }
                    if image.image != handle {
                        image.image = handle;
                    }
                }
                (Some(path), None) => {
                    let handle = assets.as_ref().map(|a| a.load(path)).unwrap_or_default();
                    commands.entity(child).insert(ImageNode {
                        image: handle,
                        image_mode: NodeImageMode::Tiled {
                            tile_x: true,
                            tile_y: true,
                            stretch_value: 1.0,
                        },
                        color,
                        ..default()
                    });
                }
                (None, image) => {
                    if image.is_some() {
                        commands.entity(child).remove::<ImageNode>();
                    }
                    if let Some(mut background) = background
                        && background.0 != color
                    {
                        background.0 = color;
                    }
                }
            }
        }
    }
}

/// Every descendant of `entity`, nearest first.
///
/// A tank's fill sits inside its well, one level further down than a bar's,
/// so the render pass walks the subtree rather than the child list.
fn child_entities(children: &Query<&Children>, entity: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut queue = vec![entity];
    while let Some(next) = queue.pop() {
        let Ok(kids) = children.get(next) else {
            continue;
        };
        for kid in kids.iter() {
            out.push(kid);
            queue.push(kid);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fill_fraction_clamps_and_survives_a_zero_max() {
        let close = |value, max, expected: f32| {
            let got = FillValue { value, max }.fraction();
            assert!((got - expected).abs() < f32::EPSILON, "{got} != {expected}");
        };
        close(5.0, 10.0, 0.5);
        close(20.0, 10.0, 1.0);
        close(5.0, 0.0, 0.0);
        close(-5.0, 10.0, 0.0);
    }

    #[test]
    fn a_label_reads_as_whole_units_and_drops_an_empty_unit() {
        let fill = FillValue {
            value: 1200.0,
            max: 8000.0,
        };
        assert_eq!(fill_label(&fill, "mB"), "1200 / 8000 mB");
        assert_eq!(fill_label(&fill, ""), "1200 / 8000");
    }

    #[test]
    fn a_fill_node_anchors_to_the_edge_it_grows_from() {
        let up = fill_node(Direction::Up, 0.25);
        assert_eq!(up.bottom, px(0));
        assert_eq!(up.height, percent(25.0));
        assert_eq!(up.width, percent(100));
        let left = fill_node(Direction::Left, 0.5);
        assert_eq!(left.right, px(0));
        assert_eq!(left.width, percent(50.0));
        assert_eq!(left.height, percent(100));
    }

    #[test]
    fn a_negative_property_value_is_no_fluid() {
        assert_eq!(fluid_id(-1), None);
        assert_eq!(fluid_id(2), Some(FluidId(2)));
    }
}
