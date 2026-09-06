//! Dev-mode HUD position editor. Phase 6 contract section 2.2. Feature `dev`.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::hud::{HudAnchored, HudLayout};

/// While `true`, anchor wrappers are pickable and draggable.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudEditMode(pub bool);

/// The key that toggles [`HudEditMode`].
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudEditKey(pub KeyCode);

impl Default for HudEditKey {
    fn default() -> Self {
        Self(KeyCode::F7)
    }
}

/// Drag snap in logical pixels.
pub const SNAP: f32 = 4.0;

/// Where [`HudLayout`] is persisted (RON). Absent: not persisted.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct HudLayoutStore {
    /// File path.
    pub path: PathBuf,
}

/// Marks the outline child drawn around a wrapper in edit mode.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct HudEditFrame;

/// `SlottedUiSet::Input`: toggles edit mode on [`HudEditKey`].
pub fn toggle_hud_edit(
    keys: Res<ButtonInput<KeyCode>>,
    key: Res<HudEditKey>,
    mut mode: ResMut<HudEditMode>,
) {
    if keys.just_pressed(key.0) {
        mode.0 = !mode.0;
    }
}

/// Marks a wrapper whose drag observers are attached. The observers stay for
/// the app's life; leaving edit mode only takes the wrapper's `Pickable` and
/// its outline away, which is what stops the drags.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct HudEditable;

/// On a wrapper between `DragStart` and `DragEnd`: the offset the drag began
/// from, so the total drag distance places the layer and `Esc` can put it
/// back.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HudDragging {
    /// `HudAnchor::offset` when the drag started.
    pub start: Vec2,
}

/// `SlottedUiSet::Render`: on `Changed<HudEditMode>` makes wrappers pickable
/// and adds or removes the [`HudEditFrame`] child and drag observers.
pub fn apply_hud_edit_mode(
    mode: Res<HudEditMode>,
    wrappers: Query<(Entity, &HudAnchored, Has<HudEditable>)>,
    frames: Query<(Entity, &ChildOf), With<HudEditFrame>>,
    mut commands: Commands,
) {
    for (entity, _, editable) in &wrappers {
        match (mode.0, editable) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert((Pickable::default(), HudEditable))
                    .observe(on_hud_drag_start)
                    .observe(on_hud_drag)
                    .observe(on_hud_drag_end);
                commands.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: percent(100),
                        height: percent(100),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    slotted_theme::Themed(slotted_theme::roles::HUD_EDIT_FRAME),
                    HudEditFrame,
                    Pickable::IGNORE,
                    ChildOf(entity),
                ));
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .insert(Pickable::IGNORE)
                    .remove::<HudEditable>()
                    .remove::<HudDragging>();
                for (frame, parent) in &frames {
                    if parent.parent() == entity {
                        commands.entity(frame).despawn();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Observer on `Pointer<DragStart>`: remembers where the layer was.
pub fn on_hud_drag_start(
    drag: On<Pointer<DragStart>>,
    anchors: Query<&crate::hud::HudAnchor, With<HudAnchored>>,
    mut commands: Commands,
) {
    if let Ok(anchor) = anchors.get(drag.entity) {
        commands.entity(drag.entity).insert(HudDragging {
            start: anchor.offset,
        });
    }
}

/// Observer on `Pointer<DragEnd>`: the drag is over; the layout file is
/// written by [`save_hud_layout`] on the next `Last`.
pub fn on_hud_drag_end(drag: On<Pointer<DragEnd>>, mut commands: Commands) {
    commands.entity(drag.entity).try_remove::<HudDragging>();
}

/// Observer on `Pointer<Drag>` for a wrapper: moves `offset` by the drag
/// delta snapped to [`SNAP`], writing `HudAnchor` and `HudLayout`.
///
/// The offset is computed from the drag's total distance rather than
/// accumulated per event: snapping every event would quantise each delta on
/// its own, and a slow drag of one pixel per frame would never move at all.
/// `crate::hud::reanchor_hud_layers` turns the new anchor into a `Node`.
pub fn on_hud_drag(
    drag: On<Pointer<Drag>>,
    mut wrappers: Query<(
        &HudAnchored,
        &mut crate::hud::HudAnchor,
        Option<&HudDragging>,
    )>,
    mut layout: ResMut<HudLayout>,
) {
    let Ok((anchored, mut anchor, dragging)) = wrappers.get_mut(drag.entity) else {
        return;
    };
    let from = dragging.map_or(anchor.offset, |d| d.start);
    let moved = dragging.map_or(drag.delta, |_| drag.distance);
    let offset = snap(from + moved);
    if anchor.offset != offset {
        anchor.offset = offset;
    }
    layout.anchors.insert(anchored.id.clone(), *anchor);
}

/// `SlottedUiSet::Input`: `Esc` puts a layer being dragged back where it was.
pub fn cancel_hud_drag(
    keys: Res<ButtonInput<KeyCode>>,
    mut wrappers: Query<(
        Entity,
        &HudAnchored,
        &mut crate::hud::HudAnchor,
        &HudDragging,
    )>,
    mut layout: ResMut<HudLayout>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    for (entity, anchored, mut anchor, dragging) in &mut wrappers {
        anchor.offset = dragging.start;
        layout.anchors.insert(anchored.id.clone(), *anchor);
        commands.entity(entity).try_remove::<HudDragging>();
    }
}

/// `v` rounded to the nearest [`SNAP`] step on both axes.
fn snap(v: Vec2) -> Vec2 {
    (v / SNAP).round() * SNAP
}

/// `Startup`: reads [`HudLayoutStore`] into [`HudLayout`] when the file exists.
pub fn load_hud_layout(store: Option<Res<HudLayoutStore>>, mut layout: ResMut<HudLayout>) {
    let Some(store) = store else {
        return;
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let text = match std::fs::read_to_string(&store.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                tracing::warn!(path = %store.path.display(), %e, "reading the HUD layout");
                return;
            }
        };
        match ron::from_str::<HudLayout>(&text) {
            Ok(loaded) => *layout = loaded,
            Err(e) => tracing::warn!(path = %store.path.display(), %e, "the HUD layout is not RON"),
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (store, &mut layout);
}

/// On `DragEnd` and `AppExit`: writes [`HudLayout`] to [`HudLayoutStore`].
pub fn save_hud_layout(store: Option<Res<HudLayoutStore>>, layout: Res<HudLayout>) {
    let Some(store) = store else {
        return;
    };
    if !layout.is_changed() || layout.anchors.is_empty() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let text = match ron::ser::to_string_pretty(&*layout, ron::ser::PrettyConfig::default()) {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(%e, "serialising the HUD layout");
                return;
            }
        };
        if let Some(dir) = store.path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(path = %dir.display(), %e, "creating the HUD layout directory");
            return;
        }
        if let Err(e) = std::fs::write(&store.path, text) {
            tracing::warn!(path = %store.path.display(), %e, "writing the HUD layout");
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = store;
}
