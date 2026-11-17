//! A 35 mm contact sheet with a shared composition for the editor and JPEG export.

use super::{Document, blit};
use crate::export::{ExportFormat, ExportOptions, encode_image};
use crate::media::{RenderJob, fit_for_display};
use crate::{Session, text::TextLayout};
use lightcraft_catalog::PhotoId;
use lightcraft_geom::Orientation;
use lightcraft_pipeline::{OutputDepth, OutputSpace, native_output_size};
use lightcraft_raster::Rgba8;

pub const CAPACITY: usize = 36;
const COLUMNS: usize = 6;
pub const PREVIEW_WIDTH: usize = 1280;
pub const EXPORT_WIDTH: usize = 3840;
pub const ASPECT: f32 = 80.0 / 79.0;
const PAPER: [u8; 4] = [239, 235, 223, 255];
const FILM: [u8; 4] = [57, 47, 39, 255];
const HOLE: [u8; 4] = [16, 14, 13, 255];
const INK: [u8; 3] = [38, 33, 28];
const EDGE_INK: [u8; 3] = [245, 234, 213];
const EDGE_INK_OPAQUE: [u8; 4] = [245, 234, 213, 255];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    pub date: String,
    pub subject: String,
    pub technical: String,
}

pub struct Prepared {
    width: usize,
    photos: Vec<(String, RenderJob)>,
}

pub fn prepare(session: &mut Session, ids: &[PhotoId], full: bool) -> Result<Prepared, String> {
    if ids.len() > CAPACITY {
        return Err("A film sheet has at most 36 frames".into());
    }
    let width = if full { EXPORT_WIDTH } else { PREVIEW_WIDTH };
    let (w, h) = (px(width, 116), px(width, 83));
    let mut photos = Vec::with_capacity(ids.len());
    for &id in ids {
        let photo = session.catalog.photo(id).ok_or("No such photo")?;
        let (src_w, src_h) = (photo.width as usize, photo.height as usize);
        let (crop_w, crop_h) = native_output_size(src_w.max(1), src_h.max(1), &photo.develop);
        // Reserve the frame's whole long edge for a portrait before turning it.
        let (max_w, max_h) = if src_w == 0 || src_h == 0 {
            (w, w)
        } else if crop_h > crop_w {
            (h, w)
        } else {
            (w, h)
        };
        let name = photo.file_name.clone();
        let job = session.export_job(id, max_w, max_h, OutputSpace::Srgb, OutputDepth::U8)?;
        photos.push((name, job));
    }
    Ok(Prepared { width, photos })
}

impl Prepared {
    pub fn render(self, progress: &mut dyn FnMut(usize, &str) -> bool) -> Result<Rgba8, String> {
        let mut sheet = blank(self.width);
        let (w, h) = (px(self.width, 116), px(self.width, 83));
        for (slot, (name, job)) in self.photos.into_iter().enumerate() {
            if !progress(slot, &name) {
                return Err("Contact sheet cancelled".into());
            }
            let image = job.run().rendered.map_err(|error| format!("{name}: {error}"))?.image;
            let (x, y) = frame(self.width, slot);
            let image = if image.height > image.width { image.into_oriented(Orientation::Rotate270) } else { image };
            // File dimensions can differ from catalog dimensions if an original was replaced.
            let image = if image.width > w || image.height > h { fit_for_display(&image, w, h) } else { image };
            if image.width > w || image.height > h {
                return Err(format!("{name}: render exceeded frame"));
            }
            fill(&mut sheet, x, y, w, h, HOLE);
            blit(&mut sheet, &image, x + (w - image.width) / 2, y + (h - image.height) / 2)?;
        }
        Ok(sheet)
    }

    pub fn jpeg(self, fields: &Fields, progress: &mut dyn FnMut(usize, &str) -> bool) -> Result<Document, String> {
        let photos = self.photos.len();
        let mut image = self.render(progress)?;
        if !progress(photos, "JPEG") {
            return Err("Contact sheet cancelled".into());
        }
        header(&mut image, fields);
        let bytes = encode_image(&image, &ExportOptions { format: ExportFormat::Jpeg, quality: 95, ..Default::default() })?;
        Ok(Document { bytes, pages: 1, photos })
    }
}

