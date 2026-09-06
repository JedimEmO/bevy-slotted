# slotted-model

The domain model of [slotted](https://github.com/mathiasmyrland/bevy-slotted):
identifiers, item stacks, inventories, menus and the click state machine.

No Bevy, no IO. Everything here is a value type or a pure function over value
types, so it tests in milliseconds and can back a headless authoritative server
as easily as a client.

## Main types

| Type | What it is |
|---|---|
| `Namespaced`, `ItemId`, `ComponentId` | Ids. `Namespaced` is the `namespace:path` string form; `ItemId` is the dense handle a frozen registry hands out. |
| `ItemStack` | An item, a count and a component patch. |
| `Inventory`, `Inventories` | A list of slots, and the set of them a menu is built from. |
| `MenuDef`, `MenuState` | The static shape of a screen's slots and its mutable state. |
| `ClickAction`, `apply_click`, `MenuDelta` | The seven click modes plus toolbar actions, and the change they produce. |
| `Authority`, `AuthorityEvent`, `MenuSnapshot` | The port a networked or local authority implements. |
| `Value` | The untyped value both RON and Lua deserialise into. |

## Example

Open a chest, then shift-click a stack of stone out of the hotbar into it.

```rust
use slotted_model::{
    apply_click, Actor, ClickAction, Inventories, ItemId, ItemStack, LookupCtx, MenuDef,
    MenuState, Namespaced, SlotIx,
};

struct Items;
impl LookupCtx for Items {
    fn max_stack(&self, _id: ItemId) -> u32 { 64 }
    fn has_tag(&self, _id: ItemId, _tag: &Namespaced) -> bool { false }
}

let def = MenuDef::chest(3);
let mut inv = Inventories::for_menu(&def);
let mut state = MenuState::new(&def);
inv[MenuDef::PLAYER_HOTBAR].set(0, Some(ItemStack::new(ItemId(1), 40)));

let delta = apply_click(
    &def, &mut inv, &mut state,
    ClickAction::QuickMove { slot: SlotIx(54) },
    &Actor::SURVIVAL, &Items,
).expect("a legal click");

assert_eq!(inv[MenuDef::CONTAINER].get(0).map(|s| s.count), Some(40));
assert_eq!(delta.slots.len(), 2);
```

## Feature flags

None. The crate has two dependencies, `serde` and `thiserror`.

## Licence

MIT OR Apache-2.0, at your option.
