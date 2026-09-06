//! `cargo xtask luau-stubs`: write `docs/guide/api/slotted.d.luau`.
//!
//! luau-lsp reads a definition file and gives a mod author completion and type
//! errors over the `slotted.*` surface. The field declarations come from the
//! `@luau` tag of the same doc blocks the reference is generated from, so the
//! stubs cannot drift from the prelude.
//!
//! The event and command name unions are read out of `ScriptEvent::name` and
//! `ScriptCommand::name` in `slotted-script`, which are the two functions that
//! decide those strings. The payload fields of each event are hand-maintained
//! below; there is no derive to read them from, and they change rarely.

use std::fmt::Write as _;

use crate::docgen::{self, Entry};

/// Payload fields per event, keyed by the `type` string. Hand-maintained
/// against `crates/slotted-script/src/events.rs`; `gen-docs` does not read it.
const EVENT_FIELDS: &[(&str, &str)] = &[
    ("data_stage", "api_version: number"),
    ("control_start", "api_version: number, mods: { string }"),
    (
        "slot_click",
        "menu: number, screen: string, slot: number, button: Button, modifiers: Modifiers, stack: StackInfo?",
    ),
    (
        "widget_activate",
        "menu: number?, screen: string, widget: string, tags: { [string]: string }",
    ),
    ("tooltip_build", "stack: StackInfo, tier: Tier"),
    ("recipe_lookup", "item: string, mode: LookupMode"),
    ("screen_opened", "menu: number?, screen: string"),
    ("screen_closed", "menu: number?, screen: string"),
    ("hud_tick", "elapsed_ms: number"),
    ("search_changed", "text: string"),
    (
        "property_changed",
        "menu: number, screen: string, property: number, value: number",
    ),
];

/// Read the `=> "name",` arms of a `name()` match in a `slotted-script` source
/// file. Both enums spell it the same way, so one reader does for both.
fn names_from(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_name_fn = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub const fn name(") {
            in_name_fn = true;
            continue;
        }
        if !in_name_fn {
            continue;
        }
        if trimmed == "}" {
            in_name_fn = false;
            continue;
        }
        if let Some((_, rest)) = trimmed.split_once("=> \"")
            && let Some((name, _)) = rest.split_once('"')
        {
            out.push(name.to_string());
        }
    }
    out
}

/// A Luau union of string literals, wrapped over several lines.
fn union(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join("\n\t| ")
}

/// The `@luau` declarations of entries whose signature starts with `prefix`.
fn fields(entries: &[Entry], prefix: &str) -> Vec<(String, String)> {
    entries
        .iter()
        .filter_map(|e| {
            let rest = e.signature.strip_prefix(prefix)?;
            // The name only, so `...` in an argument list is not read as a
            // dotted path: `slotted.cmd.log` must not match the `slotted.` pass
            // while `slotted.log(level, fmt, ...)` must.
            let name = rest.split('(').next().unwrap_or(rest);
            if name.contains('.') {
                return None;
            }
            let luau = e.luau.clone()?;
            // One line: a Luau doc comment is not wrapped by the editor.
            let doc = e
                .description
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('@'))
                .collect::<Vec<_>>()
                .join(" ");
            Some((doc, luau))
        })
        .collect()
}

/// Render one Luau table type from a set of declarations.
fn render_type(out: &mut String, name: &str, note: &str, decls: &[(String, String)]) {
    let _ = write!(out, "--- {note}\nexport type {name} = {{\n");
    for (doc, decl) in decls {
        if !doc.is_empty() {
            let _ = writeln!(out, "\t--- {doc}");
        }
        let _ = writeln!(out, "\t{decl},");
    }
    out.push_str("}\n\n");
}