fn px(width: usize, design: usize) -> usize {
    (width * design + 400) / 800
}

fn frame(width: usize, slot: usize) -> (usize, usize) {
    (px(width, 33 + slot % COLUMNS * 124), px(width, 74 + slot / COLUMNS * 116))
}

pub fn blank_preview() -> Rgba8 {
    blank(PREVIEW_WIDTH)
}

fn blank(width: usize) -> Rgba8 {
    let mut image = Rgba8::filled(width, px(width, 790), PAPER);
    for row in 0..COLUMNS {
        let (_, y) = frame(width, row * COLUMNS);
        fill(&mut image, px(width, 27), y - px(width, 14), px(width, 746), px(width, 111), FILM);
    }
    for slot in 0..CAPACITY {
        let (x, y) = frame(width, slot);
        let w = px(width, 116);
        let h = px(width, 83);
        fill(&mut image, x, y, w, h, [249, 247, 240, 255]);
        for hole in 0..8 {
            let hx = px(width, 33 + slot % COLUMNS * 124 + hole * 15);
            for hy in [y - px(width, 6), y + h] {
                fill(&mut image, hx, hy, px(width, 6), px(width, 6), HOLE);
            }
        }
        let number = (slot + 1).to_string();
        for ny in [y - px(width, 15), y + h + px(width, 2)] {
            text(&mut image, &number, x + px(width, 69), ny, px(width, 29), px(width, 12), EDGE_INK);
        }
        if slot > 0 {
            let baseline = y + h + px(width, 10);
            edge_arrow(&mut image, x + px(width, 1), baseline, width);
            text(&mut image, &format!("{slot}A"), x + px(width, 12), y + h + px(width, 6), px(width, 27), px(width, 8), EDGE_INK);
        }
    }
    image
}

pub fn header(image: &mut Rgba8, fields: &Fields) {
    let w = image.width;
    for (label, input, x, line, end) in
        [("DATE", &fields.date, 42, 81, 211), ("SUBJECT", &fields.subject, 246, 308, 470), ("TECHNICAL DATA", &fields.technical, 505, 626, 758)]
    {
        text(image, label, px(w, x), px(w, 21), px(w, line - x), px(w, 17), INK);
        let line_w = px(w, end - line);
        let line_x = px(w, line);
        fill(image, line_x, px(w, 36), line_w, px(w, 1), [83, 75, 65, 255]);
        let line_text: String = input.chars().filter(|c| !c.is_control()).take(256).collect();
        if !line_text.is_empty() {
            text(image, &line_text, line_x + px(w, 3), px(w, 20), line_w - px(w, 6), px(w, 16), INK);
        }
    }
}

// A film edge mark is drawn independently of the availability of an arrow glyph.
fn edge_arrow(image: &mut Rgba8, x: usize, center: usize, width: usize) {
    let length = px(width, 9);
    let radius = px(width, 2);
    let stroke = (px(width, 1) / 2).max(1);
    for dx in 0..length {
        let wing = radius * (length - dx) / length;
        for y in [center - wing, center + wing] {
            fill(image, x + dx, y, 1, stroke, EDGE_INK_OPAQUE);
        }
    }
    fill(image, x, center - radius, stroke, radius * 2 + stroke, EDGE_INK_OPAQUE);
}

fn text(image: &mut Rgba8, value: &str, x: usize, y: usize, w: usize, h: usize, color: [u8; 3]) {
    let font = h as f32 * 0.91;
    let Some(layout) = TextLayout::new(value, font, false, crate::CRAFT_FONTS) else { return };
    let size = font * (w as f32 / layout.width.max(1.0)).min(1.0);
    let Some(layout) = TextLayout::new(value, size, false, crate::CRAFT_FONTS) else { return };
    let origin_y = y as f32 + h as f32 - layout.height;
    layout.draw_in(x as f32, origin_y, (x + w) as f32, (y + h) as f32, |gx, gy, coverage| {
        let (Ok(gx), Ok(gy)) = (usize::try_from(gx), usize::try_from(gy)) else { return };
        if !(x..x + w).contains(&gx) || !(y..y + h).contains(&gy) {
            return;
        }
        if let Some(pixel) = image.data.get_mut(gy * image.width + gx) {
            for (channel, ink) in pixel.iter_mut().zip(color) {
                *channel = (f32::from(*channel) + (f32::from(ink) - f32::from(*channel)) * coverage).round() as u8;
            }
        }
    });
}

