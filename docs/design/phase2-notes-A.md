# Phase 2 implementation notes, agent A (`slotted-ecs`, `slotted-testutils`)

Companion to `docs/design/phase2-contract.md` section 1. Everything here is a
place where the contract was silent, self-contradictory, or where a task
instruction and the contract disagreed. Nothing outside `crates/slotted-ecs`,
`crates/slotted-testutils` and `crates/slotted-icons` was changed.

## 1. `MenuClosed` does not carry the dropped stack

The work order asked for "closing a menu with a carried stack emits
`MenuClosed` with the dropped stack". The contract fixes the event at
`MenuClosed { entity, id }`, and the ui and test crates are built against that
shape, so the field was not added.

Instead, `slotted_ecs::Dropped` is a resource holding
`Vec<DroppedStack { menu, stack }>`. Everything that leaves a menu lands there:
a click on `SlotIx::OUTSIDE`, a throw, a swap whose displaced stack found no
home, and the carried stack of a closed menu. The game drains it and spawns
item entities. `Dropped::of(menu)` reads back one menu's stacks.

If a later phase would rather put the stack on the event, the change is
additive and the resource can stay as the general dropped-item sink.

## 2. `close_menu` is a `Command`

`close_menu(&mut Commands, menu, id)` keeps its contract signature, but it now
queues a `CloseMenu` command rather than triggering and despawning inline: it
has to read the menu's carried stack out of the world before the entity goes
away, which `Commands` alone cannot do. `MenuClosed` is still triggered before
the despawn, so observers still see the components.

## 3. Where the dirty masks are cleared

The contract does not name a clearing point. `clear_dirty_masks` runs first in
`SlottedEcsSet::Input`, so a mask written during frame N survives
`SlottedUiSet::Render` and both theme sets of that same frame and is cleared at
the top of frame N+1, before that frame's prediction. The ui crate therefore
has exactly one full frame to consume a mask, and never sees a stale one.

The clear bypasses Bevy change detection, so it does not make every `Inventory`
look `Changed` every frame.

## 4. A `Resync` decrements `PendingRoundTrips`

The contract says only that `Ack` decrements it. A resync also answers a
submission, and `slotted-test`'s `settle()` blocks until the counter reaches
zero, so an authority that resyncs instead of acking would hang the harness.
`Resync` therefore decrements it too (saturating, as `Ack` does).

## 5. A resync emits only the slots that actually changed

Contract section 1 says a resync "re-emits every slot"; the `SlotSync` doc
comment in `events.rs` says "for every changed slot after prediction and after
a resync". The narrower reading won, because re-emitting 45 unchanged slots on
every resync would make `SlotChanged` useless as a UI trigger. `apply_resync`
diffs the slot contents before and after the overwrite and emits `SlotSync` and
`SlotChanged` for the differences only.

## 6. A refused submission cannot request a resync

The contract says `Err(Rejected)` from `Authority::submit` "rolls back by
requesting a resync path", but `slotted_model::Authority` has no method for
asking. `submit` therefore logs the error, does **not** count a round trip, and
re-emits every slot of that menu from the current local state, which is the
closest local approximation. A networked adapter that wants a true rollback
should answer with `AuthorityEvent::Resync` instead of `Err`.

## 7. Drag painting is driven through `MenuAction`, not `SlotClicked`

The contract puts "drag paint state" in `interpret_slot_click`, but
`SlotClicked` carries only a completed click (the ui crate triggers it on
`Pointer<Release>`), so an observer of it cannot see press, move and release.
`ClickInterpreter` grows a `drag: Option<DragKind>` field and the helpers
`begin_drag`, `paint` and `end_drag`, which return the three
`ClickAction::Drag` stages; whoever owns the pointer stream calls them and
triggers `MenuAction` directly, as the contract already says toolbar buttons
and number keys do. `interpret_slot_click` itself handles the vanilla button
mapping and the double-click window.

Number keys are the same story: `ClickInterpreter::swap(slot, hotbar)` builds
the action, but the ecs crate has no hover information, so it does not read the
keyboard itself.

## 8. `LocalAuthority` has no validation level

The task mentioned "an injected `ValidationLevel` or at least a `reject_all`
test hook **if the contract names one**". It names neither, so `LocalAuthority`
is left as the contract describes it: apply immediately, ack at the predicted
state id, never resync, never reject. Rejection and resync behaviour for tests
lives in `slotted-testutils` (`RecordingAuthority::fail_with`,
`RejectingAuthority`).

## 9. Additions to the contracted surface

All additive, none replacing anything the contract names:

| Item | Why |
|---|---|
| `PendingSubmissions(Vec<Submission>)` resource | The "stash the delta" the contract asks for between `Predict` and `Submit`. |
| `Dropped` / `DroppedStack` | Note 1. |
| `PlayerInventories` resource, `open_container_menu` | The well-known player inventory mapping; fills the `MenuDef` handle convention and spawns correctly sized inventories for handles nobody supplied. |
| `CloseMenu` command | Note 2. |
| `EmptyLookup` | The `LookupCtx` used when no `Registries` resource has been inserted: max stack 1, no tags, the same fallback the contract specifies for unknown items. |
| `clear_dirty_masks`, `mirror_favorites` systems | Note 3, and the contract's "then mirror `Favorite`". |
| `ClickInterpreter::{drag, interpret_with_time, begin_drag, paint, end_drag, swap}` | Note 7. |

`slotted-testutils` gained `bevy` (feature `std` only) and `slotted-ecs` as
dependencies, so that `minimal_ecs_app()` can build an `App`. `slotted-ecs`
dev-depends on `slotted-testutils`, which Cargo allows for dev-dependencies.

## 10. `clippy::too_many_arguments`

`predict`, `submit`, `reconcile` and `apply_resync` carry a local
`#[allow(clippy::too_many_arguments)]`. A Bevy system declares its world access
as parameters, so the lint fires on the shape rather than on a problem; the
workspace lint table already allows `needless_pass_by_value` and
`type_complexity` for the same reason. Moving it to the workspace table would
have meant editing a file another agent owns.
