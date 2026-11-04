//! Die Oberfläche.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, CornerRadius, CursorIcon, FontId, Key, KeyboardShortcut, Margin,
    Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureOptions, Ui, Vec2,
};
use serde::{Deserialize, Serialize};

use crate::model::{
    self, BarcodeEl, BarcodeKind, Element, ElementKind, ImageEl, Label, QrEl, TextEl, MAX_H_MM, MAX_W_MM, MIN_H_MM,
    MIN_W_MM, SIZE_PRESETS,
};
use crate::printer::{self, Cmd, Event, FoundPrinter, PrinterHandle};
use crate::protocol::{self, PaperType, PX_PER_MM};
use crate::render::{self, ElemBox, Fonts, ImageCache, Rendered};
use crate::ui_kit::{self as kit, icon, Pal, INK, PAPER};

const FILE_EXT: &str = "etikett";
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp"];

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    label: Label,
    density: u8,
    copies: u32,
    last_printer: Option<String>,
    /// Vorschau als einzelne Druckpunkte statt geglättet
    show_dots: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { label: Label::default(), density: 2, copies: 1, last_printer: None, show_dots: false }
    }
}

#[derive(Clone, PartialEq)]
enum Conn {
    Off,
    Searching,
    /// Auswahlliste ist offen
    Picking,
    Connecting(String),
    On(String),
}

#[derive(Clone, Copy, PartialEq)]
enum MsgKind {
    Info,
    Ok,
    Error,
}

struct Toast {
    text: String,
    kind: MsgKind,
    since: f64,
}

struct Drag {
    idx: usize,
    /// Linke obere Ecke beim Start, in Druckpunkten
    start: Vec2,
    /// Bisherige Verschiebung in Druckpunkten
    total: Vec2,
    size: Vec2,
}

enum ListAction {
    Delete,
    Duplicate,
    Forward,
    Backward,
}

pub struct App {
    s: Settings,
    selected: Option<usize>,
    fonts: Fonts,
    images: ImageCache,
    rendered: Option<Rendered>,
    rendered_for: Option<Label>,
    /// Druckbild als Textur (Ansicht „Druckpunkte“)
    texture: Option<egui::TextureHandle>,
    /// Geglättete Vorschau und wofür sie gezeichnet wurde
    preview_tex: Option<egui::TextureHandle>,
    preview_for: Option<(Label, u32)>,
    logo: egui::TextureHandle,
    printer: PrinterHandle,
    conn: Conn,
    battery: Option<u8>,
    /// Gefundene Drucker für die Auswahlliste
    choices: Vec<FoundPrinter>,
    /// Drucker, der beim Öffnen der Auswahl verbunden war
    conn_before_pick: Option<String>,
    last_battery_check: f64,
    printing: bool,
    progress: f32,
    toast: Option<Toast>,
    pending_toast: Option<(MsgKind, String)>,
    log: VecDeque<String>,
    show_log: bool,
    show_about: bool,
    file: Option<PathBuf>,
    drag: Option<Drag>,
    guides: (bool, bool),
    startup_connect: bool,
    /// Nur für Entwicklung: Bildschirmfoto speichern und beenden (ETIKETT_SCREENSHOT=pfad.png)
    screenshot: Option<(PathBuf, u32)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        kit::setup(&cc.egui_ctx);
        if let Ok(t) = std::env::var("ETIKETT_THEME") {
            cc.egui_ctx.set_theme(if t == "dunkel" { egui::Theme::Dark } else { egui::Theme::Light });
        }

        let screenshot = std::env::var_os("ETIKETT_SCREENSHOT").map(|p| (PathBuf::from(p), 0));
        let mut s: Settings = if screenshot.is_some() {
            Settings::default()
        } else {
            cc.storage.and_then(|st| eframe::get_value(st, eframe::APP_KEY)).unwrap_or_default()
        };
        if std::env::var_os("ETIKETT_DEMO").is_some() {
            s.label = demo_label();
        }

