//! `wasm-build` and `playground`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root: `tools/xtask/../..`.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// The example the playground command builds.
pub const PLAYGROUND: &str = "web-playground";

/// `cargo xtask wasm-build <example> [--debug]`.
pub fn wasm_build(args: &[String]) -> Result<(), String> {
    let mut example = None;
    let mut release = true;
    for arg in args {
        match arg.as_str() {
            "--debug" => release = false,
            "--release" => release = true,
            other if other.starts_with('-') => return Err(format!("unknown flag `{other}`")),
            other => example = Some(other.to_owned()),
        }
    }
    let example = example.ok_or("wasm-build needs an example name")?;
    build(&example, release).map(|_| ())
}

/// Builds `example` and runs `wasm-bindgen`, returning the `.wasm` sizes
/// before and after `wasm-opt`.
fn build(example: &str, release: bool) -> Result<Sizes, String> {
    let root = workspace_root();
    // `wasm-release` is defined at the workspace root: release, size-tuned,
    // and stripped, which is the difference between an 80 MiB module and a
    // shippable one.
    let profile = if release { "wasm-release" } else { "debug" };

    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo
        .current_dir(&root)
        .args(["build", "--target", "wasm32-unknown-unknown", "-p", example]);
    if release {
        cargo.args(["--profile", "wasm-release"]);
    }
    run(&mut cargo, "cargo build")?;

    let input = root
        .join("target/wasm32-unknown-unknown")
        .join(profile)
        .join(format!("{}.wasm", example.replace('-', "_")));
    if !input.is_file() {
        return Err(format!("cargo produced no {}", input.display()));
    }
    let before = size_of(&input)?;

    let out_dir = root.join("dist").join(example);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let mut bindgen = Command::new("wasm-bindgen");
    bindgen.current_dir(&root).args([
        "--target",
        "web",
        "--no-typescript",
        "--out-dir",
        &out_dir.to_string_lossy(),
        &input.to_string_lossy(),
    ]);
    run(&mut bindgen, "wasm-bindgen")
        .map_err(|e| format!("{e}\n  install it with `cargo install wasm-bindgen-cli`"))?;

    let bindgen_wasm = out_dir.join(format!("{}_bg.wasm", example.replace('-', "_")));
    let bound = size_of(&bindgen_wasm)?;

    // `wasm-opt` is optional: without it the page still works, it is just
    // bigger, and requiring a binstall to see the playground is a bad trade.
    let optimised = if which("wasm-opt") {
        let mut opt = Command::new("wasm-opt");
        opt.current_dir(&root).args([
            "-Oz",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            &bindgen_wasm.to_string_lossy(),
            "-o",
            &bindgen_wasm.to_string_lossy(),
        ]);
        run(&mut opt, "wasm-opt")?;
        Some(size_of(&bindgen_wasm)?)
    } else {
        eprintln!("xtask: wasm-opt is not on PATH; shipping the unoptimised module");
        None
    };

    let sizes = Sizes {
        before,
        bound,
        optimised,
    };
    eprintln!("xtask: {}", sizes.report());
    Ok(sizes)
}

/// `cargo xtask playground [--debug]`: the wasm build plus the page.
pub fn playground(args: &[String]) -> Result<(), String> {
    let mut owned = vec![PLAYGROUND.to_owned()];
    owned.extend(args.iter().cloned());
    let release = !args.iter().any(|a| a == "--debug");
    build(PLAYGROUND, release)?;

    let root = workspace_root();
    let out = root.join("dist").join(PLAYGROUND);
    copy_dir(&root.join("examples/web-playground/web"), &out)?;
    // Bevy's web asset reader fetches `assets/<path>` relative to the page, so
    // the shared demo assets have to sit beside index.html.
    copy_dir(&root.join("assets"), &out.join("assets"))?;
    eprintln!("xtask: dist/{PLAYGROUND}/ is ready; `cargo xtask serve`");
    Ok(())
}

/// Wasm sizes at each step, in bytes.
pub struct Sizes {
    /// What `cargo build` produced.
    pub before: u64,
    /// After `wasm-bindgen`.
    pub bound: u64,
    /// After `wasm-opt -Oz`, when it is installed.
    pub optimised: Option<u64>,
}

impl Sizes {
    fn report(&self) -> String {
        let mib = |bytes: u64| {
            // Whole MiB and hundredths, without a float: a wasm module is
            // nowhere near `u64`'s range but clippy is right that the cast
            // is lossy in principle.
            let whole = bytes / (1024 * 1024);
            let hundredths = (bytes % (1024 * 1024)) * 100 / (1024 * 1024);
            format!("{whole}.{hundredths:02} MiB")
        };
        match self.optimised {
            Some(optimised) => format!(
                "wasm {} -> bindgen {} -> wasm-opt {}",
                mib(self.before),
                mib(self.bound),
                mib(optimised)
            ),
            None => format!("wasm {} -> bindgen {}", mib(self.before), mib(self.bound)),
        }
    }
}

fn size_of(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Copies every file under `from` into `to`, creating directories as it goes.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Err(format!("{} is not a directory", from.display()));
    }
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", from.display()))?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

/// Whether `program` runs at all.
fn which(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn run(command: &mut Command, what: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("running {what}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed with {status}"))
    }
}
