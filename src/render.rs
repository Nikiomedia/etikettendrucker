//! Zeichnet ein Etikett.
//!
//! - Für den Drucker als Schwarz/Weiß-Bild in Druckerauflösung (8 Punkte/mm).
//! - Für die Vorschau in feinerer Auflösung mit Kantenglättung, damit sie so aussieht
//!   wie das gedruckte Etikett in echt (die Druckpunkte sind nur 1/8 mm groß).
//!
//! Alle Positionen werden in Druckpunkten angegeben; `Canvas::s` ist der Vergrößerungsfaktor.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{point, Font, FontVec, Glyph, PxScale, ScaleFont};
use image::DynamicImage;

use crate::i18n::tr;
use crate::model::{Align, BarcodeEl, BarcodeKind, ElementKind, ImageEl, Label, QrEl, TextEl};
use crate::protocol::PX_PER_MM;

// ---------------------------------------------------------------- Schriften

pub struct Fonts {
    db: fontdb::Database,
    families: Vec<String>,
    default_family: String,
    cache: RefCell<HashMap<(String, bool, bool), Option<Arc<FontVec>>>>,
}

impl Fonts {
    pub fn load() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut families: Vec<String> = db
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .filter(|n| !n.starts_with('@'))
            .collect();
        families.sort_by_key(|a| a.to_lowercase());
        families.dedup();
        let default_family = ["Arial", "Segoe UI", "Calibri", "Verdana"]
            .iter()
            .find(|f| families.iter().any(|x| x == *f))
            .map(|s| s.to_string())
            .or_else(|| families.first().cloned())
            .unwrap_or_default();
        Self { db, families, default_family, cache: RefCell::new(HashMap::new()) }
    }

    pub fn families(&self) -> &[String] {
        &self.families
    }

    pub fn default_family(&self) -> &str {
        &self.default_family
    }

    pub fn has_family(&self, name: &str) -> bool {
        self.families.iter().any(|f| f == name)
    }

    /// Lädt eine Schrift. Fehlt die Familie, wird die Standardschrift genommen.
    pub fn get(&self, family: &str, bold: bool, italic: bool) -> Option<Arc<FontVec>> {
        let family = if family.is_empty() || !self.has_family(family) {
            self.default_family.clone()
        } else {
            family.to_string()
        };
        let key = (family.clone(), bold, italic);
        if let Some(f) = self.cache.borrow().get(&key) {
            return f.clone();
        }
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(&family)],
            weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            stretch: fontdb::Stretch::Normal,
            style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        let font = self.db.query(&query).and_then(|id| {
            self.db
                .with_face_data(id, |data, index| FontVec::try_from_vec_and_index(data.to_vec(), index).ok())
                .flatten()
                .map(Arc::new)
        });
        self.cache.borrow_mut().insert(key, font.clone());
        font
    }
}

// ---------------------------------------------------------------- Zeichenfläche

struct Canvas {
    /// Größe in Pixeln (= Druckpunkte × s)
    w: usize,
    h: usize,
    /// Pixel pro Druckpunkt
    s: f32,
    /// 0 = weiß, 1 = schwarz
    ink: Vec<f32>,
}

impl Canvas {
    fn new(w: usize, h: usize, s: u32) -> Self {
        let s = s.max(1) as usize;
        Self { w: w * s, h: h * s, s: s as f32, ink: vec![0.0; w * h * s * s] }
    }

    fn put(&mut self, x: i32, y: i32, v: f32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = y as usize * self.w + x as usize;
            if v > self.ink[i] {
                self.ink[i] = v;
            }
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for yy in y.max(0)..(y + h).min(self.h as i32) {
            for xx in x.max(0)..(x + w).min(self.w as i32) {
                self.put(xx, yy, 1.0);
            }
        }
    }

    /// Druckpunkt-Koordinate (gerundet wie beim Druck) in Pixel umrechnen
    fn dot(&self, v: f32) -> i32 {
        (v.round() * self.s) as i32
    }
}

// ---------------------------------------------------------------- Ergebnis

