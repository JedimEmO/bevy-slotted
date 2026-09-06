//! Tooltip request, composition and placement.
//!
//! Two tiers: hovering a slot for the theme's delay composes the compact tier,
//! and holding shift composes the expanded one. The delay is measured in
//! `Time<Virtual>`, so a harness that steps frames by hand sees the same
//! timing a player does.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bevy::picking::events::{Over, Pointer};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::{PointerId, PointerLocation};
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use slotted_ecs::Registries;
use slotted_model::{ItemStack, Namespaced};
use slotted_registry::FrozenRegistries;
use slotted_theme::{ActiveTheme, Theme, Tokens};

use crate::def::{LocKey, TextRole, UiNodeDef};
use crate::item::ItemView;
use crate::layers::TooltipLayer;
use crate::screen::SpawnCtx;
use crate::semantic::SemanticRole;

/// Compact (hover) or expanded (shift held / long hover).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TooltipTier {
    /// Name, count, rarity.
    #[default]
    Compact,
    /// Plus components, tags, mod id.
    Expanded,
}

/// Ask for a tooltip on `entity`. Triggered by the slot widget after the
/// hover delay and by the harness's `request_tooltip`. The tooltip system
/// composes parts into [`TooltipContent`] on the entity and spawns the
/// tooltip node under [`crate::TooltipLayer`].
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooltipRequest {
    /// The hovered entity.
    pub entity: Entity,
    /// Which tier.
    pub tier: TooltipTier,
}

/// The composed tooltip, on the hovered entity while its tooltip is shown.
/// `slotted-test`'s `tooltip()` reads this, not the spawned nodes.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct TooltipContent {
    /// Which tier was composed.
    pub tier: TooltipTier,
    /// The parts, in order.
    pub parts: Vec<UiNodeDef>,
}

/// On the spawned tooltip root, pointing back at the hovered entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooltipHost(pub Entity);

/// On a tooltip that has been spawned but whose own size is not laid out yet.
///
/// A tooltip is placed from its size, and a node's size is only known after
/// the frame that spawned it has laid out. Rather than let the first frame
/// draw at whatever position the node happened to start at -- the top left of
/// the tooltip layer -- the tooltip stays `Visibility::Hidden` while this is
/// on it. [`place_tooltips`] takes it off and shows the tooltip on the first
/// frame it can put it in the right place.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TooltipUnplaced;

/// When the pointer entered a node, in `Time<Virtual>` elapsed. Inserted by
/// the `Pointer<Over>` observer, removed when the pointer leaves.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HoverStart(pub Duration);

/// What a part builder sees.
pub struct TooltipCtx<'a> {
    /// The stack under the pointer, if the entity is a slot with content.
    pub stack: Option<&'a ItemStack>,
    /// Registry data.
    pub registries: &'a FrozenRegistries,
    /// Requested tier.
    pub tier: TooltipTier,
}

/// One contributor to a tooltip. Built-ins: item name, count, rarity, and in
/// `dev` builds the item id.
pub trait TooltipPart: Send + Sync {
    /// Append nodes for this part. Append nothing to stay out.
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>);
}

/// Ordered registry of parts.
#[derive(Resource, Default, Clone)]
pub struct TooltipParts(pub Vec<Arc<dyn TooltipPart>>);

impl TooltipParts {
    /// Append a part.
    pub fn push(&mut self, part: impl TooltipPart + 'static) {
        self.0.push(Arc::new(part));
    }

    /// Run every part.
    pub fn compose(&self, ctx: &TooltipCtx<'_>) -> Vec<UiNodeDef> {
        let mut out = Vec::new();
        for p in &self.0 {
            p.build(ctx, &mut out);
        }
        out
    }
}

fn text_node(text: String, style: TextRole) -> UiNodeDef {
    UiNodeDef::Text {
        key: LocKey(text),
        style,
        tags: crate::def::Tags::new(),
    }
}

/// The item's display name, or its id when it has none.
#[derive(Debug, Default, Clone, Copy)]
pub struct ItemNamePart;

impl TooltipPart for ItemNamePart {
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>) {
        let Some(stack) = ctx.stack else { return };
        let def = ctx.registries.items.get(stack.id);
        let name = def
            .and_then(|d| d.display_name.clone())
            .or_else(|| {
                ctx.registries
                    .items
                    .name_of(stack.id)
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| format!("item#{}", stack.id.0));
        out.push(text_node(name, TextRole::Title));
    }
}

/// The stack count, only when it is more than one.
#[derive(Debug, Default, Clone, Copy)]
pub struct ItemCountPart;

