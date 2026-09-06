# External review, package B: property sync, invalidation, ownership

What the second review's findings 4 to 6 changed, and the decisions inside
them worth disagreeing with later. All three findings are the same shape:
state that lives in two places, kept in step by one of the two write paths and
not the other.

## 4. One property-update path

A menu property lives twice: as an integer in `OpenMenu::state.properties`,
indexed like `MenuDef::properties`, and as a `MenuProperty` component on one
child entity of the menu. The component is what everything *reads* --
`PropertyBinding`, tanks, bars, progress arrows and the icon button all query
it, and `PropertyChanged` targets it.

Three writers existed, and only two of them wrote both copies.
`AuthorityEvent::Property` and `SetProperty` each had their own hand-rolled
"update the vector, walk the children, trigger the event" block; `apply_resync`
replaced the whole vector with `menu.state = snapshot.state.clone()` and
touched no component at all. A snapshot carrying 42 therefore moved the model
to 42 and left every tank on the screen drawing 0.

`slotted_ecs::systems::write_property` is now the only way a property value
changes, and all three callers go through it. It takes the menu entity, a
`&mut OpenMenu`, the id, the value, the property query and `Commands`; it
writes the vector slot, writes every matching child component, and triggers
`PropertyChanged` for each. A write to an id the menu does not declare is
logged and returns `false`, which is the behaviour `SetProperty` already had
and `AuthorityEvent::Property` silently did not.

The resync case has one extra rule: it diffs. `apply_resync` captures the
property vector before assigning the snapshot and calls `write_property` only
for the positions the snapshot actually moved. Calling it for all of them
would have been simpler, and wrong in a way that would have been noticed
later: `PropertyChanged` is routed to scripts as `property_changed`, and a
resync every few frames would have delivered one event per property per resync
for values nobody touched. The two tests state both halves -- a snapshot with
42 moves the component and fires once, a snapshot that changes nothing fires
nothing -- and `a_tank_follows_a_property_delivered_by_a_snapshot` carries it
through to the widget the finding named.

## 5. Invalidation is a dependency question, not a name match

Respawning matched screen kinds by equality against the set of definitions
that changed. Every other way an edit reaches a screen was missed:

| Edit | Reaches |
|---|---|
| a base screen | every screen that `inherits` it, transitively |
| a widget template | every screen whose tree spawns that kind |
| an injection added | its target, or every screen with the anchor for `slotted:any` |
| an injection **removed** | the same, and this was missed entirely |
| a screen unregistered | itself and its derived screens |

`slotted_ui::ScreenDependencies` indexes the first three relations from
`Screens`, and `ScreenDependencies::invalidate(&ChangeSet) -> Vec<ScreenKind>`
answers the question. `ChangeSet` names changed screens, removed screens,
changed templates, and injections added and removed; a caller fills in the
parts of the world it touched, and an empty change set is free.

The index is built fresh per invalidation rather than kept as a resource. That
is a deliberate trade: a walk over every registered screen's resolved tree is
cheap next to the respawn it is deciding, and the alternative would have been
one more cache to keep in step with `Screens` -- which is the class of bug this
whole package is about.

`slotted_ui::respawn_screens` is now the only respawn implementation in the
workspace. There were two, one in `slotted-ui` for the `*.screen.ron` watcher
and a private copy in `slotted-packs` for the mod reload, and they had already
drifted: only the `slotted-ui` one was reachable from a test. The packs copy is
deleted. `invalidate_and_respawn` is the entry point both paths and the harness
use.

The wildcard deserves a note. An `slotted:any` injection matches a screen only
when that screen's resolved tree actually contains the anchor. Matching every
screen would respawn screens that cannot show the injection, which is a visible
blink for nothing.

## 6. Ownership, and what a reload may remove

`Screens`, `Injections`, `WidgetRegistry` and `TooltipParts` are shared between
the game's Rust registrations and whatever mods are loaded. A reload could only
add to them and replace entries in them. Nothing could ever be removed, so a
screen or a template or an injection a mod stopped shipping outlived the mod
that shipped it, for the life of the process.

Every entry now carries a `slotted_ui::Owner`: `Game`, `Mod(id)` or
`Asset(path)`. `Screens::reconcile_mods`, `WidgetRegistry::reconcile_mods`,
`reconcile_mod_injections` and `TooltipParts::set_mod_part` each replace
exactly the mod-owned set and report a `Reconciled { changed, removed }`, which
is what `slotted-packs` turns into a `ChangeSet`. `Owner::Game` entries are
never touched. The pack loader's old `PacksOwned` resource, which tracked the
same thing by index from the outside, is gone.

Two decisions worth arguing with:

**A mod may take over a game-registered screen kind.** The previous behaviour
allowed it, and forbidding it would have broken mods that reskin a base game
screen. But a takeover now stashes the game's definition in `Screens::shadowed`,
and when the mod stops registering the kind the game's definition comes back
rather than the kind vanishing. The mod owns the entry while it holds it; it
never owns the game's right to have registered it.

**A screen whose kind is gone closes.** It cannot respawn -- there is nothing
to respawn it from -- and leaving it up means a tree drawing from a definition
no registry holds. `respawn_screens` closes it and writes a
`ScreenDropped { entity, kind }` message; the pack loader turns that into a
mod-log warning naming the kind. A game that wants to route the player
somewhere on that event reads the same message.

## The architectural note: three stages, three guarantees

`slotted-packs`'s `lifecycle.rs` carried discovery, the data stage, the freeze,
publication into five registries, control-script loading and the respawn. It is
now three modules, each with its guarantees written at the top:

* `prepare.rs` -- discover, read, freeze. Guarantees that when it returns, the
  running game is untouched: everything it produced is still a value it owns.
  That is why a mod set that fails to load leaves the game alone.
* `install.rs` -- publish, with ownership. Guarantees each shared registry is
  reconciled rather than appended to, that a control script which fails leaves
  the mod running its previous one, and that nothing on screen has moved yet.
* `invalidate.rs` -- turn what the install reached into respawns, and report
  what closed.

`lifecycle.rs` keeps the resources, the messages, the stage enum and
`ModLoader::run_all` / `reload_mod`, which read as the three stages in order.
The public API did not change.

The rest of the architectural note stands unaddressed and is worth stating
plainly rather than quietly closing: frozen registries versus the UI registries
are still two copies of the same data, and `Screens::resolve` still flattens an
inheritance chain on every spawn. The property path and the screen registries
are the two places where the duplication was actually causing wrong pixels;
the rest is duplication that has not yet cost anything.
