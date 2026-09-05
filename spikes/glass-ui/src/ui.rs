//! The glass panel and its slot grid, expressed as `bsn!` scene functions.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;
use bevy::ui::Pressed;
use bevy::text::FontSize;
use bevy::ui_widgets::Button;

use crate::materials::GlassPanelMaterial;

// "Obsidian Glass" palette from docs/research/research-modern-ui.md section 5A.
pub const BASE: Color = Color::srgb(0.043, 0.055, 0.078); // #0B0E14
pub const PANEL: Color = Color::srgb(0.078, 0.102, 0.141); // #141A24
pub const HAIRLINE: Color = Color::srgb(0.165, 0.204, 0.275); // #2A3446
pub const ACCENT: Color = Color::srgb(0.498, 0.820, 1.0); // #7FD1FF
pub const WARM: Color = Color::srgb(1.0, 0.706, 0.329); // #FFB454
pub const TEXT: Color = Color::srgb(0.902, 0.929, 0.953); // #E6EDF3

pub const SLOT: f32 = 44.0;
pub const COLS: usize = 9;
pub const ROWS: usize = 3;
/// Slots that get the legendary glow material.
pub const LEGENDARY: [usize; 2] = [4, 22];
/// Slot that gets a live `ViewportNode`.
pub const VIEWPORT_SLOT: usize = 13;

/// Marks a slot so the hover system and the decorator can find it.
#[derive(Component, Default, Clone)]
pub struct Slot {
    pub index: usize,
}

#[derive(Component, Default, Clone)]
pub struct ViewportSlot;

fn slot_node() -> Node {
    Node {
        width: px(SLOT),
        height: px(SLOT),
        border: UiRect::all(px(1.0)),
        border_radius: BorderRadius::all(px(8.0)),
        ..default()
    }
}

/// A single 44px slot: matte translucent fill, hairline border, 8px radius, and a child node
/// that draws the 1px inner top highlight.
///
/// Note `BorderRadius` is a *field of `Node`* in 0.19, not a separate component as in 0.17.
pub fn slot(index: usize) -> impl Scene {
    bsn! {
        Slot { index: {index} }
        Button
        Hovered(false)
        template_value(slot_node())
        BackgroundColor({PANEL.with_alpha(0.72)})
        template_value(BorderColor::all(HAIRLINE.with_alpha(0.55)))
        Children [(
            // 1px inner top highlight. A child node rather than a shader keeps ordinary slots
            // material-free; every material node is its own draw call.
            Node {
                position_type: {PositionType::Absolute},
                left: px(1.0), right: px(1.0), top: px(1.0), height: px(1.0),
            }
            BackgroundColor({Color::WHITE.with_alpha(0.12)})
            Pickable::IGNORE
        )]
    }
}

/// A `ROWS x COLS` grid of slots.
///
/// `bsn!` has no loop syntax, so the children are built as a `Vec<impl Scene>` outside the macro
/// and spliced in with `{...}`. `Vec<S: Scene>` implements `SceneList`, so this just works.
pub fn slot_grid() -> impl Scene {
    let slots: Vec<_> = (0..COLS * ROWS).map(slot).collect();
    bsn! {
        Node {
            display: {Display::Grid},
            grid_template_columns: {vec![RepeatedGridTrack::px(COLS as u16, SLOT)]},
            grid_template_rows: {vec![RepeatedGridTrack::px(ROWS as u16, SLOT)]},
            row_gap: px(6.0),
            column_gap: px(6.0),
        }
        Children [{slots}]
    }
}

fn panel_node() -> Node {
    Node {
        padding: UiRect::all(px(20.0)),
        border: UiRect::all(px(1.0)),
        border_radius: BorderRadius::all(px(16.0)),
        flex_direction: FlexDirection::Column,
        row_gap: px(14.0),
        ..default()
    }
}

/// The whole screen: a glass panel with a title, the slot grid, and a footer strip.
pub fn glass_screen(glass: Handle<GlassPanelMaterial>) -> impl Scene {
    bsn! {
        Node {
            width: percent(100.0),
            height: percent(100.0),
            align_items: {AlignItems::Center},
            justify_content: {JustifyContent::Center},
        }
        Pickable::IGNORE
        Children [(
            // A `Handle` cannot be written directly as a bsn field value (the field position
            // wants a `HandleTemplate`), so the component is built in Rust and injected whole.
            MaterialNode<GlassPanelMaterial>({glass})
            template_value(panel_node())
            template_value(BorderColor::all(Color::WHITE.with_alpha(0.10)))
            BoxShadow({vec![ShadowStyle {
                color: Color::BLACK.with_alpha(0.55),
                x_offset: px(0.0),
                y_offset: px(14.0),
                spread_radius: px(2.0),
                blur_radius: px(38.0),
            }]})
            Children [
                (
                    Text("OBSIDIAN GLASS / CHEST")
                    TextFont { font_size: {FontSize::Px(15.0)} }
                    TextColor({TEXT.with_alpha(0.85)})
                ),
                slot_grid(),
                (
                    Text("hover or press a slot | slots 4 and 22 are legendary | slot 13 is a live viewport")
                    TextFont { font_size: {FontSize::Px(11.0)} }
                    TextColor({TEXT.with_alpha(0.45)})
                ),
            ]
        )]
    }
}

/// Attaches the glow material quad and the `ViewportNode` to already-spawned slots.
///
/// Runs in `Update` on `Added<Slot>` because `queue_spawn_scene` spawns in the `SpawnScene`
/// schedule (between `Update` and `PostUpdate`), which is not reachable from `Startup`.
pub fn decorate_slots(
    mut commands: Commands,
    slots: Query<(Entity, &Slot), Added<Slot>>,
    glow: Res<crate::GlowHandle>,
    viewport: Option<Res<crate::ViewportSetup>>,
) {
    for (entity, s) in &slots {
        if LEGENDARY.contains(&s.index) {
            commands.entity(entity).with_child((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(-10.0),
                    right: px(-10.0),
                    top: px(-10.0),
                    bottom: px(-10.0),
                    ..default()
                },
                MaterialNode(glow.0.clone()),
                Pickable::IGNORE,
            ));
        }
        if s.index == VIEWPORT_SLOT
            && let Some(viewport) = viewport.as_ref()
        {
            commands.entity(entity).with_child((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(2.0),
                    right: px(2.0),
                    top: px(2.0),
                    bottom: px(2.0),
                    border_radius: BorderRadius::all(px(6.0)),
                    ..default()
                },
                ViewportNode::new(viewport.camera),
                Pickable::IGNORE,
                ViewportSlot,
            ));
        }
    }
}

/// Hover and press feedback driven by `bevy_ui_widgets::Button`, `bevy_picking::hover::Hovered`
/// and `bevy_ui::Pressed`. No custom interaction state of our own.
pub fn slot_hover_feedback(
    mut slots: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor, &mut BorderColor), With<Slot>>,
) {
    for (hovered, pressed, mut bg, mut border) in &mut slots {
        let (fill, edge) = if pressed {
            (PANEL.mix(&ACCENT, 0.30).with_alpha(0.95), ACCENT)
        } else if hovered.get() {
            (
                PANEL.mix(&ACCENT, 0.16).with_alpha(0.90),
                ACCENT.with_alpha(0.85),
            )
        } else {
            (PANEL.with_alpha(0.72), HAIRLINE.with_alpha(0.55))
        };
        if bg.0 != fill {
            *bg = BackgroundColor(fill);
        }
        if border.top != edge {
            *border = BorderColor::all(edge);
        }
    }
}
