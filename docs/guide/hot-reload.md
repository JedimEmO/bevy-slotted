# Hot reload

Edit a mod's Lua or RON, and the mod reloads while the game keeps running. The
chest stays open and keeps its contents.

## Turning it on

```toml
slotted-packs = { version = "0.1", features = ["watch"] }
bevy = { version = "0.19.1", features = ["file_watcher"] }
```

`examples/modded` is the working example: `cargo run -p modded`, edit
`examples/modded/mods/copper_chest/control.lua`, and the change takes effect
without touching the window.

The watcher sends a `ReloadMod` message. So does the dev console, the playground's
Run button and the test harness, so all four paths run the identical code.

## What happens

A reload is not a patch. The whole data stage runs again, for every mod, in load
order:

1. Re-read every mod's data files and `data.lua`.
2. Freeze into a new `FrozenRegistries`.
3. Take back exactly what packs registered last time: its injections, its
   tooltip parts, its widget kinds.
4. Publish the new screens, injections and tooltip parts.
5. Load every `control.lua` again.
6. Remap open menus' inventories onto the new registry, by name.

Step 6 is why an open chest keeps its contents. Ids are dense handles that the
freeze assigns, so `demo:cobblestone` may be item 41 before a reload and item 43
after. The previous frozen set is kept as a snapshot precisely so the live
inventories can be translated across.

Re-running everything rather than diffing is deliberate. A mod's data stage can
depend on what mods before it registered, so a patch would have to model that
dependency; a re-run does not.

## When it fails

If anything at all goes wrong, the reload is abandoned and the previous
registries and scripts stay in place. The game keeps running the last version
that loaded, and the error goes to `ModErrors` and out as a `ModFailed` message.

That is the important property: a syntax error in `control.lua` does not take
down a session. You fix the file and save again.

Errors reach you three ways:

- the dev console in `examples/modded`, with the `dev` feature,
- the console pane in the web playground,
- `UiHarness::mod_errors()` in a test.

## What survives

| | |
|---|---|
| Open screens | Yes. They are respawned from the new `ScreenDef`. |
| Inventory contents | Yes, remapped by name. |
| An item that no longer exists | The stack is dropped; there is nothing to remap it to. |
| The carried stack | Yes. |
| Script state in a Lua local | No. A control chunk is loaded fresh. |
| A registered item's numeric id | No. Never hold one across a reload; hold the `namespace:path`. |

## In the browser

The playground calls a `wasm-bindgen` export with a mod id and the new source of
its `data.lua` and `control.lua`. The Bevy app receives it through a channel
resource and runs the same reload path. Nothing is written to a filesystem;
there isn't one.

Because a script can only emit commands, a broken script cannot corrupt
inventory state. The worst case is a rejected reload with the previous version
still running.

## What does not hot reload

Themes do, through the ordinary asset watcher: edit `glass.theme.ron` and every
themed node repaints.

A `.screen.ron` file loaded by a game with `std::fs` does not, because there is
no `ScreenLoader` asset loader yet. It is tracked in
[`docs/FOLLOWUPS.md`](../FOLLOWUPS.md). A screen registered by a mod's
`data.lua` does reload, because the whole data stage re-runs.

Rust does not. A change to a widget, a system or a def type is a rebuild. That
is the line: the parts a designer or a modder iterates on are data, and the parts
an engineer iterates on are code.
