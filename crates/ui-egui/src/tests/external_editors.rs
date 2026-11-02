//! Exercise the real copy/export/stack path with a simulated external application.
//! Affinity and Nik themselves still need an integration check on the user's Mac.

use serde_json::json;
use std::sync::{Arc, Mutex};

use crate::{LightcraftApp, Services};

type Opened = Arc<Mutex<Vec<(String, String)>>>;

fn app() -> (LightcraftApp, Opened) {
    let opened: Opened = Default::default();
    let calls = opened.clone();
    let services = Services {
        png: None,
        open_with: Some(Box::new(move |path, application| {
            calls.lock().unwrap().push((path.to_string(), application.to_string()));
            Ok(())
        })),
        ..Default::default()
    };
    (LightcraftApp::new(lightcraft_engine::Session::with_demo().with_fs(), services), opened)
}

#[test]
fn named_editors_get_their_own_file_settings_and_stack_copies() {
    let (mut app, opened) = app();
    let dir = std::env::temp_dir().join(format!("lc-named-editors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let original = app.session.active().unwrap();
    let affinity = app
        .run("app.externalEditors", json!({"action": "add", "name": "Affinity Photo 2", "application": "/Applications/Affinity Photo 2.app"}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let nik = app.run("app.externalEditors", json!({"action": "add", "name": "Silver Efex", "application": "/Applications/Nik Collection/Silver Efex.app", "file": {"colorSpace": "proPhoto"}})).unwrap()["id"].as_u64().unwrap();
    app.run("develop.set", json!({"control": "light.exposure", "value": 0.75})).unwrap();
    let mut edited = Vec::new();
    let mut profiles = Vec::new();
    for editor in [affinity, nik] {
        let r = app.run("photo.editInExternal", json!({"editorId": editor, "ids": [original.0], "dir": dir.to_string_lossy()})).unwrap();
        assert!(r["failed"].is_null(), "{r}");
        let id = lightcraft_catalog::PhotoId(r["id"].as_u64().unwrap());
        let path = r["path"].as_str().unwrap();
        let image = lightcraft_codecs::decode(&std::fs::read(path).unwrap(), Default::default()).unwrap();
        assert_eq!(image.bit_depth, 16);
        profiles.push(image.icc.expect("the TIFF must carry its colour profile"));
        assert!(path.ends_with(".tif"));
        assert!(app.session.catalog.stack_of(id).unwrap().photos.contains(&original));
        assert!(app.ui.external_edits.contains(&id.0), "the existing focus-return reload tracks each editor");
        edited.push((id, path.to_string()));
    }
    let calls = opened.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].1, "/Applications/Affinity Photo 2.app");
    assert_eq!(calls[1].1, "/Applications/Nik Collection/Silver Efex.app");
    assert_ne!(calls[0].0, calls[1].0, "unique copies, never overwrite another editor's file");
    assert_ne!(profiles[0], profiles[1], "each editor uses its own configured colour space");
    // Simulate the editor saving: replacing its TIFF with a different valid TIFF must be read
    // back into the same catalog photo, without adding another photo.
    let count = app.session.catalog.len();
    std::fs::copy(&edited[1].1, &edited[0].1).unwrap();
    let reloaded = app.run("photo.reload", json!({"ids": [edited[0].0.0], "wait": true})).unwrap();
    assert!(reloaded["reloaded"].as_array().unwrap().contains(&json!(edited[0].0.0)), "{reloaded}");
    assert_eq!(app.session.catalog.len(), count);
    assert!(app.session.catalog.photo(original).is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn missing_removed_and_unconfigured_editors_do_not_create_copies() {
    let (mut app, opened) = app();
    let count = app.session.catalog.len();
    let editor = app.run("app.externalEditors", json!({"action": "add", "name": "Silver Efex"})).unwrap()["id"].as_u64().unwrap();
    assert!(app.run("photo.editInExternal", json!({"editorId": editor})).is_err());
    app.run("app.externalEditors", json!({"action": "remove", "id": editor})).unwrap();
    assert!(app.run("photo.editInExternal", json!({"editorId": editor})).is_err());
    assert!(app.run("photo.editInExternal", json!({"editorId": "bad"})).is_err());
    assert_eq!(app.session.catalog.len(), count);
    assert!(opened.lock().unwrap().is_empty());
}

#[test]
fn named_menu_entries_use_stable_ids_and_disable_unconfigured_apps() {
    let (mut app, _) = app();
    let id = app.run("app.externalEditors", json!({"action": "add", "name": "Silver Efex", "application": "Silver Efex"})).unwrap()["id"]
        .as_u64()
        .unwrap();
    let blank = app.run("app.externalEditors", json!({"action": "add", "name": "Color Efex"})).unwrap()["id"].as_u64().unwrap();
    let bar = crate::menubar::menu_bar(&app);
    let photo = &bar.iter().find(|(title, _)| title == "Photo").unwrap().1;
    let edit_in = photo
        .iter()
        .find_map(|n| match n {
            crate::menubar::MenuNode::Submenu { label, children } if label == "Edit In" => Some(children),
            _ => None,
        })
        .unwrap();
    for (wanted, name, available) in [(id, "Silver Efex", true), (blank, "Color Efex", false)] {
        assert!(edit_in.iter().any(|n| matches!(n, crate::menubar::MenuNode::Item { id, params, label, enabled, .. }
            if id == "photo.editInExternal" && params["editorId"] == wanted && label == name && *enabled == available)));
    }
    assert_eq!(crate::menubar::display_item_label("photo.editInExternal", &json!({"editorId": id}), "Silver Efex"), "Silver Efex");
}

#[test]
fn external_editor_settings_can_add_an_editor_and_fit_the_window() {
    use crate::headless::Headless;
    use std::time::Duration;
    let timeout = Duration::from_secs(30);
    let (mut app, _) = app();
    app.run("app.externalEditors", json!({"action": "add", "name": "Affinity Photo 2", "application": "Affinity Photo 2"})).unwrap();
    let mut h = Headless::new(app, [1200.0, 900.0], 1.0);
    h.app.run("app.settings", json!({"tab": "externalEditors"})).unwrap();
    h.settle(timeout);
    let r = h.request("ui.clickWidget", json!({"id": "button:addExternalEditor"}), timeout);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(timeout);
    assert_eq!(h.app.ui.settings.external_editors.len(), 2);
    for (_, rect) in h.app.widgets.iter().filter(|(id, _)| id.contains("externalEditor") || id == "button:addExternalEditor") {
        assert!(rect.left() >= 0.0 && rect.right() <= h.size.x, "{rect:?}");
    }
    if let Some(path) = std::env::var_os("LIGHTCRAFT_EXTERNAL_EDITORS_SNAPSHOT") {
        h.app.ui.language = crate::i18n::Locale::Fr;
        h.request("ui.scroll", json!({"dy": 2000}), timeout);
        h.settle(timeout);
        let image = h.snapshot(timeout);
        let pixels = lightcraft_raster::Rgba8 {
            width: image.size[0],
            height: image.size[1],
            data: image.pixels.iter().map(egui::Color32::to_array).collect(),
        };
        let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&pixels), &Default::default()).unwrap();
        std::fs::write(path, png).unwrap();
    }
    h.settle(timeout);
}
