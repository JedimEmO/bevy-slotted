//! `cargo xtask <command>`: the build steps that are more than one shell line.
//!
//! ```text
//! cargo xtask wasm-build web-playground   release wasm + wasm-bindgen into dist/<example>/
//! cargo xtask playground                  wasm-build, then the page and the assets
//! cargo xtask serve [--dir dist] [--port 8080]
//! ```
//!
//! Nothing here depends on a crate outside `std`: `just playground` is often
//! the first thing a new checkout runs, and a build tool that needs a build is
//! a bad trade.

mod serve;
mod wasm;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        usage();
        return ExitCode::FAILURE;
    };
    let command = command.as_str();
    let result = match command {
        "wasm-build" => wasm::wasm_build(rest),
        "playground" => wasm::playground(rest),
        "serve" => serve::serve(rest),
        "help" | "--help" | "-h" => {
            usage();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`; try `cargo xtask help`")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "\
cargo xtask <command>

  wasm-build <example> [--debug]   build for wasm32-unknown-unknown and run
                                   wasm-bindgen into dist/<example>/
  playground [--debug]             wasm-build web-playground, then copy the page
                                   and assets/ into dist/web-playground/
  serve [--dir <path>] [--port <n>]
                                   serve a directory over HTTP with the wasm
                                   MIME type set (default dist, port 8080)
"
    );
}
