//! Settings bundles: settings + keybindings + global card templates as one
//! JSON file (QUESTIONNAIRE M.87), strictly validated on import.
//!
//! Secrets never travel: they live in the OS keychain, and any settings key
//! that looks like a credential is dropped on export and import.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::templates::{CardTemplate, MAX_TEMPLATE_BYTES, MAX_TEMPLATES, read_dir_templates};
use crate::app::Core;
use crate::error::{Error, Result};
use crate::fsutil::{atomic_write, sanitize_name};

pub const FORMAT: &str = "luau-settings";
pub const VERSION: u32 = 1;
pub const MAX_FILE: u64 = 8 * 1024 * 1024;
const MAX_KEYS: usize = 5_000;
const MAX_BINDINGS: usize = 5_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBundle {
    pub format: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub settings: Map<String, Value>,
    #[serde(default)]
    pub keybindings: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explorer: Option<Value>,
    #[serde(default)]
    pub templates: Vec<CardTemplate>,
}

fn is_secret_key(k: &str) -> bool {
    let l = k.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "apikey",
        "api_key",
        "credential",
        "privatekey",
    ]
    .iter()
    .any(|s| l.contains(s))
}

fn valid_binding(v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    let s = |k: &str, max: usize| {
        o.get(k)
            .and_then(Value::as_str)
            .is_some_and(|x| !x.is_empty() && x.len() <= max)
    };
    let opt = |k: &str, max: usize| match o.get(k) {
        None | Some(Value::Null) => true,
        Some(Value::String(x)) => x.len() <= max,
        _ => false,
    };
    s("command", 200) && opt("key", 100) && opt("when", 1000) && o.len() <= 8
}

/// Validate and normalize a bundle (also accepts the legacy `{ "luau": 1, … }` shape).
pub fn validate(v: Value) -> Result<SettingsBundle> {
    let Value::Object(mut o) = v else {
        return Err(Error::invalid("settings file must be a JSON object"));
    };
    let legacy = o.get("luau").and_then(Value::as_u64) == Some(1);
    if !legacy {
        if o.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(Error::invalid("not a Luau settings file"));
        }
        if o.get("version").and_then(Value::as_u64).unwrap_or(0) as u32 > VERSION {
            return Err(Error::invalid("settings file is from a newer version"));
        }
    }
    let settings = match o.remove("settings") {
        Some(Value::Object(m)) => m,
        _ => return Err(Error::invalid("`settings` must be an object")),
    };
    if settings.len() > MAX_KEYS || settings.keys().any(|k| k.is_empty() || k.len() > 200) {
        return Err(Error::invalid("invalid settings keys"));
    }
    let settings: Map<String, Value> = settings
        .into_iter()
        .filter(|(k, _)| !is_secret_key(k))
        .collect();
    let keybindings = match o.remove("keybindings") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) if a.len() <= MAX_BINDINGS && a.iter().all(valid_binding) => a,
        _ => return Err(Error::invalid("invalid keybindings")),
    };
    let templates = match o.remove("templates") {
        None | Some(Value::Null) => vec![],
        Some(v) => {
            let t: Vec<CardTemplate> =
                serde_json::from_value(v).map_err(|e| Error::invalid(format!("templates: {e}")))?;
            if t.len() > MAX_TEMPLATES
                || t.iter().any(|x| {
                    sanitize_name(&x.name, 80).is_empty()
                        || x.content.len() as u64 > MAX_TEMPLATE_BYTES
                })
            {
                return Err(Error::invalid("invalid templates"));
            }
            t
        }
    };
    let explorer = o.remove("explorer").filter(|e| e.is_object());
    Ok(SettingsBundle {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: o
            .get("exportedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        settings,
        keybindings,
        explorer,
        templates,
    })
}

impl Core {
    /// Write a settings bundle built by the UI (+ global card templates).
    pub fn settings_export(&self, path: &Path, mut bundle: Value) -> Result<SettingsBundle> {
        if let Some(o) = bundle.as_object_mut() {
            o.insert("format".into(), FORMAT.into());
            o.insert("version".into(), VERSION.into());
            o.remove("luau");
        }
        let mut b = validate(bundle)?;
        b.exported_at = Some(chrono::Utc::now().to_rfc3339());
        b.templates = read_dir_templates(&self.paths.data.join("templates"), "global");
        let text = crate::json_fmt::to_string(&b).map_err(|e| Error::Other(e.to_string()))?;
        atomic_write(path, text.as_bytes())?;
        Ok(b)
    }

    /// Read and validate a bundle; installs its global card templates (new
    /// names only, existing files are kept). The UI applies settings and keys.
    pub fn settings_import(&self, path: &Path) -> Result<SettingsBundle> {
        let text = super::archive::read_capped(path, MAX_FILE)?;
        let v: Value =
            serde_json::from_str(&text).map_err(|e| Error::invalid(format!("JSON: {e}")))?;
        let b = validate(v)?;
        if !b.templates.is_empty() {
            let dir = self.paths.data.join("templates");
            std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
            for t in &b.templates {
                let p = dir.join(format!("{}.md", sanitize_name(&t.name, 80)));
                if !p.exists() {
                    atomic_write(&p, t.content.as_bytes())?;
                }
            }
        }
        Ok(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_bundles() {
        let b = validate(json!({
            "format": FORMAT, "version": 1,
            "settings": {"theme": "dark", "jira.apiToken": "x"},
            "keybindings": [{"key": "cmd+k", "command": "palette.commands"}],
        }))
        .unwrap();
        assert_eq!(b.settings.len(), 1, "secret-looking keys are dropped");
        assert_eq!(b.keybindings.len(), 1);
        // Legacy shape from earlier builds.
        assert!(validate(json!({"luau": 1, "settings": {}})).is_ok());
        assert!(validate(json!({"format": "other", "settings": {}})).is_err());
        assert!(validate(json!({"format": FORMAT, "version": 99, "settings": {}})).is_err());
        assert!(validate(json!({"format": FORMAT, "settings": []})).is_err());
        assert!(
            validate(json!({"format": FORMAT, "settings": {}, "keybindings": [{"key": 1}]}))
                .is_err()
        );
        assert!(validate(json!([1, 2])).is_err());
    }
}
