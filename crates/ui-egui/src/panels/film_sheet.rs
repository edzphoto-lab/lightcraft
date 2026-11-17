//! The EdZ 35 mm tool. Developing selected frames is done off the UI thread.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError, channel},
};

use egui::{ColorImage, Rect, TextureHandle, TextureOptions, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_engine::{LibrarySource, Session, contact_sheet::film};
use lightcraft_raster::Rgba8;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::i18n::tr;
use crate::pick::{PickRequest, Picked};
use crate::theme::Tokens;
use crate::widgets::text_field::TextField;

const EXPORT: &str = "filmSheet.exportJpeg";
const EXPORT_LABEL: &str = "Export as Contact Sheet";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    library: u64,
    source: LibrarySource,
    revision: u64,
    ids: Vec<PhotoId>,
}

impl Key {
    fn for_page(session: &Session, ids: &[PhotoId]) -> Self {
        Self { library: session.library_generation(), source: session.source, revision: session.catalog.revision, ids: ids.to_vec() }
    }

    fn token(&self) -> Value {
        json!([self.library, self.source, self.revision, self.ids])
    }
}

struct Pending {
    key: Key,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Rgba8, String>>,
}

#[derive(Default)]
pub(crate) struct State {
    album: Option<(u64, LibrarySource)>,
    fields: film::Fields,
    pub(crate) enlarged: bool,
    page: usize,
    key: Option<Key>,
    pending: Option<Pending>,
    pixels: Option<Rgba8>,
    blank: Option<Rgba8>,
    error: Option<String>,
    texture: Option<TextureHandle>,
    drawn_fields: Option<film::Fields>,
}

impl State {
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }

    fn refresh(&mut self, ctx: &egui::Context, session: &mut Session, ids: &[PhotoId]) {
        let key = Key::for_page(session, ids);
        if self.key.as_ref() != Some(&key) {
            self.pixels = None;
            self.error = None;
            self.drawn_fields = None;
            self.key = Some(key.clone());
            if let Some(pending) = &self.pending {
                pending.cancel.store(true, Ordering::Relaxed);
            }
        }
        if let Some(pending) = &self.pending {
            let result = match pending.result.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err(tr("Contact sheet preview ended unexpectedly").into())),
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                    None
                }
            };
            if let Some(result) = result {
                if pending.key == key && !pending.cancel.load(Ordering::Relaxed) {
                    match result {
                        Ok(image) => self.pixels = Some(image),
                        Err(error) => self.error = Some(error),
                    }
                    self.drawn_fields = None;
                }
                self.pending = None;
            }
        }
        if !ids.is_empty()
            && self.pending.is_none()
            && self.pixels.is_none()
            && self.error.is_none()
            && let Err(error) = self.start(ctx, session, ids, key)
        {
            self.error = Some(error);
        }
        if self.drawn_fields.as_ref() != Some(&self.fields) {
            let mut image = self.pixels.as_ref().unwrap_or_else(|| self.blank.get_or_insert_with(film::blank_preview)).clone();
            film::header(&mut image, &self.fields);
            let egui_image = ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.as_bytes());
            if let Some(texture) = &mut self.texture {
                texture.set(egui_image, TextureOptions::LINEAR);
            } else {
                self.texture = Some(ctx.load_texture("film-contact-sheet", egui_image, TextureOptions::LINEAR));
            }
            self.drawn_fields = Some(self.fields.clone());
        }
    }

    fn start(&mut self, ctx: &egui::Context, session: &mut Session, ids: &[PhotoId], key: Key) -> Result<(), String> {
        let prepared = film::prepare(session, ids, false)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (send, result) = channel();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("film-sheet-preview".into())
            .spawn(move || {
                let _ = send.send(prepared.render(&mut |_, _| !flag.load(Ordering::Relaxed)));
                ctx.request_repaint();
            })
            .map_err(|error| error.to_string())?;
        self.pending = Some(Pending { key, cancel, result });
        Ok(())
    }
}

fn selected(session: &mut Session) -> Vec<PhotoId> {
    let LibrarySource::Album(album) = session.source else { return Vec::new() };
    let selection = session.selected();
    let mut remaining: std::collections::HashSet<_> = selection.iter().copied().collect();
    let (_, visible) = session.visible_shared();
    let mut ids: Vec<_> = visible.iter().copied().filter(|id| remaining.remove(id)).collect();
    // A collapsed stack can hide a selected photo. Keep it if it belongs to this album.
    ids.extend(selection.into_iter().filter(|id| {
        remaining.remove(id) && session.catalog.photo(*id).is_some_and(|photo| photo.in_library() && session.catalog.album_contains(album, photo))
    }));
    ids
}

