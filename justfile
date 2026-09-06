# slotted developer commands. `just --list` shows this.

default: check

# Type-check every member, including tests, examples and benches.
check:
    cargo check --workspace --all-targets

# Run the whole test suite.
test:
    cargo test --workspace

# Clippy with the workspace lint set, warnings fatal.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# Rustdoc for the workspace, with broken intra-doc links fatal.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Regenerate docs/guide/api/ from the `---` doc blocks in the Lua prelude.
gen-docs:
    cargo run -p xtask -- gen-docs
    cargo run -p xtask -- luau-stubs

# Fail if either generated file is stale. What CI runs.
gen-docs-check:
    cargo run -p xtask -- gen-docs --check
    cargo run -p xtask -- luau-stubs --check

# Licences, advisories, banned crates and sources. Needs `cargo install
# cargo-deny`. Part of `ci` so an advisory shows up on the branch that
# introduced it rather than months later.
deny:
    cargo deny check

# `slotted`'s feature comments promise that `default-features = false` gives a
# server build without Bevy's UI stack. Feature flags are additive and a single
# `bevy/bevy_ui` anywhere below the facade silently breaks that promise for
# every consumer, with nothing but the compile time to show for it. This is the
# assertion, and it is a CI job of its own.
#
# Assert the dedicated-server dependency graph really has no UI in it.
server-check:
    #!/usr/bin/env bash
    set -euo pipefail
    # Both targets: a `cfg(target_arch)` dependency can put the UI stack back
    # into one graph and not the other, and the wasm server build is a real
    # target here rather than a hypothetical one.
    for target in "" "--target wasm32-unknown-unknown"; do
        found=$(cargo tree -p slotted --no-default-features --features server -e normal $target \
            | grep -oE "bevy_(ui|text|picking|winit|window|render)[a-z_]*" | sort -u || true)
        if [ -n "$found" ]; then
            echo "the server graph pulls Bevy's UI stack (${target:-native}):" >&2
            echo "$found" >&2
            echo "find the culprit with: cargo tree -p slotted --no-default-features --features server -e normal $target -i <crate>" >&2
            exit 1
        fi
    done
    echo "no bevy_ui, bevy_text, bevy_picking, bevy_winit, bevy_window or bevy_render in the server graph, on either target"
    cargo check -p slotted --no-default-features --features server
    cargo check --target wasm32-unknown-unknown -p slotted --no-default-features --features server
    # `crates/slotted/tests/server.rs` compiles only in this profile, because
    # half of what it asserts is what is absent. It is the assertion that the
    # graph the lines above check is a stack that actually runs.
    cargo test -p slotted --no-default-features --features server --test server

# What CI runs.
ci: fmt-check lint test gen-docs-check deny server-check

# The chest example in a window: the glass chest screen over a 3D scene.
run-chest:
    cargo run -p chest

# Recapture the example's four reference screenshots.
shot-chest:
    cargo run -p chest -- --shot examples/chest/shots/chest.png
    cargo run -p chest -- --hover 0 --shot examples/chest/shots/chest-hover.png
    cargo run -p chest -- --shot examples/chest/shots/chest-browser.png
    cargo run -p chest -- --recipe minecraft:coal --shot examples/chest/shots/chest-recipe.png
    cargo run -p chest -- --paint --shot examples/chest/shots/chest-paint.png
    just shot-check

# Are the icons actually in the captures?
#
# The GPU bake draws into the atlas the UI samples, so a bake that has not
# drawn leaves every icon transparent while the layout, the panel and the
# stack counts are all still correct. Nothing but the pixels can see that,
# which is what this measures. Run it after any recapture.
shot-check:
    python3 tools/check-shot.py examples/chest/shots/chest.png \
        examples/chest/shots/chest-hover.png \
        examples/chest/shots/chest-browser.png \
        examples/chest/shots/chest-recipe.png \
        examples/chest/shots/chest-paint.png

# The chest screen and its browser in the paper and neon themes (Phase 7).
shot-chest-themes:
    cargo run -p chest -- --theme paper --shot examples/chest/shots/chest-paper.png
    cargo run -p chest -- --theme neon --shot examples/chest/shots/chest-neon.png
    cargo run -p chest -- --theme paper --recipe minecraft:coal --shot examples/chest/shots/chest-recipe-paper.png
    cargo run -p chest -- --theme neon --recipe minecraft:coal --shot examples/chest/shots/chest-recipe-neon.png

# The modded example in a window: three mods, loaded from disk, hot reloading.
run-modded:
    cargo run -p modded

# Recapture the modded example's reference screenshots.
shot-modded:
    cargo run -p modded -- --no-console --shot examples/modded/shots/modded.png
    cargo run -p modded -- --shot examples/modded/shots/modded-console.png
    cargo run -p modded -- --reload copper_chest --shot examples/modded/shots/modded-reload.png