/// Begrenzungsrahmen eines Elements in Druckpunkten.
#[derive(Clone, Copy, Debug, Default)]
pub struct ElemBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Bei automatisch angepasstem Text: die berechnete Schriftgröße
    pub fit_pt: Option<f32>,
}

impl ElemBox {
    fn scaled_down(self, s: f32) -> Self {
        Self { x: self.x / s, y: self.y / s, w: self.w / s, h: self.h / s, fit_pt: self.fit_pt }
    }
}

/// Druckbild
pub struct Rendered {
    pub width: usize,
    pub height: usize,
    /// true = schwarz
    pub pixels: Vec<bool>,
    pub boxes: Vec<ElemBox>,
    pub warnings: Vec<String>,
}

/// Geglättete Vorschau
pub struct Preview {
    pub width: usize,
    pub height: usize,
    /// 0 = weiß, 1 = schwarz
    pub ink: Vec<f32>,
}

#[derive(Default)]
pub struct ImageCache {
    map: HashMap<(usize, usize), DynamicImage>,
}

impl ImageCache {
    fn get(&mut self, data: &Arc<Vec<u8>>) -> Option<&DynamicImage> {
        let key = (Arc::as_ptr(data) as usize, data.len());
        if !self.map.contains_key(&key) {
            let img = image::load_from_memory(data).ok()?;
            self.map.insert(key, img);
        }
        self.map.get(&key)
    }
}

pub fn mm_to_px(mm: f32) -> f32 {
    mm * PX_PER_MM
}

pub fn label_px(label: &Label) -> (usize, usize) {
    (
        mm_to_px(label.width_mm).round().max(8.0) as usize,
        mm_to_px(label.height_mm).round().max(8.0) as usize,
    )
}

/// Druckbild in Druckerauflösung.
pub fn render(label: &Label, fonts: &Fonts, images: &mut ImageCache) -> Rendered {
    let (c, boxes, warnings) = render_at(label, fonts, images, 1);
    let pixels = c.ink.iter().map(|&v| v >= 0.5).collect();
    Rendered { width: c.w, height: c.h, pixels, boxes, warnings }
}

/// Geglättete Vorschau mit `s` Pixeln pro Druckpunkt.
pub fn render_preview(label: &Label, fonts: &Fonts, images: &mut ImageCache, s: u32) -> Preview {
    let (c, _, _) = render_at(label, fonts, images, s);
    Preview { width: c.w, height: c.h, ink: c.ink }
}

fn render_at(label: &Label, fonts: &Fonts, images: &mut ImageCache, s: u32) -> (Canvas, Vec<ElemBox>, Vec<String>) {
    let (w, h) = label_px(label);
    let mut c = Canvas::new(w, h, s);
    let mut boxes = Vec::with_capacity(label.elements.len());
    let mut warnings = Vec::new();

    if label.frame.enabled {
        draw_frame(&mut c, label);
    }

    for el in &label.elements {
        let x = mm_to_px(el.x_mm);
        let y = mm_to_px(el.y_mm);
        let b = match &el.kind {
            ElementKind::Text(t) => draw_text_el(&mut c, label, t, x, y, fonts, &mut warnings),
            ElementKind::Qr(q) => draw_qr(&mut c, q, x, y, &mut warnings),
            ElementKind::Barcode(bc) => draw_barcode(&mut c, bc, x, y, fonts, &mut warnings),
            ElementKind::Image(img) => draw_image(&mut c, img, x, y, images, &mut warnings),
        };
        boxes.push(b.scaled_down(c.s));
    }
    (c, boxes, warnings)
}

// ---------------------------------------------------------------- Rahmen

