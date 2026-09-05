# ADR 0002: Headless UI testing uses Bevy's real picking backend

- Status: accepted
- Date: 2026-09-05
- Phase: 0 (spike)
- Spike: `spikes/headless-ui/`
- Supersedes nothing. Relates to `docs/PLAN.md` §4.12 and the risk row "Headless picking depends
  on Bevy internals".

## Context

`slotted-test` promises that a game built on these crates can drive its own screens from
`cargo test` with no window and no GPU, going through real layout and real picking rather than a
test-only shortcut. The plan flagged one uncertainty: `bevy_picking`'s UI backend reads
`Camera` and `RenderTarget`, and it was not known whether it would hit-test at all without a
render device. The fallback on the table was a harness-side hit-test over `ComputedNode`
rectangles that injects `Pointer<_>` events itself.

## Findings

A standalone spike on Bevy 0.19.1 builds an `App` with no `RenderPlugin`, no `WinitPlugin` and no
`bevy_ui_render`, spawns a `Window` entity and a `Camera2d`, and runs 19 passing tests.

1. **Layout is exact.** A 9x3 grid of 44px slots lands on the rects the CSS implies, at scale
   factor 1.0 and 2.0. `ComputedNode` and `UiGlobalTransform` are fully populated.
2. **The real picking backend works.** Writing `PointerInput` messages produces `Pointer<Click>`
   on the correct slot for all 27 slots, nothing in the gutters, correct `GlobalZIndex` ordering,
   correct `Hovered` and `bevy_ui::Pressed` transitions, and `bevy_ui_widgets::Activate`.
3. **Focus and keyboard work.** Tab, Shift+Tab, arrow-key directional navigation, digit keys and
   Enter-to-activate all behave as they would under winit.
4. **Time is deterministic.** `TimeUpdateStrategy::ManualDuration` makes every `app.update()`
   advance `Time<Virtual>` by exactly one fixed step, so `settle()` terminates at a predictable
   frame count.
5. **It is fast.** 11 to 24 µs per frame in release, 15 to 24 µs in a dev profile with optimised
   dependencies. Harness construction is 3.3 ms and dominates a short test.

Three things must be done by hand because `RenderPlugin` is absent:

- `Camera::computed.target_info` has to be filled in on the UI camera, because `camera_system`
  lives in `bevy_render`. Without it the UI sees a zero-sized render target. `bevy_ui`'s own unit
  tests do the same, so the pattern is sanctioned, but the field is engine-owned in spirit.
- Four assets normally registered by `bevy_render` must be registered: `Image`,
  `TextureAtlasLayout`, `Mesh`, `SkinnedMeshInverseBindposes`. Otherwise three systems fail
  parameter validation and panic on frame one.
- The virtual pointer must use `PointerId::Mouse`. `bevy_picking::hover::update_is_hovered`
  hard-codes that id, so `PointerId::Custom` clicks correctly but never flips `Hovered`. This
  contradicts the plan's assumption that the harness would own a `PointerId::Custom` pointer.

## Decision

**`slotted-test` drives Bevy's real UI picking backend. The harness-side hit-test fallback is not
built.**

Concretely:

- `SlottedPlugins::headless()` is the plugin list in `spikes/headless-ui/README.md`, minus the
  three optional entries a game supplies anyway.
- The harness owns the three workarounds above and hides them behind `UiHarness::builder()`.
  Each one is pinned by a test that fails loudly if upstream changes, including a test that
  *asserts the `PointerId::Custom` limitation still exists*.
- The primary virtual pointer is `PointerId::Mouse`. Secondary pointers use `PointerId::Custom`
  and are documented as not driving `Hovered`.
- Arrow-key navigation is `slotted-ui`'s to implement. Bevy ships `AutoDirectionalNavigator` as a
  `SystemParam` and wires no keys to it.

## Consequences

- The public `slotted-test` API in the plan's sketch stands unchanged. Semantic actions
  (`h.click(slot)`) hide the fact that a click costs three frames.
- We inherit upgrade risk on three specific Bevy internals rather than on the whole picking path.
  That is a smaller surface than the fallback would have been, and the fallback would have had to
  reimplement clipping, `UiStack` ordering and `Pickable` semantics to stay faithful.
- `settle()` needs a busy signal from the widget layer. Design it as a resource counting running
  motion tweens plus pending authority round-trips, and build it alongside the widgets in Phase 2
  rather than bolting it on.
- Per-test cost is dominated by harness construction (3.3 ms), not by frames. Tests should share
  a harness across assertions where practical, and CI cost is not a concern at this scale.
- The `debug` Bevy feature should be on in test builds. Without it, a missing resource reports as
  "Enable the debug feature to see the name", which cost real time in this spike.

## Unverified

- Text measurement without a render device. The spike's grid has no text; only `default_font` was
  exercised. Parley behaviour under `FontSource`/`FontSize` is an open question for Phase 2.
- Scroll, drag and drop, multi-window, and `wasm32`.
- Whether `Camera::computed.target_info` remains publicly writable in future Bevy releases.
