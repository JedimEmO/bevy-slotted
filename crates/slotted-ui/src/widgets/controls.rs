//! The `slotted:*` registry entries of every M1 control, so a `custom` node
//! or a mod's `register_widget` template can spawn them with the same
//! params as the typed variants.

use bevy::prelude::*;
use slotted_registry::Value;

use crate::def::UiNodeDef;
use crate::screen::{SpawnCtx, Widget, WidgetRegistry};
use crate::widgets::kinds;

/// Every M1 control through the registry: the params are the typed variant's
/// fields, re-read as that variant with `type` filled in.
#[derive(Debug, Clone, Copy)]
struct VariantWidget(&'static str);

impl Widget for VariantWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, children: &[UiNodeDef]) -> Entity {
        // Re-tag the params map as the typed variant and spawn that. The
        // children go through RON text, which every `UiNodeDef` round-trips.
        // A params payload that is not a map, or does not type, logs and
        // spawns an invisible panel, like every other malformed params.
        let mut map = match params {
            Value::Map(m) => m.clone(),
            _ => ron::Map::new(),
        };
        map.insert(
            Value::String("type".to_owned()),
            Value::String(self.0.to_owned()),
        );
        if !children.is_empty() {
            let kids: Vec<Value> = children
                .iter()
                .filter_map(|c| ron::to_string(c).ok())
                .filter_map(|text| ron::from_str::<Value>(&text).ok())
                .collect();
            map.insert(Value::String("children".to_owned()), Value::Seq(kids));
        }
        let typed = slotted_registry::to_model(&Value::Map(map))
            .map_err(|e| e.to_string())
            .and_then(|v| slotted_model::from_value::<UiNodeDef>(v).map_err(|e| e.to_string()));
        match typed {
            Ok(def) => ctx.spawn_child(&def),
            Err(e) => {
                tracing::warn!(kind = self.0, error = %e, "malformed control params");
                crate::widgets::spawn_panel(
                    ctx,
                    &slotted_theme::Role::new_static("invisible"),
                    &crate::def::Layout::default(),
                    &[],
                )
            }
        }
    }
}

/// Registers every M1 control kind.
pub fn register(registry: &mut WidgetRegistry) {
    registry.register(kinds::rich_text(), VariantWidget("rich_text"));
    registry.register(kinds::toggle(), VariantWidget("toggle"));
    registry.register(kinds::slider(), VariantWidget("slider"));
    registry.register(kinds::select(), VariantWidget("select"));
    registry.register(kinds::radio_group(), VariantWidget("radio_group"));
    registry.register(kinds::key_binding(), VariantWidget("key_binding"));
    registry.register(kinds::text_field(), VariantWidget("text_field"));
    registry.register(kinds::list(), VariantWidget("list"));
    registry.register(kinds::scroll(), VariantWidget("scroll"));
    registry.register(kinds::tabs(), VariantWidget("tabs"));
    registry.register(kinds::separator(), VariantWidget("separator"));
    registry.register(kinds::spacer(), VariantWidget("spacer"));
    registry.register(kinds::image(), VariantWidget("image"));
    registry.register(kinds::close(), CloseWidget);
}

/// `slotted:close`: a button whose `Activate` pops the screen stack (menus
/// M1 contract 3.2).
#[derive(Debug, Default, Clone, Copy)]
pub struct CloseWidget;

impl Widget for CloseWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        // M1-IMPL: B — the `Activate` observer that calls `pop_screen`.
        let opts: crate::def::ButtonOpts = slotted_registry::to_model(params)
            .ok()
            .and_then(|v| slotted_model::from_value(v).ok())
            .unwrap_or_default();
        crate::widgets::spawn_button(ctx, Some(&kinds::close()), &opts)
    }
}