fn draw_frame(c: &mut Canvas, label: &Label) {
    let f = &label.frame;
    let s = c.s;
    let inset = mm_to_px(f.inset_mm) * s;
    let (x0, y0) = (inset, inset);
    let (x1, y1) = (c.w as f32 - inset, c.h as f32 - inset);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hw, hh) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let r = (mm_to_px(f.radius_mm) * s).min(hw).min(hh).max(0.0);
    let t = f.thickness_px.max(1) as f32 * s;
    for py in 0..c.h {
        for px in 0..c.w {
            let qx = (px as f32 + 0.5 - cx).abs() - (hw - r);
            let qy = (py as f32 + 0.5 - cy).abs() - (hh - r);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let sd = outside + qx.max(qy).min(0.0) - r;
            // weiche Kanten: innerhalb des Außenrands und außerhalb des Innenrands
            let cov = (0.5 - sd).clamp(0.0, 1.0) * (sd + t + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                c.put(px as i32, py as i32, cov);
            }
        }
    }
}

// ---------------------------------------------------------------- Text

struct TextLayout {
    glyphs: Vec<Glyph>,
    /// min_x, min_y, max_x, max_y der tatsächlich geschwärzten Pixel
    ink: Option<(f32, f32, f32, f32)>,
}

/// `px_per_mm`: Pixel pro Millimeter auf der Zeichenfläche
fn px_scale_for_pt(font: &FontVec, pt: f32, px_per_mm: f32) -> PxScale {
    let em_px = pt * px_per_mm * 25.4 / 72.0;
    let upem = font.units_per_em().unwrap_or(1000.0);
    PxScale::from(em_px * font.height_unscaled() / upem)
}

fn layout_text(font: &FontVec, text: &str, pt: f32, align: Align, spacing: f32, px_per_mm: f32) -> TextLayout {
    let scale = px_scale_for_pt(font, pt, px_per_mm);
    let sf = font.as_scaled(scale);
    let line_h = (sf.ascent() - sf.descent() + sf.line_gap()) * spacing.max(0.3);

    let mut lines = Vec::new();
    let mut widths = Vec::new();
    for line in text.split('\n') {
        let mut x = 0.0;
        let mut prev = None;
        let mut gs = Vec::new();
        for ch in line.chars().filter(|c| !c.is_control()) {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            gs.push((id, x));
            x += sf.h_advance(id);
            prev = Some(id);
        }
        widths.push(x);
        lines.push(gs);
    }
    let max_w = widths.iter().cloned().fold(0.0f32, f32::max);

    let mut glyphs = Vec::new();
    let mut ink: Option<(f32, f32, f32, f32)> = None;
    for (i, gs) in lines.into_iter().enumerate() {
        let off = match align {
            Align::Left => 0.0,
            Align::Center => (max_w - widths[i]) / 2.0,
            Align::Right => max_w - widths[i],
        };
        let baseline = sf.ascent() + i as f32 * line_h;
        for (id, gx) in gs {
            let g = id.with_scale_and_position(scale, point(off + gx, baseline));
            if let Some(og) = font.outline_glyph(g.clone()) {
                let b = og.px_bounds();
                ink = Some(match ink {
                    None => (b.min.x, b.min.y, b.max.x, b.max.y),
                    Some((a, bb, cc, d)) => (a.min(b.min.x), bb.min(b.min.y), cc.max(b.max.x), d.max(b.max.y)),
                });
            }
            glyphs.push(g);
        }
    }
    TextLayout { glyphs, ink }
}

/// Zeichnet Text so, dass seine linke obere Tintenkante bei (x, y) Pixeln liegt.
/// Gibt den Rahmen (x, y, w, h) in Pixeln zurück.
fn draw_text_at(c: &mut Canvas, font: &FontVec, lay: &TextLayout, x: f32, y: f32) -> (f32, f32, f32, f32) {
    let Some((ix0, iy0, ix1, iy1)) = lay.ink else {
        return (x, y, 0.0, 0.0);
    };
    let (dx, dy) = ((x - ix0).round(), (y - iy0).round());
    for g in &lay.glyphs {
        let mut g = g.clone();
        g.position = point(g.position.x + dx, g.position.y + dy);
        if let Some(og) = font.outline_glyph(g) {
            let b = og.px_bounds();
            og.draw(|gx, gy, cov| c.put(b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, cov));
        }
    }
    (ix0 + dx, iy0 + dy, ix1 - ix0, iy1 - iy0)
}

