//! Entity events and messages.
//!
//! Bevy 0.19 splits these three ways and so does this crate:
//!
//! - `EntityEvent`s target one entity and are observed. Everything a widget or
//!   a game reacts to is one of these.
//! - `Message`s are buffered and read by systems. Used for high-volume sync
//!   ([`SlotSync`]) where an observer per slot would be wasteful.
//! - Plain `Event`s (global, observed) are not used here.

use bevy::prelude::*;
use slotted_model::{Button, ClickAction, ItemStack, MenuId, PropertyId, SlotIx};

/// Keyboard modifiers held during a slot gesture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift: quick-move.
    pub shift: bool,
    /// Ctrl: throw all, drag variants.
    pub ctrl: bool,
    /// Alt: reserved.
    pub alt: bool,
}

impl Modifiers {
    /// Nothing held.
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
    };
    /// Shift only.
    pub const SHIFT: Self = Self {
        shift: true,
        ctrl: false,
        alt: false,
    };
}

/// A raw pointer gesture on a slot entity (one carrying a [`SlotRef`](crate::SlotRef)).
///
/// Triggered by the ui crate's slot widget on `Pointer<Release>` and by the
/// test harness's semantic `click_slot`. The [`ClickInterpreter`](crate::ClickInterpreter)
/// observer turns it into a [`MenuAction`], handling double-click detection and
/// drag painting.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotClicked {
    /// The slot entity.
    pub entity: Entity,
    /// Which pointer button.
    pub button: Button,
    /// Modifiers held.
    pub modifiers: Modifiers,
}

/// A fully interpreted action on a menu entity (one carrying an
/// [`OpenMenu`](crate::OpenMenu)).
///
/// This is the seam every input path converges on: the click interpreter,
/// toolbar buttons, number keys, and the test harness's semantic actions all
/// trigger it. The observer enqueues it on [`ActionQueue`](crate::ActionQueue);
/// `SlottedEcsSet::Predict` applies it.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuAction {
    /// The menu entity.
    pub entity: Entity,
    /// What to do.
    pub action: ClickAction,
}

/// A menu entity has been opened; its `OpenMenu`, `Carried` and property
/// children exist. The ui crate observes this to spawn the screen.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuOpened {
    /// The menu entity.
    pub entity: Entity,
    /// Its id with the authority.
    pub id: MenuId,
}

/// A menu entity is about to be despawned. Triggered before the despawn so
/// observers can still read the components.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuClosed {
    /// The menu entity.
    pub entity: Entity,
    /// Its id with the authority.
    pub id: MenuId,
}

/// The content of a slot that has a UI entity changed. Targets the slot
/// entity (the one with the [`SlotRef`](crate::SlotRef)).
///
/// Only slots registered through `SlotRef` receive this. The buffered
/// [`SlotSync`] message covers every slot.
#[derive(EntityEvent, Debug, Clone, PartialEq)]
pub struct SlotChanged {
    /// The slot entity.
    pub entity: Entity,
    /// The menu it belongs to.
    pub menu: Entity,
    /// Slot index within the menu.
    pub slot: SlotIx,
    /// New content.
    pub stack: Option<ItemStack>,
}

/// A synced menu property changed. Targets the property child entity
/// (the one with the [`MenuProperty`](crate::MenuProperty)).
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyChanged {
    /// The property entity.
    pub entity: Entity,
    /// The menu it belongs to.
    pub menu: Entity,
    /// Which property.
    pub id: PropertyId,
    /// New value.
    pub value: i32,
}

/// Buffered per-slot sync, written for every changed slot after prediction
/// and after a resync. Read this from a system when you want all of them.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct SlotSync {
    /// The menu entity.
    pub menu: Entity,
    /// Slot index within the menu.
    pub slot: SlotIx,
    /// New content.
    pub stack: Option<ItemStack>,
}
