//! The item renderer: icon, count, durability and the rarity ring.
//!
//! One [`ItemView`] per slot (and one on the carried node) is the whole
//! interface. The slot widget writes it from `SlotChanged`; [`render_items`]
//! turns it into the four purely visual children, none of which carry a
//! [`SemanticRole`](crate::SemanticRole), so the semantic tree still shows one
//! node per slot.

use bevy::prelude::*;
use bevy::ui::widget::TextShadow;
use slotted_ecs::{Registries, SlotChanged};
use slotted_icons::{IconRef, Icons};
use slotted_model::ItemStack;
use slotted_registry::defs::Rarity;
use slotted_theme::{Role, Themed, roles};

/// The stack a node displays. On slot entities and on the carried-stack node.
/// Set by the slot widget from `SlotChanged`; `Changed<ItemView>` drives
/// [`render_items`].
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct ItemView {
    /// What to show. `None` draws an empty slot.
    pub stack: Option<ItemStack>,
}

impl ItemView {
    /// A view of one stack.
    pub fn new(stack: Option<ItemStack>) -> Self {
        Self { stack }
    }
}

/// Marker on the icon child of an [`ItemView`] node: an `ImageNode`.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ItemIcon;

/// Marker on the count child of an [`ItemView`] node: a `Text` with
/// `Themed(count)`. Hidden when the count is 1 or the slot is empty.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ItemCount;

/// Marker on the durability track child. Hidden for undamaged stacks.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct DurabilityBar;

/// Marker on the filled part of the durability track.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct DurabilityFill;

/// Marker on the rarity ring overlay, themed `slot.rarity.<rarity>`.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct RarityRing;

/// Component keys the durability bar reads out of a stack's patch.
pub const DAMAGE_COMPONENT: &str = "slotted:damage";
/// Component key for the maximum damage a stack can take.
pub const MAX_DAMAGE_COMPONENT: &str = "slotted:max_damage";

/// Spawns the icon, count, durability and rarity children of an
/// [`ItemView`] node. Every one of them is `Pickable::IGNORE`, so the slot
/// itself stays the pick target.
pub fn spawn_item_view_children(world: &mut World, parent: Entity) {
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(80),
            height: percent(80),
            ..default()
        },
        ImageNode::default(),
        ItemIcon,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(2),
            bottom: px(1),
            ..default()
        },
        Text::new(String::new()),
        TextShadow::default(),
        Themed(roles::COUNT),
        ItemCount,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
    let bar = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(3),
                right: px(3),
                bottom: px(2),
                height: px(3),
                ..default()
            },
            Themed(Role::new_static("slot.durability")),
            DurabilityBar,
            Pickable::IGNORE,
            Visibility::Hidden,
            ChildOf(parent),
        ))
        .id();
    world.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgb(0.3, 0.9, 0.3)),
        DurabilityFill,
        Pickable::IGNORE,
        ChildOf(bar),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            border: UiRect::all(px(1)),
            ..default()
        },
        Themed(rarity_role(Rarity::Common)),
        RarityRing,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
}

/// The theme role of a rarity ring.
pub fn rarity_role(rarity: Rarity) -> Role {
    Role::new(format!("slot.rarity.{}", rarity.as_str()))
}

/// Observer: `SlotChanged` writes the new stack into the slot's [`ItemView`].
pub fn on_slot_changed(changed: On<SlotChanged>, mut views: Query<&mut ItemView>) {
    let Ok(mut view) = views.get_mut(changed.entity) else {
        return;
    };
    if view.stack != changed.stack {
        view.stack.clone_from(&changed.stack);
    }
}

/// `SlottedUiSet::Render`: for each changed [`ItemView`], resolve the icon
/// through [`Icons`], write the icon child's `ImageNode`, the count text, the
/// durability bar and the rarity ring, and refresh `SemanticLabel`.
#[allow(clippy::too_many_arguments)]
pub fn render_items(
    icons: Option<Res<Icons>>,
    registries: Option<Res<Registries>>,
    changed: Query<(Entity, &ItemView, &Children), Changed<ItemView>>,
    mut icon_nodes: Query<(&mut ImageNode, &mut Visibility), (With<ItemIcon>, Without<ItemCount>)>,
    mut counts: Query<(&mut Text, &mut Visibility), (With<ItemCount>, Without<ItemIcon>)>,
    mut bars: Query<
        (&mut Visibility, &Children),
        (
            With<DurabilityBar>,
            Without<ItemIcon>,
            Without<ItemCount>,
            Without<RarityRing>,
        ),
    >,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<DurabilityFill>>,
    mut rings: Query<
        (&mut Themed, &mut Visibility),
        (
            With<RarityRing>,
            Without<ItemIcon>,
            Without<ItemCount>,
            Without<DurabilityBar>,
        ),
    >,
    mut labels: Query<&mut crate::semantic::SemanticLabel>,
) {
    for (entity, view, children) in &changed {
        let icon = view
            .stack
            .as_ref()
            .map(|s| icons.as_ref().map_or(IconRef::Missing, |i| i.icon(s)));
        for child in children {
            if let Ok((mut image, mut visible)) = icon_nodes.get_mut(*child) {
                write_icon(&mut image, &mut visible, icon.as_ref());
            }
            if let Ok((mut text, mut visible)) = counts.get_mut(*child) {
                let count = view.stack.as_ref().map_or(0, |s| s.count);
                if count > 1 {
                    text.0 = count.to_string();
                    *visible = Visibility::Inherited;
                } else {
                    text.0.clear();
                    *visible = Visibility::Hidden;
                }
            }
            if let Ok((mut visible, bar_children)) = bars.get_mut(*child) {
                let fraction = registries
                    .as_ref()
                    .and_then(|r| durability(r, view.stack.as_ref()));
                match fraction {
                    Some(f) => {
                        *visible = Visibility::Inherited;
                        for fill in bar_children {
                            if let Ok((mut node, mut color)) = fills.get_mut(*fill) {
                                node.width = percent(f * 100.0);
                                *color = BackgroundColor(durability_color(f));
                            }
                        }
                    }
                    None => *visible = Visibility::Hidden,
                }
            }
            if let Ok((mut themed, mut visible)) = rings.get_mut(*child) {
                let rarity = registries
                    .as_ref()
                    .and_then(|r| rarity_of(r, view.stack.as_ref()));
                match rarity {
                    Some(rarity) if rarity != Rarity::Common => {
                        let role = rarity_role(rarity);
                        if themed.0 != role {
                            themed.0 = role;
                        }
                        *visible = Visibility::Inherited;
                    }
                    _ => *visible = Visibility::Hidden,
                }
            }
        }
        if let Ok(mut label) = labels.get_mut(entity) {
            let text = semantic_label(registries.as_deref(), view.stack.as_ref());
            if label.0 != text {
                label.0 = text;
            }
        }
    }
}

