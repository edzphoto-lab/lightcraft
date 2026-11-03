//! Named external editors. The menu passes a stable id, not a mutable list index.
//! Preferences remain app-wide; rendering/stacking/reloading use the existing Edit In pipeline.

use serde_json::{Value, json};

use crate::state::{AppSettings, EditFile, ExternalEditor};

pub mod discovery;

const MAX_EDITORS: usize = 256;

pub fn find(settings: &AppSettings, id: u64) -> Result<&ExternalEditor, String> {
    let mut matches = settings.external_editors.iter().filter(|e| e.id == id);
    let editor = matches.next().ok_or("this external editor no longer exists; choose it again from Edit In")?;
    if matches.next().is_some() {
        return Err("duplicate external editor ids in settings".into());
    }
    Ok(editor)
}

/// `app.externalEditors`: list (default), add, update or remove. Validate before changing state.
pub fn command(settings: &mut AppSettings, p: &Value) -> Result<Value, String> {
    let action = p.get("action").and_then(Value::as_str).unwrap_or("list");
    if action == "list" {
        return Ok(json!({"editors": settings.external_editors}));
    }
    match action {
        "add" => {
            if settings.external_editors.len() >= MAX_EDITORS {
                return Err(format!("at most {MAX_EDITORS} external editors can be configured"));
            }
            let highest = settings.external_editors.iter().map(|e| e.id).max().unwrap_or(0);
            let id = highest.checked_add(1).ok_or("external editor ids exhausted")?.max(settings.next_external_editor_id).max(1);
            let next = id.checked_add(1).ok_or("external editor ids exhausted")?;
            let mut editor = ExternalEditor { id, name: "External editor".into(), application: String::new(), file: EditFile::default() };
            update(&mut editor, p)?;
            let result = json!(editor);
            settings.external_editors.push(editor);
            settings.next_external_editor_id = next;
            Ok(result)
        }
        "update" | "remove" => {
            let id = p.get("id").and_then(Value::as_u64).ok_or("an external editor id is required")?;
            let mut editor = find(settings, id)?.clone();
            if action == "remove" {
                settings.external_editors.retain(|e| e.id != id);
                return Ok(json!({"removed": id}));
            }
            update(&mut editor, p)?;
            let result = json!(editor);
            if let Some(target) = settings.external_editors.iter_mut().find(|e| e.id == id) {
                *target = editor;
            }
            Ok(result)
        }
        _ => Err("action must be list, add, update or remove".into()),
    }
}

fn update(editor: &mut ExternalEditor, p: &Value) -> Result<(), String> {
    for (key, target, limit) in [("name", &mut editor.name, 120), ("application", &mut editor.application, 4096)] {
        if let Some(v) = p.get(key) {
            let text = v.as_str().ok_or_else(|| format!("{key} must be text"))?;
            if text.len() > limit || text.chars().any(char::is_control) {
                return Err(format!("{key} is too long or contains control characters"));
            }
            *target = text.to_string();
        }
    }
    if let Some(file) = p.get("file") {
        let file: EditFile = serde_json::from_value(file.clone()).map_err(|e| format!("invalid editor file settings: {e}"))?;
        if !matches!(file.bit_depth, 8 | 16) {
            return Err("editor bit depth must be 8 or 16".into());
        }
        editor.file = file;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_survive_and_named_editors_persist() {
        let mut settings: AppSettings = serde_json::from_value(json!({"externalEditor": "Legacy", "gpu": false})).unwrap();
        assert!(settings.external_editors.is_empty());
        let editor = command(&mut settings, &json!({"action": "add", "name": "Affinity Photo 2", "application": "Affinity Photo 2"})).unwrap();
        assert_eq!(editor["file"]["format"], "tiff");
        assert_eq!(editor["file"]["bitDepth"], 16);
        let saved: AppSettings = serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        assert_eq!(settings, saved);
        assert_eq!(saved.external_editor, "Legacy");
        assert!(!saved.gpu);
    }

    #[test]
    fn deleting_an_editor_never_retargets_an_old_menu_item() {
        let mut settings = AppSettings::default();
        let first = command(&mut settings, &json!({"action": "add", "name": "Silver Efex"})).unwrap()["id"].as_u64().unwrap();
        command(&mut settings, &json!({"action": "remove", "id": first})).unwrap();
        let second = command(&mut settings, &json!({"action": "add", "name": "Color Efex"})).unwrap()["id"].as_u64().unwrap();
        assert_ne!(first, second);
        assert!(find(&settings, first).is_err());
    }

    #[test]
    fn invalid_updates_leave_preferences_untouched() {
        let mut settings = AppSettings::default();
        let id = command(&mut settings, &json!({"action": "add"})).unwrap()["id"].as_u64().unwrap();
        let before = settings.clone();
        for bad in [
            json!({"action": "update", "id": id, "name": "changed", "file": {"bitDepth": 12}}),
            json!({"action": "update", "id": id, "application": "bad\nname"}),
            json!({"action": "update", "id": id, "name": 4}),
            json!({"action": "remove", "id": 999}),
            json!({"action": "unknown"}),
        ] {
            assert!(command(&mut settings, &bad).is_err());
            assert_eq!(settings, before);
        }
        settings.next_external_editor_id = u64::MAX;
        assert!(command(&mut settings, &json!({"action": "add"})).is_err());
    }
}
