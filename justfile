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

# What CI runs.
ci: fmt-check lint test gen-docs-check

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
test-mods dir="examples/machine/mods":
    cargo run -p xtask -- test-mods {{dir}}

# Every crate that has to reach a browser, on wasm32, with the feature set a
# wasm build actually uses. `slotted` drops `script-mlua`: Luau is C++ and
# cannot target wasm at all (ADR 0001).
wasm-check:
    cargo check --target wasm32-unknown-unknown -p slotted-model
    cargo check --target wasm32-unknown-unknown -p slotted-script
    cargo check --target wasm32-unknown-unknown -p slotted-script-piccolo
    cargo check --target wasm32-unknown-unknown -p slotted-registry --no-default-features
    cargo check --target wasm32-unknown-unknown -p slotted-ecs
    cargo check --target wasm32-unknown-unknown -p slotted-theme
    cargo check --target wasm32-unknown-unknown -p slotted-icons --all-features
    cargo check --target wasm32-unknown-unknown -p slotted-test --no-default-features --features script
    cargo check --target wasm32-unknown-unknown -p slotted-ui
    cargo check --target wasm32-unknown-unknown -p slotted-browser
    cargo check --target wasm32-unknown-unknown -p slotted-packs
    cargo check --target wasm32-unknown-unknown -p slotted --no-default-features --features ui,browser,packs,gpu-icons
    cargo check --target wasm32-unknown-unknown -p web-playground

# One example to dist/<example>/: release wasm, wasm-bindgen, wasm-opt if it
# is installed. Needs `cargo install wasm-bindgen-cli` matching the crate.
wasm-build example:
    cargo run -p xtask -- wasm-build {{example}}

# The playground: the wasm build plus the page and the shared assets.
playground:
    cargo run -p xtask -- playground

# dist/ over HTTP with the wasm MIME type right.
# http://127.0.0.1:8080/web-playground/
serve:
    cargo run -p xtask -- serve

# The playground in a window, for debugging without a wasm build.
run-playground:
    cargo run -p web-playground

# End to end in headless Chromium: load dist/, edit a mod's control.lua through
# the same export the editor uses, assert the reloaded chunk's line comes back
# out of the console queue. Needs `just playground` and `just serve` first.
smoke:
    node examples/web-playground/tests/smoke.mjs

# One screenshot of the page, for a look at what CI would deploy.
shot-playground:
    node examples/web-playground/tests/smoke.mjs \
        --url http://127.0.0.1:8080/web-playground/index.html \
        --size 1760,900 \
        --wait 12000 --shot ~/snap/chromium/common/slotted-playground.png

# Phase 2 (slotted-icons) enables this.
# bake-icons:
#     cargo run -p xtask -- bake-icons --out assets/icons
