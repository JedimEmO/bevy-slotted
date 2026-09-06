//! Phantom previews: showing what a gesture is about to do before it does it.
//!
//! Three things live here, all of them purely visual.
//!
//! * **Phantoms.** While a paint is running, every painted slot shows the
//!   incoming item dimmed, with the `+N` it is about to receive. The numbers
//!   come from [`slotted_model::preview_drag`], which is the same function the
//!   distribution itself runs, so the preview cannot drift from the result.
//! * **Hints.** An empty `Ghost`, `Filter`, `Output` or `Locked` slot draws a
//!   dimmed glyph saying what it is for, and explains itself in its tooltip.
//!   A player should never have to guess why a slot refuses a stack.
//! * **Validity.** The carried stack's ring turns to the theme's `bad` colour
//!   over a slot that will not take it and `good` over one that will.
//!
//! Nothing here writes to an inventory. The model is the authority on what a
//! drag does; this module only asks it early.

use bevy::ecs::system::SystemParam;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use slotted_ecs::{Inventory, OpenMenu, Registries, SlotRef};
use slotted_icons::Icons;
use slotted_model::{
    Inventories, ItemStack, LookupCtx, Predicate, SlotBehaviour, SlotIx, can_accept, preview_drag,
};
use slotted_theme::{ActiveTheme, Paint, Role, Theme, Themed};

use crate::item::{ItemView, write_icon};
use crate::layers::CarriedItem;

/// Role of the dimmed item a painted slot is about to receive.
pub const SLOT_PHANTOM: Role = Role::new_static("slot.phantom");
/// Role of the glyph an empty special slot shows.
pub const SLOT_HINT: Role = Role::new_static("slot.hint");
/// Role of the carried ring over a slot that will take the stack.
pub const CARRIED_GOOD: Role = Role::new_static("carried.good");
/// Role of the carried ring over a slot that will refuse it.
pub const CARRIED_BAD: Role = Role::new_static("carried.bad");

/// What one slot will receive if the running paint ends now. Absent when no
/// paint is running or the slot is not painted.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SlotPhantom {
    /// The stack the slot would hold afterwards.
    pub stack: ItemStack,
    /// How many items are about to land here.
    pub delta: u32,
}

/// Why an empty slot is not an ordinary empty slot.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum SlotHint {
    /// A `Ghost` or `Filter` slot, with the kind it accepts.
    Accepts {
        /// A representative item to draw dimmed, when the filter names one.
        example: Option<ItemStack>,
        /// How many kinds the filter lists, for the `n` badge. `None` for one.
        kinds: Option<usize>,
        /// The tooltip line.
        text: String,
    },
    /// A result slot: items come out, none go in.
    Output,
    /// Visible but frozen.
    Locked,
}

impl SlotHint {
    /// The line this hint puts in the slot's tooltip.
    pub fn tooltip(&self) -> &str {
        match self {
            Self::Accepts { text, .. } => text,
            Self::Output => "Output: takes nothing, gives what is made here",
            Self::Locked => "Locked: this slot cannot be changed",
        }
    }
}

/// Whether the slot under the pointer will take the carried stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    /// The slot accepts it.
    Good,
    /// The slot refuses it: an output, a lock, a failing filter or a full
    /// stack.
    Bad,
}

/// State of the stack following the pointer, for anything that needs to know
/// what the player is about to do.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DragGhost {
    /// What the cursor keeps if the running paint ends now. `None` when no
    /// paint is running; the carried count is then the whole truth.
    pub remaining: Option<u32>,
    /// Validity over the slot the pointer is on, if any.
    pub validity: Option<Validity>,
}

// ---------------------------------------------------------------------------
// Overlay children
// ---------------------------------------------------------------------------

/// The dimmed icon of the item a painted slot is about to receive.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PhantomIcon;

/// The `+N` label beside a phantom.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PhantomCount;

/// The glyph an empty special slot shows.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HintIcon;

/// The corner glyph marking a slot that has something to explain.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HintInfo;

/// The small `n` on a filter that accepts several kinds.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HintBadge;