fn page_ids(ids: &[PhotoId], page: usize) -> &[PhotoId] {
    ids.chunks(film::CAPACITY).nth(page).unwrap_or_default()
}

pub(crate) fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let album = (app.session.library_generation(), app.session.source);
    if app.film_sheet.album != Some(album) {
        app.film_sheet.album = Some(album);
        app.film_sheet.fields = film::Fields::default();
        app.film_sheet.page = 0;
    }
    let ids = selected(&mut app.session);
    let pages = ids.len().div_ceil(film::CAPACITY).max(1);
    app.film_sheet.page = app.film_sheet.page.min(pages - 1);
    let style = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().id_salt("film-contact-sheet-tools").show(ui, |ui| {
        egui::Frame::NONE.inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.label(egui::RichText::new(tr("Film Contact Sheet")).font(style.semibold(15.0)));
            ui.add_space(8.0);
            app.film_sheet.refresh(ui.ctx(), &mut app.session, page_ids(&ids, app.film_sheet.page));
            let width = ui.available_width().max(1.0);
            let (sheet, _) = ui.allocate_exact_size(vec2(width, width / film::ASPECT), egui::Sense::hover());
            paint(&app.film_sheet, ui, sheet);
            if pages > 1 {
                ui.horizontal(|ui| {
                    let page = app.film_sheet.page;
                    if ui.add_enabled(page > 0, egui::Button::new("‹")).on_hover_text(tr("Previous sheet")).clicked() {
                        app.film_sheet.page -= 1;
                    }
                    ui.label(format!("{} / {pages}", page + 1));
                    if ui.add_enabled(page + 1 < pages, egui::Button::new("›")).on_hover_text(tr("Next sheet")).clicked() {
                        app.film_sheet.page += 1;
                    }
                    if page != app.film_sheet.page {
                        ui.ctx().request_repaint();
                    }
                });
            }
            ui.add_space(10.0);
            let active = app.film_sheet.enlarged;
            let preview = ui.add_sized(
                vec2(width, 28.0),
                egui::Button::new(egui::RichText::new(tr("Preview")).font(style.semibold(11.5)))
                    .selected(active)
                    .fill(if active { style.pressed } else { style.button })
                    .stroke(egui::Stroke::new(1.0, style.button_border)),
            );
            crate::widgets::register(ui, "button:filmSheetPreview", preview.rect);
            if preview.clicked() {
                let _ = app.run("filmSheet.preview", json!({}));
            }
            let response = ui
                .add_enabled_ui(!ids.is_empty() && app.export.is_none() && app.services.picker.is_some(), |ui| {
                    ui.add_sized(vec2(width, 32.0), egui::Button::new(egui::RichText::new(tr(EXPORT_LABEL)).font(style.semibold(11.5))))
                })
                .inner;
            crate::widgets::register(ui, "button:filmSheetExport", response.rect);
            if response.clicked()
                && let Err(error) = app.run(EXPORT, json!({}))
            {
                app.toast_error(ui.ctx(), error);
            }
            ui.label(egui::RichText::new("JPEG 4K").color(style.text_dim).small());
            if !matches!(album.1, LibrarySource::Album(_)) {
                ui.label(tr("Select photos in an album."));
            } else if let Some(error) = &app.film_sheet.error {
                ui.label(error);
            } else if app.film_sheet.pending.is_some() && !ids.is_empty() {
                ui.label(tr("Rendering contact sheet…"));
            }
            ui.add_space(10.0);
            let fields = &mut app.film_sheet.fields;
            for (id, label, text) in [
                ("filmSheetDate", "Date", &mut fields.date),
                ("filmSheetSubject", "Subject", &mut fields.subject),
                ("filmSheetTechnical", "Technical data", &mut fields.technical),
            ] {
                ui.label(label);
                let response = TextField::singleline(id, text).label(label).width(width).show(ui).response;
                if response.changed() {
                    ui.ctx().request_repaint();
                }
                ui.add_space(4.0);
            }
        });
    });
}

