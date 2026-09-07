//! Hotkeys and the keyboard-focus guard. Package B.

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::EditableText;
use slotted_ecs::SlotRef;
use slotted_ui::{ItemView, ScreenRoot, UiAction, UiActionClaims, UiActionEvent, UiBindings};

use super::ShowsIngredient;
use super::dock;
use super::search_field::SearchField;
use crate::bookmarks::Bookmark;
use crate::events::{BookmarkToggled, OpenRecipes, OpenUses, RecipeNav};
use crate::handlers::ScreenHandlers;
use crate::ingredient::Ingredient;
use crate::runtime::BrowserRuntime;

/// The ingredient under the pointer, refreshed every frame in
/// `BrowserSet::Input`.
///
/// A card, a bookmark, a recipe slot and a menu slot all resolve to the same
/// thing here, so R and U do not care which one the pointer is over.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct HoverTarget(pub Option<Ingredient>);

/// `BrowserSet::Input`: `has_keyboard_focus` is true while `InputFocus` is on
/// an entity with `EditableText`. Runs before every hotkey system.
pub fn track_keyboard_focus(
    focus: Option<Res<InputFocus>>,
    fields: Query<(), With<EditableText>>,
    mut runtime: ResMut<BrowserRuntime>,
) {
    let focused = focus
        .and_then(|f| f.get())
        .is_some_and(|e| fields.contains(e));
    if runtime.has_keyboard_focus != focused {
        runtime.has_keyboard_focus = focused;
    }
}

/// `BrowserSet::Input`: what the pointer is over, as an [`Ingredient`].
///
/// Exclusive because the last fallback, `ScreenHandler::stack_under_cursor`,
/// takes a `&World` so a handler can read a widget the browser knows nothing
/// about (a fluid tank).
pub fn track_hover_target(world: &mut World) {
    let mut direct = world.query::<(&Hovered, &ShowsIngredient)>();
    if let Some((_, shows)) = direct.iter(world).find(|(hovered, _)| hovered.get()) {
        let found = HoverTarget(Some(shows.0.clone()));
        if *world.resource::<HoverTarget>() != found {
            world.insert_resource(found);
        }
        return;
    }
    let mut slots = world.query::<(&Hovered, &SlotRef, &ItemView)>();
    if let Some((_, _, view)) = slots.iter(world).find(|(hovered, _, _)| hovered.get()) {
        let found = HoverTarget(view.stack.as_ref().map(|s| Ingredient::item(s.id)));
        if *world.resource::<HoverTarget>() != found {
            world.insert_resource(found);
        }
        return;
    }
    let found = HoverTarget(handler_target(world));
    if *world.resource::<HoverTarget>() != found {
        world.insert_resource(found);
    }
}

/// The last fallback: every registered handler is asked what sits under the
/// pointer on its own screen.
fn handler_target(world: &mut World) -> Option<Ingredient> {
    let cursor = world
        .query::<&bevy::picking::pointer::PointerLocation>()
        .iter(world)
        .find_map(|l| l.location().map(|l| l.position))?;
    let handlers = world.resource::<ScreenHandlers>().clone();
    let screens: Vec<(Entity, ScreenRoot)> = world
        .query::<(Entity, &ScreenRoot)>()
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (entity, root) in screens {
        let Some(handler) = handlers.get(&root.kind).cloned() else {
            continue;
        };
        let geometry = geometry_for(world, entity, &root);
        if let Some(found) = handler.stack_under_cursor(&geometry, world, cursor) {
            return Some(found);
        }
    }
    None
}

fn geometry_for(
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
        dock::window_rect(&windows, slotted_ui::ui_scale_of(world)),
        &nodes,
        &children,
        &exclusions,
    )
}

/// `BrowserSet::Input`, after `UiActionEmit`: R, U, A, Ctrl+F, Backspace
/// from [`KeyMappings`](crate::runtime::KeyMappings), and the `Back` action
/// (menus contract 2.5).
///
/// `Back` blurs a focused search field, or closes an open recipe view, and
/// is claimed either way so the stack does not pop the screen on the same
/// press. When neither applies it is left unclaimed. The raw `close` key
/// stays for people who rebind it; while it is one of `Back`'s own keys
/// (Escape by default) the raw path is skipped so one press is not handled
/// twice.
///
/// Everything except the focus and blur keys is gated on
/// `has_keyboard_focus`, so typing `r` into the search field never opens a
/// recipe page.
#[allow(clippy::too_many_arguments)]
pub fn browser_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut actions: MessageReader<UiActionEvent>,
    bindings: Res<UiBindings>,
    mut claims: ResMut<UiActionClaims>,
    runtime: Res<BrowserRuntime>,
    hovered: Res<HoverTarget>,
    fields: Query<Entity, With<SearchField>>,
    focus: Option<ResMut<InputFocus>>,
    mut recipes: MessageWriter<OpenRecipes>,
    mut uses: MessageWriter<OpenUses>,
    mut bookmarks: MessageWriter<BookmarkToggled>,
    mut nav: MessageWriter<RecipeNav>,
) {
    let mapping = &runtime.keys;
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    let back_action = actions.read().any(|e| e.action == UiAction::Back);
    let close_is_back = bindings
        .keys
        .get(&UiAction::Back)
        .is_some_and(|codes| codes.contains(&mapping.close));
    let raw_close = !close_is_back && keys.just_pressed(mapping.close);
    let close = back_action || raw_close;

    // Focus and blur are the two keys that must work while the field has the
    // keyboard, so they come before the guard.
    if let Some(mut focus) = focus {
        let (wants_ctrl, code) = mapping.focus_search;
        if keys.just_pressed(code)
            && ctrl == wants_ctrl
            && let Some(field) = fields.iter().next()
        {
            focus.set(field, FocusCause::Navigated);
            return;
        }
        if close && runtime.has_keyboard_focus {
            focus.clear();
            if back_action {
                claims.claim(UiAction::Back);
            }
            return;
        }
    }

    if runtime.has_keyboard_focus {
        return;
    }

    if close && runtime.open.is_some() {
        nav.write(RecipeNav::Close);
        if back_action {
            claims.claim(UiAction::Back);
        }
    }
    if keys.just_pressed(mapping.back) {
        nav.write(RecipeNav::Back);
    }
    let Some(ingredient) = hovered.0.clone() else {
        return;
    };
    if keys.just_pressed(mapping.recipes) {
        recipes.write(OpenRecipes(ingredient.clone()));
    }
    if keys.just_pressed(mapping.uses) {
        uses.write(OpenUses(ingredient.clone()));
    }
    if keys.just_pressed(mapping.bookmark) {
        bookmarks.write(BookmarkToggled(Bookmark::Item(ingredient)));
    }
}