impl TooltipPart for ItemCountPart {
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>) {
        let Some(stack) = ctx.stack else { return };
        if stack.count > 1 {
            out.push(text_node(format!("x{}", stack.count), TextRole::Body));
        }
    }
}

/// The item's rarity, only when it is above common.
#[derive(Debug, Default, Clone, Copy)]
pub struct RarityPart;

impl TooltipPart for RarityPart {
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>) {
        let Some(stack) = ctx.stack else { return };
        let Some(def) = ctx.registries.items.get(stack.id) else {
            return;
        };
        if def.rarity != slotted_registry::defs::Rarity::Common {
            out.push(text_node(def.rarity.as_str().to_owned(), TextRole::Muted));
        }
    }
}

/// The raw item id. Expanded tier only, and only in `dev` builds.
#[derive(Debug, Default, Clone, Copy)]
pub struct ItemIdPart;

impl TooltipPart for ItemIdPart {
    fn build(&self, ctx: &TooltipCtx<'_>, out: &mut Vec<UiNodeDef>) {
        let Some(stack) = ctx.stack else { return };
        if ctx.tier != TooltipTier::Expanded {
            return;
        }
        if let Some(name) = ctx.registries.items.name_of(stack.id) {
            out.push(text_node(name.to_string(), TextRole::Muted));
        }
    }
}

/// Registers the Phase 2 built-in parts, in order.
pub fn register_builtin_parts(parts: &mut TooltipParts) {
    parts.push(ItemNamePart);
    parts.push(ItemCountPart);
    parts.push(RarityPart);
    #[cfg(feature = "dev")]
    parts.push(ItemIdPart);
}

/// An empty registry set, so a tooltip still composes in an app that has not
/// loaded any data.
fn empty_registries() -> &'static Arc<FrozenRegistries> {
    static EMPTY: OnceLock<Arc<FrozenRegistries>> = OnceLock::new();
    EMPTY.get_or_init(|| {
        let (frozen, _) = slotted_registry::registry::Registries::new()
            .freeze()
            .expect("an empty registry set freezes");
        Arc::new(frozen)
    })
}

/// Observer on a slot: records when the pointer arrived, so the delay system
/// can decide when the tooltip is due.
pub fn on_slot_over(over: On<Pointer<Over>>, time: Res<Time<Virtual>>, mut commands: Commands) {
    commands
        .entity(over.entity)
        .insert(HoverStart(time.elapsed()));
}

/// `SlottedUiSet::Render`: triggers [`TooltipRequest`] once a slot has been
/// hovered for the theme's delay, and tears the tooltip down when the pointer
/// leaves. Shift promotes the request to the expanded tier.
#[allow(clippy::too_many_arguments)]
pub fn tooltip_delay(
    time: Res<Time<Virtual>>,
    keys: Res<ButtonInput<KeyCode>>,
    tokens: ThemeTokens,
    hovered: Query<(
        Entity,
        &Hovered,
        Option<&HoverStart>,
        Option<&TooltipContent>,
    )>,
    hosts: Query<(Entity, &TooltipHost)>,
    mut commands: Commands,
) {
    let delay = Duration::from_millis(u64::from(tokens.get().durations.hover_delay_ms()));
    let tier = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        TooltipTier::Expanded
    } else {
        TooltipTier::Compact
    };
    // The shift key changes the tier of a tooltip that is already up, but
    // only on the frame it is pressed or released. Re-requesting the shift
    // tier every frame would undo a `TooltipRequest` raised by anything else
    // (a widget, a script, or the harness's `request_tooltip`) one frame
    // later. See `docs/design/phase2-contract.md` 4.4, amended in integration.
    let shift = [KeyCode::ShiftLeft, KeyCode::ShiftRight];
    let shift_changed = keys.any_just_pressed(shift) || keys.any_just_released(shift);
    for (entity, hovered, start, content) in &hovered {
        if !hovered.get() {
            // Teardown is the pointer *leaving*, which is exactly the frames
            // where a `HoverStart` is still on the node. A node that was
            // never hovered keeps whatever tooltip something else asked for:
            // a request from a widget, a script or the harness stands until
            // the pointer leaves the host or `clear_tooltip` clears it. See
            // `docs/design/phase2-contract.md` 4.4.
            if start.is_some() {
                commands.entity(entity).remove::<HoverStart>();
                if content.is_some() {
                    commands.entity(entity).remove::<TooltipContent>();
                    despawn_tooltips_for(&hosts, entity, &mut commands);
                }
            }
            continue;
        }
        let Some(start) = start else {
            // Hovered with no `HoverStart`: the pointer was already inside
            // the node when it appeared, so no `Pointer<Over>` was ever sent
            // for it. Start the clock now rather than never.
            commands.entity(entity).insert(HoverStart(time.elapsed()));
            continue;
        };
        if time.elapsed().saturating_sub(start.0) < delay {
            continue;
        }
        match content {
            None => commands.trigger(TooltipRequest { entity, tier }),
            Some(content) if shift_changed && content.tier != tier => {
                commands.trigger(TooltipRequest { entity, tier });
            }
            Some(_) => {}
        }
    }
}

