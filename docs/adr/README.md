# Architecture decision records

Records of the decisions that were expensive to make and would be expensive to
revisit. Each one states what we knew at the time, so a later reader can tell
whether the reasoning still holds.

| ADR | Title | Status | Decision in one line |
|---|---|---|---|
| [0001](0001-script-runtime.md) | Script runtime strategy | Accepted | mlua (Luau) natively, piccolo on wasm, luaur kept as the intended endgame. |
| [0002](0002-headless-ui-testing.md) | Headless UI testing uses Bevy's real picking backend | Accepted | The test harness drives screens through the real layout and picking path, not a hand-rolled hit test. |
| [0003](0003-glass-rendering.md) | Glass rendering, backdrop blur, and `bsn!` | Accepted | Glass and backdrop blur are buildable on `bevy_ui` 0.19.1; blur stays an optional feature. |

New records get the next number and follow the same shape: context, what we
found, options, decision, consequences, and an explicit list of what was not
verified.

An ADR is for a decision that was expensive to make and would be expensive to
revisit, and that a later reader would otherwise have to reconstruct. A choice
that is obvious from the code, or cheap to change, belongs in a doc comment
instead. A thing deliberately left undone belongs in
[`../FOLLOWUPS.md`](../FOLLOWUPS.md).

A record is written once and then amended rather than rewritten: its status line
changes as the decision is confirmed or superseded, and the body keeps saying
what was known at the time. Superseding a decision means a new record that names
the old one, not an edit to the old one's reasoning.
