# Values

A settings screen has no menu behind it. What its toggles and sliders are bound
to is the **value store**: one resource of named values that the game seeds,
the controls edit, and the game reads back. The rule the whole design rests on
is that a control paints from its binding and never from its own memory. A
write is a request the store validates; the control repaints from whatever the
store decided, so a refused write snaps back without the control knowing.

## The store

```rust
use slotted::prelude::*;

fn seed(mut store: ResMut<ValueStore>) {
    store.insert("settings.ui_scale", 1.0);
    store.insert("settings.reduced_motion", false);
    store.insert("settings.resolution", "1920x1080");
    store.insert("settings.player_name", "Steve");
}
```

`ValueStore` is a `BTreeMap` from a dotted key to a `Value`, plus a `version`
that bumps on every commit. `insert` is the game-side seed: no rule, no guard,
no message. `get(key)` reads; `keys()` and `iter()` walk it; `remove(key)`
forgets. Keys are free strings; the convention is `<area>.<name>`, and a mod
prefixes its own (`my_mod.difficulty`).

`Value` is untagged in data, so a screen file or a Lua table writes the
literal:

| Variant | Written as | Bound by |
|---|---|---|
| `Bool` | `true` | `toggle` |
| `Int` | `3` | `list` (the selected row), a menu property |
| `Float` | `0.5` | `slider` |
| `Text` | `"high"` | `select`, `radio_group`, `tabs` (an option or tab id), `text_field` |

`as_f64`, `as_str` and `as_bool` are the readers, and `From` covers `bool`,
`i64`, `f64`, `&str` and `String`.

## Rules

A `ValueRule` declares what a key accepts, so a `set_value` from a test, a
mod or a control that misread its range lands where it should:

```rust
fn rules(mut rules: ResMut<ValueRules>) {
    rules.insert("settings.ui_scale", ValueRule {
        min: Some(0.5), max: Some(3.0), step: Some(0.25), ..Default::default()
    });
    rules.insert("settings.resolution", ValueRule {
        options: ["1280x720", "1920x1080", "2560x1440"].map(String::from).into(),
        ..Default::default()
    });
}
```

For a number the store snaps to `min + round((v - min) / step) * step` first,
then clamps to `min..=max`; an `Int` stays an `Int`. For a `Text` with a
non-empty `options` list, anything not in the list is refused with a reason
naming it. A `Bool` passes through. A key with no rule takes anything.

A slider clamps and snaps its own proposals to the range it declares, so a
screen that forgets the rule still behaves; the rule is what catches a write
that did not come from the slider.

## Guards

A `ValueGuard` is the game's veto or rewrite, asked after the rule and before
the commit. A closure is a guard:

```rust
fn guards(mut guards: ResMut<ValueGuards>) {
    guards.push(|key: &str, proposed: &Value, _store: &ValueStore| {
        if key == "settings.ui_scale" && proposed.as_f64().is_some_and(|v| v > 2.0) {
            return Err("a UI scale above 2 does not fit this window".to_owned());
        }
        Ok(proposed.clone())
    });
}
```

Guards run in the order they were pushed, each seeing the store as it is before
the commit. The first `Err` refuses the write; an `Ok(value)` may hand back a
rewritten value, which the next guard sees. This is where "the window cannot
go that big", "that name is taken" and "unlock this first" live, in one place
rather than in every control.

## Messages

| Message | Written by | Carries |
|---|---|---|
| `SetValue { key, value, source }` | a control, a test, a mod, the game | The request. `source` is the control's entity, so a guard or a log can say who asked. |
| `ValueChanged { key, old, new, source }` | the store, on commit | What the game reads to apply a setting. |
| `ValueRefused { key, value, reason, source }` | the store, on refusal | What a screen shows as a warning. |

`apply_set_values` runs in `SlottedUiSet::Navigate`, before the stack systems,
and handles every `SetValue` of the frame: rule, guards, then either one
`ValueChanged` and a `version` bump or one `ValueRefused` and no bump. A write
that changes nothing still commits and still announces itself, so a control
whose write was a no-op hears that the store agreed.

```rust
fn apply(mut changed: MessageReader<ValueChanged>, mut motion: ResMut<Motion>) {
    for change in changed.read() {
        if change.key == "settings.reduced_motion" {
            motion.reduced = change.new.as_bool().unwrap_or(false);
        }
    }
}
```

## Bindings

Every value control carries a `ValueBinding { target }`, built from the node's
`bind` or `property` field at spawn:

- `BindingTarget::Store(key)` reads and writes the store.
- `BindingTarget::Property { menu, id }` mirrors a menu property instead: a
  write is a `slotted_ecs::SetProperty` on the menu and a change arrives as a
  `PropertyChanged`, the same path a tank or a bar uses. Only `Int` travels
  this way (`Bool` as 0 or 1, `Float` rounded); a `Text` is logged and
  dropped. A `property` on a screen with no menu is logged and dropped too.

A node that names both is a load error naming the node; a node that names
neither keeps its own state, because there is nothing to paint from.

Delivery is an entity event, `BoundValue { entity, value }`, triggered by
`sync_value_bindings` in `SlottedUiSet::Render` for every binding that is new
and for every store binding whenever `version` moved. Each control observes it
and copies the value into its state component (`ToggleState`, `SliderState`,
`SelectState`, ...), and its paint system runs after, so the value that
arrived this frame is painted this frame. No control reads the store.

A control writes through `ValueWriter`, a `SystemParam` that turns
`write(source, &binding, value)` into whichever message the binding wants. A
widget of your own does the same:

```rust
fn on_my_accept(action: On<FocusedAction>, bindings: Query<&ValueBinding>, mut writer: ValueWriter) {
    if action.action != UiAction::Accept { return; }
    if let Ok(binding) = bindings.get(action.entity) {
        writer.write(action.entity, binding, Value::Bool(true));
    }
}
```

## In a test

`UiHarness::value(key)` reads the store and `set_value(key, value)` writes a
`SetValue` and steps a frame, so a test exercises the rule and the guards the
way a control would. The settings demo's test seeds a guard that refuses a UI
scale above 2, drags the slider past it, and asserts the slider snapped back;
see [testing.md](testing.md). A mod's Lua test has `slotted.test.value(key)`
and `slotted.test.set_value(key, value)`.
