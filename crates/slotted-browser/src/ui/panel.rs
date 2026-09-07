//! Attaching a panel to a screen and tearing it down. Package B.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use slotted_theme::Themed;
use slotted_ui::{
    LocKey, Localization, ScreenClosed, ScreenLayout, ScreenRoot, SemanticRole, Side, SpawnCtx,
    Tags, TestId, UiNodeDef, zbands,
};

use super::dock::BrowserLayout;
use super::recipe_view::RecipeViewRoot;
use super::{panel_def, panel_kind, roles};
use crate::plugin::{AttachPolicy, BrowserConfig};

// ---------------------------------------------------------------------------
// The panel's own chrome, localised
// ---------------------------------------------------------------------------

/// Localisation keys for the strings the browser panel writes itself, as
/// opposed to the item and category names it reads out of the registries.
///
/// Every one is resolved through [`slotted_ui::Localization`] and falls back
/// to the English literal the panel drew before there was a catalogue, so a
/// game that installs no [`Localizer`](slotted_ui::Localizer) sees exactly
/// what it saw before. `assets/locale/en-US.ftl` carries the same English as
/// the base pack's layer, with the dots written as dashes.
pub mod keys {
    /// The grey prompt in the empty search field.
    pub const SEARCH_PLACEHOLDER: &str = "browser.search.placeholder";
    /// The search field's screen-reader label.
    pub const SEARCH_LABEL: &str = "browser.search.label";
    /// The footer while the index is still building.
    pub const STATUS_INDEXING: &str = "browser.status.indexing";
    /// The result count, with a `count` argument: `{ $count } items`, or a
    /// Fluent plural selector (menus M1 contract 2.3).
    pub const STATUS_COUNT: &str = "browser.status.count";
    /// The footer's hotkey hint.
    pub const STATUS_HINTS: &str = "browser.status.hints";
    /// The "used in" heading when the list has entries.
    pub const USES_TITLE: &str = "browser.uses.title";
    /// The "used in" heading when nothing consumes the item.
    pub const USES_EMPTY: &str = "browser.uses.empty";
    /// The transfer pill.
    pub const BUTTON_TRANSFER: &str = "browser.button.transfer";
    /// The history-back pill.
    pub const BUTTON_BACK: &str = "browser.button.back";
    /// The history-forward pill.
    pub const BUTTON_FORWARD: &str = "browser.button.forward";
}

/// A text node whose content is a [`keys`] entry plus the English literal to
/// draw when nothing resolves it.
///
/// [`render_chrome`] repaints every one of these whenever the
/// [`Localization`] resource is replaced, which is how a language switch at
/// runtime reaches strings that were written once at spawn time.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ChromeText {
    /// The key to resolve.
    pub key: LocKey,
    /// What to draw when nothing defines it.
    pub fallback: String,
}

impl ChromeText {
    /// A chrome string.
    pub fn new(key: &str, fallback: impl Into<String>) -> Self {
        Self {
            key: LocKey(key.to_owned()),
            fallback: fallback.into(),
        }
    }
}

/// `loc`'s text for `key`, else the English literal `fallback`.
///
/// Deliberately not [`Localization::text`], which falls back to the key: a
/// panel that has no catalogue must read as English, not as `browser.status.hints`.
pub fn chrome(loc: &Localization, key: &str, fallback: &str) -> String {
    loc.resolve(&LocKey(key.to_owned()))
        .unwrap_or_else(|| fallback.to_owned())
}

/// [`chrome`] for spawn code that only has the world, including a world with
/// no [`Localization`] in it at all.
pub fn chrome_in(world: &World, key: &str, fallback: &str) -> String {
    world
        .get_resource::<Localization>()
        .map_or_else(|| fallback.to_owned(), |loc| chrome(loc, key, fallback))
}

/// `BrowserSet::Render`: re-resolve every [`ChromeText`] when it appears and
/// whenever the catalogue behind [`Localization`] is replaced.
///
/// A replaced catalogue also invalidates the recipe view's title and its
/// "used in" cards, which are drawn from item display names, so the open page
/// is marked dirty and redrawn on the next pass.
pub fn render_chrome(
    loc: Res<Localization>,
    mut texts: Query<(Ref<ChromeText>, &mut Text)>,
    mut views: Query<&mut RecipeViewRoot>,
) {
    let relocalised = loc.is_changed() && !loc.is_added();
    for (chrome_text, mut text) in &mut texts {
        if !relocalised && !chrome_text.is_added() {
            continue;
        }
        let want = chrome(&loc, &chrome_text.key.0, &chrome_text.fallback);
        if text.0 != want {
            text.0 = want;
        }
    }
    if relocalised {
        for mut view in &mut views {
            view.dirty = true;
        }
    }
}

