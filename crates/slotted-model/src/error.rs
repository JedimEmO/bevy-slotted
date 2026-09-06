//! Domain error types shared across the model.

use serde::{Deserialize, Serialize};

pub use crate::id::NamespacedError;

/// Why [`apply_click`](crate::click::apply_click) refused an action.
///
/// Every variant leaves the inventories and the carried stack untouched. The
/// drag state is the exception:
/// [`InvalidDragSequence`](Self::InvalidDragSequence) clears it, as vanilla's
/// `resetQuickCraft` does, and so does any `Drag { stage: End }`, which ends
/// the drag whatever it returns.
///
/// Painting a slot that cannot take an item is not an error at all: the drag
/// skips that slot and stays alive, as it does in vanilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum ClickError {
    /// The slot id is out of range, `Disabled`, or `OUTSIDE` where a real
    /// slot is required.
    #[error("no such slot")]
    NoSuchSlot,
    /// The slot is `Locked`, or the action needs pickup from a slot that
    /// forbids it.
    #[error("slot is locked")]
    SlotLocked,
    /// The slot refuses the stack (an `Output` slot, or `accepts` failed), or
    /// the action makes no sense on this slot kind.
    #[error("action not allowed on this slot")]
    NotAllowed,
    /// Valid action, but it would change nothing.
    #[error("nothing to do")]
    NothingToDo,
    /// A drag stage arrived out of order, or a non-drag action interrupted a
    /// drag. The drag has been reset.
    #[error("invalid drag sequence")]
    InvalidDragSequence,
    /// The actor lacks the permission (creative) the action needs.
    #[error("permission denied")]
    Permission,
    /// The menu definition references an inventory or index the supplied
    /// `Inventories` do not have.
    #[error("menu definition does not match the inventories")]
    MenuMismatch,
    /// The action created or destroyed items, and
    /// [`ValidationLevel::Always`](crate::click::ValidationLevel::Always) was
    /// in force.
    ///
    /// Unlike every other variant this one is raised *after* the action ran,
    /// so the inventories and the state it was given are no longer
    /// trustworthy. A caller at this validation level applies to a scratch
    /// copy and keeps it only on `Ok`; that is what
    /// `slotted_net::MenuServer` does.
    #[error("action violated item conservation")]
    Conservation,
}
