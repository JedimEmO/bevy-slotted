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
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use slotted_ecs::Registries;
use slotted_model::ItemStack;
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
        let Some(start) = start else { continue };
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
        let composed = parts.compose(&TooltipCtx {
            stack: stack.as_ref(),
            registries: &registries,
            tier,
        });

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
        world.entity_mut(tooltip).insert(TooltipHost(entity));
    });
}

/// `SlottedUiSet::Layout`: parks each tooltip beside its host and clamps it
/// to the window.
pub fn place_tooltips(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut tooltips: Query<(&TooltipHost, &ComputedNode, &mut Node), With<SemanticRole>>,
    hosts: Query<(&ComputedNode, &UiGlobalTransform)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_size = Vec2::new(window.width(), window.height());
    for (host, tooltip_node, mut node) in &mut tooltips {
        let Ok((host_node, host_tf)) = hosts.get(host.0) else {
            continue;
        };
        let scale = host_node.inverse_scale_factor();
        let host_rect =
            Rect::from_center_size(host_tf.translation * scale, host_node.size() * scale);
        let size = tooltip_node.size() * tooltip_node.inverse_scale_factor();
        let mut pos = Vec2::new(host_rect.max.x + 8.0, host_rect.min.y);
        pos.x = pos.x.min((window_size.x - size.x).max(0.0));
        pos.y = pos.y.min((window_size.y - size.y).max(0.0));
        pos = pos.max(Vec2::ZERO);
        if node.left != Val::Px(pos.x) {
            node.left = Val::Px(pos.x);
        }
        if node.top != Val::Px(pos.y) {
            node.top = Val::Px(pos.y);
        }
    }
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
