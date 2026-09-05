//! Attaching a panel to a screen and tearing it down. Package B.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use slotted_theme::Themed;
use slotted_ui::{
    ScreenClosed, ScreenLayout, ScreenRoot, SemanticRole, Side, SpawnCtx, Tags, TestId, UiNodeDef,
    zbands,
};

use super::dock::BrowserLayout;
use super::{panel_def, panel_kind, roles};
use crate::plugin::{AttachPolicy, BrowserConfig};

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
                padding: UiRect::all(px(layout.padding)),
                ..default()
            },
            GlobalZIndex(zbands::BROWSER),
            TabGroup::new(1),
            ScreenRoot {
                kind: panel_kind(),
                menu: root.menu,
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
