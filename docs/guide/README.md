# The slotted guide

Start with the one that matches what you are doing.

| Page | For |
|---|---|
| [architecture.md](architecture.md) | Understanding the crate graph, the ports, and where a change belongs. |
| [screens.md](screens.md) | Building a screen. The `UiNodeDef` reference: every node type and its fields, plus the item icon formats. |
| [themes.md](themes.md) | Writing or editing a theme. Roles, materials, tokens, fonts, motion, and the three shipped skins. |
| [modding.md](modding.md) | Writing a mod in Lua. The stages, the events, the commands, injection. |
| [api/lua.md](api/lua.md) | The generated `slotted.*` reference. |
| [api/slotted.d.luau](api/slotted.d.luau) | Type stubs, for luau-lsp completion in a mod. |
| [testing.md](testing.md) | Testing a game's UI in Rust, or a mod's behaviour in Lua. |
| [hot-reload.md](hot-reload.md) | What reloads, what survives, and what to do when it does not. |

Everything else is upstream of the guide: [`docs/PLAN.md`](../PLAN.md) is the
implementation plan, [`docs/design/`](../design/) holds each phase's contract,
and [`docs/adr/`](../adr/) holds the decisions that were expensive to make.
