//! The slotted domain model: identifiers, item stacks, inventories, menus and
//! the click state machine.
//!
//! This crate has no Bevy dependency and does no IO. Everything in it is a
//! plain value type or a pure function over value types, so it is testable in
//! milliseconds and can back a headless authoritative server as easily as a
//! client. See `docs/PLAN.md` section 4.1.
//!
//! # Example
//!
//! Open a single chest, put a stack of stone in the player's hotbar, then
//! shift-click it into the chest.
//!
//! ```
//! use slotted_model::{
//!     apply_click, Actor, ClickAction, Inventories, ItemId, ItemStack, LookupCtx, MenuDef,
//!     MenuState, Namespaced, SlotIx,
//! };
//!
//! // The registry normally implements this; a test table does here.
//! struct Items;
//! impl LookupCtx for Items {
//!     fn max_stack(&self, _id: ItemId) -> u32 { 64 }
//!     fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool { false }
//! }
//!
//! let def = MenuDef::chest(3);                       // 0..27 chest, 27..54 main, 54..63 hotbar
//! let mut inv = Inventories::for_menu(&def);
//! let mut state = MenuState::new(&def);
//! let stone = ItemId(1);
//! inv[MenuDef::PLAYER_HOTBAR].set(0, Some(ItemStack::new(stone, 40)));
//!
//! let delta = apply_click(
//!     &def, &mut inv, &mut state,
//!     ClickAction::QuickMove { slot: SlotIx(54) },
//!     &Actor::SURVIVAL, &Items,
//! )?;
//!
//! assert_eq!(inv[MenuDef::CONTAINER].get(0).map(|s| s.count), Some(40));
//! assert!(inv[MenuDef::PLAYER_HOTBAR].get(0).is_none());
//! assert_eq!(delta.slots.len(), 2);                  // chest slot 0 and hotbar slot 54
//! assert_eq!(state.state_id, 1);
//! # Ok::<(), slotted_model::ClickError>(())
//! ```

pub mod authority;
pub mod click;
pub mod error;
pub mod id;
pub mod inventory;
pub mod menu;
pub mod stack;
pub mod value;

pub use authority::{
    Authority, AuthorityError, AuthorityEvent, MenuId, MenuSnapshot, ResyncRequest,
};
pub use click::{
    Actor, Button, ClickAction, Delta, DragKind, DragPreview, DragStage, GiveTarget, ToolbarAction,
    ValidationLevel, apply_click, apply_click_validated, can_accept, preview_drag, slot_view,
};
pub use error::ClickError;
pub use id::{ComponentId, ItemId, Namespaced, NamespacedError};
pub use inventory::{DirtyMask, Inventories, Inventory, InventoryRef};
pub use menu::{
    DragState, LookupCtx, MenuDef, MenuState, Predicate, PropertyDef, PropertyId, RoutingRule,
    RoutingTable, SlotBehaviour, SlotDef, SlotIx, SlotRange,
};
pub use stack::{ComponentPatch, ItemStack};
pub use value::{Value, ValueDeserializer, ValueError, from_value};