# The machine example in a window: tanks, bars, side tabs, a sorter mod.
run-machine:
    cargo run -p machine

# Recapture the machine example's reference screenshots.
shot-machine:
    cargo run -p machine -- --shot examples/machine/shots/machine.png
    cargo run -p machine -- --tab tab_redstone --shot examples/machine/shots/machine-tab.png

# Run every mod's tests/*.lua through the headless harness (Phase 6).
# `--screens examples/machine/screens` because `sorter` now injects into both
# the chest and the furnace from one copy under `examples/modded/mods`, and its
# `tests/sort_machine.lua` opens `machine:furnace`. The runner finds a
# `screens/` beside the mods directory on its own; the machine's is elsewhere.
test-mods dir="examples/modded/mods":
    cargo run -p xtask -- test-mods {{dir}} --screens examples/machine/screens

# Every crate that has to reach a browser, on wasm32, with the feature set a
# wasm build actually uses. There is one script runtime and it builds for both
# targets (ADR 0004), so nothing here has to swap it out.
wasm-check:
    cargo check --target wasm32-unknown-unknown -p slotted-model
    cargo check --target wasm32-unknown-unknown -p slotted-script
    cargo check --target wasm32-unknown-unknown -p slotted-script-luaur
    cargo check --target wasm32-unknown-unknown -p slotted-registry --no-default-features
    cargo check --target wasm32-unknown-unknown -p slotted-ecs
    cargo check --target wasm32-unknown-unknown -p slotted-net
    cargo check --target wasm32-unknown-unknown -p slotted-theme
    cargo check --target wasm32-unknown-unknown -p slotted-icons --all-features
    cargo check --target wasm32-unknown-unknown -p slotted-test --no-default-features --features script
    cargo check --target wasm32-unknown-unknown -p slotted-ui
    cargo check --target wasm32-unknown-unknown -p slotted-browser
    cargo check --target wasm32-unknown-unknown -p slotted-packs
    cargo check --target wasm32-unknown-unknown -p slotted --no-default-features --features ui,browser,packs,gpu-icons
    cargo check --target wasm32-unknown-unknown -p slotted --no-default-features --features server
    cargo check --target wasm32-unknown-unknown -p web-playground

# The luaur adapter in a real browser. Needs `cargo install wasm-pack` and a
# Chromium; the shim is there because the snap installs the driver as
# `chromium.chromedriver` and wasm-pack looks for that exact name.
wasm-test:
    PATH="$PWD/spikes/script-runtimes/chromedriver-shim:$PATH" \
        wasm-pack test --headless --chrome crates/slotted-script-luaur --test wasm

# One example to dist/<example>/: release wasm, wasm-bindgen, wasm-opt if it
# is installed. Needs `cargo install wasm-bindgen-cli` matching the crate.
wasm-build example:
    cargo run -p xtask -- wasm-build {{example}}

# The playground: the wasm build plus the page and the shared assets.
playground:
    cargo run -p xtask -- playground

# The small module for a tagged release: fat LTO and `wasm-opt -Oz`, ten
# minutes or more. CI and day-to-day use `playground`.
playground-dist:
    cargo run -p xtask -- playground --dist

# dist/ over HTTP with the wasm MIME type right.
# http://127.0.0.1:8080/web-playground/
serve:
    cargo run -p xtask -- serve

# The playground in a window, for debugging without a wasm build.
run-playground:
    cargo run -p web-playground

# End to end in headless Chromium. Two pages: `smoke.html` drives the raw
# exports (edit a mod's control.lua, assert the reloaded chunk's line comes back
# out of the console queue, and that a runaway chunk aborts the module as
# ADR 0004 says it must), then `index.html` is driven the way a person drives
# it, including an `error()` that traps the module and the restart that brings
# the chest back. Needs `just playground` and `just serve` first.
smoke:
    node examples/web-playground/tests/smoke.mjs

# One screenshot of the page, for a look at what CI would deploy.
shot-playground:
    node examples/web-playground/tests/smoke.mjs \
        --url http://127.0.0.1:8080/web-playground/index.html \
        --size 1760,900 \
        --wait 12000 --shot ~/snap/chromium/common/slotted-playground.png

# One screenshot per showcase scene, into examples/web-playground/shots/.
# Opens each scene through its `?scene=` link, checks the head strip against
# the scene table, and switches through the rail on the way to the next one.
# Needs `just playground` and `just serve` first.
shot-showcase:
    node examples/web-playground/tests/smoke.mjs \
        --url http://127.0.0.1:8080/web-playground/index.html \
        --size 1760,900 \
        --showcase --shots examples/web-playground/shots

# Phase 2 (slotted-icons) enables this.
# bake-icons:
#     cargo run -p xtask -- bake-icons --out assets/icons
