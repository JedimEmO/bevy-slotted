# Architecture decision records

Records of the decisions that were expensive to make and would be expensive to
revisit. Each one states what we knew at the time, so a later reader can tell
whether the reasoning still holds.

| ADR | Title | Status | Decision in one line |
|---|---|---|---|
| [0001](0001-script-runtime.md) | Script runtime strategy | Proposed | mlua (Luau) natively, piccolo on wasm, luaur kept in the tree as the intended endgame. |
| [0002](0002-headless-ui-testing.md) | Headless UI testing uses Bevy's real picking backend | Accepted | The test harness drives screens through the real layout and picking path, not a hand-rolled hit test. |
| [0003](0003-glass-rendering.md) | Glass rendering, backdrop blur, and `bsn!` | Accepted | Glass and backdrop blur are buildable on `bevy_ui` 0.19.1; blur stays an optional feature. |

New records get the next number and follow the same shape: context, what we
found, options, decision, consequences, and an explicit list of what was not
verified.