/// Glyph images, loaded once. Empty handles when there is no asset server,
/// which is the headless case: the components are still correct, nothing is
/// drawn.
#[derive(Resource, Debug, Default, Clone)]
pub struct HintGlyphs {
    /// Shown on an empty `Output` slot.
    pub output: Handle<Image>,
    /// Shown on a `Locked` slot.
    pub locked: Handle<Image>,
    /// Shown on a filter that accepts a tag rather than a named item.
    pub tag: Handle<Image>,
    /// Shown on a filter with no example item to draw.
    pub any: Handle<Image>,
    /// The corner mark saying "this slot explains itself".
    pub info: Handle<Image>,
}

/// `Startup`: load the five hint glyphs.
pub fn load_hint_glyphs(assets: Option<Res<AssetServer>>, mut commands: Commands) {
    let Some(assets) = assets else { return };
    commands.insert_resource(HintGlyphs {
        output: assets.load("icons/hint_output.png"),
        locked: assets.load("icons/hint_locked.png"),
        tag: assets.load("icons/hint_tag.png"),
        any: assets.load("icons/hint_any.png"),
        info: assets.load("icons/hint_info.png"),
    });
}

/// Spawns the phantom and hint children of a slot. All of them start hidden
/// and none is pickable, so the slot stays the pick target.
pub fn spawn_overlay_children(world: &mut World, parent: Entity) {
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(80),
            height: percent(80),
            ..default()
        },
        ImageNode::default(),
        // No `Themed`: the theme's apply pass owns a themed node's
        // `ImageNode` and strips it for any material that is not sliced or
        // tiled. The tint comes from `RoleColors` instead, which reads the
        // same role without touching the node.
        PhantomIcon,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(2),
            top: px(1),
            ..default()
        },
        Text::new(String::new()),
        Themed(slotted_theme::roles::COUNT),
        PhantomCount,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(62),
            height: percent(62),
            ..default()
        },
        ImageNode::default(),
        HintIcon,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(2),
            top: px(2),
            width: px(8),
            height: px(8),
            ..default()
        },
        ImageNode::default(),
        HintInfo,
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
        Themed(slotted_theme::roles::TEXT_MUTED),
        HintBadge,
        Pickable::IGNORE,
        Visibility::Hidden,
        ChildOf(parent),
    ));
}

// ---------------------------------------------------------------------------
// Theme colours
// ---------------------------------------------------------------------------

/// Reads role fills out of the active theme, for the two overlays that tint
/// an `ImageNode` rather than a background.
#[derive(SystemParam)]
pub struct RoleColors<'w> {
    active: Option<Res<'w, ActiveTheme>>,
    themes: Option<Res<'w, Assets<Theme>>>,
}

impl RoleColors<'_> {
    /// The fill the theme gives `role`, or `fallback` when it defines none.
    pub fn fill(&self, role: &Role, fallback: Color) -> Color {
        let Some((active, themes)) = self.active.as_ref().zip(self.themes.as_ref()) else {
            return fallback;
        };
        themes
            .get(&active.0)
            .and_then(|theme| {
                let material = theme.material(role)?;
                Paint::from_material(theme, material).background
            })
            .unwrap_or(fallback)
    }
}

/// The dim a phantom is drawn at when the theme says nothing.
const PHANTOM_FALLBACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.45);
/// The same for a hint glyph, which is quieter still.
const HINT_FALLBACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.3);

// ---------------------------------------------------------------------------
// The menu's model, read out of the ecs components
// ---------------------------------------------------------------------------

/// Assembles the menu's inventories, which live one per entity, back into the
/// `Inventories` the pure model functions take.
fn inventories_of(menu: &OpenMenu, invs: &Query<&Inventory>) -> Inventories {
    menu.inventories
        .iter()
        .map(|e| {
            invs.get(*e)
                .map_or_else(|_| slotted_model::Inventory::new(0), |i| i.0.clone())
        })
        .collect()
}

fn lookup<'a>(
    registries: Option<&'a Res<Registries>>,
) -> (Option<slotted_ecs::RegistryLookup<'a>>, &'a dyn LookupCtx) {
    static EMPTY: slotted_ecs::EmptyLookup = slotted_ecs::EmptyLookup;
    match registries {
        Some(r) => {
            let borrowed = r.lookup();
            (Some(borrowed), &EMPTY)
        }
        None => (None, &EMPTY),
    }
}

