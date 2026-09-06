# The showcase

<https://jedimemo.github.io/bevy-slotted/>

One Bevy app, one wasm module, one page, eight scenes. The showcase is where
the whole crate graph is running at once and you can press on it, which is a
different thing from reading about it. This page says what each scene is for
and which crate it puts under load, so you can go from "that looked useful" to
the code that does it.

Run it locally with:

```
just playground   # build dist/web-playground/
just serve        # http://127.0.0.1:8080/web-playground/
```

A scene is not a separate app. The 3D backdrop is spawned once and its orbit
keeps going across a switch; a scene switch despawns the previous scene's
screens and menus and spawns its own, through the same request bus the page
uses for everything else. That is why the page never reloads and never shows
a second spinner.

## The scenes

| # | Scene | What it demonstrates | Crates it exercises |
|---|---|---|---|
| 1 | Chest | The Minecraft interaction model: seven click modes, sweeps, phantom previews, tooltips. | `slotted-model`, `slotted-ui`, `slotted-theme` |
| 2 | Browser | The item and recipe browser docked beside a screen, with a search grammar and transfer checks. | `slotted-browser`, `slotted-registry` |
| 3 | Machine | Menu properties driving a tank, an energy bar, progress arrows and side tabs, plus a mod-injected button. | `slotted-ui`, `slotted-ecs`, `slotted-script` |
| 4 | Themes | The same screen tree repainted from three RON token files, in place. | `slotted-theme` |
| 5 | Mods | Four Lua mods, editable in the page, hot-reloaded into the running game. | `slotted-script`, `slotted-script-luaur`, `slotted-packs` |
| 6 | HUD | Layers anchored to the screen edges, from Rust and from a mod, and the position editor. | `slotted-ui` |
| 7 | Multiplayer | Two clients and a server in one tab over a lossy loopback link. | `slotted-net`, `slotted-ecs` |
| 8 | Testing | Lua tests driving the live screen, and a recorded session replayed through the input path. | `slotted-test`, `slotted-script` |

### 1. Chest

The screen from the moodboard, over the chest's own inventories. This is the
interaction model the whole project is built to get right: a left click picks
a stack up, a right click puts half down, holding a button and dragging across
slots sweeps a carried stack over them, and the slots a sweep will fill show a
dimmed phantom of what is about to land there.

The right column carries no controls, deliberately. Everything the scene has to
show happens under the pointer.

Read next: [screens.md](screens.md) for the node types, and
`crates/slotted-model` for the click table itself.

### 2. Browser

The same chest with the item browser docked in the strip beside it. Search is a
grammar rather than a substring match: `#ingots` is a tag, `-iron` excludes,
`@copper_chest` is everything one mod owns, and the parts combine. The right
column offers the examples as chips that type themselves into the game's field.

A recipe card knows whether the open screen could actually transfer it, which
is why pressing Enter over one either fills the grid or says why it cannot.

### 3. Machine

A furnace. The tank, the energy bar and the progress arrow are menu properties
the simulation writes each tick, not widgets the screen file animates; the
screen only says which property it draws. The two side tabs are the screen
file's. The redstone buttons in the right column stop the cook without
emptying the machine.

The **Sort** button in the action rail was injected by the `sorter` mod, which
was written against `slotted:any` and has never seen this screen. The same mod
puts the same button on the chest in the Mods scene.

### 4. Themes

Three buttons and a table. The screen tree does not change when you press one:
a theme is a RON file of tokens, roles and materials, and swapping it repaints
whatever is open in place. The table beside the buttons is what actually
differs between the three shipped skins, generated from the RON files by
`tools/gen-theme-diff.py` rather than computed in the browser.

Read next: [themes.md](themes.md).

### 5. Mods

Today's playground, unchanged. Four Lua mods with their `data.lua` and
`control.lua` in an editor, a Run button, a Tests tab and a console. Edit a
file and press Run and the mod reloads into the running game with the chest's
contents intact.

On `wasm32` an uncaught Lua error aborts the whole module, which
[ADR 0004](../adr/0004-web-runtime-luaur.md) explains. Type `error("boom")` in
`control.lua` and press Run: the page notices the trap, builds a second module,
and hands the chest back to it.

Read next: [modding.md](modding.md) and [hot-reload.md](hot-reload.md).

### 6. HUD

No screen at all. A hotbar layer registered from Rust and a clock layer
registered from a bundled mod, both anchored to a screen edge rather than to a
menu. The edit button turns on the position editor a game would give its
players; the layout you drag into place is written to `localStorage` and put
back the next time you open the page.

### 7. Multiplayer

One server, one loopback link and two clients, all in this tab and all sharing
one chest container. The left client applies your click straight away and asks
the server; the server answers, and a prediction that turns out wrong is
corrected in front of you. The latency and loss sliders feed the link's
conditions, so a raised loss slider makes a retransmit visible in the message
log rather than theoretical.

The message log is its own pane, not the console: it is read for the sequence
of messages, which is a different job from reading what a mod printed.

### 8. Testing

Two halves. On the left, each mod's `tests/*.lua` run against the live screen,
one action per frame, and the pass and fail marks stream into the list as the
frames go by. On the right, a session recorded with `cargo run -p chest --
--record` replayed through the same input path the pointer uses, with a
scrubber that seeks to any frame.

Read next: [testing.md](testing.md).

## Getting around the page

- The rail on the left switches scenes. `Alt`+`1`..`8` does the same; plain
  digits stay the game's hotbar keys.
- `?scene=<id>` opens a scene directly, and `?theme=<name>` opens it in one of
  the three skins. The ids are the lowercase scene names: `chest`, `browser`,
  `machine`, `themes`, `mods`, `hud`, `multiplayer`, `testing`.
- The console at the bottom of the right column is the game's, on every scene.
- A control whose export has not landed yet says `coming up` and stops
  responding rather than throwing. That is the page's normal state while a
  scene is being built, not a fault.

## Screenshots

`examples/web-playground/shots/showcase-<scene>.png`, one per scene.
`just shot-showcase` recaptures them: it opens each scene through its
`?scene=` link, checks the head strip against the scene table, switches through
the rail on the way to the next one, and fails if anything but an expected
`not yet:` reached the browser console. It needs `just playground` and
`just serve` running first.
