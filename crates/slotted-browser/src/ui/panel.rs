//! Attaching a panel to a screen and tearing it down. Package B.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use slotted_ui::{
    ScreenClosed, ScreenLayout, ScreenRoot, SemanticRole, Side, SpawnCtx, TestId, zbands,
};

use super::{panel_def, panel_kind};
use crate::handlers::ScreenHandlers;
use crate::plugin::{AttachPolicy, BrowserConfig};

/// On a panel root: which screen it belongs to.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserPanel {
    /// The screen root the panel is docked beside.
    pub screen: Entity,
}

/// Observer of `ScreenLayout`: spawns the panel for a screen the policy
/// covers. One panel per screen; a second layout event is ignored.
pub fn attach_panel(layout: On<ScreenLayout>, mut commands: Commands) {
    let screen = layout.entity;
    commands.queue(move |world: &mut World| {
        let Some(root) = world.get::<ScreenRoot>(screen).cloned() else {
            return;
        };
        let config = world.resource::<BrowserConfig>();
        let attach = match config.attach {
            AttachPolicy::AllMenus => root.menu.is_some(),
            AttachPolicy::Registered => world.resource::<ScreenHandlers>().contains(&root.kind),
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
        let panel = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
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
                BrowserPanel { screen },
                Visibility::Hidden,
            ))
            .id();
        let mut ctx = SpawnCtx {
            world,
            screen: panel,
            kind: panel_kind(),
            menu: root.menu,
            parent: panel,
        };
        // PHASE3-IMPL: B — pick the side from `dock::free_space` before spawning.
        ctx.spawn_child(&panel_def(Side::Right));
    });
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
