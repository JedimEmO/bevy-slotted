//! `RegistryLookup` over a small frozen registry.

#![allow(clippy::unwrap_used)]

use pretty_assertions::assert_eq;
use slotted_ecs::{Registries, RegistryLookup};
use slotted_model::{ItemId, LookupCtx};
use slotted_testutils::{food_tag, id, test_items, test_registries, tools_tag};

#[test]
fn the_lookup_reports_caps_and_tags_from_the_frozen_registry() {
    let registries = test_registries();
    let items = test_items(&registries);
    let resource = Registries(registries);
    let lookup = resource.lookup();

    assert_eq!(lookup.max_stack(items.stone), 64);
    assert_eq!(lookup.max_stack(items.egg), 16);
    assert_eq!(lookup.max_stack(items.sword), 1);

    assert!(lookup.has_tag(items.egg, &food_tag()));
    assert!(!lookup.has_tag(items.stone, &food_tag()));
    assert!(lookup.has_tag(items.sword, &tools_tag()));
    assert!(!lookup.has_tag(items.egg, &tools_tag()));
    assert!(!lookup.has_tag(items.egg, &id("test:missing")));
}

#[test]
fn an_unknown_item_stacks_to_one_and_carries_no_tags() {
    let registries = test_registries();
    let lookup = RegistryLookup(&registries);
    assert_eq!(lookup.max_stack(ItemId(4242)), 1);
    assert!(!lookup.has_tag(ItemId(4242), &food_tag()));
}

#[test]
fn the_empty_lookup_is_the_same_conservative_default() {
    let lookup = slotted_ecs::EmptyLookup;
    assert_eq!(lookup.max_stack(ItemId(0)), 1);
    assert!(!lookup.has_tag(ItemId(0), &food_tag()));
}
