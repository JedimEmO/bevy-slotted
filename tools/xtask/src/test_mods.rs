//! `cargo xtask test-mods <mods dir>`: the front door to `slotted-test`'s
//! `test-mods` binary. Phase 6 contract section 3.2. xtask stays free of
//! dependencies, so this is one `cargo run` and its exit code.

use std::process::Command;

/// Runs `cargo run -p slotted-test --features script --bin test-mods -- <args>`.
pub fn test_mods(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("test-mods needs a mods directory".to_owned());
    }
    // PHASE6-IMPL: C. Forward `--filter`, print the binary's report verbatim,
    // and turn a non-zero exit into `Err`.
    let status = Command::new(env!("CARGO"))
        .args([
            "run",
            "-p",
            "slotted-test",
            "--features",
            "script",
            "--bin",
            "test-mods",
            "--",
        ])
        .args(args)
        .status()
        .map_err(|e| format!("running cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("test-mods failed with {status}"))
    }
}
