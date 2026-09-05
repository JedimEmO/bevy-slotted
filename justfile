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

doc:
    cargo doc --workspace --no-deps --all-features

# What CI runs.
ci: fmt-check lint test

# The chest example in a window: the glass chest screen over a 3D scene.
run-chest:
    cargo run -p chest

# Recapture the example's four reference screenshots.
shot-chest:
    cargo run -p chest -- --shot examples/chest/shots/chest.png
    cargo run -p chest -- --hover 0 --shot examples/chest/shots/chest-hover.png
    cargo run -p chest -- --shot examples/chest/shots/chest-browser.png
    cargo run -p chest -- --recipe minecraft:coal --shot examples/chest/shots/chest-recipe.png

# The modded example in a window: three mods, loaded from disk, hot reloading.
run-modded:
    cargo run -p modded

# Recapture the modded example's reference screenshots.
shot-modded:
    cargo run -p modded -- --no-console --shot examples/modded/shots/modded.png
    cargo run -p modded -- --shot examples/modded/shots/modded-console.png
    cargo run -p modded -- --reload copper_chest --shot examples/modded/shots/modded-reload.png

# Phase 5 (web playground) enables these.
# wasm-build example:
#     cargo build --target wasm32-unknown-unknown -p {{example}} --release
#     wasm-bindgen --target web --out-dir dist target/wasm32-unknown-unknown/release/{{example}}.wasm

# serve:
#     python3 -m http.server --directory dist 8080

# Phase 2 (slotted-icons) enables this.
# bake-icons:
#     cargo run -p xtask -- bake-icons --out assets/icons
