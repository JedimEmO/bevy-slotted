//! Bevy adapter of the slotted domain model.
//!
//! This crate turns [`slotted_model`] into components, entity events and
//! systems, and owns the client side of the prediction loop: gather a
//! [`ClickAction`](slotted_model::ClickAction), apply it locally, submit it
//! to the [`Authority`] resource, then reconcile acks and resyncs. It knows
//! nothing about rendering or `bevy_ui`; the ui crate attaches a [`SlotRef`]
//! to whatever entity draws a slot and this crate does the rest.
//!
//! Frame order inside `Update` is [`SlottedEcsSet`]: `Input` drains the
//! queue of actions that observers collected, `Predict` runs `apply_click`,
//! `Submit` hands the delta to the authority, `Reconcile` polls it.
//!
//! See `docs/design/phase2-contract.md` for the full contract.

pub mod authority;
pub mod events;
pub mod lookup;
pub mod menu;
pub mod plugin;
pub mod systems;

pub use authority::{Authority, LocalAuthority, PendingRoundTrips};
pub use events::{
    MenuAction, MenuClosed, MenuOpened, Modifiers, PropertyChanged, SetProperty, SetSlot,
    SlotChanged, SlotClicked, SlotSync,
};
pub use lookup::{Registries, RegistryLookup};
pub use menu::{
    Carried, CloseMenu, Dropped, DroppedStack, Favorite, Inventory, MenuIdAllocator, MenuProperty,
    OpenMenu, PlayerInventories, SlotEntities, SlotRef, close_menu, open_container_menu, open_menu,
};
pub use plugin::{SlottedEcsPlugin, SlottedEcsSet};
pub use systems::{
    ActionQueue, ClickInterpreter, EmptyLookup, PendingSubmissions, Submission, apply_set_property,
    apply_set_slot,
};

/// Everything a game needs to open menus and react to them.
pub mod prelude {
    pub use crate::{
        Authority, Carried, Dropped, Inventory, LocalAuthority, MenuAction, MenuClosed, MenuOpened,
        Modifiers, OpenMenu, PlayerInventories, PropertyChanged, Registries, SetProperty, SetSlot,
        SlotChanged, SlotClicked, SlotRef, SlottedEcsPlugin, SlottedEcsSet, close_menu,
        open_container_menu, open_menu,
    };
    pub use slotted_model::{
        Actor, Button, ClickAction, ItemStack, MenuDef, SlotIx, ToolbarAction,
    };
}
