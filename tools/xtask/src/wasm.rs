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

/// How a wasm example is built.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The `debug` profile, no optimiser: for a quick look.
    Debug,
    /// `wasm-release` (thin LTO, symbols stripped) and `wasm-opt -O1`: the
    /// CI default, a few minutes end to end.
    Release,
    /// `wasm-dist` (fat LTO, one codegen unit) and `wasm-opt -Oz`: the small
    /// module for a tagged release, ten minutes or more.
    Dist,
}

impl Mode {
    fn from_args(args: &[String]) -> Result<(Self, Vec<String>), String> {
        let mut mode = Self::Release;
        let mut rest = Vec::new();
        for arg in args {
            match arg.as_str() {
                "--debug" => mode = Self::Debug,
                "--release" => mode = Self::Release,
                "--dist" => mode = Self::Dist,
                other if other.starts_with('-') => return Err(format!("unknown flag `{other}`")),
                other => rest.push(other.to_owned()),
            }
        }
        Ok((mode, rest))
    }

    fn profile(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "wasm-release",
            Self::Dist => "wasm-dist",
        }
    }

    fn opt_level(self) -> &'static str {
        match self {
            Self::Debug | Self::Release => "-O1",
            Self::Dist => "-Oz",
        }
    }
}

/// `cargo xtask wasm-build <example> [--debug|--release|--dist]`.
pub fn wasm_build(args: &[String]) -> Result<(), String> {
    let (mode, rest) = Mode::from_args(args)?;
    let example = rest.first().ok_or("wasm-build needs an example name")?;
    build(example, mode).map(|_| ())
}

/// Builds `example` and runs `wasm-bindgen`, returning the `.wasm` sizes
/// before and after `wasm-opt`.
fn build(example: &str, mode: Mode) -> Result<Sizes, String> {
    let root = workspace_root();
    // The profiles are defined at the workspace root; see `Mode`.
    let profile = mode.profile();

    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo
        .current_dir(&root)
        .args(["build", "--target", "wasm32-unknown-unknown", "-p", example]);
    if mode != Mode::Debug {
        cargo.args(["--profile", profile]);
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
    // `-O1` by default: measured on the playground, it reaches 31 MiB raw and
    // 7.7 MiB gzipped in about a minute, where `-Oz` reaches 26.5 MiB in six.
    // Pages serves the module compressed, so the visitor pays under a megabyte
    // for five minutes off every CI run. `--dist` (or `SLOTTED_WASM_OPT=-Oz`)
    // for a release.
    let level = std::env::var("SLOTTED_WASM_OPT").unwrap_or_else(|_| mode.opt_level().to_owned());
    eprintln!("xtask: running wasm-opt {level}; this can take a while with no output");
    let optimised = if which("wasm-opt") {
        let mut opt = Command::new("wasm-opt");
        opt.current_dir(&root).args([
            &level,
            // The module uses reference types (the function table grows at
            // runtime for closures) and multivalue. An optimiser that is not
            // told so emits a table that cannot grow: `Table.grow() failed`
            // on first load, which is what an unpinned binaryen shipped.
            "--enable-reference-types",
            "--enable-multivalue",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--strip-debug",
            "--strip-producers",
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

/// `cargo xtask playground [--debug|--release|--dist]`: the wasm build plus
/// the page.
pub fn playground(args: &[String]) -> Result<(), String> {
    let (mode, _) = Mode::from_args(args)?;
    build(PLAYGROUND, mode)?;

    let root = workspace_root();
    let out = root.join("dist").join(PLAYGROUND);
    copy_dir(&root.join("examples/web-playground/web"), &out)?;
    // Bevy's web asset reader fetches `assets/<path>` relative to the page, so
    // the shared demo assets have to sit beside index.html.
    copy_dir(&root.join("assets"), &out.join("assets"))?;
    eprintln!(
        "xtask: dist/{PLAYGROUND}/ is ready. Run `just serve` and open http://127.0.0.1:8080/{PLAYGROUND}/"
    );
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
