//! The `slotted.test` protocol types (Phase 6 contract section 3.1). A test
//! body runs as a coroutine inside the script state; every harness action is a
//! [`TestOp`] yielded to the host, whose answer resumes the body.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use slotted_model::Value;

/// A node locator as a Lua table: `{ role = "slot", tag = { region = "chest" },
/// index = 0 }`. `slotted-test` maps it onto its `by::*` locators.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestLocator {
    /// `SemanticRole` name in snake case (`slot`, `side_tab`, `hud_layer`).
    #[serde(default)]
    pub role: Option<String>,
    /// Tags that must all match.
    #[serde(default)]
    pub tag: BTreeMap<String, String>,
    /// `test_id`.
    #[serde(default)]
    pub test_id: Option<String>,
    /// Displayed text.
    #[serde(default)]
    pub text: Option<String>,
    /// `WidgetKind`, `namespace:path`.
    #[serde(default)]
    pub widget: Option<String>,
    /// A slot holding this item.
    #[serde(default)]
    pub item: Option<String>,
    /// Which of the matches.
    #[serde(default)]
    pub index: Option<usize>,
}

/// One action a test body asks the harness to perform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TestOp {
    /// Open a screen with a fixture; replies `{ menu, screen }`.
    OpenScreen {
        /// Screen kind.
        kind: String,
        /// `"empty"` (the default), a host alias, or `{ slots = {..}, fill = {..} }`.
        #[serde(with = "crate::value::untagged", default = "unit")]
        fixture: Value,
    },
    /// Left click.
    Click {
        /// Target.
        loc: TestLocator,
    },
    /// Shift + left click.
    ShiftClick {
        /// Target.
        loc: TestLocator,
    },
    /// Right click.
    RightClick {
        /// Target.
        loc: TestLocator,
    },
    /// Move the pointer over.
    Hover {
        /// Target.
        loc: TestLocator,
    },
    /// Press and release a key by its `KeyCode` name (`Enter`, `KeyA`).
    Key {
        /// Key name.
        key: String,
    },
    /// Type text into the focused field.
    TypeText {
        /// Text.
        text: String,
    },
    /// `UiHarness::settle`.
    Settle,
    /// Advance frames.
    Step {
        /// How many.
        frames: u32,
    },
    /// Replies `{ item, count }` or nothing.
    StackAt {
        /// Target slot.
        loc: TestLocator,
    },
    /// Replies the node's text.
    TextOf {
        /// Target.
        loc: TestLocator,
    },
    /// Replies `{ id, value }` of the bound property, or nothing.
    PropertyOf {
        /// Target.
        loc: TestLocator,
    },
    /// Replies the fill fraction `0..=1`.
    TankFill {
        /// Target.
        loc: TestLocator,
    },
    /// Replies a bool.
    IsVisible {
        /// Target.
        loc: TestLocator,
    },
    /// Replies whether any script log line contains `text`.
    LogContains {
        /// Needle.
        text: String,
    },
    /// Cycle an icon button.
    Cycle {
        /// Target.
        loc: TestLocator,
        /// Forward or back.
        forward: bool,
    },
}

fn unit() -> Value {
    Value::Str("empty".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn a_step_reads_from_the_untyped_value_a_lua_table_becomes() {
        let mut loc = BTreeMap::new();
        loc.insert("role".to_owned(), Value::Str("slot".into()));
        loc.insert("index".to_owned(), Value::Int(3));
        let mut op = BTreeMap::new();
        op.insert("op".to_owned(), Value::Str("click".into()));
        op.insert("loc".to_owned(), Value::Map(loc));
        let parsed: TestOp = slotted_model::from_value(Value::Map(op)).expect("deserialises");
        assert_eq!(
            parsed,
            TestOp::Click {
                loc: TestLocator {
                    role: Some("slot".into()),
                    index: Some(3),
                    ..Default::default()
                },
            }
        );
    }
}
