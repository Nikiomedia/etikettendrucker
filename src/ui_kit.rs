//! Design-System: Farben, Schriften, Symbole und eigene Bedienelemente.

use std::sync::Arc;

use eframe::egui::{
    self, epaint::Shadow, pos2, text::LayoutJob, vec2, Align2, Color32, CornerRadius, FontData, FontDefinitions,
    FontFamily, FontId, Margin, Rect, Response, Sense, Stroke, StrokeKind, TextFormat, Theme, Ui, Visuals,
};

// ---------------------------------------------------------------- Symbole (Segoe Fluent Icons / MDL2)

pub mod icon {
    pub const PRINT: char = '\u{E749}';
    pub const ADD: char = '\u{E710}';
    pub const SAVE: char = '\u{E74E}';
    pub const SAVE_AS: char = '\u{E792}';
    pub const OPEN: char = '\u{E8E5}';
    pub const NEW: char = '\u{E7C3}';
    pub const BLUETOOTH: char = '\u{E702}';
    pub const DISCONNECT: char = '\u{E8CD}';
    pub const DELETE: char = '\u{E74D}';
    pub const COPY: char = '\u{E8C8}';
    pub const TEXT: char = '\u{E8D2}';
    pub const QR: char = '\u{ED14}';
    pub const BARCODE: char = '\u{EE6F}';
    pub const IMAGE: char = '\u{E91B}';
    pub const CHECK: char = '\u{E73E}';
    pub const CLOSE: char = '\u{E711}';
    pub const WARNING: char = '\u{E7BA}';
    pub const INFO: char = '\u{E946}';
    pub const UP: char = '\u{E70E}';
    pub const DOWN: char = '\u{E70D}';
    pub const BOLD: char = '\u{E8DD}';
    pub const ITALIC: char = '\u{E8DB}';
    pub const ALIGN_LEFT: char = '\u{E8E4}';
    pub const ALIGN_CENTER: char = '\u{E8E3}';
    pub const ALIGN_RIGHT: char = '\u{E8E2}';
    pub const LAYERS: char = '\u{E81E}';
    pub const LIST: char = '\u{E8FD}';
    pub const MOVE: char = '\u{E7C2}';

    /// Akkusymbol passend zum Ladestand (0–100 %)
    pub fn battery(level: u8) -> char {
        let step = (level as u32 + 5) / 10;
        if step >= 10 {
            '\u{E83F}'
        } else {
            char::from_u32(0xE850 + step).unwrap_or('\u{E83F}')
        }
    }
}

pub fn icon_family() -> FontFamily {
    FontFamily::Name("icons".into())
}

pub fn semibold_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, icon_family())
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, semibold_family())
}

// ---------------------------------------------------------------- Farben

#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color32,
    pub card: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub hover: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub canvas: Color32,
    pub grid: Color32,
    pub input: Color32,
    pub ok: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub guide: Color32,
}

impl Pal {
    pub fn get(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                bg: Color32::from_rgb(22, 23, 28),
                card: Color32::from_rgb(32, 34, 41),
                border: Color32::from_rgb(48, 51, 61),
                border_strong: Color32::from_rgb(70, 74, 88),
                hover: Color32::from_rgb(44, 47, 57),
                text: Color32::from_rgb(234, 236, 242),
                weak: Color32::from_rgb(146, 151, 166),
                accent: Color32::from_rgb(124, 117, 255),
                accent_hover: Color32::from_rgb(143, 137, 255),
                accent_soft: Color32::from_rgb(47, 45, 82),
                canvas: Color32::from_rgb(17, 18, 22),
                grid: Color32::from_rgb(38, 40, 48),
                input: Color32::from_rgb(25, 26, 32),
                ok: Color32::from_rgb(64, 196, 120),
                danger: Color32::from_rgb(255, 102, 102),
                warn: Color32::from_rgb(245, 176, 65),
                guide: Color32::from_rgb(255, 92, 170),
            }
        } else {
            Self {
                dark,
                bg: Color32::from_rgb(243, 244, 248),
                card: Color32::WHITE,
                border: Color32::from_rgb(226, 228, 236),
                border_strong: Color32::from_rgb(200, 204, 216),
                hover: Color32::from_rgb(240, 241, 247),
                text: Color32::from_rgb(28, 30, 40),
                weak: Color32::from_rgb(112, 117, 134),
                accent: Color32::from_rgb(88, 80, 236),
                accent_hover: Color32::from_rgb(72, 63, 214),
                accent_soft: Color32::from_rgb(236, 235, 255),
                canvas: Color32::from_rgb(233, 235, 242),
                grid: Color32::from_rgb(211, 214, 225),
                input: Color32::from_rgb(247, 248, 251),
                ok: Color32::from_rgb(22, 163, 90),
                danger: Color32::from_rgb(220, 50, 60),
                warn: Color32::from_rgb(214, 132, 0),
                guide: Color32::from_rgb(236, 50, 140),
            }
        }
    }

    pub fn of(ui: &Ui) -> Self {
        Self::get(ui.visuals().dark_mode)
    }
}

