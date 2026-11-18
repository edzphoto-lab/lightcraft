//! The EdZ 35 mm tool. Developing selected frames is done off the UI thread.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError, channel},
};

use egui::{Align2, Color32, ColorImage, Rect, Stroke, StrokeKind, TextureHandle, TextureOptions, pos2, vec2};
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
    page: Option<film::Page>,
}

impl Key {
    fn for_page(session: &Session, ids: &[PhotoId], page: Option<film::Page>) -> Self {
        Self { library: session.library_generation(), source: session.source, revision: session.catalog.revision, ids: ids.to_vec(), page }
    }

    fn token(&self) -> Value {
        let page = self.page.map(|page| (page.number, page.total));
        json!([self.library, self.source, self.revision, self.ids, page])
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

    fn refresh(&mut self, ctx: &egui::Context, session: &mut Session, ids: &[PhotoId], page: Option<film::Page>) {
        let key = Key::for_page(session, ids, page);
        if self.key.as_ref() != Some(&key) {
            self.pixels = None;
            self.blank = None;
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
            let mut image = self.pixels.as_ref().unwrap_or_else(|| self.blank.get_or_insert_with(|| film::blank_preview(ids.len(), page))).clone();
            film::header(&mut image, &self.fields, page);
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
        let prepared = film::prepare(session, ids, false, key.page)?;
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

fn page_number(page: usize, pages: usize) -> Option<film::Page> {
    (pages > 1).then_some(film::Page { number: page + 1, total: pages })
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
            let page = app.film_sheet.page;
            app.film_sheet.refresh(ui.ctx(), &mut app.session, page_ids(&ids, page), page_number(page, pages));
            let width = ui.available_width().max(1.0);
            let (sheet, _) = ui.allocate_exact_size(vec2(width, width / film::ASPECT), egui::Sense::hover());
            paint(&app.film_sheet, ui, sheet);
            if pages > 1 {
                navigation(app, ui, &ids, page, pages, width);
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

fn navigation(app: &mut LightcraftApp, ui: &mut egui::Ui, ids: &[PhotoId], page: usize, pages: usize, width: f32) {
    let mut next_page = page;
    let style = Tokens::get(ui.ctx());
    let gap = ui.spacing().item_spacing.x;
    let button_width = 30.0;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let arrow = |ui: &mut egui::Ui, enabled, symbol, help, id| {
            let response = ui.add_enabled(enabled, egui::Button::new(symbol).min_size(vec2(button_width, 24.0))).on_hover_text(tr(help));
            crate::widgets::register(ui, id, response.rect);
            crate::widgets::announce(&response, tr(help));
            response.clicked()
        };
        if arrow(ui, page > 0, "‹", "Previous sheet", "button:filmSheetPrevious") {
            next_page = page.saturating_sub(1);
        }
        let size = vec2((width - 2.0 * (button_width + gap)).max(1.0), 24.0);
        ui.allocate_ui_with_layout(size, egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
            ui.label(crate::i18n::tr_format!("Sheet {number} of {total}", number = page + 1, total = pages));
        });
        if arrow(ui, page + 1 < pages, "›", "Next sheet", "button:filmSheetNext") {
            next_page = page + 1;
        }
    });

    // Show three nearby pages so albums of any length retain the same layout.
    let first = page.saturating_sub(1).min(pages.saturating_sub(3));
    let thumb_width = ((width - 2.0 * gap) / 3.0).max(1.0);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for index in first..(first + 3).min(pages) {
            let number = film::Page { number: index + 1, total: pages };
            let height = thumb_width / film::ASPECT;
            let (slot, response) = ui.allocate_exact_size(vec2(thumb_width, height + 19.0), egui::Sense::click());
            let sheet = Rect::from_min_size(slot.min, vec2(thumb_width, height)).shrink(3.0);
            crate::widgets::register(ui, format!("button:filmSheetPage{}", number.number), slot);
            let label = crate::i18n::tr_format!("Sheet {number} of {total}", number = number.number, total = number.total);
            crate::widgets::announce_selected(&response, label, index == page);
            if ui.is_rect_visible(slot) {
                paint_thumbnail(app, ui, sheet, page_ids(ids, index));
                if index == page || response.hovered() || response.has_focus() {
                    let color = if index == page { style.text } else { style.text_dim };
                    ui.painter().rect_stroke(sheet.expand(2.0), 3.0, Stroke::new(1.0, color), StrokeKind::Outside);
                }
                ui.painter().text(pos2(slot.center().x, slot.bottom() - 8.0), Align2::CENTER_CENTER, number.label(), style.font(11.0), style.text);
            }
            if response.clicked() {
                next_page = index;
            }
        }
    });
    if page != next_page {
        app.film_sheet.page = next_page;
        ui.ctx().request_repaint();
    }
}

fn paint_thumbnail(app: &mut LightcraftApp, ui: &egui::Ui, sheet: Rect, ids: &[PhotoId]) {
    let painter = ui.painter();
    painter.rect_filled(sheet, 2.0, Color32::from_rgb(239, 235, 223));
    let scale = sheet.width() / 800.0;
    for (row, strip) in ids.chunks(film::COLUMNS).enumerate() {
        let y = sheet.top() + (60 + row * 116) as f32 * scale;
        let band = Rect::from_min_size(pos2(sheet.left() + 27.0 * scale, y), vec2(746.0 * scale, 111.0 * scale));
        painter.rect_filled(band, 0.5, Color32::from_rgb(57, 47, 39));
        for (col, &id) in strip.iter().enumerate() {
            super::grid::request_thumb(app, id, 64, super::grid::BACKGROUND_THUMB_PRIORITY);
            let frame =
                Rect::from_min_size(pos2(sheet.left() + (33 + col * 124) as f32 * scale, y + 14.0 * scale), vec2(116.0 * scale, 83.0 * scale));
            painter.rect_filled(frame, 0.0, Color32::from_rgb(16, 14, 13));
            let Some(texture) = app.renderer.thumb(id) else { continue };
            let [w, h] = texture.size.map(|edge| edge as f32);
            let turned = h > w;
            let size = if turned { vec2(h, w) } else { vec2(w, h) };
            let fit = (frame.width() / size.x.max(1.0)).min(frame.height() / size.y.max(1.0));
            let rect = Rect::from_center_size(frame.center(), vec2(w, h) * fit);
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            let mut mesh = egui::Mesh::with_texture(texture.tex.id());
            mesh.add_rect_with_uv(rect, uv, Color32::WHITE);
            if turned {
                mesh.rotate(egui::emath::Rot2::from_angle(-std::f32::consts::FRAC_PI_2), frame.center());
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

pub(crate) fn paint(state: &State, ui: &egui::Ui, area: Rect) {
    let Some(texture) = &state.texture else { return };
    let width = area.width().min(area.height() * film::ASPECT).max(0.0);
    let rect = Rect::from_center_size(area.center(), vec2(width, width / film::ASPECT));
    ui.painter().image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
}

pub(crate) fn export(app: &mut LightcraftApp, params: &Value) -> Result<Value, String> {
    let ids = selected(&mut app.session);
    let pages = ids.len().div_ceil(film::CAPACITY).max(1);
    let album = (app.session.library_generation(), app.session.source);
    let page = if app.film_sheet.album == Some(album) { app.film_sheet.page.min(pages - 1) } else { 0 };
    let number = page_number(page, pages);
    let frames = page_ids(&ids, page);
    if frames.is_empty() {
        return Err(tr("Select photos in an album.").into());
    }
    let current = Key::for_page(&app.session, frames, number).token();
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
            let name = if pages > 1 {
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
    let prepared = film::prepare(&mut app.session, frames, true, number)?;
    let fields = if app.film_sheet.album == Some(album) { app.film_sheet.fields.clone() } else { film::Fields::default() };
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
    fn navigates_three_sheets_and_resets_after_the_selection_shrinks() {
        let library = crate::tests::fixtures::SyntheticLibrary::with_photos("film-sheet-pages", 75);
        let mut app = library.app();
        let ids: Vec<_> = app.session.catalog.photos().map(|photo| photo.id.0).collect();
        app.run("library.select", json!({"ids": &ids})).unwrap();
        let album = app.run("album.create", json!({"name": "Film", "addSelected": true})).unwrap()["id"].as_u64().unwrap();
        app.run("library.source", json!({"kind": "album", "id": album})).unwrap();
        app.run("library.select", json!({"ids": &ids})).unwrap();
        app.run("panel.filmSheet", json!({})).unwrap();
        let mut app = crate::headless::Headless::new(app, [1360.0, 900.0], 1.0);
        let timeout = std::time::Duration::from_secs(40);
        assert!(app.settle(timeout));
        let all = selected(&mut app.app.session);
        let check_page = |app: &crate::headless::Headless, page, len| {
            let key = app.app.film_sheet.key.as_ref().unwrap();
            assert_eq!(key.page, Some(film::Page { number: page + 1, total: 3 }));
            assert_eq!(key.ids, page_ids(&all, page));
            assert_eq!(key.ids.len(), len);
            assert!(app.app.film_sheet.pixels.is_some(), "{:?}", app.app.film_sheet.error);
        };
        let click = |app: &mut crate::headless::Headless, id| {
            let result = app.request("ui.clickWidget", json!({"id": id}), timeout);
            assert_eq!(result["ok"], true, "{result}");
            assert!(app.settle(timeout));
        };
        check_page(&app, 0, 36);
        click(&mut app, "button:filmSheetNext");
        check_page(&app, 1, 36);
        let stale = app.app.film_sheet.key.as_ref().unwrap().token();
        click(&mut app, "button:filmSheetPage3");
        check_page(&app, 2, 3);
        assert!(export(&mut app.app, &json!({"path": "Sheet.jpg", "selection": stale})).is_err());
        click(&mut app, "button:filmSheetPrevious");
        check_page(&app, 1, 36);

        app.app.run("library.select", json!({"ids": &ids[..36]})).unwrap();
        assert!(app.settle(timeout));
        let key = app.app.film_sheet.key.as_ref().unwrap();
        assert_eq!(app.app.film_sheet.page, 0);
        assert_eq!(key.page, None);
        assert_eq!(key.ids.len(), 36);
        assert!(!app.app.widgets.iter().any(|(id, _)| id == "button:filmSheetNext"));
    }
}
