//! `cargo xtask test-mods <mods dir>`: the front door to `slotted-test`'s
//! `test-mods` binary. Phase 6 contract section 3.2. xtask stays free of
//! dependencies, so this is one `cargo run` and its exit code.

use std::process::Command;

/// Runs `cargo run -p slotted-test --features script-luaur --bin test-mods --
/// <args>`, inheriting stdout and stderr so the report is the binary's own.
///
/// `script-luaur` rather than `script`: the binary needs a script runtime of
/// its own, which a consumer of the harness would otherwise unify in through
/// its own `slotted` dependency.
///
/// # Errors
///
/// No mods directory, an argument this does not forward, cargo failing to
/// start, or a non-zero exit, which is what a failing test produces.
pub fn test_mods(args: &[String]) -> Result<(), String> {
    let Some((dir, rest)) = args.split_first() else {
        return Err("test-mods needs a mods directory".to_owned());
    };
    let mut forwarded: Vec<String> = vec![dir.clone()];
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--filter" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--filter needs a mod id".to_owned())?;
                forwarded.push("--filter".to_owned());
                forwarded.push(value.clone());
            }
            // Repeatable: one mod can inject into more than one game, and its
            // test files then want a screen from each.
            "--screens" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--screens needs a directory".to_owned())?;
                forwarded.push("--screens".to_owned());
                forwarded.push(value.clone());
            }
            other => return Err(format!("unknown test-mods argument `{other}`")),
        }
    }

    let status = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "slotted-test",
            "--features",
            "script-luaur",
            "--bin",
            "test-mods",
            "--",
        ])
        .args(&forwarded)
        .status()
        .map_err(|e| format!("running cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("mod tests failed ({status})"))
    }
}
