use super::widgets::{choices, field, heading, hint, row};
use super::{LightcraftApp, Tokens, json, register};

pub(super) fn external_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    discovery(app, ui, t);
    named_editors(app, ui, t);
    heading(ui, t, crate::i18n::tr("External Editing"));
    row(ui, t, crate::i18n::tr("PhotoCraft"), |ui| {
        field(
            ui,
            crate::text_field::TextField::singleline("field:photocraftPath", &mut app.ui.settings.photocraft_path)
                .hint(crate::i18n::tr("Found automatically"))
                .width(220.0),
        );
    });
    edit_file_rows(ui, t, "photocraftFile", &mut app.ui.settings.photocraft_file);
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Photo ▸ Edit In ▸ Edit in PhotoCraft (⇧⌘E, or right-click a photo) renders a copy with your edits, stacks it with the original and opens it in PhotoCraft. Save there and the copy updates here. As PSD, PhotoCraft saves straight back to it. Leave the location empty unless you use a portable copy.",
        ),
    );
    row(ui, t, crate::i18n::tr("Other editor"), |ui| {
        field(
            ui,
            crate::text_field::TextField::singleline("field:externalEditor", &mut app.ui.settings.external_editor)
                .hint(crate::i18n::tr("System default"))
                .width(220.0),
        );
    });
    edit_file_rows(ui, t, "externalFile", &mut app.ui.settings.external_file);
    hint(ui, t, crate::i18n::tr("Edit in Other Editor (⌥⌘E) opens the copy in this application (an app name on macOS, a program path elsewhere)."));
    hint(
        ui,
        t,
        crate::i18n::tr(
            "To hand this editor the original file itself instead (the raw, without a copy: DxO PhotoLab, for example), right-click a photo → Edit In → Open Original in Other Editor (RAW, no copy).",
        ),
    );
}

/// Each editor has its own copy settings; changing one never changes another editor or the
/// legacy Other Editor. Commands make the same configuration available to CLI/control clients.
fn named_editors(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, "Named external editors");
    hint(ui, t, "Add Affinity Photo and each Nik application separately. On macOS, enter its exact application name or full .app path.");
    hint(ui, t, "Save the edited TIFF to the same path, then return to LightCraft to refresh it. Save As to another path is not tracked.");
    row(ui, t, "", |ui| {
        let r = ui.button(crate::i18n::tr("Add editor"));
        register(ui, "button:addExternalEditor", r.rect);
        if r.clicked()
            && let Err(e) = app.run("app.externalEditors", json!({"action": "add"}))
        {
            app.toast(ui.ctx(), e);
        }
    });
    let editors = app.ui.settings.external_editors.clone();
    for original in editors {
        let mut editor = original.clone();
        let key = format!("externalEditor{}", editor.id);
        ui.separator();
        row(ui, t, "Editor name", |ui| {
            field(ui, crate::text_field::TextField::singleline(&format!("field:{key}Name"), &mut editor.name).width(220.0));
        });
        row(ui, t, "Application", |ui| {
            field(
                ui,
                crate::text_field::TextField::singleline(&format!("field:{key}Application"), &mut editor.application)
                    .hint("Affinity Photo 2 / Silver Efex / Color Efex")
                    .width(220.0),
            );
        });
        edit_file_rows(ui, t, &key, &mut editor.file);
        if editor != original {
            let p = json!({"action": "update", "id": editor.id, "name": editor.name, "application": editor.application, "file": editor.file});
            if let Err(e) = app.run("app.externalEditors", p) {
                app.toast(ui.ctx(), e);
            }
        }
        row(ui, t, "", |ui| {
            let r = ui.button(crate::i18n::tr("Remove editor"));
            register(ui, format!("button:{key}Remove"), r.rect);
            if r.clicked() {
                let _ = app.run("app.externalEditors", json!({"action": "remove", "id": editor.id}));
            }
        });
    }
}

/// The file Edit In hands an editor: format, colour space and bit depth (`button:{id}Format-…`).
fn edit_file_rows(ui: &mut egui::Ui, t: &Tokens, id: &str, file: &mut crate::state::EditFile) {
    use crate::state::{EditFormat, EditSpace};
    row(ui, t, "", |ui| {
        choices(ui, &format!("{id}Format"), &[(EditFormat::Psd, "PSD"), (EditFormat::Tiff, "TIFF")], &mut file.format);
        ui.separator();
        choices(ui, &format!("{id}Depth"), &[(16u8, "16-bit"), (8, "8-bit")], &mut file.bit_depth);
    });
    row(ui, t, "", |ui| {
        choices(
            ui,
            &format!("{id}Space"),
            &[(EditSpace::AdobeRgb, "Adobe RGB"), (EditSpace::ProPhoto, "ProPhoto"), (EditSpace::DisplayP3, "P3"), (EditSpace::Srgb, "sRGB")],
            &mut file.color_space,
        );
    });
}

type ScanResult = std::sync::Arc<std::sync::Mutex<Option<Result<crate::external_editors::discovery::Report, String>>>>;

#[derive(Clone, Default)]
struct ScanState {
    running: bool,
    result: ScanResult,
    status: String,
}

fn discovery(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let key = egui::Id::unique("external-editor-discovery");
    let mut state = ui.ctx().data_mut(|data| data.get_temp::<ScanState>(key).unwrap_or_default());
    let result = match state.result.lock() {
        Ok(mut result) => result.take(),
        Err(_) => Some(Err("Application detection failed. Please try again.".into())),
    };
    if let Some(result) = result {
        state.running = false;
        state.status = match result {
            Ok(report) => match crate::external_editors::discovery::merge(&mut app.ui.settings, &report.editors) {
                Ok(added) => format!(
                    "{} : {} · {} : {} · {} : {}{}",
                    crate::i18n::tr("Detected"),
                    report.editors.len(),
                    crate::i18n::tr("Added"),
                    added,
                    crate::i18n::tr("Unreadable locations"),
                    report.unreadable,
                    if report.limited { crate::i18n::tr(" (search limit reached)") } else { "" }
                ),
                Err(error) => error,
            },
            Err(error) => error,
        };
    }
    heading(ui, t, "External editors");
    hint(
        ui,
        t,
        "Detect photo applications in Applications and Application Support, including subfolders. Detection does not open applications or change existing editor settings.",
    );
    let r = ui.add_enabled(!state.running && cfg!(target_os = "macos"), egui::Button::new(crate::i18n::tr("Automatically detect installed editors")));
    register(ui, "button:detectExternalEditors", r.rect);
    if r.clicked() {
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let output = result.clone();
        let ctx = ui.ctx().clone();
        match std::thread::Builder::new().name("external-editor-discovery".into()).spawn(move || {
            let found = crate::external_editors::discovery::discover();
            if let Ok(mut result) = output.lock() {
                *result = Some(found);
            }
            ctx.request_repaint();
        }) {
            Ok(_) => {
                state.result = result;
                state.running = true;
                state.status.clear();
            }
            Err(error) => {
                state.status = error.to_string();
            }
        }
    }
    if state.running {
        ui.spinner();
        hint(ui, t, "Searching for photo applications…");
    }
    if !cfg!(target_os = "macos") {
        hint(ui, t, "Automatic detection is currently available on macOS only.");
    }
    if !state.status.is_empty() {
        ui.label(&state.status);
    }
    hint(
        ui,
        t,
        "Remove editor only removes the LightCraft entry. A later detection can add it again. Applications outside the searched folders can be added manually.",
    );
    ui.ctx().data_mut(|data| data.insert_temp(key, state));
    ui.separator();
}