/// Rückgabe in Pixeln der Zeichenfläche (wird vom Aufrufer auf Druckpunkte umgerechnet).
fn draw_text_el(
    c: &mut Canvas,
    label: &Label,
    t: &TextEl,
    x: f32,
    y: f32,
    fonts: &Fonts,
    warnings: &mut Vec<String>,
) -> ElemBox {
    let s = c.s;
    let ppm = PX_PER_MM * s;
    if !t.font.is_empty() && !fonts.has_family(&t.font) {
        warnings.push(trf!(
            "Schrift „{}“ ist nicht installiert, nehme {}.",
            "Font “{}” is not installed, using {}.",
            t.font,
            fonts.default_family()
        ));
    }
    let Some(font) = fonts.get(&t.font, t.bold, t.italic) else {
        warnings.push(tr("Keine Schrift gefunden.", "No font found.").into());
        return ElemBox { x: x * s, y: y * s, w: 8.0 * s, h: 8.0 * s, fit_pt: None };
    };

    if t.fit {
        let mut margin = mm_to_px(label.margin_mm);
        if label.frame.enabled {
            margin += mm_to_px(label.frame.inset_mm) + label.frame.thickness_px as f32;
        }
        let margin = margin * s;
        let avail_w = (c.w as f32 - 2.0 * margin).max(4.0);
        let avail_h = (c.h as f32 - 2.0 * margin).max(4.0);
        let probe = layout_text(&font, &t.text, 100.0, t.align, t.line_spacing, ppm);
        let Some((a, b, cc, d)) = probe.ink else {
            return ElemBox { x: margin, y: margin, w: 8.0 * s, h: 8.0 * s, fit_pt: Some(t.size_pt) };
        };
        let (iw, ih) = ((cc - a).max(1.0), (d - b).max(1.0));
        let pt = (100.0 * (avail_w / iw).min(avail_h / ih)).clamp(3.0, 400.0);
        let lay = layout_text(&font, &t.text, pt, t.align, t.line_spacing, ppm);
        let (w, h) = lay.ink.map(|(a, b, cc, d)| (cc - a, d - b)).unwrap_or((0.0, 0.0));
        let tx = match t.align {
            Align::Left => margin,
            Align::Center => (c.w as f32 - w) / 2.0,
            Align::Right => c.w as f32 - margin - w,
        };
        let ty = (c.h as f32 - h) / 2.0;
        let (bx, by, bw, bh) = draw_text_at(c, &font, &lay, tx, ty);
        ElemBox { x: bx, y: by, w: bw, h: bh, fit_pt: Some(pt) }
    } else {
        let lay = layout_text(&font, &t.text, t.size_pt.max(1.0), t.align, t.line_spacing, ppm);
        let (bx, by, bw, bh) = draw_text_at(c, &font, &lay, x * s, y * s);
        ElemBox { x: bx, y: by, w: bw.max(8.0 * s), h: bh.max(8.0 * s), fit_pt: None }
    }
}

// ---------------------------------------------------------------- QR-Code

fn draw_qr(c: &mut Canvas, q: &QrEl, x: f32, y: f32, warnings: &mut Vec<String>) -> ElemBox {
    let s = c.s;
    let size = mm_to_px(q.size_mm).max(8.0);
    let code = match qrcode::QrCode::with_error_correction_level(q.content.as_bytes(), qrcode::EcLevel::M) {
        Ok(code) => code,
        Err(_) => {
            warnings.push(tr("QR-Code: Inhalt ist zu lang.", "QR code: content is too long.").into());
            return ElemBox { x: x * s, y: y * s, w: size * s, h: size * s, fit_pt: None };
        }
    };
    let n = code.width();
    // Modulgröße in ganzen Druckpunkten, damit der Code scharf druckt
    let module_dots = ((size / n as f32).floor() as i32).max(1);
    if (size / n as f32) < 2.0 {
        warnings.push(
            tr(
                "QR-Code ist sehr klein – evtl. schlecht lesbar. Größer machen oder Inhalt kürzen.",
                "QR code is very small and may be hard to scan. Make it larger or shorten the content.",
            )
            .into(),
        );
    }
    let module = module_dots * s as i32;
    let colors = code.to_colors();
    let (x0, y0) = (c.dot(x), c.dot(y));
    for my in 0..n {
        for mx in 0..n {
            if colors[my * n + mx] == qrcode::Color::Dark {
                c.fill_rect(x0 + mx as i32 * module, y0 + my as i32 * module, module, module);
            }
        }
    }
    let real = (n as i32 * module) as f32;
    ElemBox { x: x0 as f32, y: y0 as f32, w: real, h: real, fit_pt: None }
}

