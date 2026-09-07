//! The value store (menus M1 contract 1.1): what a menu control is bound to
//! when there is no menu behind the screen.
//!
//! A control is a view of the store, never its own memory. A write is a
//! [`SetValue`] the store clamps and snaps by its [`ValueRule`], offers to
//! every [`ValueGuard`], then commits as a [`ValueChanged`] or refuses as a
//! [`ValueRefused`]. The control repaints from the store either way, so a
//! refused write snaps back with no code in the control.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::PropertyId;

/// A bound value. Untagged in data: `true`, `3`, `0.5`, `"high"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// A toggle.
    Bool(bool),
    /// A count, an index, a menu property.
    Int(i64),
    /// A slider.
    Float(f64),
    /// Free text, or an option id.
    Text(String),
}

impl Value {
    /// The value as a number, for rules and sliders.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Self::Int(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            Self::Bool(_) | Self::Text(_) => None,
        }
    }

    /// The value as text, for selects and Fluent arguments.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The value as a flag.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Text(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

/// The values, keyed by dotted string (`audio.master`, `my_mod.difficulty`).
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ValueStore {
    values: BTreeMap<String, Value>,
    version: u64,
}

impl ValueStore {
    /// The value under `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Bumps whenever a value commits or is inserted; controls repaint on it.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Game-side seed: no rule, no guard, no `ValueChanged`.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(key.into(), value.into());
        self.version += 1;
    }

    /// Removes a value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let out = self.values.remove(key);
        if out.is_some() {
            self.version += 1;
        }
        out
    }

    /// Every key, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// Every entry, sorted by key.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The commit path [`apply_set_values`] uses.
    #[allow(dead_code)]
    pub(crate) fn commit(&mut self, key: &str, value: Value) -> Option<Value> {
        let old = self.values.insert(key.to_owned(), value);
        self.version += 1;
        old
    }
}

/// Declared constraints on one key: what a control draws its range from and
/// what the store clamps and snaps to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ValueRule {
    /// Lower bound for numbers.
    #[serde(default)]
    pub min: Option<f64>,
    /// Upper bound for numbers.
    #[serde(default)]
    pub max: Option<f64>,
    /// Snap numbers to multiples of this above `min`.
    #[serde(default)]
    pub step: Option<f64>,
    /// Allowed ids for a `Text` value; empty = anything.
    #[serde(default)]
    pub options: Vec<String>,
}

/// The rules, keyed like the store.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ValueRules(pub BTreeMap<String, ValueRule>);

impl ValueRules {
    /// The rule for `key`, if any.
    pub fn get(&self, key: &str) -> Option<&ValueRule> {
        self.0.get(key)
    }

    /// Declares a rule.
    pub fn insert(&mut self, key: impl Into<String>, rule: ValueRule) {
        self.0.insert(key.into(), rule);
    }
}

/// A game's veto or rewrite over a write, asked after the rule and before
/// the commit.
pub trait ValueGuard: Send + Sync + 'static {
    /// `Ok(value)` commits `value` (possibly rewritten); `Err(reason)`
    /// refuses.
    fn check(&self, key: &str, proposed: &Value, store: &ValueStore) -> Result<Value, String>;
}

impl<F> ValueGuard for F
where
    F: Fn(&str, &Value, &ValueStore) -> Result<Value, String> + Send + Sync + 'static,
{
    fn check(&self, key: &str, proposed: &Value, store: &ValueStore) -> Result<Value, String> {
        self(key, proposed, store)
    }
}

/// The guards, asked in order.
#[derive(Resource, Default, Clone)]
pub struct ValueGuards(pub Vec<Arc<dyn ValueGuard>>);

impl ValueGuards {
    /// Adds a guard at the end.
    pub fn push(&mut self, guard: impl ValueGuard) {
        self.0.push(Arc::new(guard));
    }
}

/// A write request. Controls, the harness and (later) Lua send these.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct SetValue {
    /// The key.
    pub key: String,
    /// The proposed value.
    pub value: Value,
    /// The control that asked, if any.
    pub source: Option<Entity>,
}

/// A committed write.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ValueChanged {
    /// The key.
    pub key: String,
    /// What it was.
    pub old: Option<Value>,
    /// What it is.
    pub new: Value,
    /// The control that asked, if any.
    pub source: Option<Entity>,
}

/// A refused write.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ValueRefused {
    /// The key.
    pub key: String,
    /// What was proposed.
    pub value: Value,
    /// Why, from the guard or the rule.
    pub reason: String,
    /// The control that asked, if any.
    pub source: Option<Entity>,
}

/// What a control is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingTarget {
    /// A store key.
    Store(String),
    /// A menu property, mirrored through `SetProperty` and `PropertyChanged`.
    Property {
        /// The `OpenMenu` entity.
        menu: Entity,
        /// The property.
        id: PropertyId,
    },
}

/// On a value control: where its value lives.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ValueBinding {
    /// Where.
    pub target: BindingTarget,
}

/// `SlottedUiSet::Navigate`, before the stack systems: applies every
/// [`SetValue`] (menus M1 contract 1.1).
pub fn apply_set_values(
    _requests: MessageReader<SetValue>,
    _rules: Res<ValueRules>,
    _guards: Res<ValueGuards>,
    _store: ResMut<ValueStore>,
    _changed: MessageWriter<ValueChanged>,
    _refused: MessageWriter<ValueRefused>,
    _commands: Commands,
) {
    // M1-IMPL: B
}

/// The system set the value systems run in, first inside `Navigate`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueApply;

/// Registers the store's resources, messages and systems.
pub fn build(app: &mut App) {
    app.init_resource::<ValueStore>()
        .init_resource::<ValueRules>()
        .init_resource::<ValueGuards>()
        .add_message::<SetValue>()
        .add_message::<ValueChanged>()
        .add_message::<ValueRefused>()
        .configure_sets(
            Update,
            ValueApply.in_set(crate::plugin::SlottedUiSet::Navigate),
        )
        .add_systems(Update, apply_set_values.in_set(ValueApply));
}
