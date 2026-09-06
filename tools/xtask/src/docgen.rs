//! Parse the `---` doc blocks out of the Lua prelude and write the guide's API
//! reference.
//!
//! The prelude is the only place the `slotted.*` surface is defined, so it is
//! the only place its documentation can live without drifting. A block is a run
//! of `---` lines above a definition:
//!
//! ```text
//! --- @group Data stage
//! --- Add an item to the registry.
//! ---
//! --- @signature slotted.register_item(id, def)
//! --- @stage data
//! --- @param id string An item id.
//! --- @return nil
//! --- @luau register_item: (id: string, def: { [string]: any }) -> ()
//! --- @example
//! --- slotted.register_item("copper_ingot", {})
//! ```
//!
//! `@group` carries forward to later blocks until another one changes it.
//! `@signature` is the identity of the entry and must be present; a run of
//! `---` lines without one is skipped, so a plain comment can still use the
//! marker.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One `@param` line: a name, a type, and the rest of the line.
pub struct Param {
    /// The argument name, or `...`.
    pub name: String,
    /// The declared type, one word.
    pub ty: String,
    /// What the argument means.
    pub doc: String,
}

/// One documented entry of the Lua API.
pub struct Entry {
    /// The `@group` in force, which becomes a section heading.
    pub group: String,
    /// The call as a modder writes it, from `@signature`.
    pub signature: String,
    /// Which stage the call is legal in, from `@stage`.
    pub stage: String,
    /// The prose above the tags.
    pub description: Vec<String>,
    /// The `@param` lines, in order.
    pub params: Vec<Param>,
    /// The `@return` type and its description, when there is one.
    pub returns: Option<(String, String)>,
    /// The Luau field declaration, from `@luau`.
    pub luau: Option<String>,
    /// The `@example` body.
    pub example: Vec<String>,
}

impl Entry {
    /// The heading anchor GitHub gives this entry's heading: lowercased, with
    /// everything but letters, digits, underscores and hyphens dropped, and
    /// spaces turned into hyphens. Dots and brackets go, so
    /// `slotted.register_item(id, def)` becomes `slottedregister_itemid-def`.
    fn anchor(&self) -> String {
        let mut out = String::new();
        for ch in self.signature.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if ch == '_' || ch == '-' {
                out.push(ch);
            } else if ch == ' ' {
                out.push('-');
            }
        }
        out
    }
}

/// Which tag a `--- @name rest` line carries, if any.
fn tag_of(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('@')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some((&rest[..end], rest[end..].trim_start()))
}

/// Parse every doc block in `source`. `group` carries in and out so several
/// files can share one running group.
pub fn parse(source: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut group = String::from("Other");
    let mut block: Vec<String> = Vec::new();

    let flush = |block: &mut Vec<String>, group: &mut String, entries: &mut Vec<Entry>| {
        if block.is_empty() {
            return;
        }
        let lines = std::mem::take(block);
        if let Some(entry) = parse_block(&lines, group) {
            entries.push(entry);
        }
    };

    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("---") {
            block.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else {
            flush(&mut block, &mut group, &mut entries);
        }
    }
    flush(&mut block, &mut group, &mut entries);
    entries
}

/// Turn one run of doc lines into an entry, updating the running group.
fn parse_block(lines: &[String], group: &mut String) -> Option<Entry> {
    let mut description = Vec::new();
    let mut signature = None;
    let mut stage = String::from("any");
    let mut params = Vec::new();
    let mut returns = None;
    let mut luau = None;
    let mut example: Vec<String> = Vec::new();
    let mut in_example = false;

    for line in lines {
        match tag_of(line.trim_start()) {
            Some(("group", rest)) => {
                *group = rest.to_string();
                in_example = false;
            }
            Some(("signature", rest)) => {
                signature = Some(rest.to_string());
                in_example = false;
            }
            Some(("stage", rest)) => {
                stage = rest.to_string();
                in_example = false;
            }
            Some(("param", rest)) => {
                in_example = false;
                let mut words = rest.splitn(3, char::is_whitespace);
                let name = words.next().unwrap_or_default().to_string();
                let ty = words.next().unwrap_or_default().to_string();
                let doc = words.next().unwrap_or_default().trim().to_string();
                params.push(Param { name, ty, doc });
            }
            Some(("return", rest)) => {
                in_example = false;
                let (ty, doc) = match rest.split_once(char::is_whitespace) {
                    Some((ty, doc)) => (ty.to_string(), doc.trim().to_string()),
                    None => (rest.to_string(), String::new()),
                };
                returns = Some((ty, doc));
            }
            Some(("luau", rest)) => {
                luau = Some(rest.to_string());
                in_example = false;
            }
            Some(("example", _)) => in_example = true,
            Some(_) | None => {
                if in_example {
                    example.push(line.clone());
                } else if !(line.trim().is_empty() && description.is_empty()) {
                    description.push(line.clone());
                }
            }
        }
    }

    while description.last().is_some_and(|l| l.trim().is_empty()) {
        description.pop();
    }
    while example.last().is_some_and(|l| l.trim().is_empty()) {
        example.pop();
    }

    Some(Entry {
        group: group.clone(),
        signature: signature?,
        stage,
        description,
        params,
        returns,
        luau,
        example,
    })
}