fn fill(image: &mut Rgba8, x: usize, y: usize, w: usize, h: usize, color: [u8; 4]) {
    for row in image.data.chunks_exact_mut(image.width).skip(y).take(h) {
        if let Some(span) = row.get_mut(x..x + w) {
            span.fill(color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn film_sheet_develops_the_crop_and_exposure_then_turns_the_portrait() {
        use serde_json::json;

        let mut session = Session::with_demo();
        let id = session.active().expect("demo photo");
        let before = prepare(&mut session, &[id], false).unwrap().render(&mut |_, _| true).unwrap();
        session.execute("crop.set", &json!({"rect": [0.25, 0.08, 0.65, 0.92]})).unwrap();
        let crop_only = prepare(&mut session, &[id], false).unwrap().render(&mut |_, _| true).unwrap();
        session.execute("develop.set", &json!({"control": "light.exposure", "value": 1.5})).unwrap();
        let w = px(PREVIEW_WIDTH, 116);
        let h = px(PREVIEW_WIDTH, 83);
        let developed = session.render_export(id, h, w, OutputSpace::Srgb, OutputDepth::U8).unwrap().image;
        assert!(developed.width < developed.height);
        assert!(developed.height > h, "the portrait must use the frame's long edge");
        let after = prepare(&mut session, &[id], false).unwrap().render(&mut |_, _| true).unwrap();
        let (x, y) = frame(PREVIEW_WIDTH, 0);
        let offset = (y + (h - developed.width) / 2) * PREVIEW_WIDTH + x + (w - developed.height) / 2;
        assert_ne!(before.data, crop_only.data);
        assert_ne!(crop_only.data, after.data);
        for (row, actual) in after.data[offset..].chunks(PREVIEW_WIDTH).take(developed.width).enumerate() {
            for (col, pixel) in actual.iter().take(developed.height).enumerate() {
                assert_eq!(*pixel, developed.get(developed.width - 1 - row, col), "90° counterclockwise at ({col}, {row})");
            }
        }
    }

    #[test]
    fn film_sheet_exports_a_4k_jpeg() {
        let mut session = Session::with_demo();
        let id = session.active().expect("demo photo");
        let document = prepare(&mut session, &[id], true).unwrap().jpeg(&Fields::default(), &mut |_, _| true).unwrap();
        assert!(document.bytes.starts_with(&[0xff, 0xd8]));
        let decoded = lightcraft_codecs::decode(&document.bytes, Default::default()).unwrap();
        let image = decoded.to_srgb8();
        assert_eq!((image.width, image.height), (3840, 3792));
    }

    #[test]
    fn film_sheet_reserves_all_frames_and_keeps_the_header_separate() {
        let mut sheet = blank_preview();
        assert_eq!((sheet.width, sheet.height), (1280, 1264));
        for slot in 0..CAPACITY {
            let (x, y) = frame(sheet.width, slot);
            assert_eq!(sheet.data[y * sheet.width + x], [249, 247, 240, 255]);
            assert_eq!(sheet.data[(y - px(sheet.width, 8)) * sheet.width + x], FILM);
        }
        let before = sheet.clone();
        header(&mut sheet, &Fields { subject: "Séance 0387".into(), ..Default::default() });
        let strips = px(sheet.width, 60) * sheet.width;
        assert_ne!(before.data[..strips], sheet.data[..strips]);
        assert_eq!(before.data[strips..], sheet.data[strips..]);
        assert_eq!(px(EXPORT_WIDTH, 790), 3792);
    }
}
