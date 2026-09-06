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

/// `SlottedUiSet::Render`: on `Changed<HudEditMode>` makes wrappers pickable
/// and adds or removes the [`HudEditFrame`] child and drag observers.
pub fn apply_hud_edit_mode(
    mode: Res<HudEditMode>,
    wrappers: Query<(Entity, &HudAnchored)>,
    mut commands: Commands,
) {
    // PHASE6-IMPL: B.
    let _ = (&mode, &wrappers, &mut commands);
}

/// Observer on `Pointer<Drag>` for a wrapper: moves `offset` by the drag
/// delta snapped to [`SNAP`], writing `HudAnchor` and `HudLayout`.
pub fn on_hud_drag(
    drag: On<Pointer<Drag>>,
    mut wrappers: Query<(&HudAnchored, &mut crate::hud::HudAnchor, &mut Node)>,
    mut layout: ResMut<HudLayout>,
) {
    // PHASE6-IMPL: B.
    let _ = (&drag, &mut wrappers, &mut layout);
}

/// `Startup`: reads [`HudLayoutStore`] into [`HudLayout`] when the file exists.
pub fn load_hud_layout(store: Option<Res<HudLayoutStore>>, mut layout: ResMut<HudLayout>) {
    // PHASE6-IMPL: B. std::fs on native; no-op on wasm32.
    let _ = (&store, &mut layout);
}

/// On `DragEnd` and `AppExit`: writes [`HudLayout`] to [`HudLayoutStore`].
pub fn save_hud_layout(store: Option<Res<HudLayoutStore>>, layout: Res<HudLayout>) {
    // PHASE6-IMPL: B.
    let _ = (&store, &layout);
}
