use std::{collections::HashMap, fs, path::Path};

mod action;
mod keybinding;

pub use action::{Action, PanelContext};
pub use keybinding::KeyBinding;

use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const KEYBIND_DEFAULTS: &str = include_str!("../../keybind_defaults.json");

type KeyBindMap = HashMap<Action, KeyBinding>;

#[cfg(test)]
#[path = "tests/keybinds_tests.rs"]
mod tests;

#[derive(Debug)]
pub struct KeyBindings {
    global: KeyBindMap,
    contexts: HashMap<PanelContext, KeyBindMap>,
}

type Json = serde_json::Value;
impl KeyBindings {
    fn parse_json(json: &str) -> Result<Self> {
        let mut global: KeyBindMap = HashMap::new();
        let mut contexts: HashMap<PanelContext, KeyBindMap> = HashMap::new();

        let parsed: Json = serde_json::from_str(json)
            .map_err(|err| anyhow::anyhow!("at {}; {err}", err.line()))?;

        let Json::Object(obj) = parsed else {
            bail!("at 1; expected a top-level JSON object");
        };

        for (key, value) in &obj {
            if let Some(context) = PanelContext::from_description(key) {
                let map = match value {
                    Json::Object(val) => val,
                    other => {
                        return Err(span_err(
                            json,
                            key,
                            format_args!("{other:?} is invalid for keybind json"),
                        ));
                    }
                };

                let context_map = contexts.entry(context).or_default();

                for (inner_key, value) in map {
                    let (action, keybind) = parse_keybind(json, inner_key, value)?;
                    context_map.insert(action, keybind);
                }
                continue;
            }

            let (action, keybind) = parse_keybind(json, key, value)?;
            global.insert(action, keybind);
        }

        Ok(Self { global, contexts })
    }

    pub fn load(config_dir: &Path) -> Result<Self> {
        let mut bindings = Self {
            global: HashMap::new(),
            contexts: HashMap::new(),
        };

        let config_path = config_dir.join("keybindings.json");
        if let Ok(content) = fs::read_to_string(&config_path) {
            let loaded = Self::parse_json(&content)?;
            for (action, binding) in loaded.global {
                bindings.global.insert(action, binding);
            }
            for (ctx, map) in loaded.contexts {
                let ctx_map = bindings.contexts.entry(ctx).or_default();
                for (action, binding) in map {
                    ctx_map.insert(action, binding);
                }
            }
        } else {
            bindings = Self::parse_json(KEYBIND_DEFAULTS)?;
        }

        Ok(bindings)
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let mut root: serde_json::Map<String, Json> = serde_json::Map::new();

        for (action, binding) in &self.global {
            let key = format!("{action:?}");
            let value = Json::String(binding.to_string());
            root.insert(key, value);
        }

        for (ctx, map) in &self.contexts {
            let ctx_obj: serde_json::Map<String, Json> = map
                .iter()
                .map(|(a, b)| (format!("{a:?}"), Json::String(b.to_string())))
                .collect();
            root.insert(ctx.description().to_string(), Json::Object(ctx_obj));
        }

        let json = serde_json::to_string_pretty(&Json::Object(root))?;
        let config_path = config_dir.join("keybindings.json");
        fs::write(config_path, json)?;
        Ok(())
    }

    pub fn resolve(&self, key: &KeyEvent, context: PanelContext) -> Option<Action> {
        if matches!(key.code, KeyCode::Char(_))
            && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
        {
            return Some(Action::InsertChar);
        }

        if let Some(context_map) = self.contexts.get(&context)
            && let Some(action) = find_entry(context_map, key)
        {
            return Some(action);
        }

        find_entry(&self.global, key)
    }

    pub fn resolve_global(&self, key: &KeyEvent) -> Option<Action> {
        find_entry(&self.global, key)
    }

    pub fn resolve_context(&self, key: &KeyEvent, context: PanelContext) -> Option<Action> {
        self.contexts
            .get(&context)
            .and_then(|map| find_entry(map, key))
    }

    pub fn rebind(&mut self, action: Action, binding: KeyBinding, context: Option<PanelContext>) {
        match context {
            Some(ctx) => {
                let ctx_map = self.contexts.entry(ctx).or_default();
                ctx_map.retain(|_, b| *b != binding);
                ctx_map.insert(action, binding);
            }
            None => {
                self.global.retain(|_, b| *b != binding);
                self.global.insert(action, binding);
            }
        }
    }

    pub fn get(&self, action: &Action, context: PanelContext) -> Option<&KeyBinding> {
        let Some(context_map) = self.contexts.get(&context) else {
            return self.global.get(action);
        };

        context_map.get(action).or(self.global.get(action))
    }

    pub fn get_context_map(&self, context: PanelContext) -> Option<&HashMap<Action, KeyBinding>> {
        self.contexts.get(&context)
    }

    pub fn iter_global(&self) -> impl Iterator<Item = (Action, &KeyBinding)> {
        self.global.iter().map(|(a, b)| (*a, b))
    }

    pub fn iter_contexts(
        &self,
    ) -> impl Iterator<Item = (&PanelContext, &HashMap<Action, KeyBinding>)> {
        self.contexts.iter()
    }
}

fn find_entry(map: &KeyBindMap, key: &KeyEvent) -> Option<Action> {
    map.iter()
        .find(|(_, binding)| binding.matches(key))
        .map(|(action, _)| *action)
}

fn parse_keybind(json: &str, key: &str, value: &Json) -> Result<(Action, KeyBinding)> {
    let action = Action::from_name(key)
        .ok_or_else(|| span_err(json, key, format_args!("Action {key:?} is invalid")))?;

    let value_str = value.as_str().ok_or_else(|| {
        span_err(
            json,
            key,
            format_args!("Value of Action {key:?} is not of type String"),
        )
    })?;

    KeyBinding::parse(value_str)
        .map(|bind| (action, bind))
        .map_err(|err| span_err(json, key, err))
}

fn span_err(json: &str, key: &str, msg: impl std::fmt::Display) -> anyhow::Error {
    match span_of_key(json, key) {
        Some(line) => anyhow::anyhow!("at {line}; {msg}"),
        None => anyhow::anyhow!("at ?; {msg}"),
    }
}

fn span_of_key(json: &str, key: &str) -> Option<usize> {
    let offset = json.find(&format!("\"{key}\""))?;
    Some(line_column_at(json, offset))
}

fn line_column_at(json: &str, offset: usize) -> usize {
    let before = &json[..offset];
    before.bytes().filter(|&b| b == b'\n').count() + 1
}
