//! The mods and base assets, compiled in, and the editable source over them.

use std::sync::{Arc, Mutex};

use bevy::prelude::Resource;
use slotted_registry::{AssetSource, InMemorySource, SourceError};

use crate::bus::quote;

include!(concat!(env!("OUT_DIR"), "/bundle.rs"));

/// The mod ids the bundle carries, in directory order.
///
/// [`ModSet::discover_in`](slotted_packs::ModSet::discover_in) names the mod
/// directories itself, because the [`AssetSource`] port lists files and never
/// directories.
pub fn mod_ids() -> Vec<String> {
    let mut ids: Vec<String> = FILES
        .iter()
        .filter_map(|(path, _)| path.strip_prefix("mods/"))
        .filter_map(|rest| rest.strip_suffix("/mod.toml"))
        .map(ToOwned::to_owned)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// A mod's script entry points as `(name, logical path)`, in the order the
/// editor should show them.
pub fn script_files(mod_id: &str) -> Vec<(String, String)> {
    let prefix = format!("scripts/{mod_id}/");
    let mut files: Vec<(String, String)> = FILES
        .iter()
        .filter_map(|(path, _)| path.strip_prefix(&prefix).map(|name| (name, *path)))
        .filter(|(name, _)| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|e| e == "lua")
        })
        .map(|(name, path)| (name.to_owned(), path.to_owned()))
        .collect();
    // `data.lua` before `control.lua`: the stages run in that order and the
    // tab bar should read the same way.
    files.sort_by_key(|(name, _)| (name != "data.lua", name.clone()));
    files
}

/// The mods and their editable files, as the JSON the page reads.
///
/// ```json
/// [{"id":"copper_chest","files":[{"name":"data.lua","path":"scripts/..."}]}]
/// ```
///
/// It lives here rather than in `bridge` so a native test can assert the shape
/// the page parses; the `wasm-bindgen` export is one line over it.
pub fn mods_json() -> String {
    let mods: Vec<String> = mod_ids()
        .into_iter()
        .map(|id| {
            let files: Vec<String> = script_files(&id)
                .into_iter()
                .map(|(name, path)| {
                    format!("{{\"name\":{},\"path\":{}}}", quote(&name), quote(&path))
                })
                .collect();
            format!("{{\"id\":{},\"files\":[{}]}}", quote(&id), files.join(","))
        })
        .collect();
    format!("[{}]", mods.join(","))
}

/// One bundled mod file's text.
///
/// `name` is the file name the editor shows (`data.lua`), not a path.
///
/// # Errors
///
/// A mod the bundle does not carry, or a file that mod does not have. The page
/// asks for both by name out of whatever is in its URL, so this is a value and
/// not a panic: on wasm a panic is an aborted module and a dead canvas.
pub fn read_mod_file(mod_id: &str, name: &str) -> Result<String, String> {
    // Reads a fresh bundle rather than the live source: the page asks for this
    // to fill the editor, and the bundle is what "Reset" means.
    let source = EditableSource::from_bundle();
    if !mod_ids().iter().any(|id| id == mod_id) {
        return Err(format!("there is no mod `{mod_id}` in the bundle"));
    }
    source
        .read_text(&format!("scripts/{mod_id}/{name}"))
        .ok_or_else(|| format!("`{mod_id}` has no file `{name}`"))
}

/// The bundle as an [`AssetSource`] whose files can be replaced at runtime.
///
/// The editor writes a new `control.lua` here and then asks for a reload;
/// `ModLoader::reload_mod` reads the new text back out through the very same
/// handle, so the browser path and the disk path differ in nothing but where
/// the bytes came from.
#[derive(Resource, Clone, Default)]
pub struct EditableSource(Arc<Mutex<InMemorySource>>);

impl EditableSource {
    /// Every file the build script baked in.
    pub fn from_bundle() -> Self {
        let mut source = InMemorySource::new();
        for (path, contents) in FILES {
            source.insert(*path, *contents);
        }
        Self(Arc::new(Mutex::new(source)))
    }

    /// Replaces one file, adding it when it was not there.
    pub fn write(&self, path: &str, contents: &str) {
        self.lock().insert(path, contents);
    }

    /// One file's contents as text, when it is there and is UTF-8.
    pub fn read_text(&self, path: &str) -> Option<String> {
        self.lock()
            .read(path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InMemorySource> {
        // The only writers are the wasm bridge and the reload system, both on
        // the browser's single thread, so this never actually contends.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl std::fmt::Debug for EditableSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EditableSource(..)")
    }
}

impl AssetSource for EditableSource {
    fn list(&self, dir: &str) -> Result<Vec<String>, SourceError> {
        self.lock().list(dir)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        self.lock().read(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_carries_the_three_demo_mods() {
        assert_eq!(mod_ids(), ["appleskin_like", "copper_chest", "sorter"]);
    }

    #[test]
    fn every_mod_ships_a_data_and_a_control_chunk() {
        for id in mod_ids() {
            let names: Vec<String> = script_files(&id).into_iter().map(|(n, _)| n).collect();
            assert_eq!(names, ["data.lua", "control.lua"], "mod {id}");
        }
    }

    #[test]
    fn a_write_is_what_the_next_read_sees() {
        let source = EditableSource::from_bundle();
        let path = "scripts/copper_chest/control.lua";
        assert!(
            source
                .read_text(path)
                .is_some_and(|t| t.contains("slotted.on"))
        );
        source.write(path, "-- replaced\n");
        assert_eq!(source.read_text(path).as_deref(), Some("-- replaced\n"));
    }
}
