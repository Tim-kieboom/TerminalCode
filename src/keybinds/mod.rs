use std::{collections::HashMap, fs, path::Path};

mod action;
mod keybinding;

pub use action::{Action, PanelContext};
pub use keybinding::KeyBinding;

use anyhow::{Result, bail};
use crossterm::event::KeyEvent;

const KEYBIND_DEFAULTS: &str = include_str!("../../keybind_defaults.json");

pub struct KeyBindings {
    global: HashMap<Action, KeyBinding>,
    contexts: HashMap<PanelContext, HashMap<Action, KeyBinding>>,
}

type Json = serde_json::Value;
impl KeyBindings {
    fn parse_json(json: &str) -> Result<Self> {
        let mut global = HashMap::new();
        let mut contexts = HashMap::new();

        let Ok(Json::Object(obj)) = serde_json::from_str::<Json>(json) else {
            return Ok(Self { global, contexts });
        };

        for (key, value) in &obj {
            if let Some(context) = PanelContext::try_from_str(key) {
                let map = match value {
                    Json::Object(val) => val,
                    other => bail!("{other:?} is invalid for keybind json"),
                };

                let context_map = contexts.entry(context).or_default();

                for (key, value) in map {
                    if let Some((action, keybind)) = parse_keybind(key, value) {
                        context_map.insert(action, keybind);
                    }
                }
                continue;
            }

            if let Some((action, keybind)) = parse_keybind(key, value) {
                global.insert(action, keybind);
            }
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
            let key = action.description().to_string();
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
        if let Some(ctx_map) = self.contexts.get(&context) {
            for (action, binding) in ctx_map {
                if binding.matches(key) {
                    return Some(*action);
                }
            }
        }

        for (action, binding) in &self.global {
            if binding.matches(key) {
                return Some(*action);
            }
        }

        None
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
            return self.global.get(action)
        };
        
        context_map.get(action)
            .or(self.global.get(action))
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

fn parse_keybind(key: &str, value: &Json) -> Option<(Action, KeyBinding)> {
    let action = serde_json::from_str::<Action>(&format!("\"{key}\"")).ok()?;
    let value_str = value.as_str()?;
    KeyBinding::parse(value_str).map(|bind| (action, bind))
}