// ---------------------------------------------------------------- Barcode

fn encode_barcode(b: &BarcodeEl) -> Result<Vec<u8>, String> {
    match b.kind {
        BarcodeKind::Code128 => {
            if b.content.is_empty() {
                return Err(tr("Barcode: Inhalt fehlt.", "Barcode: content is missing.").into());
            }
            if !b.content.chars().all(|c| (' '..='~').contains(&c)) {
                return Err(tr(
                    "Code 128: nur Zeichen ohne Umlaute/Sonderzeichen erlaubt.",
                    "Code 128: only plain characters, no accents or special characters.",
                )
                .into());
            }
            barcoders::sym::code128::Code128::new(format!("\u{0181}{}", b.content))
                .map(|bc| bc.encode())
                .map_err(|e| format!("Code 128: {e}"))
        }
        BarcodeKind::Ean13 => {
            let digits: String = b.content.chars().filter(|c| !c.is_whitespace()).collect();
            if !digits.chars().all(|c| c.is_ascii_digit()) || !(digits.len() == 12 || digits.len() == 13) {
                return Err(tr("EAN-13: bitte 12 oder 13 Ziffern eingeben.", "EAN-13: please enter 12 or 13 digits.").into());
            }
            let first12 = &digits[..12];
            if digits.len() == 13 && ean_checksum(first12) != digits.as_bytes()[12] - b'0' {
                return Err(tr("EAN-13: Prüfziffer (letzte Ziffer) stimmt nicht.", "EAN-13: the check digit (last digit) is wrong.").into());
            }
            barcoders::sym::ean13::EAN13::new(first12)
                .map(|bc| bc.encode())
                .map_err(|e| format!("EAN-13: {e}"))
        }
    }
}

