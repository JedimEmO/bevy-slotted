//! Mod discovery, load order and the layered source. Contract section 2.1.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use slotted_packs::{LayeredSource, ModError, ModSet, PackLayout};
use slotted_registry::AssetSource;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn layout() -> PackLayout {
    PackLayout::new(fixtures().join("base"))
        .with_mods(&fixtures().join("mods"))
        .expect("the fixture mods discover")
}

#[test]
fn discovery_reads_every_manifest_and_sorts_by_dependency() {
    let mods = ModSet::discover(&fixtures().join("mods")).expect("fixtures are valid");

    // Alphabetically alpha, beta, zeta; by dependency zeta, alpha, beta.
    let order: Vec<String> = mods.load_order().iter().map(ToString::to_string).collect();
    assert_eq!(order, ["zeta", "alpha", "beta"]);
    assert_eq!(mods.len(), 3);

    let beta = mods
        .get(&slotted_registry::ModId::new("beta").expect("a literal id"))
        .expect("beta is discovered");
    assert_eq!(beta.manifest.name, "Beta");
    assert_eq!(
        beta.data_script(),
        Some(fixtures().join("mods/beta/data.lua"))
    );
    assert_eq!(
        beta.control_script(),
        Some(fixtures().join("mods/beta/control.lua"))
    );
    assert_eq!(beta.asset_root(), fixtures().join("mods/beta"));
    assert_eq!(beta.locale_dir(), fixtures().join("mods/beta/locale"));
    assert_eq!(beta.paths().data, beta.data_script());
}

#[test]
fn a_missing_mods_directory_is_an_empty_set() {
    let mods = ModSet::discover(&fixtures().join("no-such-directory")).expect("not an error");
    assert!(mods.is_empty());
}

#[test]
fn a_directory_without_a_manifest_is_a_manifest_error() {
    let temp = tempdir("no-manifest");
    std::fs::create_dir_all(temp.join("lonely")).expect("temp dir");

    let error = ModSet::discover(&temp).expect_err("no mod.toml");
    let ModError::Manifest(error) = error else {
        panic!("expected a manifest error, got {error:?}");
    };
    assert!(error.message.contains("no mod.toml"), "{}", error.message);
    std::fs::remove_dir_all(&temp).ok();
}

#[test]
fn an_id_that_does_not_match_the_directory_is_refused() {
    let temp = tempdir("id-mismatch");
    std::fs::create_dir_all(temp.join("copper")).expect("temp dir");
    std::fs::write(
        temp.join("copper/mod.toml"),
        "id = \"tin\"\nname = \"Tin\"\nversion = \"1.0.0\"\napi_version = 1\n",
    )
    .expect("write manifest");

    let error = ModSet::discover(&temp).expect_err("the id is wrong");
    let ModError::Manifest(error) = error else {
        panic!("expected a manifest error, got {error:?}");
    };
    assert!(
        error.message.contains("does not match the directory name"),
        "{}",
        error.message
    );
    std::fs::remove_dir_all(&temp).ok();
}

#[test]
fn a_bad_version_is_a_manifest_error() {
    let temp = tempdir("bad-version");
    std::fs::create_dir_all(temp.join("copper")).expect("temp dir");
    std::fs::write(
        temp.join("copper/mod.toml"),
        "id = \"copper\"\nname = \"Copper\"\nversion = \"one point oh\"\napi_version = 1\n",
    )
    .expect("write manifest");

    let error = ModSet::discover(&temp).expect_err("the version is not semver");
    assert!(
        matches!(error, ModError::Manifest(_)),
        "expected a manifest error, got {error:?}"
    );
    std::fs::remove_dir_all(&temp).ok();
}

#[test]
fn the_last_mod_in_load_order_wins_a_path() {
    let source = LayeredSource::new(&layout());

    // zeta, then beta, both ship `textures/gui/chest.png`; beta loads later,
    // so beta's file is the one the game sees.
    let bytes = source.read("textures/gui/chest.png").expect("chest exists");
    assert_eq!(String::from_utf8(bytes).expect("utf-8").trim(), "beta");

    // A path only the base ships still resolves.
    let bytes = source.read("textures/gui/slot.png").expect("slot exists");
    assert_eq!(String::from_utf8(bytes).expect("utf-8").trim(), "base-only");

    assert!(matches!(
        source.read("textures/gui/nothing.png"),
        Err(slotted_registry::SourceError::NotFound(_))
    ));
}

#[test]
fn listing_a_directory_is_the_union_of_every_root() {
    let source = LayeredSource::new(&layout());
    let listed = source.list("textures/gui").expect("the directory exists");
    assert_eq!(listed, ["textures/gui/chest.png", "textures/gui/slot.png"]);

    // A directory nobody ships lists empty rather than failing.
    assert_eq!(
        source.list("textures/nothing").expect("empty"),
        Vec::<String>::new()
    );
}

#[test]
fn the_asset_reader_layers_and_reports_directories() {
    use bevy::asset::io::AssetReader;
    use bevy::tasks::block_on;
    use bevy::tasks::futures_lite::StreamExt;

    let reader = slotted_packs::LayeredAssetReader::new(&layout());

    assert!(block_on(reader.is_directory(Path::new("textures/gui"))).expect("no io error"));
    assert!(
        !block_on(reader.is_directory(Path::new("textures/gui/chest.png"))).expect("no io error")
    );

    let entries: Vec<PathBuf> = block_on(async {
        let mut stream = reader
            .read_directory(Path::new("textures/gui"))
            .await
            .expect("the directory exists");
        let mut out = Vec::new();
        while let Some(path) = stream.next().await {
            out.push(path);
        }
        out
    });
    assert_eq!(
        entries,
        [
            PathBuf::from("textures/gui/chest.png"),
            PathBuf::from("textures/gui/slot.png")
        ]
    );

    assert!(block_on(reader.read_directory(Path::new("textures/nothing"))).is_err());
}

/// A scratch directory under the target dir, removed by the test that made it.
fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slotted-packs-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}