/// Weißes Etikettenpapier und Druckfarbe – in beiden Themen gleich, wie im echten Leben.
pub const PAPER: Color32 = Color32::from_rgb(253, 253, 251);
pub const INK: Color32 = Color32::from_rgb(24, 24, 28);

// ---------------------------------------------------------------- Einrichtung

pub fn setup(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Light, visuals(&Pal::get(false)));
    ctx.set_visuals_of(Theme::Dark, visuals(&Pal::get(true)));
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::*;
        for (ts, font) in style.text_styles.iter_mut() {
            font.size = match ts {
                Heading => 17.0,
                Body | Button => 14.0,
                Monospace => 12.5,
                Small => 12.0,
                _ => font.size,
            };
        }
        style.text_styles.insert(Heading, semibold(17.0));
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.interact_size = vec2(40.0, 28.0);
        style.spacing.combo_height = 320.0;
        style.spacing.window_margin = Margin::same(14);
        style.spacing.menu_margin = Margin::same(6);
        style.interaction.selectable_labels = false;
    });
}

fn windows_font(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    std::fs::read(std::path::Path::new(&dir).join("Fonts").join(name)).ok()
}

fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let base: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut prop = base.clone();
    if let Some(b) = windows_font("segoeui.ttf") {
        defs.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(b)));
        prop.insert(0, "segoe".into());
    }
    let mut semi = prop.clone();
    if let Some(b) = windows_font("seguisb.ttf") {
        defs.font_data.insert("segoe-sb".into(), Arc::new(FontData::from_owned(b)));
        semi.insert(0, "segoe-sb".into());
    }
    let mut icons = Vec::new();
    if let Some(b) = windows_font("SegoeIcons.ttf").or_else(|| windows_font("segmdl2.ttf")) {
        defs.font_data.insert("icons".into(), Arc::new(FontData::from_owned(b)));
        icons.push("icons".into());
    }
    icons.extend(base);

    defs.families.insert(FontFamily::Proportional, prop);
    defs.families.insert(semibold_family(), semi);
    defs.families.insert(icon_family(), icons);
    ctx.set_fonts(defs);
}

fn visuals(p: &Pal) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(8);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.hover;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(if p.dark { 110 } else { 30 }) };
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.hyperlink_color = p.accent;
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);
    v.slider_trailing_fill = true;
    v.indent_has_left_vline = false;
    v.override_text_color = None;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.input;
    w.inactive.weak_bg_fill = p.card;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.inactive.corner_radius = r;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5_f32, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent_soft;
    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    w.active.fg_stroke = Stroke::new(1.5_f32, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.hover;
    w.open.weak_bg_fill = p.hover;
    w.open.bg_stroke = Stroke::new(1.0_f32, p.accent);
    w.open.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.open.corner_radius = r;
    v
}

// ---------------------------------------------------------------- Bausteine

/// Symbol + Text als ein Textblock. Farbe wird vom Bedienelement eingesetzt.
pub fn icon_text(icon: char, text: &str, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &icon.to_string(),
        0.0,
        TextFormat { font_id: icon_font(size), color: Color32::PLACEHOLDER, valign: egui::Align::Center, ..Default::default() },
    );
    if !text.is_empty() {
        job.append(
            text,
            8.0,
            TextFormat {
                font_id: FontId::proportional(size),
                color: Color32::PLACEHOLDER,
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
    }
    job
}

/// Weißer Kasten mit feinem Rand und weichem Schatten.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Pal::of(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .shadow(Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(if p.dark { 40 } else { 10 }) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Überschrift in einer Karte, mit Symbol.
pub fn card_title(ui: &mut Ui, icon: char, title: &str) {
    let p = Pal::of(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon.to_string()).font(icon_font(15.0)).color(p.accent));
        ui.label(egui::RichText::new(title).font(semibold(15.0)).color(p.text));
    });
    ui.add_space(2.0);
}