fn ean_checksum(d12: &str) -> u8 {
    let sum: u32 = d12
        .bytes()
        .enumerate()
        .map(|(i, b)| (b - b'0') as u32 * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    ((10 - sum % 10) % 10) as u8
}

fn barcode_caption(b: &BarcodeEl) -> String {
    match b.kind {
        BarcodeKind::Code128 => b.content.clone(),
        BarcodeKind::Ean13 => {
            let d: String = b.content.chars().filter(|c| c.is_ascii_digit()).collect();
            if d.len() == 12 {
                format!("{d}{}", ean_checksum(&d))
            } else {
                d
            }
        }
    }
}

fn draw_barcode(c: &mut Canvas, b: &BarcodeEl, x: f32, y: f32, fonts: &Fonts, warnings: &mut Vec<String>) -> ElemBox {
    let s = c.s;
    let bar_h_dots = mm_to_px(b.height_mm).max(4.0) as i32;
    let modules = match encode_barcode(b) {
        Ok(m) => m,
        Err(e) => {
            warnings.push(e);
            return ElemBox { x: x * s, y: y * s, w: 80.0 * s, h: bar_h_dots as f32 * s, fit_pt: None };
        }
    };
    let m_dots = b.module_px.clamp(1, 6) as i32;
    let (m, bar_h) = (m_dots * s as i32, bar_h_dots * s as i32);
    let (x0, y0) = (c.dot(x), c.dot(y));
    for (i, &bit) in modules.iter().enumerate() {
        if bit == 1 {
            c.fill_rect(x0 + i as i32 * m, y0, m, bar_h);
        }
    }
    let bw = (modules.len() as i32 * m) as f32;
    let mut total_h = bar_h as f32;
    if b.show_text {
        if let Some(font) = fonts.get("", false, false) {
            let lay = layout_text(&font, &barcode_caption(b), 6.0, Align::Center, 1.0, PX_PER_MM * s);
            if let Some((a, _, cc, _)) = lay.ink {
                let tw = cc - a;
                let ty = (y0 + bar_h) as f32 + 2.0 * s;
                let (_, by, _, bh) = draw_text_at(c, &font, &lay, x0 as f32 + (bw - tw) / 2.0, ty);
                total_h = by + bh - y0 as f32;
            }
        }
    }
    if bw > c.w as f32 {
        warnings.push(
            tr("Barcode ist breiter als das Etikett. Strichbreite verkleinern.", "Barcode is wider than the label. Reduce the bar width.")
                .into(),
        );
    }
    ElemBox { x: x0 as f32, y: y0 as f32, w: bw, h: total_h, fit_pt: None }
}

// ---------------------------------------------------------------- Bild

fn image_gray(img: &DynamicImage, w: u32, h: u32, invert: bool) -> Vec<f32> {
    img.resize_exact(w, h, image::imageops::FilterType::Triangle)
        .to_luma_alpha8()
        .pixels()
        .map(|p| {
            let [l, a] = p.0;
            let a = a as f32 / 255.0;
            // Graustufen mit weißem Hintergrund
            let v = l as f32 * a + 255.0 * (1.0 - a);
            if invert {
                255.0 - v
            } else {
                v
            }
        })
        .collect()
}

fn draw_image(c: &mut Canvas, img_el: &ImageEl, x: f32, y: f32, images: &mut ImageCache, warnings: &mut Vec<String>) -> ElemBox {
    let s = c.s;
    let si = s as i32;
    let tw = mm_to_px(img_el.width_mm).round().max(2.0) as u32;
    let Some(img) = images.get(&img_el.data) else {
        warnings.push(tr("Bild konnte nicht gelesen werden.", "Image could not be read.").into());
        return ElemBox { x: x * s, y: y * s, w: tw as f32 * s, h: tw as f32 * s, fit_pt: None };
    };
    let (iw, ih) = (img.width().max(1), img.height().max(1));
    let th = ((tw as f32) * ih as f32 / iw as f32).round().max(1.0) as u32;
    let (x0, y0) = (c.dot(x), c.dot(y));

    if img_el.dither {
        // Rasterung immer in Druckerauflösung, damit die Vorschau das echte Punktmuster zeigt
        let (w, h) = (tw as usize, th as usize);
        let mut gray = image_gray(img, tw, th, img_el.invert);
        let black = floyd_steinberg(&mut gray, w, h);
        for yy in 0..h {
            for xx in 0..w {
                if black[yy * w + xx] {
                    c.fill_rect(x0 + xx as i32 * si, y0 + yy as i32 * si, si, si);
                }
            }
        }
    } else {
        // Schwelle in Zielauflösung: in der Vorschau mit glatten Kanten
        let (w, h) = (tw * si as u32, th * si as u32);
        let gray = image_gray(img, w, h, img_el.invert);
        let thr = img_el.threshold as f32;
        for yy in 0..h as usize {
            for xx in 0..w as usize {
                let v = gray[yy * w as usize + xx];
                let ink = if si == 1 {
                    if v < thr { 1.0 } else { 0.0 }
                } else {
                    // weicher Übergang um die Schwelle herum
                    ((thr - v) / 48.0 + 0.5).clamp(0.0, 1.0)
                };
                if ink > 0.0 {
                    c.put(x0 + xx as i32, y0 + yy as i32, ink);
                }
            }
        }
    }
    ElemBox { x: x0 as f32, y: y0 as f32, w: tw as f32 * s, h: th as f32 * s, fit_pt: None }
}

/// Floyd-Steinberg-Rasterung, Schlangenlinie (wie die Original-App). true = schwarz.
fn floyd_steinberg(px: &mut [f32], w: usize, h: usize) -> Vec<bool> {
    let mut out = vec![false; w * h];
    for y in 0..h {
        let ltr = y % 2 == 0;
        for i in 0..w {
            let x = if ltr { i } else { w - 1 - i };
            let idx = y * w + x;
            let old = px[idx];
            let new = if old >= 128.0 { 255.0 } else { 0.0 };
            out[idx] = new == 0.0;
            let err = old - new;
            let fwd = if ltr { x.checked_add(1).filter(|&v| v < w) } else { x.checked_sub(1) };
            let back = if ltr { x.checked_sub(1) } else { x.checked_add(1).filter(|&v| v < w) };
            if let Some(f) = fwd {
                px[y * w + f] += err * 7.0 / 16.0;
            }
            if y + 1 < h {
                if let Some(b) = back {
                    px[(y + 1) * w + b] += err * 3.0 / 16.0;
                }
                px[(y + 1) * w + x] += err * 5.0 / 16.0;
                if let Some(f) = fwd {
                    px[(y + 1) * w + f] += err * 1.0 / 16.0;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Element, Label, QrEl};

    fn sample_label() -> Label {
        let mut label = Label::default();
        label.elements = vec![Element::new_text("Gewürze – Kümmel")];
        label.frame.enabled = true;
        label
    }

    /// Rendert ein Beispieletikett und legt es als PNG ab (zum Ansehen).
    #[test]
    fn render_sample() {
        let fonts = Fonts::load();
        let mut images = ImageCache::default();
        let label = sample_label();
        let r = render(&label, &fonts, &mut images);
        assert_eq!((r.width, r.height), (320, 96));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let black = r.pixels.iter().filter(|&&p| p).count();
        assert!(black > 500, "zu wenig Schwarz: {black}");

        let mut img = image::GrayImage::new(r.width as u32, r.height as u32);
        for (i, p) in r.pixels.iter().enumerate() {
            img.put_pixel((i % r.width) as u32, (i / r.width) as u32, image::Luma([if *p { 0 } else { 255 }]));
        }
        let out = std::env::temp_dir().join("etikett-test.png");
        img.save(&out).unwrap();
        eprintln!("gespeichert: {}", out.display());
    }

    /// Die Vorschau muss deckungsgleich mit dem Druckbild sein: Rahmen und Positionen stimmen überein.
    #[test]
    fn preview_matches_print() {
        let fonts = Fonts::load();
        let mut images = ImageCache::default();
        let mut label = sample_label();
        label.elements.push(Element { x_mm: 30.0, y_mm: 1.0, kind: ElementKind::Qr(QrEl { size_mm: 9.0, ..Default::default() }) });
        let r = render(&label, &fonts, &mut images);
        let s = 4;
        let p = render_preview(&label, &fonts, &mut images, s);
        assert_eq!((p.width, p.height), (r.width * 4, r.height * 4));
        // Schwarzanteil soll ungefähr gleich sein (Kantenglättung verschiebt ihn nur leicht)
        let print_black = r.pixels.iter().filter(|&&b| b).count() as f32 / r.pixels.len() as f32;
        let prev_black = p.ink.iter().sum::<f32>() / p.ink.len() as f32;
        assert!((print_black - prev_black).abs() < 0.03, "Druck {print_black}, Vorschau {prev_black}");

        let mut img = image::GrayImage::new(p.width as u32, p.height as u32);
        for (i, v) in p.ink.iter().enumerate() {
            img.put_pixel((i % p.width) as u32, (i / p.width) as u32, image::Luma([(255.0 * (1.0 - v)) as u8]));
        }
        img.save(std::env::temp_dir().join("etikett-vorschau.png")).unwrap();
    }

    #[test]
    fn barcodes_encode() {
        let mut b = BarcodeEl::default();
        assert!(encode_barcode(&b).is_ok());
        b.kind = BarcodeKind::Ean13;
        b.content = "400638133393".into();
        assert!(encode_barcode(&b).is_ok());
        b.content = "4006381333931".into();
        assert!(encode_barcode(&b).is_ok());
        b.content = "4006381333932".into();
        assert!(encode_barcode(&b).is_err());
    }
}