pub(crate) fn paint(state: &State, ui: &egui::Ui, area: Rect) {
    let Some(texture) = &state.texture else { return };
    let width = area.width().min(area.height() * film::ASPECT).max(0.0);
    let rect = Rect::from_center_size(area.center(), vec2(width, width / film::ASPECT));
    ui.painter().image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
}

pub(crate) fn export(app: &mut LightcraftApp, params: &Value) -> Result<Value, String> {
    let ids = selected(&mut app.session);
    let page = app.film_sheet.page;
    let frames = page_ids(&ids, page);
    if frames.is_empty() {
        return Err(tr("Select photos in an album.").into());
    }
    let current = Key::for_page(&app.session, frames).token();
    let path = match params.get("path").and_then(Value::as_str) {
        Some(path) if !path.trim().is_empty() => {
            if params.get("selection").is_some_and(|selection| *selection != current) {
                return Err(tr("The contact sheet changed. Export it again.").into());
            }
            let jpeg = std::path::Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ["jpg", "jpeg"].iter().any(|jpeg| ext.eq_ignore_ascii_case(jpeg)));
            if jpeg {
                path.to_owned()
            } else {
                let candidate = format!("{path}.jpg");
                if std::path::Path::new(&candidate).exists() {
                    return Err(tr("Choose a JPEG output path that does not already exist.").into());
                }
                candidate
            }
        }
        _ => {
            let album = match app.session.source {
                LibrarySource::Album(id) => app.session.catalog.album(id).map(|a| a.name.as_str()).unwrap_or(""),
                _ => "",
            };
            let name: String = album.chars().filter(|c| !c.is_control() && !"/\\:".contains(*c)).take(80).collect();
            let name = if ids.len() > film::CAPACITY {
                format!("Planche-contact-{name}-{:02}.jpg", page + 1)
            } else if name.is_empty() {
                "Planche-contact.jpg".into()
            } else {
                format!("Planche-contact-{name}.jpg")
            };
            let request = PickRequest::save(tr(EXPORT_LABEL), "JPEG", &["jpg", "jpeg"], name);
            return match crate::pick::ask(app, EXPORT, &json!({"selection": current}), "path", request, |_| None) {
                Picked::Later | Picked::Now(_) => Ok(Value::Null),
                Picked::Unavailable => Err(tr("Choose a JPEG output path.").into()),
            };
        }
    };
    let prepared = film::prepare(&mut app.session, frames, true)?;
    let fields = app.film_sheet.fields.clone();
    crate::export_task::start_sheet(app, path, frames.len(), move |progress| prepared.jpeg(&fields, progress))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_tools_and_toggles_between_the_sheet_and_active_photo() {
        let mut session = Session::with_demo();
        let album = session.catalog.albums().find(|album| !album.folder && !album.photos.is_empty()).unwrap().id;
        session.source = LibrarySource::Album(album);
        session.keep_active_in_view();
        let active = session.active();
        let app = crate::LightcraftApp::new(session, crate::Services::default());
        let mut app = crate::headless::Headless::new(app, [1360.0, 850.0], 1.0);
        let timeout = std::time::Duration::from_secs(40);
        assert!(app.settle(timeout));

        let opened = app.request("ui.clickWidget", json!({"id": "icon:filmSheet"}), timeout);
        assert_eq!(opened["ok"], true, "{opened}");
        assert!(app.settle(timeout));
        assert_eq!(app.app.ui.right, crate::state::RightPanel::FilmSheet);
        assert!(app.app.film_sheet.pixels.is_some(), "{:?}", app.app.film_sheet.error);
        let button = json!({"id": "button:filmSheetPreview"});
        assert_eq!(app.request("ui.clickWidget", button.clone(), timeout)["ok"], true);
        assert!(app.settle(timeout));
        assert!(app.app.film_sheet.enlarged);
        assert!(app.app.image_rect.is_none());
        assert_eq!(app.app.session.active(), active);
        assert_eq!(app.request("ui.clickWidget", button, timeout)["ok"], true);
        assert!(app.settle(timeout));
        assert!(!app.app.film_sheet.enlarged);
        assert_eq!(app.app.session.active(), active);
    }

    #[test]
    fn extra_selected_frames_start_another_sheet() {
        let photos: Vec<_> = (1..=39).map(PhotoId).collect();
        assert_eq!(page_ids(&photos, 0).len(), 36);
        assert_eq!(page_ids(&photos, 1), &[PhotoId(37), PhotoId(38), PhotoId(39)]);
        assert!(page_ids(&photos, 2).is_empty());
    }
}
