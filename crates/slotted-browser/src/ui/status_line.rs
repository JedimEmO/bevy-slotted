//! `slotted:status_line`: the panel's footer. Package B.
//!
//! The moodboard's `.bfoot` row: a result count on the left and the hotkey
//! hint on the right. It is also the panel's loading affordance, because the
//! card grid has nothing to bind while `IndexState::Building` and an empty
//! grid with no explanation reads as "no items" rather than "not yet".
//!
//! Neither node carries a [`SemanticRole`](slotted_ui::SemanticRole), so the
//! screen tree elides the whole footer and the contract's tree shape is
//! unchanged.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{SpawnCtx, TestId, UiNodeDef, Widget};

use super::panel::{ChromeText, chrome, chrome_in, keys};
use super::roles;
use crate::index::IndexState;
use crate::runtime::BrowserRuntime;

/// The English the hotkey hint falls back to.
const HINTS: &str = "R recipes / U uses / A bookmark";

/// Marker on the count text, the left half of the footer.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct StatusText;

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct StatusLineWidget;

impl Widget for StatusLineWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        let row = ctx.spawn_node((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            Pickable::IGNORE,
        ));
        // `render_status` owns this one's text outright, so it carries no
        // `ChromeText`: it re-resolves through `Res<Localization>` every pass.
        let indexing = chrome_in(ctx.world, keys::STATUS_INDEXING, "indexing…");
        ctx.world.spawn((
            Node::default(),
            Text::new(indexing),
            Themed(roles::STATUS),
            TestId::new("browser.status"),
            StatusText,
            Pickable::IGNORE,
            ChildOf(row),
        ));
        let hints = chrome_in(ctx.world, keys::STATUS_HINTS, HINTS);
        ctx.world.spawn((
            Node::default(),
            Text::new(hints),
            Themed(roles::HINT),
            ChromeText::new(keys::STATUS_HINTS, HINTS),
            Pickable::IGNORE,
            ChildOf(row),
        ));
        row
    }
}

/// `BrowserSet::Render`: the footer says whether the index has landed and, once
/// it has, how many entries the query left.
///
/// The count and its noun are composed here rather than interpolated into one
/// key, because [`Localizer`](slotted_ui::Localizer) resolves a key with no
/// arguments; a locale that needs a different word order can still say so by
/// giving the two nouns whatever text it likes.
pub fn render_status(
    index: Res<IndexState>,
    runtime: Res<BrowserRuntime>,
    loc: Res<slotted_ui::Localization>,
    mut texts: Query<&mut Text, With<StatusText>>,
    fresh: Query<(), Added<StatusText>>,
    mut last: Local<Option<(bool, usize)>>,
) {
    // The line is a function of exactly two numbers and the catalogue, so
    // nothing is composed until one of them moves. Without this the footer
    // resolves two locale keys and formats a string on every frame of every
    // open browser, for ever, and throws all of it away: the write below was
    // guarded but the work above it was not. A key nothing defines makes it
    // worse rather than better, because the resolve then walks every locale
    // layer before giving up.
    //
    // The two numbers are read here rather than taken from `Res::is_changed`,
    // because `BrowserRuntime` is written by systems that change things the
    // status line does not show, and one of them touches it every frame.
    let now = (index.ready().is_some(), runtime.visible.len());
    if *last == Some(now) && !loc.is_changed() && fresh.is_empty() {
        return;
    }
    *last = Some(now);
    let want = if index.ready().is_some() {
        let n = runtime.visible.len();
        let unit = if n == 1 {
            chrome(&loc, keys::STATUS_ITEM, "item")
        } else {
            chrome(&loc, keys::STATUS_ITEMS, "items")
        };
        format!("{n} {unit}")
    } else {
        chrome(&loc, keys::STATUS_INDEXING, "indexing…")
    };
    for mut text in &mut texts {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}