/// Kleine graue Feldbeschriftung.
pub fn field_label(ui: &mut Ui, text: &str) {
    let p = Pal::of(ui);
    ui.label(egui::RichText::new(text).size(12.5).color(p.weak));
}

/// Schlichter Knopf ohne Rahmen (für Werkzeugleisten).
pub fn tool_button(ui: &mut Ui, icon: char, text: &str) -> Response {
    let p = Pal::of(ui);
    let galley = ui.painter().layout_job(icon_text(icon, text, 14.0));
    let pad = vec2(if text.is_empty() { 8.0 } else { 10.0 }, 6.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() || resp.is_pointer_button_down_on() {
            let fill = if resp.is_pointer_button_down_on() { p.border } else { p.hover };
            ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        }
        let col = if ui.is_enabled() { p.text } else { p.weak };
        ui.painter().galley(rect.min + pad, galley, col);
    }
    resp
}

/// Kleiner runder Symbolknopf.
pub fn icon_button(ui: &mut Ui, icon: char, color: Option<Color32>) -> Response {
    let p = Pal::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(8), p.hover);
        }
        let col = if !ui.is_enabled() { p.weak.gamma_multiply(0.5) } else { color.unwrap_or(p.text) };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, icon_font(14.0), col);
    }
    resp
}

/// Großer farbiger Hauptknopf; zeigt optional einen Fortschritt.
pub fn primary_button(ui: &mut Ui, icon: char, text: &str, size: egui::Vec2, enabled: bool, progress: Option<f32>) -> Response {
    let p = Pal::of(ui);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let r = CornerRadius::same(10);
        let painter = ui.painter();
        if let Some(f) = progress {
            painter.rect_filled(rect, r, p.accent_soft);
            let mut done = rect;
            done.set_width(rect.width() * f.clamp(0.0, 1.0));
            painter.rect_filled(done, r, p.accent);
        } else if enabled {
            let fill = if resp.hovered() { p.accent_hover } else { p.accent };
            if resp.hovered() {
                painter.add(
                    Shadow { offset: [0, 4], blur: 14, spread: 0, color: p.accent.gamma_multiply(0.35) }.as_shape(rect, r),
                );
            }
            painter.rect_filled(rect, r, fill);
        } else {
            painter.rect_filled(rect, r, if p.dark { p.border } else { p.border });
        }
        let col = if progress.is_some() || !enabled {
            if progress.is_some() { p.text } else { p.weak }
        } else {
            Color32::WHITE
        };
        let galley = painter.layout_job(icon_text(icon, text, 15.5));
        let pos = rect.center() - galley.size() / 2.0;
        painter.galley(pos, galley, col);
    }
    resp
}

/// Segmentierter Umschalter (z. B. Hell | Normal | Dunkel).
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], icons: bool) -> bool {
    let p = Pal::of(ui);
    let font = if icons { icon_font(14.0) } else { FontId::proportional(13.5) };
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, t)| ui.painter().layout_no_wrap(t.to_string(), font.clone(), p.text))
        .collect();
    let seg_w: Vec<f32> = galleys.iter().map(|g| g.size().x + if icons { 22.0 } else { 26.0 }).collect();
    let h = 30.0;
    let total = seg_w.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(9), if p.dark { p.input } else { p.bg });
    painter.rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
    let mut x = rect.min.x + 3.0;
    let mut changed = false;
    for (i, (val, _)) in options.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.min.y + 3.0), vec2(seg_w[i], h - 6.0));
        let resp = ui.interact(r, ui.id().with(("seg", i, rect.min.x as i32, rect.min.y as i32)), Sense::click());
        let on = *value == *val;
        if on {
            painter.add(Shadow { offset: [0, 1], blur: 4, spread: 0, color: Color32::from_black_alpha(30) }.as_shape(r, CornerRadius::same(7)));
            painter.rect_filled(r, CornerRadius::same(7), p.card);
        } else if resp.hovered() {
            painter.rect_filled(r, CornerRadius::same(7), p.hover);
        }
        let col = if on { p.accent } else if resp.hovered() { p.text } else { p.weak };
        let g = ui.painter().layout_no_wrap(options[i].1.to_string(), font.clone(), col);
        painter.galley(r.center() - g.size() / 2.0, g, col);
        if resp.clicked() && !on {
            *value = *val;
            changed = true;
        }
        x += seg_w[i];
    }
    changed
}