/// On a panel root: which screen it belongs to.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserPanel {
    /// The screen root the panel is docked beside.
    pub screen: Entity,
}

/// Observer of `ScreenLayout`: spawns the panel for a screen the policy
/// covers. One panel per screen; a second layout event is ignored.
///
/// `ScreenLayout` rather than `ScreenSpawned` because the side is read off the
/// screen's rectangle, which does not exist until the first layout. The panel
/// starts hidden; `dock_panels` runs later in the same `PostUpdate` and gives
/// it a side, a rectangle and a visibility.
pub fn attach_panel(layout: On<ScreenLayout>, mut commands: Commands) {
    let screen = layout.entity;
    commands.queue(move |world: &mut World| {
        let Some(root) = world.get::<ScreenRoot>(screen).cloned() else {
            return;
        };
        // The panel is itself a `ScreenRoot`, so it lays out and would
        // otherwise grow a panel of its own.
        if root.kind == panel_kind() {
            return;
        }
        let config = world.resource::<BrowserConfig>();
        let attach = match config.attach {
            AttachPolicy::AllMenus => root.menu.is_some(),
            AttachPolicy::Registered => world
                .resource::<crate::handlers::ScreenHandlers>()
                .contains(&root.kind),
        };
        if !attach {
            return;
        }
        let already = world
            .query::<&BrowserPanel>()
            .iter(world)
            .any(|p| p.screen == screen);
        if already {
            return;
        }
        spawn_panel(world, screen, &root);
    });
}

/// The panel root plus its five widgets. The widgets are spawned directly
/// under the root, so the semantic tree is
/// `Browser > TextField, Panel, Grid, Panel, RecipeView` with no wrapper in
/// between (contract section 7).
fn spawn_panel(world: &mut World, screen: Entity, root: &ScreenRoot) {
    let def = panel_def(Side::Right);
    let UiNodeDef::Panel {
        layout, children, ..
    } = &def
    else {
        return;
    };
    let panel = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                row_gap: px(layout.gap),
                padding: UiRect::all(px(layout.padding.top)),
                ..default()
            },
            GlobalZIndex(zbands::BROWSER),
            TabGroup::new(1),
            ScreenRoot {
                kind: panel_kind(),
                menu: root.menu,
                // The browser panel is a sibling of the screen, not a stack
                // entry; `overlay` keeps it out of `Back` and HUD counting.
                presentation: slotted_ui::Presentation {
                    mode: slotted_ui::PresentationMode::Overlay,
                    ..Default::default()
                },
                initial_focus: None,
            },
            SemanticRole::Browser,
            TestId::new("browser"),
            Themed(roles::PANEL),
            Tags::new().with("side", Side::Right.as_str()),
            BrowserPanel { screen },
            Visibility::Hidden,
        ))
        .id();

    let ctx = &mut SpawnCtx {
        world,
        screen: panel,
        kind: panel_kind(),
        menu: root.menu,
        parent: panel,
    };
    ctx.spawn_children(panel, children);
}

/// Observer of `ScreenClosed`: despawns the screen's panel.
pub fn detach_panel(
    closed: On<ScreenClosed>,
    panels: Query<(Entity, &BrowserPanel)>,
    mut commands: Commands,
) {
    for (entity, panel) in &panels {
        if panel.screen == closed.entity {
            commands.entity(entity).despawn();
        }
    }
}

/// The panel attached to `screen`, if any. The harness's `is_attached`.
pub fn panel_of(world: &World, screen: Entity) -> Option<Entity> {
    let mut query = world.try_query::<(Entity, &BrowserPanel)>()?;
    query
        .iter(world)
        .find(|(_, panel)| panel.screen == screen)
        .map(|(entity, _)| entity)
}

/// The placement computed for `screen`'s panel, if it is docked.
pub fn layout_of(world: &World, screen: Entity) -> Option<BrowserLayout> {
    panel_of(world, screen).and_then(|panel| world.get::<BrowserLayout>(panel).copied())
}

/// The panel a node belongs to, walking up the hierarchy.
pub fn panel_ancestor(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if world.get::<BrowserPanel>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}
