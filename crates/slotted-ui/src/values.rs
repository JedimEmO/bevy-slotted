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

/// The number a rule can clamp and snap, kept in the value's own variant.
fn apply_rule(rule: &ValueRule, value: &Value) -> Result<Value, String> {
    match value {
        Value::Bool(_) => Ok(value.clone()),
        Value::Text(id) => {
            if rule.options.is_empty() || rule.options.iter().any(|o| o == id) {
                Ok(value.clone())
            } else {
                Err(format!(
                    "`{id}` is not one of the allowed options ({})",
                    rule.options.join(", ")
                ))
            }
        }
        Value::Int(_) | Value::Float(_) => {
            let mut v = value.as_f64().unwrap_or_default();
            if let Some(step) = rule.step.filter(|s| *s > 0.0) {
                let origin = rule.min.unwrap_or(0.0);
                v = origin + ((v - origin) / step).round() * step;
            }
            if let Some(min) = rule.min {
                v = v.max(min);
            }
            if let Some(max) = rule.max {
                v = v.min(max);
            }
            Ok(match value {
                #[allow(clippy::cast_possible_truncation)]
                Value::Int(_) => Value::Int(v.round() as i64),
                _ => Value::Float(v),
            })
        }
    }
}

/// `SlottedUiSet::Navigate`, before the stack systems: applies every
/// [`SetValue`] (menus M1 contract 1.1).
///
/// The rule clamps and snaps first, then every guard is asked in order (the
/// first `Err` refuses, an `Ok` may rewrite), then the value commits and
/// `version` bumps. A write that changes nothing still commits and still
/// writes `ValueChanged`: the store is the truth and says so every time.
pub fn apply_set_values(
    mut requests: MessageReader<SetValue>,
    rules: Res<ValueRules>,
    guards: Res<ValueGuards>,
    mut store: ResMut<ValueStore>,
    mut changed: MessageWriter<ValueChanged>,
    mut refused: MessageWriter<ValueRefused>,
) {
    for request in requests.read() {
        let constrained = match rules.get(&request.key) {
            Some(rule) => apply_rule(rule, &request.value),
            None => Ok(request.value.clone()),
        };
        let checked = constrained.and_then(|mut value| {
            for guard in &guards.0 {
                value = guard.check(&request.key, &value, &store)?;
            }
            Ok(value)
        });
        match checked {
            Ok(value) => {
                let old = store.commit(&request.key, value.clone());
                changed.write(ValueChanged {
                    key: request.key.clone(),
                    old,
                    new: value,
                    source: request.source,
                });
            }
            Err(reason) => {
                tracing::debug!(key = %request.key, %reason, "value refused");
                refused.write(ValueRefused {
                    key: request.key.clone(),
                    value: request.value.clone(),
                    reason,
                    source: request.source,
                });
            }
        }
    }
}

impl ValueBinding {
    /// The binding a node's `bind` / `property` pair asks for, or `None`
    /// when it names neither. A `property` outside a menu-driven screen is
    /// logged and dropped: there is no menu to mirror.
    pub fn from_def(def: &crate::def::BindDef, menu: Option<Entity>) -> Option<Self> {
        if let Some(key) = &def.bind {
            return Some(Self {
                target: BindingTarget::Store(key.clone()),
            });
        }
        let id = def.property?;
        let Some(menu) = menu else {
            tracing::warn!(
                ?id,
                "a `property` binding on a screen with no menu; ignored"
            );
            return None;
        };
        Some(Self {
            target: BindingTarget::Property { menu, id },
        })
    }

    /// The store key, when this is a store binding.
    pub fn key(&self) -> Option<&str> {
        match &self.target {
            BindingTarget::Store(key) => Some(key),
            BindingTarget::Property { .. } => None,
        }
    }
}

