//! `slotted:chip_row`: one toggle chip per recipe category. Package B.
//!
//! A chip is a shortcut for one `%category` term, so chips and typing are the
//! same feature: toggling a chip rewrites the query, and a query typed by
//! hand lights the matching chips.

use bevy::picking::events::{Click, Pointer};
use bevy::prelude::*;
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{SemanticLabel, SemanticRole, SpawnCtx, Tags, UiNodeDef, Widget};

/// Marker on a chip's label text, so the selected state can repaint it.
#[derive(bevy::prelude::Component, Debug, Default, Clone, Copy)]
pub struct ChipText;

use super::panel::{ChromeText, chrome};
use super::roles;
use crate::events::SearchChanged;
use crate::recipes::Categories;
use crate::runtime::BrowserRuntime;

/// On a chip: which category it filters.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Chip(pub crate::category::CategoryId);

/// Marker on the chip row root.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ChipRowMarker;

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChipRowWidget;

impl Widget for ChipRowWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        ctx.spawn_node((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            SemanticRole::Panel,
            ChipRowMarker,
        ))
    }
}

/// The `%category` term for an id.
pub fn category_term(id: &crate::category::CategoryId) -> String {
    format!("%{}", id.0)
}

/// The query with `term` added if absent, removed if present.
pub fn toggle_term(query: &str, term: &str) -> String {
    let kept: Vec<&str> = query.split_whitespace().filter(|t| *t != term).collect();
    if kept.len() == query.split_whitespace().count() {
        let mut out = kept.join(" ");
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(term);
        out
    } else {
        kept.join(" ")
    }
}

/// `BrowserSet::Render`: keep one chip per registered category, and light the
/// ones the current query selects.
pub fn render_chips(
    categories: Res<Categories>,
    loc: Res<slotted_ui::Localization>,
    runtime: Res<BrowserRuntime>,
    rows: Query<(Entity, Option<&Children>), With<ChipRowMarker>>,
    mut chips: Query<(&Chip, &mut Themed, Option<&Children>)>,
    mut texts: Query<&mut Themed, (With<ChipText>, Without<Chip>)>,
    mut commands: Commands,
) {
    let wanted: Vec<(crate::category::CategoryId, slotted_ui::LocKey)> =
        categories.iter().map(|c| (c.id(), c.title_key())).collect();
    for (row, children) in &rows {
        let have: Vec<crate::category::CategoryId> = children
            .into_iter()
            .flatten()
            .filter_map(|c| chips.get(*c).ok().map(|(chip, _, _)| chip.0.clone()))
            .collect();
        if have != wanted.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>() {
            for child in children.into_iter().flatten().copied() {
                if chips.get(child).is_ok() {
                    commands.entity(child).despawn();
                }
            }
            for (id, title_key) in &wanted {
                spawn_chip(&mut commands, row, id.clone(), title_key.clone(), &loc);
            }
        }
    }
    for (chip, mut themed, children) in &mut chips {
        let selected = runtime
            .filter_text
            .split_whitespace()
            .any(|t| t == category_term(&chip.0));
        let want = if selected {
            roles::CHIP_ACTIVE
        } else {
            roles::CHIP
        };
        if themed.0 != want {
            themed.0 = want;
        }
        let want_text = if selected {
            roles::CHIP_TEXT_ACTIVE
        } else {
            roles::CHIP_TEXT
        };
        for child in children.into_iter().flatten() {
            if let Ok(mut text) = texts.get_mut(*child)
                && text.0 != want_text
            {
                text.0.clone_from(&want_text);
            }
        }
    }
}

/// One chip. The `entry` tag and the semantic label stay the category's raw
/// path, so a locator names `crafting` whatever language the chip is drawn in;
/// only the text a player reads is localised.
fn spawn_chip(
    commands: &mut Commands,
    row: Entity,
    id: crate::category::CategoryId,
    title_key: slotted_ui::LocKey,
    loc: &slotted_ui::Localization,
) {
    let label = id.0.path().to_owned();
    let text = chrome(loc, &title_key.0, &label);
    commands
        .spawn((
            Node {
                padding: UiRect::axes(px(10), px(4)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Themed(roles::CHIP),
            SemanticRole::Chip,
            SemanticLabel(label.clone()),
            Tags::new().with("category", id.0.as_ref()),
            Chip(id),
            Pickable::default(),
            ChildOf(row),
        ))
        .with_child((
            Text::new(text),
            Themed(roles::CHIP_TEXT),
            ChromeText {
                key: title_key,
                fallback: label,
            },
            ChipText,
            Pickable::IGNORE,
        ))
        .observe(on_chip_click);
}

/// Observer on a chip: toggle its `%category` term in the query.
fn on_chip_click(
    click: On<Pointer<Click>>,
    chips: Query<&Chip>,
    runtime: Res<BrowserRuntime>,
    mut changed: MessageWriter<SearchChanged>,
) {
    let Ok(chip) = chips.get(click.entity) else {
        return;
    };
    changed.write(SearchChanged {
        text: toggle_term(&runtime.filter_text, &category_term(&chip.0)),
    });
}

#[cfg(test)]
mod tests {
    use super::toggle_term;

    #[test]
    fn a_term_goes_on_and_comes_off_again() {
        assert_eq!(toggle_term("", "%demo:crafting"), "%demo:crafting");
        assert_eq!(toggle_term("iron", "%demo:crafting"), "iron %demo:crafting");
        assert_eq!(toggle_term("iron %demo:crafting", "%demo:crafting"), "iron");
        assert_eq!(toggle_term("%demo:crafting", "%demo:crafting"), "");
    }
}