/// Drops the tooltip `entity` hosts, whether hover or a request raised it.
///
/// The counterpart to [`TooltipRequest`]: a request stands until the pointer
/// leaves the host, so a caller that raised one without hovering has to say
/// when it is done with it.
pub fn clear_tooltip(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).try_remove::<TooltipContent>();
    commands.queue(move |world: &mut World| {
        let stale: Vec<Entity> = world
            .query::<(Entity, &TooltipHost)>()
            .iter(world)
            .filter(|(_, host)| host.0 == entity)
            .map(|(e, _)| e)
            .collect();
        for tooltip in stale {
            if let Ok(entity) = world.get_entity_mut(tooltip) {
                entity.despawn();
            }
        }
    });
}

/// `SlottedUiSet::Render`: despawns any tooltip whose host node is gone.
///
/// A tooltip lives under [`TooltipLayer`], not under the screen that owns the
/// node it describes, so despawning a screen root (or one slot of it) leaves
/// the tooltip on screen with nothing behind it. `tooltip_delay` cannot clean
/// these up: it only looks at entities that still have a `Hovered`.
pub fn despawn_orphan_tooltips(
    hosts: Query<(Entity, &TooltipHost)>,
    alive: Query<Entity>,
    mut commands: Commands,
) {
    for (tooltip, host) in &hosts {
        if alive.get(host.0).is_err() {
            commands.entity(tooltip).despawn();
        }
    }
}

fn despawn_tooltips_for(
    hosts: &Query<(Entity, &TooltipHost)>,
    target: Entity,
    commands: &mut Commands,
) {
    for (tooltip, host) in hosts {
        if host.0 == target {
            commands.entity(tooltip).despawn();
        }
    }
}

/// The theme's token table, or the defaults when no theme has loaded.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ThemeTokens<'w> {
    active: Option<Res<'w, ActiveTheme>>,
    themes: Option<Res<'w, Assets<Theme>>>,
}

impl ThemeTokens<'_> {
    /// The tokens in force.
    pub fn get(&self) -> Tokens {
        self.active
            .as_ref()
            .zip(self.themes.as_ref())
            .and_then(|(a, t)| t.get(&a.0))
            .map_or_else(Tokens::default, |t| t.tokens.clone())
    }
}

