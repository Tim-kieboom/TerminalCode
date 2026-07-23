use std::{collections::HashMap, fs, path::Path};

mod action;
mod keybinding;

pub use action::Action;
pub use keybinding::KeyBinding;

use anyhow::Result;
use crossterm::event::KeyEvent;

const KEYBIND_DEFAULTS: &str = include_str!("../../keybind_defaults.json");

pub struct KeyBindings {
    map: HashMap<Action, KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::parse_json(KEYBIND_DEFAULTS)
    }
}

impl KeyBindings {
    fn parse_json(json: &str) -> Self {
        let mut map = HashMap::new();

        let obj = match serde_json::from_str::<serde_json::Value>(json) {
            Ok(serde_json::Value::Object(obj)) => obj,
            _ => return Self { map },
        };

        for (key, value) in &obj {
            let action = match serde_json::from_str::<Action>(&format!("\"{key}\"")) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let s = match value.as_str() {
                Some(s) => s,
                None => continue,
            };
            if let Some(binding) = KeyBinding::parse(s) {
                map.insert(action, binding);
            }
        }

        Self { map }
    }

    pub fn load(config_dir: &Path) -> Self {
        let mut bindings = Self::default();

        let config_path = config_dir.join("keybindings.json");
        if let Ok(content) = fs::read_to_string(&config_path) {
            let loaded = Self::parse_json(&content);
            for (action, binding) in loaded.map {
                bindings.map.insert(action, binding);
            }
        }

        bindings
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let map: HashMap<String, String> = self
            .map
            .iter()
            .map(|(action, binding)| (format!("{action:?}"), binding.to_string()))
            .collect();

        let json = serde_json::to_string_pretty(&map)?;
        let config_path = config_dir.join("keybindings.json");
        fs::write(config_path, json)?;
        Ok(())
    }

    pub fn resolve(&self, key: &KeyEvent) -> Option<Action> {
        for (action, binding) in &self.map {
            if binding.matches(key) {
                return Some(*action);
            }
        }
        None
    }

    pub fn rebind(&mut self, action: Action, binding: KeyBinding) {
        self.map.retain(|_, b| *b != binding);
        self.map.insert(action, binding);
    }

    pub fn get(&self, action: &Action) -> Option<&KeyBinding> {
        self.map.get(action)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Action, &KeyBinding)> {
        self.map.iter().map(|(a, b)| (*a, b))
    }
}