/// Zahl mit − und + Knöpfen.
pub fn stepper(ui: &mut Ui, value: &mut u32, min: u32, max: u32) {
    let p = Pal::of(ui);
    let h = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(110.0, h), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(9), if p.dark { p.input } else { p.bg });
    painter.rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
    let bw = 32.0;
    let minus = Rect::from_min_size(rect.min, vec2(bw, h)).shrink(3.0);
    let plus = Rect::from_min_size(pos2(rect.max.x - bw, rect.min.y), vec2(bw, h)).shrink(3.0);
    for (r, sym, delta) in [(minus, "−", -1i32), (plus, "+", 1)] {
        let resp = ui.interact(r, ui.id().with(("step", sym)), Sense::click());
        if resp.hovered() {
            painter.rect_filled(r, CornerRadius::same(7), p.hover);
        }
        let enabled = if delta < 0 { *value > min } else { *value < max };
        painter.text(r.center(), Align2::CENTER_CENTER, sym, FontId::proportional(17.0), if enabled { p.text } else { p.border_strong });
        if resp.clicked() && enabled {
            *value = (*value as i32 + delta) as u32;
        }
    }
    let mid = Rect::from_min_max(pos2(rect.min.x + bw, rect.min.y), pos2(rect.max.x - bw, rect.max.y));
    painter.text(mid.center(), Align2::CENTER_CENTER, value.to_string(), semibold(15.0), p.text);
}

/// Zahlenfeld mit deutschem Komma und Einheit.
pub fn num<'a>(
    v: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    decimals: usize,
    suffix: &str,
) -> egui::DragValue<'a> {
    egui::DragValue::new(v)
        .range(range)
        .speed(speed)
        .suffix(suffix.to_string())
        .custom_formatter(move |x, _| format!("{x:.decimals$}").replace('.', ","))
        .custom_parser(|s| {
            let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '-')).collect();
            cleaned.replace(',', ".").parse().ok()
        })
}

/// Kachel mit maßstäblicher Etikettenform.
pub fn size_tile(ui: &mut Ui, w_mm: f32, h_mm: f32, selected: bool, size: egui::Vec2) -> Response {
    let p = Pal::of(ui);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = CornerRadius::same(9);
        let (fill, stroke) = if selected {
            (p.accent_soft, Stroke::new(1.5_f32, p.accent))
        } else if resp.hovered() {
            (p.hover, Stroke::new(1.0_f32, p.border_strong))
        } else {
            (p.card, Stroke::new(1.0_f32, p.border))
        };
        painter.rect_filled(rect, r, fill);
        painter.rect_stroke(rect, r, stroke, StrokeKind::Inside);
        let k = ((rect.width() - 10.0) / 50.0).min(1.1);
        let shape = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 18.0), vec2(w_mm * k, h_mm * k));
        painter.rect_filled(shape.translate(vec2(0.0, 1.0)), CornerRadius::same(2), Color32::from_black_alpha(25));
        painter.rect_filled(shape, CornerRadius::same(2), PAPER);
        painter.rect_stroke(shape, CornerRadius::same(2), Stroke::new(1.0_f32, if selected { p.accent } else { p.border_strong }), StrokeKind::Outside);
        painter.text(
            pos2(rect.center().x, rect.max.y - 6.0),
            Align2::CENTER_BOTTOM,
            format!("{}×{}", fmt_num(w_mm), fmt_num(h_mm)),
            FontId::proportional(11.5),
            if selected { p.accent } else { p.weak },
        );
    }
    resp
}

/// Große Kachel zum Hinzufügen eines Elements.
pub fn add_tile(ui: &mut Ui, icon: char, text: &str, size: egui::Vec2) -> Response {
    let p = Pal::of(ui);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = CornerRadius::same(10);
        let hov = resp.hovered();
        painter.rect_filled(rect, r, if hov { p.accent_soft } else { p.card });
        painter.rect_stroke(rect, r, Stroke::new(1.0_f32, if hov { p.accent } else { p.border }), StrokeKind::Inside);
        let col = if hov { p.accent } else { p.text };
        painter.text(pos2(rect.center().x, rect.center().y - 8.0), Align2::CENTER_CENTER, icon, icon_font(20.0), col);
        painter.text(pos2(rect.center().x, rect.max.y - 9.0), Align2::CENTER_BOTTOM, text, FontId::proportional(12.5), col);
    }
    resp
}