/// Observer: compose the parts for the requested tier, record them on the
/// hovered entity as [`TooltipContent`], and spawn a `slotted:tooltip` under
/// the tooltip layer. Any tooltip already hosted by that entity is replaced.
pub fn show_tooltip(request: On<TooltipRequest>, mut commands: Commands) {
    let TooltipRequest { entity, tier } = *request;
    commands.queue(move |world: &mut World| {
        let stack = world.get::<ItemView>(entity).and_then(|v| v.stack.clone());
        let registries = world
            .get_resource::<Registries>()
            .map_or_else(|| empty_registries().clone(), |r| r.0.clone());
        let parts = world
            .get_resource::<TooltipParts>()
            .cloned()
            .unwrap_or_default();
        let mut composed = parts.compose(&TooltipCtx {
            stack: stack.as_ref(),
            registries: &registries,
            tier,
        });
        // The widget itself contributes last: a tank's amount line sits below
        // whatever the registered parts had to say (Phase 6 contract 1.2).
        let widget = world
            .get::<crate::semantic::WidgetNode>(entity)
            .map(|w| w.0.clone())
            .and_then(|kind| {
                world
                    .get_resource::<crate::screen::WidgetRegistry>()
                    .and_then(|r| r.get(&kind).cloned())
            });
        if let Some(widget) = widget {
            widget.tooltip(entity, world, &mut composed);
        }

        // A live icon source wants a turning 3D item at the head of the
        // tooltip rather than a flat cell. The viewport node is the same one
        // a screen can ask for by hand; headless it lays out with no camera.
        if let Some(subject) = live_subject(world, stack.as_ref()) {
            composed.insert(
                0,
                crate::def::UiNodeDef::Viewport {
                    subject: crate::def::ViewSubject::Item(subject),
                    size: LIVE_PREVIEW_SIZE,
                    tags: crate::def::Tags::default(),
                },
            );
        }

        // Replace any tooltip this entity already hosts.
        let existing: Vec<Entity> = world
            .query::<(Entity, &TooltipHost)>()
            .iter(world)
            .filter(|(_, host)| host.0 == entity)
            .map(|(e, _)| e)
            .collect();
        for old in existing {
            world.entity_mut(old).despawn();
        }

        if !world.entities().contains(entity) {
            return;
        }
        // Nothing to say (an empty slot, a widget with no parts): no tooltip.
        // Recording empty content would make the host look like it has one.
        if composed.is_empty() {
            world.entity_mut(entity).remove::<TooltipContent>();
            return;
        }
        world.entity_mut(entity).insert(TooltipContent {
            tier,
            parts: composed.clone(),
        });

        let Some(layer) = world
            .query_filtered::<Entity, With<TooltipLayer>>()
            .iter(world)
            .next()
        else {
            tracing::debug!("no tooltip layer; content recorded but nothing spawned");
            return;
        };
        let mut ctx = SpawnCtx {
            world,
            screen: layer,
            kind: crate::def::ScreenKind::new("slotted:tooltip"),
            menu: None,
            parent: layer,
        };
        let tooltip = crate::widgets::spawn_tooltip(&mut ctx, &composed);
        // Place it on the frame it is born, from the host rect that layout
        // already knows, and keep it hidden until `place_tooltips` has
        // clamped it against its own measured size. A tooltip is never drawn
        // at the layer's origin and never seen moving into position.
        let anchor = spawn_anchor(world, entity);
        let mut tooltip_entity = world.entity_mut(tooltip);
        tooltip_entity.insert((TooltipHost(entity), TooltipUnplaced, Visibility::Hidden));
        if let Some(anchor) = anchor
            && let Some(mut node) = tooltip_entity.get_mut::<Node>()
        {
            node.left = Val::Px(anchor.x);
            node.top = Val::Px(anchor.y);
        }
    });
}

/// Where a tooltip sits: just outside the host's right edge, level with its
/// top, clamped so the whole box stays inside the window.
///
/// Shared by the spawn-time placement in [`show_tooltip`] and the per-frame
/// [`place_tooltips`], so the position a tooltip is born at and the position
/// it settles at are computed the same way and it never jumps between them.
fn tooltip_position(host: Rect, size: Vec2, window: Vec2) -> Vec2 {
    let mut pos = Vec2::new(host.max.x + TOOLTIP_GAP, host.min.y);
    pos.x = pos.x.min((window.x - size.x).max(0.0));
    pos.y = pos.y.min((window.y - size.y).max(0.0));
    pos.max(Vec2::ZERO)
}

/// Gap between a host node and its tooltip, in logical px.
const TOOLTIP_GAP: f32 = 8.0;

/// The host's rect in logical px, or a slot-sized box around the pointer when
/// the host has not been laid out.
fn host_rect(
    hosts: &Query<(&ComputedNode, &UiGlobalTransform)>,
    pointers: &Query<(&PointerId, &PointerLocation)>,
    host: Entity,
) -> Option<Rect> {
    if let Ok((node, tf)) = hosts.get(host) {
        let scale = node.inverse_scale_factor();
        let size = node.size() * scale;
        if size.x > 0.0 && size.y > 0.0 {
            return Some(Rect::from_center_size(tf.translation * scale, size));
        }
    }
    // No layout for the host yet. The pointer is the next best anchor, and it
    // is where the player is looking.
    let p = pointers
        .iter()
        .find(|(id, _)| matches!(id, PointerId::Mouse))
        .and_then(|(_, l)| l.location().map(|l| l.position))?;
    Some(Rect::from_center_size(
        p,
        Vec2::splat(crate::widgets::SLOT_SIZE),
    ))
}

