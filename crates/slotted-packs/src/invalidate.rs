//! Stage three of a load: work out what an install reached, and respawn it.
//!
//! What this module guarantees:
//!
//! * every open screen the change set reaches is closed and re-opened on the
//!   same menu entity, so its slots re-seed without an inventory write;
//! * "reaches" is the full dependency, not the exact kind that changed: the
//!   screens that inherit a changed screen, the screens that spawn a changed
//!   widget template, and the screens an injection was added to or removed
//!   from, `slotted:any` included;
//! * an open screen whose kind nothing registers any more is closed rather
//!   than left drawing from a definition that is gone, and the close is
//!   reported to the mod log.
//!
//! The respawn itself lives in `slotted-ui`, in
//! [`slotted_ui::respawn_screens`], and is shared with the `*.screen.ron`
//! watcher. There is one implementation of it in the workspace.

use bevy::prelude::*;
use slotted_script::LogLevel;

use slotted_ui::ChangeSet;

use crate::lifecycle::log;

/// Contract 2.6 step 4: turn a reconcile's change set into respawns, and
/// report every screen that closed because nothing registers it any more.
///
/// The respawn itself lives in `slotted-ui` -- there is exactly one
/// implementation, shared with the `*.screen.ron` watcher -- so all this adds
/// is the mod-facing report.
#[cfg(feature = "ui")]
pub(crate) fn apply_invalidation(world: &mut World, change: &ChangeSet) {
    if change.is_empty() {
        return;
    }
    slotted_ui::invalidate_and_respawn(world, change);
    let dropped: Vec<String> = world
        .get_resource::<Messages<slotted_ui::ScreenDropped>>()
        .map(|messages| {
            messages
                .iter_current_update_messages()
                .map(|dropped| dropped.kind.0.to_string())
                .collect()
        })
        .unwrap_or_default();
    for kind in dropped {
        log(
            world,
            None,
            LogLevel::Warn,
            format!("screen `{kind}` is no longer registered; the open one was closed"),
        );
    }
}