/// Where the prelude lives, relative to the workspace root.
pub const PRELUDE_DIR: &str = "crates/slotted-script/prelude";

/// The workspace root, found by walking up from this file's crate.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// Read and parse both prelude files. Returns `(slotted entries, test entries)`.
pub fn parse_prelude(root: &Path) -> Result<(Vec<Entry>, Vec<Entry>), String> {
    let read = |name: &str| -> Result<String, String> {
        let path = root.join(PRELUDE_DIR).join(name);
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    Ok((
        parse(&read("slotted.lua")?),
        parse(&read("slotted_test.lua")?),
    ))
}

/// `cargo xtask gen-docs`: write `docs/guide/api/lua.md`.
pub fn gen_docs(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "cargo xtask gen-docs [--check]\n\n  Regenerate docs/guide/api/lua.md from the\n  `---` doc blocks in crates/slotted-script/prelude/.\n  --check fails instead of writing when the file is stale."
        );
        return Ok(());
    }
    let check = args.iter().any(|a| a == "--check");
    let root = workspace_root();
    let (api, test) = parse_prelude(&root)?;
    if api.is_empty() {
        return Err("no doc blocks found in slotted.lua".to_string());
    }
    let rendered = render_markdown(&api, &test);
    let out = root.join("docs/guide/api/lua.md");
    write_or_check(&out, &rendered, check)
}

/// Write `text` to `path`, or compare and fail when `check` is set.
pub fn write_or_check(path: &Path, text: &str, check: bool) -> Result<(), String> {
    if check {
        let current = std::fs::read_to_string(path).unwrap_or_default();
        if current == text {
            println!("{} is up to date", path.display());
            return Ok(());
        }
        return Err(format!(
            "{} is stale; run `cargo xtask gen-docs` and `cargo xtask luau-stubs`",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Group entries by their `@group`, keeping first-seen order.
fn grouped(entries: &[Entry]) -> Vec<(String, Vec<&Entry>)> {
    let mut out: Vec<(String, Vec<&Entry>)> = Vec::new();
    for entry in entries {
        match out.iter_mut().find(|(name, _)| *name == entry.group) {
            Some((_, list)) => list.push(entry),
            None => out.push((entry.group.clone(), vec![entry])),
        }
    }
    out
}

/// Render both preludes as one Markdown page.
fn render_markdown(api: &[Entry], test: &[Entry]) -> String {
    let mut out = String::new();
    out.push_str(
        "# The Lua API\n\n\
         Generated by `cargo xtask gen-docs` from the `---` doc blocks in\n\
         `crates/slotted-script/prelude/slotted.lua` and `slotted_test.lua`. Edit those\n\
         files, not this one.\n\n\
         Every function here only builds tables. Nothing in a script reaches the host\n\
         directly: a data script emits registrations, a control script answers events with\n\
         commands, and the host validates each one before applying it.\n\n\
         Type stubs for luau-lsp are in [`slotted.d.luau`](slotted.d.luau).\n\n",
    );

    for (title, entries, intro) in [
        (
            "`slotted`",
            api,
            "Available in every script. `data.lua` may register; `control.lua` may not.",
        ),
        (
            "`slotted.test`",
            test,
            "Available only in a mod's `tests/*.lua`, which the harness runs headless. See [testing](../testing.md).",
        ),
    ] {
        if entries.is_empty() {
            continue;
        }
        let _ = write!(out, "## {title}\n\n{intro}\n\n");
        let groups = grouped(entries);
        for (group, list) in &groups {
            let _ = write!(out, "**{group}.** ");
            let links: Vec<String> = list
                .iter()
                .map(|e| format!("[`{}`](#{})", call_name(&e.signature), e.anchor()))
                .collect();
            let _ = writeln!(out, "{}\n", links.join(", "));
        }
        for (group, list) in &groups {
            let _ = write!(out, "### {group}\n\n");
            for entry in list {
                render_entry(&mut out, entry);
            }
        }
    }
    out
}

/// The dotted name of a signature, without its argument list.
fn call_name(signature: &str) -> &str {
    signature.split('(').next().unwrap_or(signature).trim()
}

/// One entry as Markdown.
fn render_entry(out: &mut String, entry: &Entry) {
    let _ = write!(out, "#### `{}`\n\n", entry.signature);
    if entry.stage != "any" {
        let _ = write!(out, "*Stage: `{}`.*\n\n", entry.stage);
    }
    for line in &entry.description {
        let _ = writeln!(out, "{line}");
    }
    out.push('\n');
    if !entry.params.is_empty() {
        out.push_str("| Argument | Type | |\n|---|---|---|\n");
        for param in &entry.params {
            let _ = writeln!(out, "| `{}` | `{}` | {} |", param.name, param.ty, param.doc);
        }
        out.push('\n');
    }
    if let Some((ty, doc)) = &entry.returns {
        if ty == "nil" {
            out.push_str("Returns nothing.\n\n");
        } else {
            let _ = write!(out, "Returns `{ty}`");
            if doc.is_empty() {
                out.push_str(".\n\n");
            } else {
                let _ = write!(out, ": {doc}\n\n");
            }
        }
    }
    if !entry.example.is_empty() {
        out.push_str("```lua\n");
        for line in &entry.example {
            let _ = writeln!(out, "{line}");
        }
        out.push_str("```\n\n");
    }
}
