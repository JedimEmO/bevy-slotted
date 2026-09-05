//! The playground in a window, for debugging the parts that are not the page.
//!
//! ```text
//! cargo run -p web-playground
//! ```
//!
//! Same app, same bundled mods, same runtime. What is missing is the editor:
//! nothing pushes onto the bus, so nothing ever reloads. Use it to check the
//! scene, the screen and the console without a wasm build in between.

fn main() {
    web_playground::build_app(web_playground::bus::Bus::new()).run();
}
