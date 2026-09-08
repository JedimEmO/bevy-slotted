//! The skeleton's own guard: every embedded template parses (menus M2
//! contract 0). The packages replace this with `templates.rs`.

use slotted_menu::templates;

#[test]
fn every_embedded_template_parses_and_names_its_kind() {
    let defs = templates::all();
    let kinds: Vec<_> = defs.iter().map(|d| d.kind.clone()).collect();
    assert_eq!(kinds, templates::kinds::all().to_vec());
    let toast: slotted_ui::UiNodeDef =
        ron::from_str(templates::TOAST_NODE).expect("the toast snippet parses");
    assert_eq!(toast.id(), Some("toast"));
}
