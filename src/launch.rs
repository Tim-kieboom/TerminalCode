use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};

#[cfg(test)]
#[path = "tests/launch_tests.rs"]
mod tests;

type Json = serde_json::Value;

#[derive(Debug)]
pub struct LaunchConfig {
    configurations: Vec<LaunchConfiguration>,
}
impl LaunchConfig {
    pub fn load(project_path: &Path) -> Result<Option<Self>> {
        let config_path = project_path.join(".terminalcode/launch.json");
        let content = match fs::read_to_string(&config_path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };

        let parsed: Json = serde_json::from_str(&content)
            .map_err(|err| anyhow::anyhow!("at {}; {err}", err.line()))?;

        let workspace_folder = project_path.display().to_string();
        Self::parse(&content, &parsed, &workspace_folder).map(Some)
    }

    pub fn configurations(&self) -> &[LaunchConfiguration] {
        &self.configurations
    }

    fn parse(json: &str, value: &Json, workspace_folder: &str) -> Result<Self> {
        let Json::Object(obj) = value else {
            bail!("at 1; expected a top-level JSON object");
        };

        let mut configurations = Vec::new();
        if let Some(value) = obj.get("configurations") {
            let Json::Array(array) = value else {
                return Err(span_err(
                    json,
                    "configurations",
                    format_args!("must be an array"),
                ));
            };

            for (i, config) in array.iter().enumerate() {
                configurations.push(parse_configuration(json, config, i, workspace_folder)?);
            }
        }

        Ok(Self { configurations })
    }
}

#[derive(Debug)]
pub struct LaunchConfiguration {
    name: String,
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: HashMap<String, String>,
}
impl LaunchConfiguration {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }
}

fn parse_configuration(
    json: &str,
    value: &Json,
    index: usize,
    workspace_folder: &str,
) -> Result<LaunchConfiguration> {
    let Json::Object(obj) = value else {
        return Err(span_err(
            json,
            "configurations",
            format_args!("entry {index} must be an object"),
        ));
    };

    let name = match obj.get("name") {
        Some(Json::String(s)) if !s.is_empty() => s.clone(),
        _ => {
            return Err(span_err(
                json,
                "configurations",
                format_args!("entry {index} requires a non-empty \"name\""),
            ));
        }
    };

    let program = match obj.get("program") {
        Some(Json::String(s)) if !s.is_empty() => s.clone(),
        _ => {
            return Err(span_err(
                json,
                "configurations",
                format_args!("entry {index} requires a non-empty \"program\""),
            ));
        }
    };

    let args = match obj.get("args") {
        None => Vec::new(),
        Some(Json::Array(items)) => {
            let mut args = Vec::with_capacity(items.len());
            for item in items {
                let Json::String(s) = item else {
                    return Err(span_err(
                        json,
                        "args",
                        format_args!("entry {index} args must be strings"),
                    ));
                };
                args.push(substitute_variables(s, workspace_folder));
            }
            args
        }
        Some(_) => {
            return Err(span_err(
                json,
                "args",
                format_args!("entry {index} args must be an array"),
            ));
        }
    };

    let cwd = match obj.get("cwd") {
        None => None,
        Some(Json::String(s)) if !s.is_empty() => {
            Some(PathBuf::from(substitute_variables(s, workspace_folder)))
        }
        Some(_) => {
            return Err(span_err(
                json,
                "cwd",
                format_args!("entry {index} cwd must be a non-empty string"),
            ));
        }
    };

    let env = match obj.get("env") {
        None => HashMap::new(),
        Some(Json::Object(map)) => {
            let mut env = HashMap::with_capacity(map.len());
            for (key, value) in map {
                let Json::String(s) = value else {
                    return Err(span_err(
                        json,
                        "env",
                        format_args!("entry {index} env values must be strings"),
                    ));
                };
                env.insert(key.clone(), substitute_variables(s, workspace_folder));
            }
            env
        }
        Some(_) => {
            return Err(span_err(
                json,
                "env",
                format_args!("entry {index} env must be an object"),
            ));
        }
    };

    Ok(LaunchConfiguration {
        name,
        program: substitute_variables(&program, workspace_folder),
        args,
        cwd,
        env,
    })
}

fn substitute_variables(text: &str, workspace_folder: &str) -> String {
    text.replace("${workspaceFolder}", workspace_folder)
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
