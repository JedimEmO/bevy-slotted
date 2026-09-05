//! `slotted:bookmarks_strip`: the bookmark row. Package B.

use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{
    ItemView, SemanticLabel, SemanticRole, SpawnCtx, Tags, UiNodeDef, Widget, widgets::SLOT_SIZE,
};

use super::{ShowsIngredient, ingredient_key, ingredient_stack, roles};
use crate::bookmarks::{Bookmark, Bookmarks};
use crate::events::OpenRecipes;
use crate::ingredient::{Ingredient, IngredientTypes};

/// Marker on the strip root.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct BookmarksStrip;

/// On one bookmark node: its position in [`Bookmarks::entries`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookmarkNode(pub usize);

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct BookmarksStripWidget;

impl Widget for BookmarksStripWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        ctx.spawn_node((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(4),
                min_height: px(SLOT_SIZE * 0.6),
                flex_shrink: 0.0,
                ..default()
            },
            SemanticRole::Panel,
            BookmarksStrip,
        ))
    }
}

/// `BrowserSet::Render`: one `Bookmark`-role node per entry, rebuilt when the
/// bookmark list changes.
pub fn render_bookmarks(
    bookmarks: Res<Bookmarks>,
    types: Res<IngredientTypes>,
    registries: Option<Res<slotted_ecs::Registries>>,
    strips: Query<(Entity, Option<&Children>), With<BookmarksStrip>>,
    nodes: Query<&BookmarkNode>,
    mut commands: Commands,
) {
    if !bookmarks.is_changed() {
        return;
    }
    for (strip, children) in &strips {
        for child in children.into_iter().flatten().copied() {
            if nodes.get(child).is_ok() {
                commands.entity(child).despawn();
            }
        }
        for (index, bookmark) in bookmarks.entries.iter().enumerate() {
            let Bookmark::Item(ingredient) = bookmark else {
                continue;
            };
            spawn_bookmark(
                &mut commands,
                strip,
                index,
                ingredient,
                &types,
                registries.as_deref(),
            );
        }
    }
}

fn spawn_bookmark(
    commands: &mut Commands,
    strip: Entity,
    index: usize,
    ingredient: &Ingredient,
    types: &IngredientTypes,
    registries: Option<&slotted_ecs::Registries>,
) {
    let key = ingredient_key(registries, ingredient);
    let stack = ingredient_stack(types, ingredient, 1);
    let node = commands
        .spawn((
            Node {
                width: px(SLOT_SIZE * 0.6),
                height: px(SLOT_SIZE * 0.6),
                ..default()
            },
            Themed(roles::BOOKMARK),
            SemanticRole::Bookmark,
            SemanticLabel(key.clone()),
            Tags::new().with("entry", &key),
            ItemView::new(stack),
            ShowsIngredient(ingredient.clone()),
            BookmarkNode(index),
            Hovered::default(),
            Pickable::default(),
            ChildOf(strip),
        ))
        .observe(on_bookmark_click)
        .id();
    commands.queue(move |world: &mut World| {
        slotted_ui::item::spawn_item_view_children(world, node);
    });
}

/// Observer on a bookmark: opens its recipes, like a card.
fn on_bookmark_click(
    click: On<Pointer<Click>>,
    nodes: Query<&ShowsIngredient, With<BookmarkNode>>,
    mut recipes: MessageWriter<OpenRecipes>,
) {
    if let Ok(shows) = nodes.get(click.entity) {
        recipes.write(OpenRecipes(shows.0.clone()));
    }
}
