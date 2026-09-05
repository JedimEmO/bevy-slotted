//! Ghost drag from a card to a Ghost or Filter slot. Package B.
//!
//! Dragging a card puts a translucent copy of it under the carried layer, so
//! it draws above every screen. Dropping it on a slot asks that screen's
//! handler what the drop means; the handler returns click actions, and those
//! go out as ordinary `MenuAction`s, so a ghost drop conserves items exactly
//! as a click does.

use bevy::picking::events::{DragDrop, DragEnd, DragStart, Pointer};
use bevy::prelude::*;
use slotted_ecs::{MenuAction, SlotRef};
use slotted_ui::{CarriedLayer, ItemView, ScreenRoot};

use super::{ShowsIngredient, dock, ingredient_stack};
use crate::handlers::{GhostDrop, ScreenHandlers};
use crate::ingredient::{Ingredient, IngredientTypes};

/// On the ghost node under the carried layer while a card is dragged.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct GhostDrag(pub Ingredient);

/// Observer of `Pointer<DragStart>` on a card: spawns the ghost.
pub fn on_card_drag_start(
    start: On<Pointer<DragStart>>,
    cards: Query<&ShowsIngredient>,
    layers: Query<Entity, With<CarriedLayer>>,
    existing: Query<Entity, With<GhostDrag>>,
    types: Res<IngredientTypes>,
    mut commands: Commands,
) {
    let Ok(shows) = cards.get(start.entity) else {
        return;
    };
    let Ok(layer) = layers.single() else {
        return;
    };
    for old in &existing {
        commands.entity(old).despawn();
    }
    let stack = ingredient_stack(&types, &shows.0, 1);
    let ghost = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: px(slotted_ui::widgets::SLOT_SIZE),
                height: px(slotted_ui::widgets::SLOT_SIZE),
                ..default()
            },
            ItemView::new(stack),
            GhostDrag(shows.0.clone()),
            Pickable::IGNORE,
            ChildOf(layer),
        ))
        .id();
    commands.queue(move |world: &mut World| {
        slotted_ui::item::spawn_item_view_children(world, ghost);
    });
}

/// Observer of `Pointer<DragEnd>` on a card: the ghost goes away whether or
/// not the drop landed on anything.
pub fn on_card_drag_end(
    _end: On<Pointer<DragEnd>>,
    ghosts: Query<Entity, With<GhostDrag>>,
    mut commands: Commands,
) {
    for ghost in &ghosts {
        commands.entity(ghost).despawn();
    }
}

/// Observer of `Pointer<DragDrop>` on a `SlotRef`: asks the screen handler's
/// `ghost_drop` and triggers the returned actions.
///
/// Exclusive work is queued as a command because `ScreenHandler::ghost_drop`
/// takes a `&World`.
pub fn on_ghost_drop(drop_event: On<Pointer<DragDrop>>, mut commands: Commands) {
    let target = drop_event.entity;
    let dropped = drop_event.event.dropped;
    commands.queue(move |world: &mut World| {
        let Some(ingredient) = world
            .get::<ShowsIngredient>(dropped)
            .map(|shows| shows.0.clone())
        else {
            return;
        };
        let Some(slot) = world.get::<SlotRef>(target).copied() else {
            return;
        };
        let Some(screen) = screen_of_menu(world, slot.menu) else {
            return;
        };
        let Some(root) = world.get::<ScreenRoot>(screen).cloned() else {
            return;
        };
        let Some(handler) = world.resource::<ScreenHandlers>().get(&root.kind).cloned() else {
            return;
        };
        let geometry = geometry(world, screen, &root);
        let decision = handler.ghost_drop(&geometry, world, target, &ingredient);
        let GhostDrop::Accept(actions) = decision else {
            return;
        };
        let mut queue = world.commands();
        for action in actions {
            queue.trigger(MenuAction {
                entity: slot.menu,
                action,
            });
        }
        world.flush();
    });
}

fn screen_of_menu(world: &mut World, menu: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &ScreenRoot)>()
        .iter(world)
        .find(|(_, root)| root.menu == Some(menu) && root.kind != super::panel_kind())
        .map(|(entity, _)| entity)
}

fn geometry(
    world: &mut World,
    screen: Entity,
    root: &ScreenRoot,
) -> crate::handlers::ScreenGeometry {
    let mut state = bevy::ecs::system::SystemState::<(
        slotted_ui::Exclusions,
        Query<(&ComputedNode, &bevy::ui::ui_transform::UiGlobalTransform)>,
        Query<&Children>,
        Query<&Window>,
    )>::new(world);
    let Ok((exclusions, nodes, children, windows)) = state.get(world) else {
        return crate::handlers::ScreenGeometry {
            screen,
            kind: root.kind.clone(),
            menu: root.menu,
            root_rect: Rect::default(),
            window: Rect::default(),
            exclusions: Vec::new(),
        };
    };
    dock::geometry_of(
        screen,
        root,
        dock::window_rect(&windows),
        &nodes,
        &children,
        &exclusions,
    )
}