/// Zeile in der Ebenenliste.
pub fn layer_row(ui: &mut Ui, icon: char, title: &str, sub: &str, selected: bool) -> Response {
    let p = Pal::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = CornerRadius::same(9);
        if selected {
            painter.rect_filled(rect, r, p.accent_soft);
        } else if resp.hovered() {
            painter.rect_filled(rect, r, p.hover);
        }
        let ib = Rect::from_min_size(pos2(rect.min.x + 7.0, rect.center().y - 14.0), vec2(28.0, 28.0));
        painter.rect_filled(ib, CornerRadius::same(7), if selected { p.accent } else if p.dark { p.border } else { p.bg });
        painter.text(ib.center(), Align2::CENTER_CENTER, icon, icon_font(13.0), if selected { Color32::WHITE } else { p.weak });
        let tx = ib.max.x + 10.0;
        painter.text(pos2(tx, rect.center().y - 9.0), Align2::LEFT_CENTER, title, semibold(13.0), p.text);
        let max_w = rect.max.x - tx - 8.0;
        let g = painter.layout(sub.to_string(), FontId::proportional(12.0), p.weak, max_w);
        let g = if g.rows.len() > 1 { painter.layout_no_wrap(truncate(sub, 26), FontId::proportional(12.0), p.weak) } else { g };
        painter.galley(pos2(tx, rect.center().y + 1.0), g, p.weak);
    }
    resp
}

