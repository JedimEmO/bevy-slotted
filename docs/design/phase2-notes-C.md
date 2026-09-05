# Phase 2 notes, package C (slotted-test + facade)

Deviations from `docs/design/phase2-contract.md` §6 and additions the contract
does not mention. Nothing here changes a type another package owns; the two
items that touch the shared contract are marked **contract**.

## Additions

- **`Locator::visible()`** (`.visible` field). A filter: it narrows the match
  set and leaves the count to `find`. The contract lists only `.nth_visible(n)`,
  which is a *selector* and therefore cannot be combined with `find`'s
  "exactly one" check. Both exist; the doc comments say which is which.
- **`assert_tree_text_snapshot!`** next to the contract's
  `assert_tree_snapshot!`. The contract fixes the latter as
  `insta::assert_ron_snapshot!` and that is unchanged. `ScreenTree` also
  implements `Display` as an indented one-line-per-node text tree, which reads
  better in a review diff and is what `find`'s failure message embeds. Same
  data, same stability guarantee.
- **`ScreenSource`** (`fixture.rs`). `open_screen` takes `impl ScreenSource`
  rather than a `ScreenKind`, so a test can pass a `ScreenDef` (by value,
  reference or `Arc`) without registering it first. A def passed this way is
  registered on the way through, so `by::screen(kind)` still finds it. Passing
  a `ScreenKind` behaves exactly as the contract says, and now panics with the
  registered kinds listed.
- **`locator::describe`** is public: a one-line description of a node, used by
  the `find` failure message and useful in a test's own assertions.
- **`Locator::near_misses`** is public for the same reason.
- **`hud_layers()`** returns an empty `Vec` and says so in its doc comment, as
  the work package allows. Phase 3 fills it in.
- **`fixtures` module**: `TestRegistries::basic()` (eleven items from the
  moodboard, frozen once per process so `ItemId`s are stable), `ChestFixture`
  and `PlayerFixture`. `ChestFixture::filled()` reproduces the moodboard demo's
  chest so `examples/chest` can reuse it.

## Decisions inside the crate

- **`by::text` matches `Text` as well as `SemanticLabel`.** This resolves the
  `PHASE2-IMPL: agent C` note in `locator.rs`. A `UiNodeDef::Text` node carries
  both, so either spelling finds it.
- **`tree::roots` sorts screen roots by entity *index*, not by `Entity`.**
  Bevy 0.19's `Entity: Ord` compares `to_bits()`, which packs the generation
  above the index, so a root that landed on a recycled index sorts after a root
  spawned later. Index order is stable and matches spawn order whenever nothing
  has been despawned. `tests/semantics.rs::locators_resolve_in_tree_order`
  fails without this.
- **`find` panics with near-misses.** Nodes that fail exactly one criterion are
  listed with the criterion they fail, then the whole semantic tree. With a
  single-criterion locator "fails exactly one" would mean "every node", so the
  message instead lists the nodes carrying the component that criterion reads.
- **Durability lives in `ItemDef::components` under `slotted:max_durability`.**
  Neither `slotted-model` nor `slotted-registry` has a durability concept in
  Phase 2, and inventing one is not package C's call. The fixture tools carry
  the number as a registry component so a later phase can pick it up without
  the fixture table changing.

## Contract questions for the integration step

- **contract** `tooltip()` returns `Option<TooltipContent>`, per contract §6.
  The work package asked for a `TooltipSnapshot`. `TooltipContent` is already a
  clonable value type with a `tier` and a `Vec<UiNodeDef>`, so a wrapper would
  only rename it. If a serde-friendly flattening of the parts turns out to be
  what tests want, that is a Phase 3 addition and not a change to what
  `slotted-ui` writes.
- **contract** The work package asked for `slot_click(loc, button, modifiers)`.
  The contract names it `click_slot`, which is what is implemented. Same
  signature, same `SlotClicked` dispatch.
- Pointer and semantic actions take `Entity`, not `Locator`, as the contract
  says. `h.click(h.find(&loc))` is the idiom. Taking a locator would need
  `&mut self` to resolve and then act, which reads worse at the call site.

## State of `tests/chest_screen.rs`

Seven tests, all `#[ignore = "phase2: enabled when ui+ecs land"]`. They are
written against the contract, not against the current stubs, and cover: the
grids a `ScreenDef` spawns, shift-click across inventories, right-click split
on the 16-cap item, number-key hotbar swap, the action rail's sort reaching the
model, the tooltip hover delay and tier promotion, a tree snapshot, and the
semantic and pointer paths agreeing. Un-ignore them in the integration step;
the two snapshots have no accepted `.snap` file yet and will need one review
pass.