/// The bound value arrived at a control (menus M1 contract 3.1): triggered
/// by [`sync_value_bindings`] when the binding is new, when the store's
/// `version` moved, or when the mirrored menu property changed. A control
/// observes this and copies the value into its state component; it never
/// reads the store itself.
#[derive(EntityEvent, Debug, Clone, PartialEq)]
pub struct BoundValue {
    /// The control.
    pub entity: Entity,
    /// The value. A menu property arrives as `Value::Int`.
    pub value: Value,
}

/// A control's write path (menus M1 contract 1.1): a `SetValue` for a store
/// binding, a `SetProperty` for a menu property.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ValueWriter<'w, 's> {
    set: MessageWriter<'w, SetValue>,
    commands: Commands<'w, 's>,
}

impl ValueWriter<'_, '_> {
    /// Asks the store (or the menu) to take `value` on behalf of `source`.
    pub fn write(&mut self, source: Entity, binding: &ValueBinding, value: Value) {
        match &binding.target {
            BindingTarget::Store(key) => {
                self.set.write(SetValue {
                    key: key.clone(),
                    value,
                    source: Some(source),
                });
            }
            BindingTarget::Property { menu, id } => {
                let Some(int) = property_int(&value) else {
                    tracing::warn!(?value, "a text value cannot mirror a menu property");
                    return;
                };
                self.commands.trigger(slotted_ecs::SetProperty {
                    entity: *menu,
                    id: *id,
                    value: int,
                });
            }
        }
    }
}

/// A `Value` as the `i32` a menu property holds.
fn property_int(value: &Value) -> Option<i32> {
    match value {
        Value::Bool(b) => Some(i32::from(*b)),
        Value::Int(i) => i32::try_from(*i).ok(),
        #[allow(clippy::cast_possible_truncation)]
        Value::Float(f) => Some(f.round() as i32),
        Value::Text(_) => None,
    }
}

/// `SlottedUiSet::Render`, before the controls' paint systems: delivers the
/// bound value to every control whose binding is new or whose store key may
/// have moved (the store's `version` changed). Menu properties are delivered
/// on `Added` here and by [`on_bound_property_changed`] afterwards.
pub fn sync_value_bindings(
    store: Res<ValueStore>,
    bindings: Query<(Entity, Ref<ValueBinding>)>,
    properties: Query<(&slotted_ecs::MenuProperty, &ChildOf)>,
    mut commands: Commands,
) {
    let store_moved = store.is_changed();
    for (entity, binding) in &bindings {
        let added = binding.is_added();
        match &binding.target {
            BindingTarget::Store(key) => {
                if !(added || store_moved) {
                    continue;
                }
                if let Some(value) = store.get(key) {
                    commands.trigger(BoundValue {
                        entity,
                        value: value.clone(),
                    });
                }
            }
            BindingTarget::Property { menu, id } => {
                if !added {
                    continue;
                }
                let value = properties
                    .iter()
                    .find(|(p, child_of)| child_of.parent() == *menu && p.id == *id)
                    .map(|(p, _)| p.value);
                if let Some(value) = value {
                    commands.trigger(BoundValue {
                        entity,
                        value: Value::Int(i64::from(value)),
                    });
                }
            }
        }
    }
}

/// Observer on `slotted_ecs::PropertyChanged`: the menu side of the mirror.
pub fn on_bound_property_changed(
    event: On<slotted_ecs::PropertyChanged>,
    bindings: Query<(Entity, &ValueBinding)>,
    mut commands: Commands,
) {
    let slotted_ecs::PropertyChanged {
        menu, id, value, ..
    } = *event;
    for (entity, binding) in &bindings {
        if binding.target == (BindingTarget::Property { menu, id }) {
            commands.trigger(BoundValue {
                entity,
                value: Value::Int(i64::from(value)),
            });
        }
    }
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
        .add_systems(Update, apply_set_values.in_set(ValueApply))
        .add_systems(Update, sync_value_bindings.in_set(ValueSync))
        .configure_sets(
            Update,
            ValueSync.in_set(crate::plugin::SlottedUiSet::Render),
        )
        .add_observer(on_bound_property_changed);
}

/// The system set [`sync_value_bindings`] runs in, inside `Render`; every
/// control's paint system runs after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueSync;