        let ctx = cc.egui_ctx.clone();
        let printer = printer::spawn(move || ctx.request_repaint());
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([64, 64], &crate::icon::draw_icon(64)),
            TextureOptions::LINEAR,
        );

        let mut app = Self {
            s,
            selected: None,
            fonts: Fonts::load(),
            images: ImageCache::default(),
            rendered: None,
            rendered_for: None,
            texture: None,
            preview_tex: None,
            preview_for: None,
            logo,
            printer,
            conn: Conn::Off,
            battery: None,
            choices: Vec::new(),
            conn_before_pick: None,
            last_battery_check: 0.0,
            printing: false,
            progress: 0.0,
            toast: None,
            pending_toast: None,
            log: VecDeque::new(),
            show_log: false,
            show_about: false,
            file: None,
            drag: None,
            guides: (false, false),
            startup_connect: false,
            screenshot,
        };
        if !app.s.label.elements.is_empty() {
            app.selected = Some(0);
        }
        // Nur für Bildschirmfotos während der Entwicklung
        if let Ok(n) = std::env::var("ETIKETT_SELECT").map(|s| s.parse::<usize>().unwrap_or(0)) {
            app.selected = Some(n);
        }
        if std::env::var("ETIKETT_DEMO").as_deref() == Ok("auswahl") {
            app.conn = Conn::Picking;
            app.s.last_printer = Some("P15_8C3F".into());
            app.choices = vec![
                FoundPrinter { address: "C4:2E:91:5A:8C:3F".into(), name: "P15_8C3F".into(), rssi: Some(-58) },
                FoundPrinter { address: "C4:2E:91:77:21:B0".into(), name: "P15_21B0".into(), rssi: Some(-74) },
                FoundPrinter { address: "DC:0D:30:12:4E:9A".into(), name: "P15S_4E9A".into(), rssi: Some(-86) },
            ];
        }
        if std::env::var("ETIKETT_DEMO").as_deref() == Ok("verbunden") {
            app.conn = Conn::On("P15_8C3F".into());
            app.battery = Some(76);
            app.notify(MsgKind::Ok, "Verbunden mit P15_8C3F");
        }
        // Zuletzt benutzten Drucker automatisch verbinden
        if app.screenshot.is_none() {
            if let Some(name) = app.s.last_printer.clone() {
                app.startup_connect = true;
                app.conn = Conn::Searching;
                app.printer.send(Cmd::Connect { preferred: Some(name) });
            }
        }
        app
    }

    fn notify(&mut self, kind: MsgKind, text: impl Into<String>) {
        self.pending_toast = Some((kind, text.into()));
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() > 400 {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Suche starten und immer die Auswahlliste zeigen.
    fn choose_printer(&mut self) {
        self.conn_before_pick = match &self.conn {
            Conn::On(n) => Some(n.clone()),
            _ => None,
        };
        self.conn = Conn::Picking;
        self.choices.clear();
        self.toast = None;
        self.printer.send(Cmd::Choose);
    }

    fn picker_window(&mut self, ctx: &egui::Context) {
        let p = Pal::get(ctx.style().visuals.dark_mode);
        let frame = egui::Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(22))
            .shadow(Shadow { offset: [0, 16], blur: 40, spread: 0, color: Color32::from_black_alpha(if p.dark { 150 } else { 60 }) });
        let resp = egui::Modal::new(egui::Id::new("druckerwahl"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(if p.dark { 140 } else { 70 }))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                let mut picked: Option<String> = None;
                let mut keep_current = false;
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 20.0, p.accent_soft);
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, icon::BLUETOOTH, kit::icon_font(17.0), p.accent);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new("Drucker auswählen").font(kit::semibold(18.0)).color(p.text));
                        let hint = if self.choices.len() + self.conn_before_pick.is_some() as usize > 1 {
                            "Mehrere Drucker in der Nähe – welcher soll es sein?"
                        } else {
                            "Wähle den Drucker, mit dem du drucken möchtest."
                        };
                        ui.label(RichText::new(hint).size(13.0).color(p.weak));
                    });
                });
                ui.add_space(10.0);

                ui.spacing_mut().item_spacing.y = 8.0;
                if let Some(cur) = &self.conn_before_pick {
                    let sub = "Ist gerade verbunden";
                    if kit::printer_row(ui, cur, sub, Some("Verbunden"), 4, true).on_hover_text("Verbunden bleiben").clicked() {
                        keep_current = true;
                    }
                }
                for f in &self.choices {
                    let (bars, word) = kit::signal_level(f.rssi);
                    let short_addr: String = f.address.chars().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect();
                    let sub = format!("Signal {word}  ·  …{short_addr}");
                    let badge = (self.s.last_printer.as_deref() == Some(f.name.as_str())).then_some("Zuletzt benutzt");
                    let resp = kit::printer_row(ui, &f.name, &sub, badge, bars, false)
                        .on_hover_text(format!("Bluetooth-Adresse {}", f.address));
                    if resp.clicked() {
                        picked = Some(f.address.clone());
                    }
                }
                if self.choices.is_empty() && self.conn_before_pick.is_none() {
                    ui.add_space(14.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(22.0).color(p.accent));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Suche Drucker in der Nähe …").color(p.weak));
                    });
                    ui.add_space(14.0);
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(12.0).color(p.weak));
                    ui.label(
                        RichText::new("Suche läuft weiter – eingeschaltete Drucker erscheinen automatisch.")
                            .size(12.0)
                            .color(p.weak),
                    );
                });
                ui.add_space(4.0);
                let mut cancel = false;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if kit::tool_button(ui, icon::CLOSE, "Abbrechen").clicked() {
                        cancel = true;
                    }
                });
                (picked, keep_current || cancel)
            });

        let should_close = resp.should_close();
        let (picked, close) = resp.inner;
        if let Some(addr) = picked {
            let name = self.choices.iter().find(|f| f.address == addr).map(|f| f.name.clone()).unwrap_or_default();
            self.printer.send(Cmd::ConnectTo(addr));
            self.conn = Conn::Connecting(name);
            self.choices.clear();
        } else if close || should_close {
            self.printer.send(Cmd::CancelChoice);
            self.close_picker();
        }
    }

    /// Auswahl schließen und den vorherigen Zustand wiederherstellen.
    fn close_picker(&mut self) {
        self.choices.clear();
        self.conn = match self.conn_before_pick.take() {
            Some(n) => Conn::On(n),
            None => Conn::Off,
        };
    }

    fn connect(&mut self) {
        self.startup_connect = false;
        self.conn = Conn::Searching;
        self.toast = None;
        self.printer.send(Cmd::Connect { preferred: self.s.last_printer.clone() });
    }

    fn connected(&self) -> bool {
        matches!(self.conn, Conn::On(_))
    }

    fn can_print(&self) -> bool {
        self.connected() && !self.printing && self.rendered.is_some()
    }

    fn print(&mut self) {
        let Some(r) = &self.rendered else { return };
        let (rot, w, h) = protocol::rotate_cw(&r.pixels, r.width, r.height);
        let bmp = protocol::pack_bits(&rot, w, h);
        let job = protocol::build_print_job(&bmp, self.s.density, self.s.label.paper);
        self.printer.send(Cmd::Print { job, copies: self.s.copies.max(1) });
        self.printing = true;
        self.progress = 0.0;
        self.toast = None;
    }

    // ------------------------------------------------------------ Ereignisse

    fn handle_printer_events(&mut self) {
        while let Some(ev) = self.printer.poll() {
            match ev {
                Event::Searching => self.conn = Conn::Searching,
                Event::Choices(list) => {
                    if self.conn != Conn::Picking {
                        // Automatische Suche hat mehrere Drucker gefunden
                        self.conn_before_pick = None;
                        self.conn = Conn::Picking;
                        self.startup_connect = false;
                    }
                    self.choices = list;
                }
                Event::ChoiceClosed => {
                    if self.conn == Conn::Picking {
                        self.close_picker();
                    }
                }
                Event::Connecting(n) => self.conn = Conn::Connecting(n),
                Event::Connected(n) => {
                    self.push_log(format!("Verbunden mit {n}"));
                    self.notify(MsgKind::Ok, format!("Verbunden mit {n}"));
                    self.conn = Conn::On(n.clone());
                    self.conn_before_pick = None;
                    self.choices.clear();
                    self.s.last_printer = Some(n);
                    self.startup_connect = false;
                }
                Event::Disconnected => {
                    if self.connected() {
                        self.conn = Conn::Off;
                        self.battery = None;
                        if !self.printing {
                            self.notify(MsgKind::Info, "Drucker getrennt.");
                        }
                    } else if self.conn == Conn::Picking {
                        // Der bisher verbundene Drucker ist während der Auswahl weggefallen
                        self.conn_before_pick = None;
                        self.battery = None;
                    }
                }
                Event::Battery(b) => self.battery = Some(b),
                Event::Status(s) => {
                    self.push_log(format!("Status: {s}"));
                    let kind = if s.contains("geschlossen") { MsgKind::Info } else { MsgKind::Error };
                    self.notify(kind, s);
                }
                Event::Progress { sent, total } => {
                    self.progress = if total > 0 { sent as f32 / total as f32 } else { 0.0 };
                }
                Event::PrintDone(Ok(())) => {
                    self.printing = false;
                    self.progress = 1.0;
                    let n = self.s.copies.max(1);
                    self.notify(
                        MsgKind::Ok,
                        if n == 1 { "Etikett gedruckt.".to_string() } else { format!("{n} Etiketten gedruckt.") },
                    );
                }
                Event::PrintDone(Err(e)) => {
                    self.printing = false;
                    self.push_log(format!("Druckfehler: {e}"));
                    self.notify(MsgKind::Error, e);
                }
                Event::Error(e) => {
                    self.push_log(format!("Fehler: {e}"));
                    if matches!(self.conn, Conn::Searching | Conn::Connecting(_) | Conn::Picking) {
                        self.conn = Conn::Off;
                        self.choices.clear();
                    }
                    if self.startup_connect {
                        self.notify(MsgKind::Info, "Drucker nicht gefunden. Einschalten und auf „Verbinden“ klicken.");
                    } else {
                        self.notify(MsgKind::Error, e);
                    }
                    self.startup_connect = false;
                }
                Event::Log(s) => self.push_log(s),
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        if ctx.input_mut(|i| i.consume_shortcut(&ctrl(Key::P))) && self.can_print() {
            self.print();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&ctrl(Key::S))) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&ctrl(Key::O))) {
            self.open_dialog();
        }

        // Tasten für das ausgewählte Element, nur wenn kein Eingabefeld aktiv ist
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let Some(idx) = self.selected.filter(|&i| i < self.s.label.elements.len()) else { return };
        if ctx.input(|i| i.key_pressed(Key::Delete)) {
            self.list_action(idx, ListAction::Delete);
            return;
        }
        let (step, l, r, u, d) = ctx.input(|i| {
            (
                if i.modifiers.shift { 1.0 } else { 0.25 },
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
            )
        });
        if l || r || u || d {
            let el = &mut self.s.label.elements[idx];
            if matches!(&el.kind, ElementKind::Text(t) if t.fit) {
                return;
            }
            el.x_mm += step * ((r as i32 - l as i32) as f32);
            el.y_mm += step * ((d as i32 - u as i32) as f32);
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let Some(path) = f.path else { continue };
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if ext == FILE_EXT {
                self.open_file(&path);
            } else if IMAGE_EXTS.contains(&ext.as_str()) {
                self.add_image(&path);
            }
        }
    }

    // ------------------------------------------------------------ Dateien

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Etikett-Vorlage", &[FILE_EXT]).pick_file() {
            self.open_file(&p);
        }
    }

    fn open_file(&mut self, path: &Path) {
        let res = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Label>(&s).map_err(|e| e.to_string()));
        match res {
            Ok(label) => {
                self.s.label = label;
                self.selected = if self.s.label.elements.is_empty() { None } else { Some(0) };
                self.file = Some(path.to_path_buf());
                self.notify(MsgKind::Ok, format!("Geöffnet: {}", file_name(path)));
            }
            Err(e) => self.notify(MsgKind::Error, format!("Datei konnte nicht geöffnet werden: {e}")),
        }
    }

    fn save(&mut self, save_as: bool) {
        let path = match (&self.file, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("Etikett-Vorlage", &[FILE_EXT])
                .set_file_name(format!("etikett.{FILE_EXT}"))
                .save_file(),
        };
        let Some(mut path) = path else { return };
        if path.extension().is_none() {
            path.set_extension(FILE_EXT);
        }
        let json = serde_json::to_string_pretty(&self.s.label).expect("Etikett als JSON");
        match std::fs::write(&path, json) {
            Ok(()) => {
                self.notify(MsgKind::Ok, format!("Gespeichert: {}", file_name(&path)));
                self.file = Some(path);
            }
            Err(e) => self.notify(MsgKind::Error, format!("Speichern fehlgeschlagen: {e}")),
        }
    }

    // ------------------------------------------------------------ Elemente

    fn add_element(&mut self, el: Element) {
        self.s.label.elements.push(el);
        self.selected = Some(self.s.label.elements.len() - 1);
    }

    fn add_text(&mut self) {
        let first = !self.s.label.elements.iter().any(|e| matches!(&e.kind, ElementKind::Text(t) if t.fit));
        let el = Element {
            x_mm: 1.0,
            y_mm: 1.0,
            kind: ElementKind::Text(TextEl {
                text: "Text".into(),
                font: self.fonts.default_family().to_string(),
                fit: first,
                size_pt: 12.0,
                align: if first { model::Align::Center } else { model::Align::Left },
                ..Default::default()
            }),
        };
        self.add_element(el);
    }

    fn add_qr(&mut self) {
        let size = (self.s.label.height_mm - 2.0).clamp(4.0, 30.0);
        self.add_element(Element {
            x_mm: (self.s.label.width_mm - size - 1.0).max(0.0),
            y_mm: (self.s.label.height_mm - size) / 2.0,
            kind: ElementKind::Qr(QrEl { size_mm: size, ..Default::default() }),
        });
    }

    fn add_barcode(&mut self) {
        let h = (self.s.label.height_mm - 5.0).max(3.0);
        self.add_element(Element {
            x_mm: 2.0,
            y_mm: 1.0,
            kind: ElementKind::Barcode(BarcodeEl { height_mm: h, ..Default::default() }),
        });
    }

    fn add_image_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Bilder", IMAGE_EXTS).pick_file() {
            self.add_image(&p);
        }
    }

    fn add_image(&mut self, path: &Path) {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => return self.notify(MsgKind::Error, format!("Bild nicht lesbar: {e}")),
        };
        let Ok(img) = image::load_from_memory(&data) else {
            return self.notify(MsgKind::Error, "Dieses Bildformat wird nicht unterstützt.");
        };
        let l = &self.s.label;
        let max_h = (l.height_mm - 2.0).max(2.0);
        let max_w = (l.width_mm - 2.0).max(2.0);
        let aspect = img.width().max(1) as f32 / img.height().max(1) as f32;
        let width_mm = (max_h * aspect).min(max_w);
        let height_mm = width_mm / aspect;
        // Fotos rastern, Grafiken scharf lassen
        let photo = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"));
        self.add_element(Element {
            x_mm: 1.0,
            y_mm: ((l.height_mm - height_mm) / 2.0).max(0.0),
            kind: ElementKind::Image(ImageEl { data: Arc::new(data), width_mm, dither: photo, ..Default::default() }),
        });
    }

    fn list_action(&mut self, idx: usize, action: ListAction) {
        let els = &mut self.s.label.elements;
        if idx >= els.len() {
            return;
        }
        match action {
            ListAction::Delete => {
                els.remove(idx);
                self.selected = if els.is_empty() { None } else { Some(idx.min(els.len() - 1)) };
            }
            ListAction::Duplicate => {
                let mut el = els[idx].clone();
                if let ElementKind::Text(t) = &mut el.kind {
                    t.fit = false;
                }
                el.x_mm += 1.0;
                el.y_mm += 1.0;
                els.insert(idx + 1, el);
                self.selected = Some(idx + 1);
            }
            ListAction::Forward => {
                if idx + 1 < els.len() {
                    els.swap(idx, idx + 1);
                    self.selected = Some(idx + 1);
                }
            }
            ListAction::Backward => {
                if idx > 0 {
                    els.swap(idx, idx - 1);
                    self.selected = Some(idx - 1);
                }
            }
        }
    }

    // ------------------------------------------------------------ Darstellung

    fn ensure_rendered(&mut self, ctx: &egui::Context) {
        if self.rendered_for.as_ref() == Some(&self.s.label) {
            return;
        }
        let r = render::render(&self.s.label, &self.fonts, &mut self.images);
        let mut img = egui::ColorImage::new([r.width, r.height], PAPER);
        for (dst, &black) in img.pixels.iter_mut().zip(&r.pixels) {
            if black {
                *dst = INK;
            }
        }
        match &mut self.texture {
            Some(t) => t.set(img, TextureOptions::NEAREST),
            None => self.texture = Some(ctx.load_texture("etikett", img, TextureOptions::NEAREST)),
        }
        self.rendered = Some(r);
        self.rendered_for = Some(self.s.label.clone());
    }

    /// Zeichnet die geglättete Vorschau neu, wenn sich Etikett oder Zoom geändert haben.
    fn ensure_preview(&mut self, ctx: &egui::Context, s: u32) {
        if let Some((l, ps)) = &self.preview_for {
            if *ps == s && *l == self.s.label {
                return;
            }
        }
        let pv = render::render_preview(&self.s.label, &self.fonts, &mut self.images, s);
        let mut img = egui::ColorImage::new([pv.width, pv.height], PAPER);
        let (pr, pg, pb) = (PAPER.r() as f32, PAPER.g() as f32, PAPER.b() as f32);
        let (ir, ig, ib) = (INK.r() as f32, INK.g() as f32, INK.b() as f32);
        for (dst, &v) in img.pixels.iter_mut().zip(&pv.ink) {
            if v > 0.0 {
                let v = v.min(1.0);
                *dst = Color32::from_rgb(
                    (pr + (ir - pr) * v) as u8,
                    (pg + (ig - pg) * v) as u8,
                    (pb + (ib - pb) * v) as u8,
                );
            }
        }
        let opts = TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification: egui::TextureFilter::Linear,
            mipmap_mode: Some(egui::TextureFilter::Linear),
            ..Default::default()
        };
        match &mut self.preview_tex {
            Some(t) => t.set(img, opts),
            None => self.preview_tex = Some(ctx.load_texture("vorschau", img, opts)),
        }
        self.preview_for = Some((self.s.label.clone(), s));
    }

    fn header(&mut self, ctx: &egui::Context) {
        let p = Pal::get(ctx.style().visuals.dark_mode);
        let frame = egui::Frame::new().fill(p.card).inner_margin(Margin { left: 16, right: 12, top: 10, bottom: 10 });
        egui::TopBottomPanel::top("kopf").frame(frame).show_separator_line(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Logo und Name
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                ui.painter().image(self.logo.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new("Etikettendrucker").font(kit::semibold(16.0)).color(p.text));
                    let sub = self.file.as_deref().map(file_name).unwrap_or_else(|| "Pristar · Marklife P15".into());
                    ui.label(RichText::new(sub).size(12.0).color(p.weak));
                });
                ui.add_space(14.0);
                let (sep, _) = ui.allocate_exact_size(vec2(1.0, 28.0), Sense::hover());
                ui.painter().rect_filled(sep, CornerRadius::ZERO, p.border);
                ui.add_space(6.0);

                ui.spacing_mut().item_spacing.x = 2.0;
                if kit::tool_button(ui, icon::NEW, "Neu").on_hover_text("Neues Etikett").clicked() {
                    self.s.label = Label::default();
                    self.selected = Some(0);
                    self.file = None;
                }
                if kit::tool_button(ui, icon::OPEN, "Öffnen").on_hover_text("Vorlage öffnen (Strg+O)").clicked() {
                    self.open_dialog();
                }
                if kit::tool_button(ui, icon::SAVE, "Speichern").on_hover_text("Vorlage speichern (Strg+S)").clicked() {
                    self.save(false);
                }
                if kit::tool_button(ui, icon::SAVE_AS, "").on_hover_text("Speichern unter …").clicked() {
                    self.save(true);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if kit::icon_button(ui, icon::INFO, None).on_hover_text("Über diese App").clicked() {
                        self.show_about = !self.show_about;
                    }
                    if kit::icon_button(ui, icon::LIST, if self.show_log { Some(p.accent) } else { None })
                        .on_hover_text("Protokoll (für Fehlersuche)")
                        .clicked()
                    {
                        self.show_log = !self.show_log;
                    }
                    ui.add_space(6.0);
                    match self.conn.clone() {
                        Conn::Off => {
                            if kit::primary_button(ui, icon::BLUETOOTH, "Verbinden", vec2(124.0, 32.0), true, None)
                                .on_hover_text("Drucker per Bluetooth suchen und verbinden")
                                .clicked()
                            {
                                self.connect();
                            }
                            if kit::status_chip(ui, kit::ChipState::Off)
                                .on_hover_text("Klicken, um einen Drucker aus einer Liste zu wählen")
                                .clicked()
                            {
                                self.choose_printer();
                            }
                        }
                        Conn::Searching => {
                            kit::status_chip(ui, kit::ChipState::Busy("Suche Drucker …"));
                        }
                        Conn::Picking => {
                            kit::status_chip(ui, kit::ChipState::Busy("Drucker auswählen …"));
                        }
                        Conn::Connecting(n) => {
                            kit::status_chip(ui, kit::ChipState::Busy(&format!("Verbinde mit {n} …")));
                        }
                        Conn::On(name) => {
                            if kit::icon_button(ui, icon::DISCONNECT, None).on_hover_text("Verbindung trennen").clicked() {
                                self.printer.send(Cmd::Disconnect);
                            }
                            if kit::status_chip(ui, kit::ChipState::On { name: &name, battery: self.battery })
                                .on_hover_text("Verbunden über Bluetooth – klicken, um einen anderen Drucker zu wählen")
                                .clicked()
                                && !self.printing
                            {
                                self.choose_printer();
                            }
                        }
                    }
                });
            });
        });
        // feine Linie unter dem Kopf
        egui::TopBottomPanel::top("kopf-linie")
            .exact_height(1.0)
            .frame(egui::Frame::new().fill(p.border))
            .show_separator_line(false)
            .show(ctx, |_| {});
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        let p = Pal::get(ctx.style().visuals.dark_mode);
        let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::same(12));
        egui::SidePanel::left("links").resizable(false).exact_width(300.0).frame(frame).show_separator_line(false).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                kit::card(ui, |ui| {
                    kit::card_title(ui, icon::NEW, "Etikett");
                    kit::field_label(ui, "Größe in mm (Breite × Höhe)");
                    let l = &mut self.s.label;
                    let gap = 6.0;
                    let tile_w = ((ui.available_width() - 3.0 * gap) / 4.0).floor();
                    for row in SIZE_PRESETS.chunks(4) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for &(w, h) in row {
                                let on = (w - l.width_mm).abs() < 0.01 && (h - l.height_mm).abs() < 0.01;
                                if kit::size_tile(ui, w, h, on, vec2(tile_w, 50.0)).clicked() {
                                    l.width_mm = w;
                                    l.height_mm = h;
                                }
                            }
                        });
                        ui.add_space(-4.0);
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        kit::field_label(ui, "Eigene:");
                        ui.add_sized([80.0, 28.0], kit::num(&mut l.width_mm, MIN_W_MM..=MAX_W_MM, 0.5, 1, " mm"))
                            .on_hover_text("Breite");
                        ui.label(RichText::new("×").color(p.weak));
                        ui.add_sized([80.0, 28.0], kit::num(&mut l.height_mm, MIN_H_MM..=MAX_H_MM, 0.2, 1, " mm"))
                            .on_hover_text("Höhe");
                    });
                    ui.add_space(4.0);
                    kit::field_label(ui, "Papier");
                    kit::segmented(
                        ui,
                        &mut l.paper,
                        &[(PaperType::Gap, "Etiketten mit Lücke"), (PaperType::Continuous, "Endlos")],
                        false,
                    );
                    ui.add_space(4.0);
                    kit::switch_row(ui, "Rahmen", &mut l.frame.enabled);
                    if l.frame.enabled {
                        egui::Grid::new("rahmen").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            kit::field_label(ui, "Linie");
                            ui.add(egui::DragValue::new(&mut l.frame.thickness_px).range(1..=8).suffix(" Punkte"));
                            ui.end_row();
                            kit::field_label(ui, "Abstand");
                            ui.add(kit::num(&mut l.frame.inset_mm, 0.0..=4.0, 0.05, 1, " mm"));
                            ui.end_row();
                            kit::field_label(ui, "Ecken");
                            ui.add(kit::num(&mut l.frame.radius_mm, 0.0..=6.0, 0.05, 1, " mm"));
                            ui.end_row();
                        });
                    }
                });

                kit::card(ui, |ui| {
                    kit::card_title(ui, icon::ADD, "Hinzufügen");
                    let gap = 8.0;
                    let w = ((ui.available_width() - gap) / 2.0).floor();
                    let size = vec2(w, 60.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        if kit::add_tile(ui, icon::TEXT, "Text", size).clicked() {
                            self.add_text();
                        }
                        if kit::add_tile(ui, icon::QR, "QR-Code", size).clicked() {
                            self.add_qr();
                        }
                    });
                    ui.add_space(-2.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        if kit::add_tile(ui, icon::BARCODE, "Barcode", size).clicked() {
                            self.add_barcode();
                        }
                        if kit::add_tile(ui, icon::IMAGE, "Bild", size).on_hover_text("Bild wählen oder ins Fenster ziehen").clicked() {
                            self.add_image_dialog();
                        }
                    });
                });

                kit::card(ui, |ui| {
                    kit::card_title(ui, icon::LAYERS, "Ebenen");
                    if self.s.label.elements.is_empty() {
                        ui.label(RichText::new("Noch leer – füge oben etwas hinzu.").color(p.weak));
                    }
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for i in (0..self.s.label.elements.len()).rev() {
                        let el = &self.s.label.elements[i];
                        let sub = el.summary();
                        let sub = if sub.is_empty() { "(leer)".to_string() } else { sub };
                        if kit::layer_row(ui, element_icon(el), el.type_name(), &sub, self.selected == Some(i)).clicked() {
                            self.selected = Some(i);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Tipp: Elemente in der Vorschau mit der Maus verschieben, mit den Pfeiltasten feinjustieren. Bilder einfach ins Fenster ziehen.")
                        .size(12.0)
                        .color(p.weak),
                );
            });
        });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        let p = Pal::get(ctx.style().visuals.dark_mode);
        let frame = egui::Frame::new().fill(p.bg).inner_margin(Margin::same(12));
        egui::SidePanel::right("rechts").resizable(false).exact_width(320.0).frame(frame).show_separator_line(false).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let Some(idx) = self.selected.filter(|&i| i < self.s.label.elements.len()) else {
                    kit::card(ui, |ui| {
                        kit::card_title(ui, icon::MOVE, "Eigenschaften");
                        ui.label(RichText::new("Klicke ein Element in der Vorschau oder in der Ebenenliste an, um es zu bearbeiten.").color(p.weak));
                    });
                    return;
                };
                let bx = self.rendered.as_ref().and_then(|r| r.boxes.get(idx).copied());
                let label_px = render::label_px(&self.s.label);
                let fonts = &self.fonts;
                let label = &mut self.s.label;
                let margin = &mut label.margin_mm;
                let el = &mut label.elements[idx];
                let action = kit::card(ui, |ui| element_props(ui, el, fonts, bx, label_px, margin));
                if let Some(a) = action {
                    self.list_action(idx, a);
                }
            });
        });
    }

    fn center(&mut self, ctx: &egui::Context) {
        let p = Pal::get(ctx.style().visuals.dark_mode);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.canvas)).show(ctx, |ui| {
            egui::TopBottomPanel::bottom("druckleiste")
                .frame(egui::Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }))
                .show_separator_line(false)
                .show_inside(ui, |ui| self.print_bar(ui));
            egui::CentralPanel::default().frame(egui::Frame::new()).show_inside(ui, |ui| self.canvas(ui));
        });
    }

    fn print_bar(&mut self, ui: &mut Ui) {
        kit::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    kit::field_label(ui, "Druckstärke");
                    kit::segmented(ui, &mut self.s.density, &[(1, "Hell"), (2, "Normal"), (3, "Dunkel")], false);
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    kit::field_label(ui, "Anzahl");
                    kit::stepper(ui, &mut self.s.copies, 1, 99);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let size = vec2(200.0, 46.0);
                    match &self.conn {
                        _ if self.printing => {
                            let pct = (self.progress * 100.0).round() as u32;
                            kit::primary_button(ui, icon::PRINT, &format!("Drucke … {pct} %"), size, false, Some(self.progress));
                        }
                        Conn::On(_) => {
                            let n = self.s.copies.max(1);
                            let text = if n == 1 { "Drucken".to_string() } else { format!("{n}× drucken") };
                            let resp = kit::primary_button(ui, icon::PRINT, &text, size, self.can_print(), None);
                            if resp.on_hover_text("Strg+P").clicked() {
                                self.print();
                            }
                        }
                        Conn::Off => {
                            // Ohne Verbindung verbindet der Knopf erst einmal
                            let resp = kit::primary_button(ui, icon::BLUETOOTH, "Drucker verbinden", size, true, None);
                            if resp.on_hover_text("Drucker einschalten, dann hier klicken").clicked() {
                                self.connect();
                            }
                        }
                        Conn::Picking => {
                            kit::primary_button(ui, icon::BLUETOOTH, "Drucker wählen …", size, false, None);
                        }
                        Conn::Searching | Conn::Connecting(_) => {
                            kit::primary_button(ui, icon::BLUETOOTH, "Suche Drucker …", size, false, None);
                        }
                    }
                });
            });
        });
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let p = Pal::of(ui);
        let area = ui.available_rect_before_wrap();
        let painter = ui.painter_at(area);

        // Punktraster als Hintergrund
        let step = 20.0;
        let mut y = area.min.y + step / 2.0;
        while y < area.max.y {
            let mut x = area.min.x + step / 2.0;
            while x < area.max.x {
                painter.circle_filled(pos2(x, y), 1.0, p.grid);
                x += step;
            }
            y += step;
        }

        if self.rendered.is_none() {
            return;
        }
        let (lw_px, lh_px) = render::label_px(&self.s.label);
        let ruler = 26.0;
        let pad = 40.0;
        let scale = ((area.width() - 2.0 * pad - ruler) / lw_px as f32)
            .min((area.height() - 2.0 * pad - ruler - 120.0) / lh_px as f32)
            .clamp(0.5, 10.0);
        let size = vec2(lw_px as f32 * scale, lh_px as f32 * scale);
        let rect = Rect::from_center_size(area.center() + vec2(ruler / 2.0, ruler / 2.0 - 40.0), size);

        // Umschalter: geglättete Vorschau oder einzelne Druckpunkte
        let toggle_row = Rect::from_min_max(pos2(area.min.x, rect.max.y + 48.0), pos2(area.max.x, rect.max.y + 82.0));
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(toggle_row).layout(egui::Layout::top_down(egui::Align::Center)),
            |ui| {
                kit::segmented(ui, &mut self.s.show_dots, &[(false, "Glatt"), (true, "Druckpunkte")], false);
            },
        );

        // Vorschau in der Auflösung, in der sie gerade angezeigt wird
        if !self.s.show_dots {
            let ppp = ui.ctx().pixels_per_point();
            let max_side = ui.ctx().input(|i| i.max_texture_side).max(2048);
            let limit = (max_side / lw_px.max(lh_px)).max(1) as u32;
            let s = ((scale * ppp).ceil() as u32).clamp(1, 8).min(limit);
            self.ensure_preview(ui.ctx(), s);
        }
        let tex = if self.s.show_dots { &self.texture } else { &self.preview_tex };
        let (Some(r), Some(tex)) = (&self.rendered, tex) else { return };
        let resp = ui.interact(rect, ui.id().with("vorschau"), Sense::click_and_drag());
        let l = &self.s.label;

        // Lineale
        let tick = Stroke::new(1.0_f32, p.weak.gamma_multiply(0.6));
        let px_mm = PX_PER_MM * scale;
        let y0 = rect.min.y - 10.0;
        let mut mm = 0;
        while mm as f32 <= l.width_mm + 0.01 {
            let x = rect.min.x + mm as f32 * px_mm;
            let len = if mm % 10 == 0 { 9.0 } else if mm % 5 == 0 { 6.0 } else { 3.0 };
            if px_mm >= 3.0 || mm % 5 == 0 {
                painter.vline(x, (y0 - len)..=y0, tick);
            }
            if mm % 10 == 0 {
                painter.text(pos2(x + 3.0, y0 - 9.0), Align2::LEFT_BOTTOM, mm.to_string(), FontId::proportional(10.5), p.weak);
            }
            mm += 1;
        }
        let x0 = rect.min.x - 10.0;
        let mut mm = 0;
        while mm as f32 <= l.height_mm + 0.01 {
            let y = rect.min.y + mm as f32 * px_mm;
            let len = if mm % 10 == 0 { 9.0 } else if mm % 5 == 0 { 6.0 } else { 3.0 };
            if px_mm >= 3.0 || mm % 5 == 0 {
                painter.hline((x0 - len)..=x0, y, tick);
            }
            if mm % 5 == 0 && mm > 0 {
                painter.text(pos2(x0 - 11.0, y), Align2::RIGHT_CENTER, mm.to_string(), FontId::proportional(10.5), p.weak);
            }
            mm += 1;
        }

        // Etikett mit Schatten und runden Ecken
        let cr = (1.2 * px_mm).min(14.0).max(2.0) as u8;
        painter.add(Shadow { offset: [0, 8], blur: 26, spread: 0, color: Color32::from_black_alpha(if p.dark { 140 } else { 45 }) }.as_shape(rect, CornerRadius::same(cr)));
        egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), size))
            .corner_radius(CornerRadius::same(cr))
            .paint_at(ui, rect);
        painter.rect_stroke(rect, CornerRadius::same(cr), Stroke::new(1.0_f32, Color32::from_black_alpha(30)), StrokeKind::Outside);

        let to_screen = |b: &ElemBox| {
            Rect::from_min_size(rect.min + vec2(b.x, b.y) * scale, vec2(b.w.max(1.0), b.h.max(1.0)) * scale).expand(4.0)
        };
        let hit = |pos: Pos2| -> Option<usize> { (0..r.boxes.len()).rev().find(|&i| to_screen(&r.boxes[i]).contains(pos)) };

        // Hilfslinien beim Zentrieren
        if self.drag.is_some() {
            let guide = Stroke::new(1.0_f32, p.guide);
            if self.guides.0 {
                painter.vline(rect.center().x, (rect.min.y - 6.0)..=(rect.max.y + 6.0), guide);
            }
            if self.guides.1 {
                painter.hline((rect.min.x - 6.0)..=(rect.max.x + 6.0), rect.center().y, guide);
            }
        }

        // Auswahl und Hover
        let hover = resp.hover_pos().and_then(hit);
        if let Some(h) = hover.filter(|&h| Some(h) != self.selected) {
            painter.rect_stroke(to_screen(&r.boxes[h]), CornerRadius::same(4), Stroke::new(1.0_f32, p.accent.gamma_multiply(0.5)), StrokeKind::Outside);
        }
        if let Some((i, b)) = self.selected.and_then(|s| r.boxes.get(s).map(|b| (s, b))) {
            let sr = to_screen(b);
            painter.rect_filled(sr, CornerRadius::same(4), p.accent.gamma_multiply(0.07));
            painter.rect_stroke(sr, CornerRadius::same(4), Stroke::new(1.5_f32, p.accent), StrokeKind::Outside);
            // kleines Namensschild über der Auswahl
            if let Some(el) = l.elements.get(i) {
                let g = painter.layout_no_wrap(el.type_name().to_string(), FontId::proportional(11.0), Color32::WHITE);
                let tag = Rect::from_min_size(pos2(sr.min.x, sr.min.y - g.size().y - 8.0), g.size() + vec2(12.0, 4.0));
                painter.rect_filled(tag, CornerRadius::same(5), p.accent);
                painter.galley(tag.min + vec2(6.0, 2.0), g, Color32::WHITE);
            }
        }

        // Infozeile unter dem Etikett
        let caption = format!(
            "{} × {} mm   ·   {}   ·   {}",
            kit::fmt_num(l.width_mm),
            kit::fmt_num(l.height_mm),
            if l.paper == PaperType::Gap { "Etiketten mit Lücke" } else { "Endlospapier" },
            if self.s.show_dots { "jeder Druckpunkt = 1/8 mm" } else { "Vorschau in Originaloptik" }
        );
        painter.text(pos2(rect.center().x, rect.max.y + 20.0), Align2::CENTER_TOP, caption, FontId::proportional(12.5), p.weak);
        let mut warnings = r.warnings.clone();
        warnings.dedup();
        let mut wy = rect.max.y + 94.0;
        for w in warnings {
            let g = painter.layout_no_wrap(w, FontId::proportional(12.5), p.warn);
            let total = g.size().x + 24.0;
            let x = rect.center().x - total / 2.0;
            painter.text(pos2(x, wy + g.size().y / 2.0), Align2::LEFT_CENTER, icon::WARNING, kit::icon_font(13.0), p.warn);
            painter.galley(pos2(x + 22.0, wy), g, p.warn);
            wy += 22.0;
        }

        // Toast oben
        if let Some((kind, text)) = self.pending_toast.take() {
            self.toast = Some(Toast { text, kind, since: ui.input(|i| i.time) });
        }
        let now = ui.input(|i| i.time);
        let mut close = false;
        if let Some(t) = &self.toast {
            let dur = if t.kind == MsgKind::Error { 9.0 } else { 4.0 };
            if now - t.since > dur {
                close = true;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(dur - (now - t.since)));
                let (ic, col) = match t.kind {
                    MsgKind::Ok => (icon::CHECK, p.ok),
                    MsgKind::Info => (icon::INFO, p.accent),
                    MsgKind::Error => (icon::WARNING, p.danger),
                };
                let g = painter.layout(t.text.clone(), FontId::proportional(13.5), p.text, 520.0);
                let size = vec2(g.size().x + 84.0, g.size().y.max(18.0) + 22.0);
                let tr = Rect::from_center_size(pos2(area.center().x, area.min.y + 18.0 + size.y / 2.0), size);
                painter.add(Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(if p.dark { 120 } else { 35 }) }.as_shape(tr, CornerRadius::same(12)));
                painter.rect_filled(tr, CornerRadius::same(12), p.card);
                painter.rect_stroke(tr, CornerRadius::same(12), Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
                let c = pos2(tr.min.x + 22.0, tr.center().y);
                painter.circle_filled(c, 11.0, col.gamma_multiply(0.15));
                painter.text(c, Align2::CENTER_CENTER, ic, kit::icon_font(11.0), col);
                painter.galley(pos2(tr.min.x + 42.0, tr.center().y - g.size().y / 2.0), g, p.text);
                let xr = Rect::from_center_size(pos2(tr.max.x - 20.0, tr.center().y), vec2(22.0, 22.0));
                let xresp = ui.interact(xr, ui.id().with("toast-x"), Sense::click());
                if xresp.hovered() {
                    painter.rect_filled(xr, CornerRadius::same(6), p.hover);
                }
                painter.text(xr.center(), Align2::CENTER_CENTER, icon::CLOSE, kit::icon_font(9.0), p.weak);
                if xresp.clicked() {
                    close = true;
                }
            }
        }
        if close {
            self.toast = None;
        }

        // Maus: auswählen und verschieben
        let boxes = r.boxes.clone();
        let (lw, lh) = (r.width as f32, r.height as f32);
        if hover.is_some() || self.drag.is_some() {
            ui.ctx().set_cursor_icon(if self.drag.is_some() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
        if resp.clicked() {
            self.selected = resp.interact_pointer_pos().and_then(hit);
        }
        if resp.drag_started() {
            self.drag = None;
            if let Some(i) = resp.interact_pointer_pos().and_then(hit) {
                self.selected = Some(i);
                let b = boxes[i];
                let el = &mut self.s.label.elements[i];
                if let ElementKind::Text(t) = &mut el.kind {
                    if t.fit {
                        t.fit = false;
                        if let Some(pt) = b.fit_pt {
                            t.size_pt = (pt * 10.0).round() / 10.0;
                        }
                    }
                }
                el.x_mm = b.x / PX_PER_MM;
                el.y_mm = b.y / PX_PER_MM;
                self.drag = Some(Drag { idx: i, start: vec2(b.x, b.y), total: Vec2::ZERO, size: vec2(b.w, b.h) });
            }
        }
        if let Some(d) = &mut self.drag {
            if resp.dragged() {
                d.total += resp.drag_delta() / scale;
                let mut pnt = d.start + d.total;
                let snap = 6.0 / scale;
                self.guides = (false, false);
                if ((pnt.x + d.size.x / 2.0) - lw / 2.0).abs() < snap {
                    pnt.x = (lw - d.size.x) / 2.0;
                    self.guides.0 = true;
                }
                if ((pnt.y + d.size.y / 2.0) - lh / 2.0).abs() < snap {
                    pnt.y = (lh - d.size.y) / 2.0;
                    self.guides.1 = true;
                }
                if let Some(el) = self.s.label.elements.get_mut(d.idx) {
                    el.x_mm = pnt.x.round() / PX_PER_MM;
                    el.y_mm = pnt.y.round() / PX_PER_MM;
                }
            }
            if resp.drag_stopped() || (!resp.dragged() && !resp.drag_started()) {
                self.drag = None;
                self.guides = (false, false);
            }
        }
    }

    fn log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_log;
        egui::Window::new(RichText::new("Protokoll").font(kit::semibold(15.0)))
            .open(&mut open)
            .default_size([560.0, 340.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if kit::tool_button(ui, icon::COPY, "Kopieren").clicked() {
                        ctx.copy_text(self.log.iter().cloned().collect::<Vec<_>>().join("\n"));
                    }
                    if kit::tool_button(ui, icon::DELETE, "Leeren").clicked() {
                        self.log.clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    if self.log.is_empty() {
                        ui.label(RichText::new("Noch keine Einträge.").weak());
                    }
                    for line in &self.log {
                        ui.label(RichText::new(line).monospace());
                    }
                });
            });
        self.show_log = open;
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let p = Pal::get(ctx.style().visuals.dark_mode);
        let mut open = self.show_about;
        egui::Window::new("Über")
            .open(&mut open)
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size([380.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
                    ui.painter().image(self.logo.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                    ui.add_space(4.0);
                    ui.label(RichText::new("Etikettendrucker").font(kit::semibold(20.0)).color(p.text));
                    ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(p.weak));
                    ui.add_space(8.0);
                    ui.label("Etiketten gestalten und drucken mit Bluetooth-Druckern der Reihe Pristar / Marklife P15 – direkt vom PC.");
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Das Druckprotokoll stammt aus dem Open-Source-Projekt „thermoprint“ (MIT-Lizenz). \
                             Kein offizielles Produkt von Pristar oder Marklife.",
                        )
                        .size(12.5)
                        .color(p.weak),
                    );
                    ui.hyperlink_to("github.com/tomLadder/thermoprint", "https://github.com/tomLadder/thermoprint");
                    ui.add_space(10.0);
                    if kit::primary_button(ui, icon::CHECK, "Schließen", vec2(140.0, 34.0), true, None).clicked() {
                        self.show_about = false;
                    }
                    ui.add_space(4.0);
                });
            });
        if !open {
            self.show_about = false;
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, frames)) = &mut self.screenshot else { return };
        *frames += 1;
        if *frames == 20 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, img.as_raw().to_vec()) {
                let _ = buf.save(&*path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_printer_events();
        // Akkustand alle zwei Minuten auffrischen
        let now = ctx.input(|i| i.time);
        if self.connected() && !self.printing && now - self.last_battery_check > 120.0 {
            self.last_battery_check = now;
            self.printer.send(Cmd::Battery);
        }
        if self.connected() {
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
        }
        self.handle_shortcuts(ctx);
        self.handle_drops(ctx);
        self.ensure_rendered(ctx);

        self.header(ctx);
        self.left_panel(ctx);
        self.right_panel(ctx);
        self.center(ctx);
        if self.show_log {
            self.log_window(ctx);
        }
        if self.conn == Conn::Picking {
            self.picker_window(ctx);
        }
        if self.show_about {
            self.about_window(ctx);
        }
        self.handle_screenshot(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.screenshot.is_none() {
            eframe::set_value(storage, eframe::APP_KEY, &self.s);
        }
    }
}

// ---------------------------------------------------------------- Eigenschaften

fn element_icon(el: &Element) -> char {
    match el.kind {
        ElementKind::Text(_) => icon::TEXT,
        ElementKind::Qr(_) => icon::QR,
        ElementKind::Barcode(_) => icon::BARCODE,
        ElementKind::Image(_) => icon::IMAGE,
    }
}

fn section(ui: &mut Ui, title: &str) {
    let p = Pal::of(ui);
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, p.border);
    ui.add_space(4.0);
    ui.label(RichText::new(title).font(kit::semibold(13.0)).color(p.weak));
}

