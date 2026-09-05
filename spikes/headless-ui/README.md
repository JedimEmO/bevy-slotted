# Spike: headless `bevy_ui` layout, picking, focus and time

Phase 0 spike for `slotted-test` (see `docs/PLAN.md` §4.12). Standalone package, not a
workspace member. Bevy 0.19.1, Rust 1.96.1.

```sh
cd spikes/headless-ui
CARGO_TARGET_DIR=$PWD/target cargo test
CARGO_TARGET_DIR=$PWD/target cargo run --release --bin timing
```

19 tests, all passing, no window, no GPU, no winit.

## Verdict

The **real `bevy_ui` picking backend works headless**. No harness-side hit-test fallback is
needed. `Pointer<Click>`, `Pointer<Press>`, `Pointer<Release>`, `Hovered`, `bevy_ui::Pressed`
and `bevy_ui_widgets::Activate` all behave the same as they would under winit, driven purely by
`PointerInput` messages.

## The plugin list that actually worked

```rust
TaskPoolPlugin::default()          // optional
FrameCountPlugin                   // optional
TimePlugin                         // REQUIRED
ScheduleRunnerPlugin::run_once()   // optional (we call app.update() ourselves)
TransformPlugin                    // optional for UI; bevy_ui computes UiGlobalTransform itself
AssetPlugin::default()             // REQUIRED
WindowPlugin { primary_window: Some(..), exit_condition: DontExit, close_when_requested: false }
InputPlugin                        // REQUIRED
bevy::a11y::AccessibilityPlugin    // optional
bevy_camera::CameraPlugin          // REQUIRED (InheritedVisibility, which picking checks)
bevy_text::TextPlugin              // REQUIRED (bevy_ui's text measurement systems)
bevy_ui::UiPlugin                  // pulls in UiPickingPlugin via the `bevy_picking` feature
bevy_picking::PickingPlugin
bevy_picking::InteractionPlugin
bevy_input_focus::InputFocusPlugin // REQUIRED
bevy_input_focus::InputDispatchPlugin
bevy_input_focus::tab_navigation::TabNavigationPlugin
bevy_input_focus::directional_navigation::DirectionalNavigationPlugin
bevy_ui_widgets::UiWidgetsPlugins
```

"REQUIRED"/"optional" was established by deleting each entry and re-running the suite.
`RenderPlugin`, `WinitPlugin`, `bevy_ui_render` and `PointerInputPlugin` are all absent.

`bevy` features used: `std, bevy_asset, bevy_camera, bevy_input_focus, bevy_log, bevy_picking,
bevy_text, bevy_ui, bevy_ui_widgets, bevy_window, default_font, ui_picking, debug`.
The `debug` feature is only there so system-parameter validation failures name the system.

## Three things the harness has to do by hand

Each of these is a consequence of `RenderPlugin` being absent. All three are small, stable and
easy to keep working across upgrades.

1. **Fill in `Camera::computed.target_info` on the UI camera.** `camera_system`, which normally
   does this, lives in `bevy_render`. Without it, `propagate_ui_target_cameras` sees a
   `UVec2::ZERO` render target, every percentage-sized node collapses and layout is meaningless.
   `bevy_ui`'s own unit tests do exactly the same thing, so the pattern is sanctioned.
2. **Register four assets that `bevy_render` would register:** `Image`, `TextureAtlasLayout`,
   `Mesh`, `SkinnedMeshInverseBindposes`. Without them
   `bevy_ui::widget::image::update_image_content_size_system` and
   `bevy_camera::visibility::{calculate_bounds, update_skinned_mesh_bounds}` fail parameter
   validation and panic on the first frame.
3. **Use `PointerId::Mouse` for the primary virtual pointer.**
   `bevy_picking::hover::update_is_hovered` hard-codes `PointerId::Mouse`, so a
   `PointerId::Custom` pointer produces correct `Pointer<Click>` events but never flips the
   `Hovered` / `DirectlyHovered` components. Nothing spawns a mouse pointer without
   `PointerInputPlugin` (winit), so the harness spawns one itself. This is pinned by
   `custom_pointer_clicks_but_does_not_update_hovered` in `tests/picking.rs`, which will start
   failing if upstream generalises the system.

## What the tests cover

- `tests/layout.rs` — a 9x3 grid of 44px slots at a known origin lands on exactly the expected
  logical rects, at scale factor 1.0 and 2.0; the root fills the virtual window.
- `tests/picking.rs` — a click at a slot centre reaches that slot and only that slot, for all 27
  slots; a click in the 4px gutter reaches nothing; `Hovered` and `Pressed` flip on move, press
  and release; a sibling with a higher `GlobalZIndex` wins the hit; `bevy_ui_widgets::Activate`
  fires from a synthetic click.
- `tests/focus.rs` — Tab and Shift+Tab walk `TabIndex` order; arrow keys navigate geometrically
  through `AutoDirectionalNavigator`; a digit key reaches an ordinary system through
  `ButtonInput<KeyCode>`; Enter activates the focused `Button` with no pointer involved.
- `tests/time_and_settle.rs` — `TimeUpdateStrategy::ManualDuration` gives an identical delta on
  every frame; `settle()` terminates at the exact frame a 100ms animation ends and gives up at
  `max_frames` when motion never ends.

Arrow-key navigation is *not* wired up by Bevy. `AutoDirectionalNavigator` is a `SystemParam`;
the ~15-line system that reads arrow keys and calls it lives in `tests/focus.rs` and will have to
live in `slotted-ui`.

## Timings

27-slot screen, 32-core machine, 2000 frames after warm-up.

| measurement | dev profile | release |
| --- | --- | --- |
| harness build + first 3 frames | 3.68 ms | 3.28 ms |
| `app.update()`, layout clean | 14.9 µs | 11.5 µs |
| `app.update()`, layout dirty every frame | 24.0 µs | 20.4 µs |
| `pointer_click()` (3 frames + messages) | 51.2 µs | 41.3 µs |

The dev profile here is `opt-level = 1` with `opt-level = 3` for dependencies, which is what a
consumer's `cargo test` should use. A test that opens a screen, settles and performs twenty
clicks costs roughly 5 ms, so the per-test cost is dominated by harness construction.

## Harness ergonomics

`Harness` in `src/lib.rs` is deliberately thin: `step(n)`, `settle(max_frames)`,
`find_by_name`, `rect_of`, `center_of`, `pointer_move_to`, `pointer_press`, `pointer_release`,
`click_at`, `pointer_click`, `key_press`, `hold_key`, `focus`, `set_focus`.

Two ergonomics notes for `slotted-test`:

- `settle()` needs a busy signal from the widget layer. The spike models it as a `Busy(u32)`
  resource; the real one should be a count of running motion tweens plus pending authority
  round-trips, so `settle()` is meaningful with motion both on and off.
- A pointer action costs one frame each. `pointer_click` is therefore three frames. Callers
  should never have to know that, which argues for the semantic actions in the plan's API sketch
  rather than exposing raw pointer steps.

## Not verified here

- Text content and `FontSource`/`FontSize`: the grid has no text, so `default_font` was enough.
  Whether parley measures text identically without a render device is untested.
- Scroll, drag and drop, and multi-window.
- `wasm32`. The whole thing is native-only in this spike.
- Whether `Camera::computed.target_info` stays writable. It is a public field on a public struct
  today, but it is engine-owned in spirit and could be sealed in a future release.
