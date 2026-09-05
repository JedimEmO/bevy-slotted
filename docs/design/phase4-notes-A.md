# Phase 4 notes: package A (script port, mlua adapter, conformance suite)

What A changed outside its own files, and what B and C need to know. Contract:
`docs/design/phase4-contract.md`, sections 1.4 to 1.7.

## Changes to shared files

### `slotted-script/prelude/slotted.lua` -- one output buffer instead of two

The prelude kept registrations in `pending` and log lines in `log_buffer`, and
`__slotted_dispatch` drained the log buffer *before* the pending list. Two
buffers cannot represent the order the calls happened in, and the contract's own
seed case `control_subscribe_and_log.lua` expects the handler's `Log` before the
command the handler returned. Both buffers are now one `emitted` array in call
order, and `drain` empties it:

- chunk-time emissions (`register_*`, `inject`, `add_tooltip_part`,
  `slotted.log`, `print`, `__deprecated`) come out in the order they were
  called;
- at `control_start` the `Subscribe` command is emitted first, then the buffer,
  then handler output, which is the order the contract's section 1.4 specifies;
- inside the handler loop, `drain` now runs after each handler's `pcall` and
  before its return value is appended, so a handler that logs and then returns a
  command produces the log first.

No signature changed and no function was added or removed. **B and C:** the only
visible difference is command ordering within one dispatch reply. If a pack
applies commands in return order, which section 2.4 says it does, nothing else
moves.

### `slotted-testutils` -- `serde` added to the `conformance` feature

`conformance` now enables `dep:serde` as well as `dep:slotted-script` and
`dep:ron`; the RON directive parser is generic over `DeserializeOwned`. No other
crate is affected.

### `slotted-testutils/src/conformance.rs` -- three additions, no removals

Every signature the contract names is unchanged. Added alongside them:

- `Report { results: Vec<CaseResult> }` with `total`, `passed` and `failures`,
- `run_conformance_dir(&mut dyn ScriptRuntime, &Path) -> Report`, the
  directory-taking form; `run_all` is now a call to it with `cases_dir()`,
- `load_cases_from(&Path)`, the directory-taking form of `load_cases`,
- `ExpectedError::as_str`.

`assert_conformance` additionally fails when the suite holds fewer than the 20
cases the contract requires, so a case file that silently disappears is caught.

`slotted-script/src/*` needed no change: the port, both enums and the `Value`
bridge were right as shipped.

## Decisions the contract left open

**Directive parsing.** `STAGE` and `EXPECT_ERROR` take the rest of their own
line. `EVENTS` and `EXPECT` accumulate following `-- ` lines until the joined
text parses as RON, so a block may span as many lines as it likes. Any other
leading comment line is prose and is ignored, which is what lets
`sandbox_io.lua` explain itself under its `EXPECT_ERROR`.

**A data chunk calling `slotted.on` is allowed, not an error.** Section 1.4 says
the `data_stage` reply is the pending list followed by what `on("data_stage")`
handlers returned, so the prelude has to accept it. `on_in_data_stage.lua` pins
that. A control chunk calling `register_item` *is* an error, raised by the
prelude's `data_only` guard; `register_in_control_stage.lua` pins that.

**`ScriptError::Sandbox` at run time.** The contract lists "write to a frozen
table" under `Sandbox`, but Luau raises an ordinary runtime error for it. The
adapter therefore classifies a runtime error whose text mentions `readonly` or
`read-only` as `Sandbox`. Reaching a removed global (`io.write(...)`) is a plain
`Runtime` error, because indexing `nil` is all Luau sees; the contract's own
seed case `sandbox_io.lua` already expects `runtime`.

**Budget exhaustion leaves the state loaded and usable.** The state is not
recreated. The interrupt counter and the budget are re-armed at the top of every
`call`, so the next event runs with a full budget in the same state, with
whatever the script had already registered still there.
`the_state_survives_budget_exhaustion` and `the_state_survives_a_handler_error`
pin this.

**Tracebacks.** mlua gives no traceback for an error raised in a Lua chunk called
from Rust, and `debug` is removed from the sandbox. `call` therefore invokes
`xpcall(__slotted_dispatch, __slotted_traceback, event)`, where
`__slotted_traceback` is a Rust function installed before sandboxing that calls
`Lua::traceback` while the erroring stack is still live, stashes the result and
returns the error value unchanged. `__slotted_traceback` is a global a mod can
see; it only ever returns its argument.

## Cost

Measured on this machine, release build, one loaded control script with one
`slot_click` handler, 50 000 calls after a warm-up:

| event | per call |
|---|---|
| `SlotClick` with a stack and modifiers | 7.1 us |
| `SearchChanged` with a short string | 1.6 us |

The interrupt budget is not the cost: the same numbers come back with the budget
at `u64::MAX`. It is the serde crossing, dominated by building the event table.
Resolving `__slotted_dispatch`, `xpcall` and the message handler once at load
rather than per call took `SlotClick` from 8.1 us to 7.1 us. This is two orders
of magnitude under a frame, so per-click and per-hover hooks are affordable, but
it is 20x the spike's 308 ns figure, which timed a bare `Function::call` with a
two-key table and no serde on either side.

## Suite

30 cases in `crates/slotted-testutils/conformance/`. Beyond the four seeds:
every `register_*` shape including a nested screen tree and a widget template,
`inject`, `add_tooltip_part`, id namespacing, the `slotted.*` globals, both
value-bridge halves (integral versus fractional numbers, array versus map,
empty table), unicode, `print` routing, every `slotted.cmd.*` constructor, a
handler for each of the nine event variants, several handlers on one event,
`deprecated` reported once, and the negative cases: budget, memory, compile,
runtime, wrong arguments, `register_*` at the control stage, a frozen `slotted`
table, a frozen `string` library, and every forbidden global asserted nil.