fn element_props(
    ui: &mut Ui,
    el: &mut Element,
    fonts: &Fonts,
    bx: Option<ElemBox>,
    (lw, lh): (usize, usize),
    margin_mm: &mut f32,
) -> Option<ListAction> {
    let p = Pal::of(ui);
    let (mut x_mm, mut y_mm) = (el.x_mm, el.y_mm);
    let mut positioned = true;
    let full = f32::INFINITY;
    let mut action = None;

    // Kopf: Symbol, Name, Schnellaktionen
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(8), p.accent_soft);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, element_icon(el), kit::icon_font(15.0), p.accent);
        ui.label(RichText::new(el.type_name()).font(kit::semibold(16.0)).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if kit::icon_button(ui, icon::DELETE, Some(p.danger)).on_hover_text("Löschen (Entf)").clicked() {
                action = Some(ListAction::Delete);
            }
            if kit::icon_button(ui, icon::COPY, None).on_hover_text("Duplizieren").clicked() {
                action = Some(ListAction::Duplicate);
            }
            if kit::icon_button(ui, icon::DOWN, None).on_hover_text("Eine Ebene nach hinten").clicked() {
                action = Some(ListAction::Backward);
            }
            if kit::icon_button(ui, icon::UP, None).on_hover_text("Eine Ebene nach vorne").clicked() {
                action = Some(ListAction::Forward);
            }
        });
    });

    match &mut el.kind {
        ElementKind::Text(t) => {
            section(ui, "Inhalt");
            ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(3).desired_width(full).hint_text("Text eingeben …"));
            section(ui, "Schrift");
            let shown = if t.font.is_empty() { fonts.default_family().to_string() } else { t.font.clone() };
            egui::ComboBox::from_id_salt("schrift")
                .selected_text(&shown)
                .width(ui.available_width())
                .height(360.0)
                .show_ui(ui, |ui| {
                    for fam in fonts.families() {
                        if ui.selectable_label(*fam == shown, fam).clicked() {
                            t.font = fam.clone();
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                kit::icon_toggle(ui, icon::BOLD, &mut t.bold, "Fett");
                kit::icon_toggle(ui, icon::ITALIC, &mut t.italic, "Kursiv");
                ui.add_space(8.0);
                let al = icon::ALIGN_LEFT.to_string();
                let ac = icon::ALIGN_CENTER.to_string();
                let ar = icon::ALIGN_RIGHT.to_string();
                kit::segmented(
                    ui,
                    &mut t.align,
                    &[(model::Align::Left, al.as_str()), (model::Align::Center, ac.as_str()), (model::Align::Right, ar.as_str())],
                    true,
                );
            });
            section(ui, "Größe");
            let was_fit = t.fit;
            kit::switch_row(ui, "Automatisch ans Etikett anpassen", &mut t.fit);
            if was_fit && !t.fit {
                if let Some(b) = bx {
                    if let Some(pt) = b.fit_pt {
                        t.size_pt = (pt * 10.0).round() / 10.0;
                    }
                    x_mm = b.x / PX_PER_MM;
                    y_mm = b.y / PX_PER_MM;
                }
            }
            egui::Grid::new("textgrid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                kit::field_label(ui, "Schriftgröße");
                if t.fit {
                    let pt = bx.and_then(|b| b.fit_pt).unwrap_or(t.size_pt);
                    ui.label(RichText::new(format!("{pt:.0} pt")).color(p.weak));
                    ui.end_row();
                    kit::field_label(ui, "Abstand zum Rand");
                    ui.add_sized([110.0, 28.0], kit::num(margin_mm, 0.0..=5.0, 0.05, 1, " mm"));
                } else {
                    ui.add_sized([110.0, 28.0], kit::num(&mut t.size_pt, 3.0..=300.0, 0.2, 1, " pt"));
                }
                ui.end_row();
                if t.text.contains('\n') {
                    kit::field_label(ui, "Zeilenabstand");
                    ui.add_sized([110.0, 28.0], kit::num(&mut t.line_spacing, 0.5..=3.0, 0.01, 2, ""));
                    ui.end_row();
                }
            });
            positioned = !t.fit;
        }
        ElementKind::Qr(q) => {
            section(ui, "Inhalt");
            ui.add(
                egui::TextEdit::multiline(&mut q.content)
                    .desired_rows(2)
                    .desired_width(full)
                    .hint_text("Text oder Link, z. B. https://…"),
            );
            section(ui, "Größe");
            egui::Grid::new("qrgrid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                kit::field_label(ui, "Kantenlänge");
                ui.add_sized([110.0, 28.0], kit::num(&mut q.size_mm, 3.0..=60.0, 0.1, 1, " mm"));
                ui.end_row();
            });
        }
        ElementKind::Barcode(b) => {
            section(ui, "Inhalt");
            kit::segmented(ui, &mut b.kind, &[(BarcodeKind::Code128, "Code 128"), (BarcodeKind::Ean13, "EAN-13")], false);
            let hint = if b.kind == BarcodeKind::Ean13 { "12 oder 13 Ziffern" } else { "Buchstaben und Zahlen" };
            ui.add(egui::TextEdit::singleline(&mut b.content).desired_width(full).hint_text(hint));
            kit::switch_row(ui, "Klartext darunter", &mut b.show_text);
            section(ui, "Größe");
            egui::Grid::new("bcgrid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                kit::field_label(ui, "Höhe");
                ui.add_sized([110.0, 28.0], kit::num(&mut b.height_mm, 2.0..=40.0, 0.1, 1, " mm"));
                ui.end_row();
                kit::field_label(ui, "Strichbreite");
                ui.add_sized([110.0, 28.0], egui::DragValue::new(&mut b.module_px).range(1..=6).suffix(" Punkte"));
                ui.end_row();
            });
        }
        ElementKind::Image(img) => {
            section(ui, "Bild");
            if kit::tool_button(ui, icon::OPEN, "Anderes Bild wählen …").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("Bilder", IMAGE_EXTS).pick_file() {
                    if let Ok(data) = std::fs::read(&path) {
                        img.data = Arc::new(data);
                    }
                }
            }
            egui::Grid::new("imggrid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                kit::field_label(ui, "Breite");
                ui.add_sized([110.0, 28.0], kit::num(&mut img.width_mm, 1.0..=MAX_W_MM, 0.1, 1, " mm"));
                ui.end_row();
            });
            section(ui, "Darstellung");
            kit::switch_row(ui, "Rastern (für Fotos)", &mut img.dither);
            if !img.dither {
                kit::field_label(ui, "Schwelle hell/dunkel");
                ui.add(egui::Slider::new(&mut img.threshold, 1..=254).show_value(false));
            }
            kit::switch_row(ui, "Invertieren", &mut img.invert);
        }
    }

    if positioned {
        section(ui, "Position");
        egui::Grid::new("posgrid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            kit::field_label(ui, "Von links");
            ui.add_sized([110.0, 28.0], kit::num(&mut x_mm, -50.0..=MAX_W_MM, 0.05, 2, " mm"));
            ui.end_row();
            kit::field_label(ui, "Von oben");
            ui.add_sized([110.0, 28.0], kit::num(&mut y_mm, -50.0..=MAX_W_MM, 0.05, 2, " mm"));
            ui.end_row();
        });
        if let Some(b) = bx {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                kit::field_label(ui, "Mittig:");
                if kit::tool_button(ui, icon::ALIGN_CENTER, "Waagrecht").clicked() {
                    x_mm = ((lw as f32 - b.w) / 2.0).round() / PX_PER_MM;
                }
                if kit::tool_button(ui, icon::MOVE, "Senkrecht").clicked() {
                    y_mm = ((lh as f32 - b.h) / 2.0).round() / PX_PER_MM;
                }
            });
        }
    }
    el.x_mm = x_mm;
    el.y_mm = y_mm;
    action
}

// ---------------------------------------------------------------- Hilfsfunktionen

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn demo_label() -> Label {
    Label {
        width_mm: 50.0,
        height_mm: 15.0,
        frame: model::Frame { enabled: true, ..Default::default() },
        elements: vec![
            Element {
                x_mm: 2.5,
                y_mm: 3.0,
                kind: ElementKind::Text(TextEl {
                    text: "Kabel Büro\nRouter → Switch".into(),
                    fit: false,
                    size_pt: 11.0,
                    align: model::Align::Left,
                    ..Default::default()
                }),
            },
            Element { x_mm: 37.5, y_mm: 2.0, kind: ElementKind::Qr(QrEl { size_mm: 11.0, ..Default::default() }) },
        ],
        ..Default::default()
    }
}