/// `PostUpdate`, before `UiSystems::Layout`: parks each tooltip beside its
/// host, clamps it to the window, and reveals it once it is in place.
///
/// This runs *before* the layout pass rather than after it. Writing `Node`
/// after `UiSystems::Layout` leaves the change for the next frame's
/// `UiGlobalTransform`, which is what made a tooltip flash at the corner of
/// the screen and then snap across. Reading the previous frame's
/// `ComputedNode` size and writing the position before layout means the
/// frame a tooltip first becomes visible is already the frame it is in the
/// right place.
pub fn place_tooltips(
    windows: Query<&Window, With<PrimaryWindow>>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    mut tooltips: Query<
        (
            Entity,
            &TooltipHost,
            &ComputedNode,
            &mut Node,
            &mut Visibility,
            Has<TooltipUnplaced>,
        ),
        With<SemanticRole>,
    >,
    hosts: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_size = Vec2::new(window.width(), window.height());
    for (entity, host, tooltip_node, mut node, mut visibility, unplaced) in &mut tooltips {
        let Some(host_rect) = host_rect(&hosts, &pointers, host.0) else {
            continue;
        };
        let size = tooltip_node.size() * tooltip_node.inverse_scale_factor();
        let pos = tooltip_position(host_rect, size, window_size);
        if node.left != Val::Px(pos.x) {
            node.left = Val::Px(pos.x);
        }
        if node.top != Val::Px(pos.y) {
            node.top = Val::Px(pos.y);
        }
        // A zero size means the tooltip has not been laid out yet, so the
        // clamp above is a guess. Wait one more frame before showing it.
        if unplaced && size.x > 0.0 && size.y > 0.0 {
            commands.entity(entity).remove::<TooltipUnplaced>();
            *visibility = Visibility::Inherited;
        }
    }
}

/// Edge of the live item preview at the head of a tooltip, in px. Two slots:
/// big enough to read the shape, small enough that the tooltip stays a
/// tooltip.
pub const LIVE_PREVIEW_SIZE: f32 = 72.0;

/// The position a tooltip for `host` starts at, before its own size is known:
/// beside the host, clamped to the window as if the tooltip were empty.
fn spawn_anchor(world: &mut World, host: Entity) -> Option<Vec2> {
    let (node, tf) = world
        .get::<ComputedNode>(host)
        .zip(world.get::<UiGlobalTransform>(host))?;
    let scale = node.inverse_scale_factor();
    let size = node.size() * scale;
    if size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    let rect = Rect::from_center_size(tf.translation * scale, size);
    let window = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(world)
        .ok()
        .map_or(Vec2::ZERO, |w| Vec2::new(w.width(), w.height()));
    if window.x <= 0.0 {
        return Some(Vec2::new(rect.max.x + TOOLTIP_GAP, rect.min.y));
    }
    Some(tooltip_position(rect, Vec2::ZERO, window))
}

/// The item a live [`Icons`](slotted_icons::Icons) source would rather show
/// in a viewport than as a flat cell, if the hovered stack is one.
fn live_subject(world: &World, stack: Option<&slotted_model::ItemStack>) -> Option<Namespaced> {
    let stack = stack?;
    let icons = world.get_resource::<slotted_icons::Icons>()?;
    if !matches!(icons.icon(stack), slotted_icons::IconRef::Live(_)) {
        return None;
    }
    world
        .get_resource::<Registries>()?
        .0
        .items
        .name_of(stack.id)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotted_model::ItemId;

    fn ctx(stack: Option<&ItemStack>, tier: TooltipTier) -> TooltipCtx<'_> {
        TooltipCtx {
            stack,
            registries: empty_registries(),
            tier,
        }
    }

    #[test]
    fn an_empty_slot_composes_nothing() {
        let mut parts = TooltipParts::default();
        register_builtin_parts(&mut parts);
        assert!(parts.compose(&ctx(None, TooltipTier::Compact)).is_empty());
    }

    #[test]
    fn a_stack_composes_a_name_and_a_count() {
        let mut parts = TooltipParts::default();
        register_builtin_parts(&mut parts);
        let stack = ItemStack::new(ItemId(7), 12);
        let nodes = parts.compose(&ctx(Some(&stack), TooltipTier::Compact));
        assert_eq!(nodes.len(), 2, "{nodes:?}");
        let UiNodeDef::Text { key, style, .. } = &nodes[0] else {
            panic!("expected text");
        };
        assert_eq!(key.0, "item#7");
        assert_eq!(*style, TextRole::Title);
        let UiNodeDef::Text { key, .. } = &nodes[1] else {
            panic!("expected text");
        };
        assert_eq!(key.0, "x12");
    }

    #[test]
    fn a_single_item_has_no_count_line() {
        let mut parts = TooltipParts::default();
        register_builtin_parts(&mut parts);
        let stack = ItemStack::new(ItemId(1), 1);
        assert_eq!(
            parts
                .compose(&ctx(Some(&stack), TooltipTier::Compact))
                .len(),
            1
        );
    }
}
