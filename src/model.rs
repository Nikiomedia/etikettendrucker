//! Datenmodell eines Etiketts. Wird als Vorlage (.etikett, JSON) gespeichert.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::protocol::PaperType;

/// Etikettengrößen, die es für den P15 gibt (Breite × Höhe in mm).
pub const SIZE_PRESETS: &[(f32, f32)] = &[
    (22.0, 12.0),
    (22.0, 14.0),
    (26.0, 15.0),
    (30.0, 12.0),
    (30.0, 14.0),
    (30.0, 15.0),
    (40.0, 12.0),
    (40.0, 14.0),
    (40.0, 15.0),
    (50.0, 15.0),
];

pub const MIN_W_MM: f32 = 10.0;
pub const MAX_W_MM: f32 = 200.0;
pub const MIN_H_MM: f32 = 5.0;
pub const MAX_H_MM: f32 = 15.0;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Label {
    pub width_mm: f32,
    pub height_mm: f32,
    pub paper: PaperType,
    /// Abstand zum Rand für automatisch angepassten Text
    pub margin_mm: f32,
    pub frame: Frame,
    pub elements: Vec<Element>,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            width_mm: 40.0,
            height_mm: 12.0,
            paper: PaperType::Gap,
            margin_mm: 1.0,
            frame: Frame::default(),
            elements: vec![Element::new_text("Beschriftung")],
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Frame {
    pub enabled: bool,
    /// Linienstärke in Druckpunkten (1 Punkt = 1/8 mm)
    pub thickness_px: u32,
    pub inset_mm: f32,
    pub radius_mm: f32,
}

impl Default for Frame {
    fn default() -> Self {
        Self { enabled: false, thickness_px: 3, inset_mm: 0.5, radius_mm: 1.5 }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// Linke obere Ecke in mm
    pub x_mm: f32,
    pub y_mm: f32,
    pub kind: ElementKind,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "typ")]
pub enum ElementKind {
    Text(TextEl),
    Qr(QrEl),
    Barcode(BarcodeEl),
    Image(ImageEl),
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextEl {
    pub text: String,
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    pub size_pt: f32,
    /// Größe automatisch so wählen, dass der Text das Etikett ausfüllt
    pub fit: bool,
    pub align: Align,
    pub line_spacing: f32,
}

impl Default for TextEl {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: String::new(),
            bold: true,
            italic: false,
            size_pt: 20.0,
            fit: true,
            align: Align::Center,
            line_spacing: 1.0,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QrEl {
    pub content: String,
    pub size_mm: f32,
}

impl Default for QrEl {
    fn default() -> Self {
        Self { content: "https://example.com".into(), size_mm: 10.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarcodeKind {
    Code128,
    Ean13,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarcodeEl {
    pub kind: BarcodeKind,
    pub content: String,
    pub height_mm: f32,
    /// Breite des schmalsten Strichs in Druckpunkten
    pub module_px: u32,
    pub show_text: bool,
}

impl Default for BarcodeEl {
    fn default() -> Self {
        Self {
            kind: BarcodeKind::Code128,
            content: "12345678".into(),
            height_mm: 6.0,
            module_px: 2,
            show_text: true,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageEl {
    /// Originaldatei (PNG/JPG/…), in der Vorlage als Base64 gespeichert
    #[serde(with = "b64")]
    pub data: Arc<Vec<u8>>,
    pub width_mm: f32,
    /// Graustufen durch Punktmuster darstellen (für Fotos)
    pub dither: bool,
    /// Schwelle für Schwarz/Weiß, wenn nicht gerastert wird
    pub threshold: u8,
    pub invert: bool,
}

impl Default for ImageEl {
    fn default() -> Self {
        Self { data: Arc::new(Vec::new()), width_mm: 10.0, dither: false, threshold: 128, invert: false }
    }
}

impl Element {
    pub fn new_text(text: &str) -> Self {
        Self {
            x_mm: 1.0,
            y_mm: 1.0,
            kind: ElementKind::Text(TextEl { text: text.into(), ..Default::default() }),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self.kind {
            ElementKind::Text(_) => "Text",
            ElementKind::Qr(_) => "QR-Code",
            ElementKind::Barcode(_) => "Barcode",
            ElementKind::Image(_) => "Bild",
        }
    }

    /// Kurzbeschreibung für die Elementliste
    pub fn summary(&self) -> String {
        let s = match &self.kind {
            ElementKind::Text(t) => t.text.lines().next().unwrap_or("").to_string(),
            ElementKind::Qr(q) => q.content.clone(),
            ElementKind::Barcode(b) => b.content.clone(),
            ElementKind::Image(i) => format!("{:.0} mm breit", i.width_mm),
        };
        let mut short: String = s.chars().take(22).collect();
        if s.chars().count() > 22 {
            short.push('…');
        }
        short
    }
}

mod b64 {
    use std::sync::Arc;

    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(data.as_slice()))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s.as_bytes()).map(Arc::new).map_err(serde::de::Error::custom)
    }
}