fn write_icon(image: &mut ImageNode, visible: &mut Visibility, icon: Option<&IconRef>) {
    match icon {
        Some(IconRef::Atlas {
            image: handle,
            layout,
            index,
        }) => {
            image.image = handle.clone();
            image.texture_atlas = Some(TextureAtlas {
                layout: layout.clone(),
                index: *index,
            });
            image.color = Color::WHITE;
            *visible = Visibility::Inherited;
        }
        Some(IconRef::Solid(color)) => {
            image.image = Handle::default();
            image.texture_atlas = None;
            image.color = *color;
            *visible = Visibility::Inherited;
        }
        Some(IconRef::Missing) => {
            image.image = Handle::default();
            image.texture_atlas = None;
            // The theme's missing-icon glyph is a flat magenta square in
            // Phase 2, the same signal a missing palette colour gives.
            image.color = Color::srgb(1.0, 0.0, 1.0);
            *visible = Visibility::Inherited;
        }
        None => {
            image.texture_atlas = None;
            *visible = Visibility::Hidden;
        }
    }
}

/// The label a screen reader and the harness read off a slot.
pub fn semantic_label(registries: Option<&Registries>, stack: Option<&ItemStack>) -> String {
    let Some(stack) = stack else {
        return String::new();
    };
    let name = registries
        .and_then(|r| r.items.name_of(stack.id).map(ToString::to_string))
        .unwrap_or_else(|| format!("item#{}", stack.id.0));
    if stack.count > 1 {
        format!("{name} x{}", stack.count)
    } else {
        name
    }
}

/// Remaining durability in 0..=1, or `None` when the stack has no damage
/// components or is undamaged.
pub fn durability(registries: &Registries, stack: Option<&ItemStack>) -> Option<f32> {
    let stack = stack?;
    let damage = patch_number(registries, stack, DAMAGE_COMPONENT)?;
    let max = patch_number(registries, stack, MAX_DAMAGE_COMPONENT)?;
    if max <= 0.0 {
        return None;
    }
    let remaining = ((max - damage) / max).clamp(0.0, 1.0);
    (remaining < 1.0).then_some(remaining)
}

fn patch_number(registries: &Registries, stack: &ItemStack, key: &str) -> Option<f32> {
    let name = slotted_model::Namespaced::parse(key).ok()?;
    let id = registries.components.get(&name)?;
    number(stack.patch.get(id)?)
}

/// A component value read as a number, whichever numeric shape it took.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn number(value: &slotted_model::Value) -> Option<f32> {
    match value {
        slotted_model::Value::Int(i) => Some(*i as f32),
        slotted_model::Value::Float(f) => Some(*f as f32),
        _ => None,
    }
}

/// Green at full, red at empty.
fn durability_color(fraction: f32) -> Color {
    Color::srgb(1.0 - fraction, 0.25 + 0.65 * fraction, 0.2)
}

/// The rarity of a stack's item, `Common` when unknown.
pub fn rarity_of(registries: &Registries, stack: Option<&ItemStack>) -> Option<Rarity> {
    let stack = stack?;
    Some(
        registries
            .items
            .get(stack.id)
            .map_or(Rarity::Common, |d| d.rarity),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_registry::defs::Rarity;

    #[test]
    fn rarity_roles_are_dotted_under_slot() {
        assert_eq!(
            rarity_role(Rarity::Legendary).as_str(),
            "slot.rarity.legendary"
        );
        assert_eq!(
            rarity_role(Rarity::Common)
                .parent()
                .map(|r| r.as_str().to_owned()),
            Some("slot.rarity".to_owned())
        );
    }

    #[test]
    fn an_empty_slot_has_an_empty_label() {
        assert_eq!(semantic_label(None, None), "");
    }

    #[test]
    fn durability_colour_runs_green_to_red() {
        let full = durability_color(1.0);
        let empty = durability_color(0.0);
        assert!(full.to_srgba().green > full.to_srgba().red);
        assert!(empty.to_srgba().red > empty.to_srgba().green);
    }
}