// ---------------------------------------------------------------------------
// Phantoms
// ---------------------------------------------------------------------------

#[allow(clippy::single_match_else)]
/// `SlottedUiSet::Render`: put a [`SlotPhantom`] on every slot the running
/// paint would write, take it off every slot it would not, and record what
/// the cursor keeps in [`DragGhost`].
pub fn update_drag_phantoms(
    menus: Query<(Entity, &OpenMenu)>,
    invs: Query<&Inventory>,
    registries: Option<Res<Registries>>,
    slots: Query<(Entity, &SlotRef, Option<&SlotPhantom>)>,
    mut ghost: ResMut<DragGhost>,
    mut commands: Commands,
) {
    let (borrowed, empty) = lookup(registries.as_ref());
    let ctx: &dyn LookupCtx = borrowed.as_ref().map_or(empty, |l| l);

    let mut wanted: Vec<(Entity, SlotIx, SlotPhantom)> = Vec::new();
    let mut remaining = None;
    for (menu_entity, menu) in &menus {
        if menu.state.drag.is_none() {
            continue;
        }
        let inv = inventories_of(menu, &invs);
        let planned = preview_drag(&menu.def, &inv, &menu.state, ctx);
        let carried = menu.state.carried.as_ref().map_or(0, |c| c.count);
        let placed: u32 = planned.iter().map(|(_, p)| p.delta).sum();
        remaining = Some(carried.saturating_sub(placed));
        for (slot, preview) in planned {
            wanted.push((
                menu_entity,
                slot,
                SlotPhantom {
                    stack: preview.stack,
                    delta: preview.delta,
                },
            ));
        }
    }
    if ghost.remaining != remaining {
        ghost.remaining = remaining;
    }

    for (entity, slot_ref, current) in &slots {
        let want = wanted
            .iter()
            .find(|(menu, slot, _)| *menu == slot_ref.menu && *slot == slot_ref.slot)
            .map(|(_, _, p)| p);
        match (want, current) {
            (Some(want), Some(current)) if want == current => {}
            (Some(want), _) => {
                commands.entity(entity).insert(want.clone());
            }
            (None, Some(_)) => {
                commands.entity(entity).try_remove::<SlotPhantom>();
            }
            (None, None) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Hints
// ---------------------------------------------------------------------------

/// `SlottedUiSet::Render`: keep a [`SlotHint`] on every empty special slot.
///
/// Runs when a slot's contents change, which includes the frame it is spawned:
/// a hint appears with the screen and disappears the moment something real is
/// in the slot.
pub fn update_slot_hints(
    menus: Query<&OpenMenu>,
    registries: Option<Res<Registries>>,
    slots: Query<(Entity, &SlotRef, &ItemView, Option<&SlotHint>), Changed<ItemView>>,
    mut commands: Commands,
) {
    for (entity, slot_ref, view, current) in &slots {
        let hint = menus
            .get(slot_ref.menu)
            .ok()
            .filter(|_| view.stack.is_none())
            .and_then(|menu| menu.def.slot(slot_ref.slot))
            .and_then(|sd| hint_for(sd, registries.as_deref()));
        match (hint, current) {
            (Some(hint), Some(current)) if hint == *current => {}
            (Some(hint), _) => {
                commands.entity(entity).insert(hint);
            }
            (None, Some(_)) => {
                commands.entity(entity).try_remove::<SlotHint>();
            }
            (None, None) => {}
        }
    }
}

/// The hint an empty slot of this definition shows, if any.
fn hint_for(sd: &slotted_model::SlotDef, registries: Option<&Registries>) -> Option<SlotHint> {
    match sd.behaviour {
        SlotBehaviour::Output => Some(SlotHint::Output),
        SlotBehaviour::Locked => Some(SlotHint::Locked),
        SlotBehaviour::Ghost | SlotBehaviour::Filter => Some(accepts_hint(sd, registries)),
        SlotBehaviour::Normal | SlotBehaviour::Disabled => None,
    }
}

/// The name a player knows an item by, falling back to its numeric id.
fn item_name(registries: Option<&Registries>, id: slotted_model::ItemId) -> String {
    registries.and_then(|r| r.0.items.get(id)).map_or_else(
        || format!("item {}", id.0),
        |def| {
            def.display_name
                .clone()
                .unwrap_or_else(|| def.name.as_str().to_owned())
        },
    )
}

fn accepts_hint(sd: &slotted_model::SlotDef, registries: Option<&Registries>) -> SlotHint {
    match sd.accepts.as_ref() {
        Some(Predicate::ItemIs(id)) => SlotHint::Accepts {
            example: Some(ItemStack::new(*id, 1)),
            kinds: None,
            text: format!("Accepts {}", item_name(registries, *id)),
        },
        Some(Predicate::AnyOf(ids)) => {
            let names: Vec<String> = ids.iter().map(|id| item_name(registries, *id)).collect();
            SlotHint::Accepts {
                example: ids.first().map(|id| ItemStack::new(*id, 1)),
                kinds: Some(ids.len()),
                text: format!("Accepts any of: {}", names.join(", ")),
            }
        }
        Some(Predicate::Tag(tag)) => SlotHint::Accepts {
            example: None,
            kinds: None,
            text: format!("Accepts items tagged {tag}"),
        },
        Some(other) => SlotHint::Accepts {
            example: None,
            kinds: None,
            text: format!("Accepts {}", describe(other)),
        },
        None => SlotHint::Accepts {
            example: None,
            kinds: None,
            text: "Accepts any item".to_owned(),
        },
    }
}

fn describe(predicate: &Predicate) -> String {
    match predicate {
        Predicate::Any => "any item".to_owned(),
        Predicate::Not(_) => "all but a few kinds".to_owned(),
        Predicate::ItemIs(_) | Predicate::AnyOf(_) | Predicate::Tag(_) => "a few kinds".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// `SlottedUiSet::Render`: draw the phantom and hint children from the two
/// components. One pass over the slots, like the state roles beside it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
#[allow(clippy::single_match_else)]
pub fn render_overlays(
    icons: Option<Res<Icons>>,
    glyphs: Option<Res<HintGlyphs>>,
    colors: RoleColors,
    slots: Query<(&Children, Option<&SlotPhantom>, Option<&SlotHint>), With<SlotRef>>,
    mut phantom_icons: Query<
        (&mut ImageNode, &mut Visibility),
        (
            With<PhantomIcon>,
            Without<PhantomCount>,
            Without<HintIcon>,
            Without<HintInfo>,
            Without<HintBadge>,
        ),
    >,
    mut phantom_counts: Query<
        (&mut Text, &mut Visibility),
        (
            With<PhantomCount>,
            Without<PhantomIcon>,
            Without<HintIcon>,
            Without<HintInfo>,
            Without<HintBadge>,
        ),
    >,
    mut hint_icons: Query<
        (&mut ImageNode, &mut Visibility),
        (
            With<HintIcon>,
            Without<PhantomIcon>,
            Without<PhantomCount>,
            Without<HintInfo>,
            Without<HintBadge>,
        ),
    >,
    mut hint_infos: Query<
        (&mut ImageNode, &mut Visibility),
        (
            With<HintInfo>,
            Without<PhantomIcon>,
            Without<PhantomCount>,
            Without<HintIcon>,
            Without<HintBadge>,
        ),
    >,
    mut hint_badges: Query<
        (&mut Text, &mut Visibility),
        (
            With<HintBadge>,
            Without<PhantomIcon>,
            Without<PhantomCount>,
            Without<HintIcon>,
            Without<HintInfo>,
        ),
    >,
) {
    let phantom_tint = colors.fill(&SLOT_PHANTOM, PHANTOM_FALLBACK);
    let hint_tint = colors.fill(&SLOT_HINT, HINT_FALLBACK);
    for (children, phantom, hint) in &slots {
        for child in children {
            if let Ok((mut image, mut visible)) = phantom_icons.get_mut(*child) {
                match phantom {
                    Some(p) => {
                        let icon = icons.as_ref().map(|i| i.flat_icon(&p.stack));
                        write_icon(&mut image, &mut visible, icon.as_ref());
                        image.color = phantom_tint;
                    }
                    None => *visible = Visibility::Hidden,
                }
            }
            if let Ok((mut text, mut visible)) = phantom_counts.get_mut(*child) {
                match phantom.filter(|p| p.delta > 0) {
                    Some(p) => {
                        let label = format!("+{}", p.delta);
                        if text.0 != label {
                            text.0 = label;
                        }
                        *visible = Visibility::Inherited;
                    }
                    None => {
                        text.0.clear();
                        *visible = Visibility::Hidden;
                    }
                }
            }
            if let Ok((mut image, mut visible)) = hint_icons.get_mut(*child) {
                match hint {
                    Some(hint) => {
                        let example = match hint {
                            SlotHint::Accepts { example, .. } => example.as_ref(),
                            _ => None,
                        };
                        match (example, icons.as_ref()) {
                            (Some(stack), Some(icons)) => {
                                let icon = icons.flat_icon(stack);
                                write_icon(&mut image, &mut visible, Some(&icon));
                            }
                            _ => {
                                image.image = glyph_for(hint, glyphs.as_deref());
                                image.texture_atlas = None;
                                *visible = Visibility::Inherited;
                            }
                        }
                        image.color = hint_tint;
                    }
                    None => *visible = Visibility::Hidden,
                }
            }
            if let Ok((mut image, mut visible)) = hint_infos.get_mut(*child) {
                match hint {
                    Some(_) => {
                        image.image = glyphs.as_ref().map(|g| g.info.clone()).unwrap_or_default();
                        image.color = hint_tint;
                        *visible = Visibility::Inherited;
                    }
                    None => *visible = Visibility::Hidden,
                }
            }
            if let Ok((mut text, mut visible)) = hint_badges.get_mut(*child) {
                match hint.and_then(|h| match h {
                    SlotHint::Accepts { kinds, .. } => kinds.filter(|n| *n > 1),
                    _ => None,
                }) {
                    Some(n) => {
                        let label = format!("n{n}");
                        if text.0 != label {
                            text.0 = label;
                        }
                        *visible = Visibility::Inherited;
                    }
                    None => {
                        text.0.clear();
                        *visible = Visibility::Hidden;
                    }
                }
            }
        }
    }
}

fn glyph_for(hint: &SlotHint, glyphs: Option<&HintGlyphs>) -> Handle<Image> {
    let Some(g) = glyphs else {
        return Handle::default();
    };
    match hint {
        SlotHint::Output => g.output.clone(),
        SlotHint::Locked => g.locked.clone(),
        SlotHint::Accepts { text, .. } if text.contains("tagged") => g.tag.clone(),
        SlotHint::Accepts { .. } => g.any.clone(),
    }
}

// ---------------------------------------------------------------------------
// Validity
// ---------------------------------------------------------------------------

/// `SlottedUiSet::Render`: colour the carried stack's ring by whether the slot
/// under the pointer will take it.
pub fn update_carried_validity(
    menus: Query<(&OpenMenu, &slotted_ecs::Carried)>,
    invs: Query<&Inventory>,
    registries: Option<Res<Registries>>,
    hovered: Query<(&SlotRef, &Hovered)>,
    mut ghost: ResMut<DragGhost>,
    mut carried: Query<&mut Themed, With<CarriedItem>>,
) {
    let (borrowed, empty) = lookup(registries.as_ref());
    let ctx: &dyn LookupCtx = borrowed.as_ref().map_or(empty, |l| l);

    let mut validity = None;
    for (slot_ref, is_hovered) in &hovered {
        if !is_hovered.get() {
            continue;
        }
        let Ok((menu, carried_stack)) = menus.get(slot_ref.menu) else {
            continue;
        };
        let Some(stack) = carried_stack.0.as_ref() else {
            continue;
        };
        let inv = inventories_of(menu, &invs);
        validity = Some(if can_accept(&menu.def, &inv, ctx, slot_ref.slot, stack) {
            Validity::Good
        } else {
            Validity::Bad
        });
        break;
    }
    if ghost.validity != validity {
        ghost.validity = validity;
    }
    let role = match validity {
        Some(Validity::Good) => CARRIED_GOOD,
        Some(Validity::Bad) => CARRIED_BAD,
        None => slotted_theme::roles::CARRIED,
    };
    for mut themed in &mut carried {
        if themed.0 != role {
            themed.0 = role.clone();
        }
    }
}
