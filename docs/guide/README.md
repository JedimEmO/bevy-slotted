# The slotted guide

Start with the one that matches what you are doing.

| Page | For |
|---|---|
| [showcase.md](showcase.md) | The eight-scene web showcase: what each scene demonstrates and which crate it exercises. |
| [architecture.md](architecture.md) | Understanding the crate graph, the ports, and where a change belongs. |
| [screens.md](screens.md) | Building a screen. The `UiNodeDef` reference: every node type and its fields, the layout model, presentation, the screen stack, plus the item icon formats. |
| [input.md](input.md) | Keyboard and gamepad: the action vocabulary, bindings, input mode, claims, focused actions, the focus ring, and driving them from a test. |
| [values.md](values.md) | The value store a settings screen binds to: values, rules, guards, the messages, and the menu-property mirror. |
| [rich-text.md](rich-text.md) | The rich-text markup: bold, colour, size, key glyphs, item icons, and how it meets the locale file. |
| [themes.md](themes.md) | Writing or editing a theme. Roles, materials, tokens, typography, control sizes, motion, and the three shipped skins. |
| [modding.md](modding.md) | Writing a mod in Lua. The stages, the events, the commands, injection. |
| [api/lua.md](api/lua.md) | The generated `slotted.*` reference. |
| [api/slotted.d.luau](api/slotted.d.luau) | Type stubs, for luau-lsp completion in a mod. |
| [testing.md](testing.md) | Testing a game's UI in Rust, or a mod's behaviour in Lua. |
| [hot-reload.md](hot-reload.md) | What reloads, what survives, and what to do when it does not. |

Everything else is upstream of the guide: [`docs/PLAN.md`](../PLAN.md) is the
implementation plan, [`docs/design/`](../design/) holds each phase's contract,
and [`docs/adr/`](../adr/) holds the decisions that were expensive to make.