/// Statusanzeige der Verbindung als Pille.
pub enum ChipState<'a> {
    Off,
    Busy(&'a str),
    On { name: &'a str, battery: Option<u8> },
}

pub fn status_chip(ui: &mut Ui, state: ChipState) -> Response {
    let p = Pal::of(ui);
    let (dot, text, battery) = match &state {
        ChipState::Off => (p.border_strong, "Nicht verbunden".to_string(), None),
        ChipState::Busy(t) => (p.warn, t.to_string(), None),
        ChipState::On { name, battery } => (p.ok, name.to_string(), *battery),
    };
    let galley = ui.painter().layout_no_wrap(text, FontId::proportional(13.5), p.text);
    let bat_w = if battery.is_some() { 58.0 } else { 0.0 };
    let size = vec2(galley.size().x + 38.0 + bat_w, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let hov = resp.hovered() && !matches!(state, ChipState::Busy(_));
    painter.rect_filled(rect, CornerRadius::same(15), if hov { p.hover } else if p.dark { p.input } else { p.bg });
    painter.rect_stroke(rect, CornerRadius::same(15), Stroke::new(1.0_f32, if hov { p.border_strong } else { p.border }), StrokeKind::Inside);
    let c = pos2(rect.min.x + 16.0, rect.center().y);
    if let ChipState::Busy(_) = state {
        egui::Spinner::new().size(12.0).color(p.warn).paint_at(ui, Rect::from_center_size(c, vec2(12.0, 12.0)));
    } else {
        painter.circle_filled(c, 7.0, dot.gamma_multiply(0.25));
        painter.circle_filled(c, 4.0, dot);
    }
    painter.galley(pos2(rect.min.x + 28.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
    if let Some(b) = battery {
        let x = rect.max.x - bat_w;
        painter.vline(x, (rect.min.y + 8.0)..=(rect.max.y - 8.0), Stroke::new(1.0_f32, p.border));
        let col = if b < 20 { p.danger } else { p.weak };
        painter.text(pos2(x + 10.0, rect.center().y), Align2::LEFT_CENTER, icon::battery(b), icon_font(14.0), col);
        painter.text(pos2(rect.max.x - 10.0, rect.center().y), Align2::RIGHT_CENTER, format!("{b}%"), FontId::proportional(12.0), col);
    }
    resp
}

/// Signalstärke in 0–4 Balken und als Wort.
pub fn signal_level(rssi: Option<i16>) -> (u8, &'static str) {
    match rssi {
        Some(r) if r >= -60 => (4, "sehr gut"),
        Some(r) if r >= -70 => (3, "gut"),
        Some(r) if r >= -80 => (2, "mittel"),
        Some(_) => (1, "schwach"),
        None => (0, "unbekannt"),
    }
}

/// Zeile in der Druckerauswahl: Symbol, Name, Untertitel, optionales Abzeichen, Signalbalken.
pub fn printer_row(ui: &mut Ui, name: &str, sub: &str, badge: Option<&str>, bars: u8, highlight: bool) -> Response {
    let p = Pal::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = CornerRadius::same(10);
        let hov = resp.hovered();
        let fill = if hov { p.accent_soft } else if highlight { p.hover } else { p.card };
        painter.rect_filled(rect, r, fill);
        painter.rect_stroke(rect, r, Stroke::new(1.0_f32, if hov { p.accent } else { p.border }), StrokeKind::Inside);

        let ib = Rect::from_min_size(pos2(rect.min.x + 12.0, rect.center().y - 17.0), vec2(34.0, 34.0));
        painter.rect_filled(ib, CornerRadius::same(9), if hov { p.accent } else if p.dark { p.border } else { p.bg });
        painter.text(ib.center(), Align2::CENTER_CENTER, icon::PRINT, icon_font(16.0), if hov { Color32::WHITE } else { p.text });

        let tx = ib.max.x + 12.0;
        let title = painter.layout_no_wrap(name.to_string(), semibold(14.5), p.text);
        let title_w = title.size().x;
        painter.galley(pos2(tx, rect.center().y - 18.0), title, p.text);
        if let Some(b) = badge {
            let g = painter.layout_no_wrap(b.to_string(), FontId::proportional(11.0), p.accent);
            let br = Rect::from_min_size(pos2(tx + title_w + 8.0, rect.center().y - 16.0), g.size() + vec2(12.0, 4.0));
            painter.rect_filled(br, CornerRadius::same(6), if hov { p.card } else { p.accent_soft });
            painter.galley(br.min + vec2(6.0, 2.0), g, p.accent);
        }
        painter.text(pos2(tx, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), p.weak);

        // Signalbalken rechts
        let base = pos2(rect.max.x - 18.0, rect.center().y + 8.0);
        for i in 0..4u8 {
            let h = 5.0 + i as f32 * 4.0;
            let x = base.x - (3 - i) as f32 * 6.0;
            let bar = Rect::from_min_max(pos2(x - 2.0, base.y - h), pos2(x + 2.0, base.y));
            let on = i < bars;
            painter.rect_filled(bar, CornerRadius::same(1), if on { p.accent } else { p.border_strong.gamma_multiply(0.6) });
        }
    }
    resp
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Moderner Ein/Aus-Schalter.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let p = Pal::of(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(resp.id, *on);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(11), lerp_color(p.border_strong, p.accent, t));
        let x = rect.left() + 11.0 + (rect.width() - 22.0) * t;
        painter.circle_filled(pos2(x, rect.center().y + 0.5), 8.5, Color32::from_black_alpha(30));
        painter.circle_filled(pos2(x, rect.center().y), 8.0, Color32::WHITE);
    }
    resp
}

/// Zeile mit Beschriftung links und Schalter rechts. Gibt true zurück, wenn umgeschaltet wurde.
pub fn switch_row(ui: &mut Ui, text: &str, on: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = switch(ui, on).changed();
        });
    });
    changed
}

/// Symbolknopf, der an/aus sein kann (Fett, Kursiv).
pub fn icon_toggle(ui: &mut Ui, icon: char, on: &mut bool, tip: &str) -> Response {
    let p = Pal::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 30.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = CornerRadius::same(8);
        if *on {
            painter.rect_filled(rect, r, p.accent_soft);
            painter.rect_stroke(rect, r, Stroke::new(1.0_f32, p.accent), StrokeKind::Inside);
        } else {
            painter.rect_filled(rect, r, if resp.hovered() { p.hover } else { p.card });
            painter.rect_stroke(rect, r, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
        }
        painter.text(rect.center(), Align2::CENTER_CENTER, icon, icon_font(14.0), if *on { p.accent } else { p.text });
    }
    resp.on_hover_text(tip)
}

pub fn fmt_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.01 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v).replace('.', ",")
    }
}

pub fn truncate(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}
