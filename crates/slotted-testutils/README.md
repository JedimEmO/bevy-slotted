# slotted-testutils

Internal test doubles and builders for the
[slotted](https://github.com/mathiasmyrland/bevy-slotted) workspace.

This crate is a dev-dependency of the other members and is never published
(`publish = false`). Consumer-facing UI automation lives in `slotted-test`
instead, which is a normal published crate.

## Main types

| Type | What it is |
|---|---|
| `RecordingAuthority`, `RejectingAuthority`, `Submitted` | Fake authorities. |
| `minimal_ecs_app`, `ecs_app_with` | A headless `App` factory. |
| `builders::*` | Inventory and registry builders the crates' tests share. |
| `conformance::*` | The script conformance suite, over any `&mut dyn ScriptRuntime`. |

## Example

```rust
use slotted_testutils::{RecordingAuthority, ecs_app_with};

let authority = RecordingAuthority::new();
let mut app = ecs_app_with(authority.clone());
app.update();
assert!(authority.is_empty());
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `conformance` | no | The script conformance suite. Adapters turn it on from their dev-dependencies. |

## Licence

MIT OR Apache-2.0, at your option.