/// `cargo xtask luau-stubs`.
pub fn luau_stubs(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "cargo xtask luau-stubs [--check]\n\n  Regenerate docs/guide/api/slotted.d.luau from the\n  `@luau` tags in crates/slotted-script/prelude/ and the\n  event and command names in slotted-script.\n  --check fails instead of writing when the file is stale."
        );
        return Ok(());
    }
    let check = args.iter().any(|a| a == "--check");
    let root = docgen::workspace_root();
    let (api, test) = docgen::parse_prelude(&root)?;

    let read = |rel: &str| -> Result<String, String> {
        let path = root.join(rel);
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let events = names_from(&read("crates/slotted-script/src/events.rs")?);
    let commands = names_from(&read("crates/slotted-script/src/commands.rs")?);
    if events.is_empty() || commands.is_empty() {
        return Err("could not read the event or command names from slotted-script".to_string());
    }

    let mut out = String::new();
    out.push_str(
        "--!strict\n\
         -- slotted.d.luau: type definitions for the `slotted.*` Lua API.\n\
         --\n\
         -- Generated by `cargo xtask luau-stubs`. Do not edit; edit the `@luau` tags in\n\
         -- crates/slotted-script/prelude/slotted.lua and slotted_test.lua instead.\n\
         --\n\
         -- Point luau-lsp at this file to get completion and type errors over a mod's\n\
         -- data.lua, control.lua and tests/*.lua:\n\
         --\n\
         --     luau-lsp analyze --definitions=slotted.d.luau data.lua control.lua\n\
         --\n\
         -- or, in .luaurc:\n\
         --\n\
         --     { \"aliases\": {}, \"languageMode\": \"strict\" }\n\
         --\n\
         -- with `luau-lsp.types.definitionFiles` set to this path in your editor.\n\n",
    );

    let _ = write!(
        out,
        "--- The `type` field of an event handed to a `slotted.on` handler, which is also\n\
         --- the name you subscribe with.\nexport type EventName =\n\t| {}\n\n",
        union(&events)
    );
    let _ = write!(
        out,
        "--- The `type` field of a command a script returns. You never write these\n\
         --- literals: `slotted.cmd.*` and the `register_*` calls build them.\n\
         export type CommandName =\n\t| {}\n\n",
        union(&commands)
    );

    out.push_str(
        "export type Button = \"left\" | \"right\" | \"middle\"\nexport type Tier = \"compact\" | \"expanded\"\nexport type LookupMode = \"recipes\" | \"uses\"\nexport type LogLevel = \"trace\" | \"debug\" | \"info\" | \"warn\" | \"error\"\n\nexport type Modifiers = { shift: boolean, ctrl: boolean, alt: boolean }\n\n--- A stack as a script sees it: names, not interned ids.\nexport type StackInfo = {\n\titem: string,\n\tcount: number,\n\tcomponents: { [string]: any },\n}\n\n--- A command table. Build these with `slotted.cmd.*` rather than by hand.\nexport type Command = { type: CommandName, [string]: any }\n\n",
    );

    out.push_str("--- The event tables, by their `type` field.\n");
    for (name, fields) in EVENT_FIELDS {
        let ty: String = name
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        let _ = writeln!(
            out,
            "export type {ty}Event = {{ type: \"{name}\", {fields} }}"
        );
    }
    out.push('\n');

    render_type(
        &mut out,
        "SlottedCmd",
        "Command builders. A control handler returns one of these, or an array of them.",
        &fields(&api, "slotted.cmd."),
    );
    render_type(
        &mut out,
        "SlottedTest",
        "The harness a mod's tests/*.lua drives. Only present in the test stage.",
        &fields(&test, "slotted.test."),
    );

    let mut api_fields = fields(&api, "slotted.");
    api_fields.push((
        "Command builders.".to_string(),
        "cmd: SlottedCmd".to_string(),
    ));
    api_fields.push((
        "The test harness, in the test stage only.".to_string(),
        "test: SlottedTest?".to_string(),
    ));
    render_type(
        &mut out,
        "Slotted",
        "The global every script sees. Read-only: assigning into it raises.",
        &api_fields,
    );

    out.push_str("declare slotted: Slotted\n");
    for entry in &api {
        if entry.signature.starts_with("slotted") {
            continue;
        }
        if let Some(luau) = &entry.luau {
            let _ = writeln!(out, "declare {luau}");
        }
    }
    out.push('\n');

    docgen::write_or_check(&root.join("docs/guide/api/slotted.d.luau"), &out, check)
}
