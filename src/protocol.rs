//! Byte-Protokoll der Marklife/Pristar-P15-Familie ("L11").
//!
//! Die Befehle entsprechen dem, was die offizielle Marklife-App und das
//! Open-Source-Projekt thermoprint (MIT) senden.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::i18n::tr;

/// 203 dpi ≈ 8 Punkte pro Millimeter.
pub const PX_PER_MM: f32 = 8.0;

pub const SERVICE_UUID: Uuid = Uuid::from_u128(0x0000ff00_0000_1000_8000_00805f9b34fb);
/// Drucker → PC (Benachrichtigungen: Status, Antworten)
pub const RX_UUID: Uuid = Uuid::from_u128(0x0000ff01_0000_1000_8000_00805f9b34fb);
/// PC → Drucker (Schreiben ohne Antwort)
pub const TX_UUID: Uuid = Uuid::from_u128(0x0000ff02_0000_1000_8000_00805f9b34fb);
/// Flusssteuerung (Credits, MTU)
pub const CX_UUID: Uuid = Uuid::from_u128(0x0000ff03_0000_1000_8000_00805f9b34fb);

/// Paketgröße, die die P15-Firmware verträgt.
pub const PACKET_SIZE: usize = 95;
/// Abstand zwischen zwei Paketen (wie in der Original-App).
pub const PACKET_DELAY_MS: u64 = 30;

/// Gerätenamen, unter denen sich P15-kompatible Drucker melden.
pub const NAME_PREFIXES: &[&str] = &[
    "P15", "P15R", "P15S", "P7", "iSPACE_LP15", "OUT_LPC", "M1", "LP15", "S15", "S12", "P1s",
    "LPC74",
];

pub fn is_supported_name(name: &str) -> bool {
    NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

pub const CMD_BATTERY: [u8; 4] = [0x10, 0xff, 0x50, 0xf1];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperType {
    /// Einzelne Etiketten mit Lücke dazwischen
    Gap,
    /// Endlospapier
    Continuous,
}

/// 1-Bit-Rasterbild, MSB zuerst, 1 = schwarz.
#[derive(Clone, Debug)]
pub struct Bitmap1 {
    #[allow(dead_code)]
    pub width: usize,
    pub height: usize,
    pub bytes_per_row: usize,
    pub data: Vec<u8>,
}

/// Packt ein Schwarz/Weiß-Bild (true = schwarz) in Bytes.
pub fn pack_bits(pixels: &[bool], width: usize, height: usize) -> Bitmap1 {
    let bytes_per_row = width.div_ceil(8);
    let mut data = vec![0u8; bytes_per_row * height];
    for y in 0..height {
        for x in 0..width {
            if pixels[y * width + x] {
                data[y * bytes_per_row + x / 8] |= 1 << (7 - (x % 8));
            }
        }
    }
    Bitmap1 { width, height, bytes_per_row, data }
}

/// Dreht ein Bild um 90° im Uhrzeigersinn. Ergebnis: (Pixel, neue Breite, neue Höhe).
///
/// Das Etikett wird quer gestaltet, der Druckkopf läuft aber über die kurze Seite.
pub fn rotate_cw(src: &[bool], w: usize, h: usize) -> (Vec<bool>, usize, usize) {
    let (nw, nh) = (h, w);
    let mut dst = vec![false; nw * nh];
    for sy in 0..h {
        for sx in 0..w {
            let dx = nw - 1 - sy;
            let dy = sx;
            dst[dy * nw + dx] = src[sy * w + sx];
        }
    }
    (dst, nw, nh)
}

/// Ein Druckauftrag: Vorspann (Einstellungen, Aufwecken) und Hauptteil (Bild, Vorschub, Ende).
#[derive(Clone, Debug)]
pub struct PrintJob {
    pub preamble: Vec<u8>,
    pub bulk: Vec<u8>,
}

/// Baut die Bytefolge für ein Etikett.
///
/// `density`: 1 = hell, 2 = normal, 3 = dunkel.
pub fn build_print_job(bmp: &Bitmap1, density: u8, paper: PaperType) -> PrintJob {
    let mut preamble = Vec::with_capacity(32);
    // Druckstärke ("thickness") — so stellt die Marklife-App den P15 ein
    preamble.extend_from_slice(&[0x10, 0xff, 0x10, 0x00, density.clamp(1, 3)]);
    // Aufwecken: 15 Null-Bytes
    preamble.extend_from_slice(&[0u8; 15]);
    // Druckwerk aktivieren
    preamble.extend_from_slice(&[0x10, 0xff, 0xf1, 0x02]);

    let mut bulk = Vec::with_capacity(bmp.data.len() + 16);
    let bpr = bmp.bytes_per_row as u16;
    let h = bmp.height as u16;
    // GS v 0: Rasterbild
    bulk.extend_from_slice(&[0x1d, 0x76, 0x30, 0x00]);
    bulk.extend_from_slice(&bpr.to_le_bytes());
    bulk.extend_from_slice(&h.to_le_bytes());
    bulk.extend_from_slice(&bmp.data);
    match paper {
        // Bis zur nächsten Lücke vorschieben
        PaperType::Gap => bulk.extend_from_slice(&[0x1d, 0x0c]),
        // 100 Punkte vorschieben
        PaperType::Continuous => bulk.extend_from_slice(&[0x1b, 0x4a, 100]),
    }
    // Druckauftrag beenden
    bulk.extend_from_slice(&[0x10, 0xff, 0xf1, 0x45]);

    PrintJob { preamble, bulk }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// Etikett fertig gedruckt
    Success,
    /// Drucker erlaubt n weitere Pakete
    Credit(u8),
    /// Ausgehandelte MTU
    Mtu(u16),
    /// Statusmeldung
    Status(u8),
}

pub fn parse_response(data: &[u8]) -> Option<Response> {
    let first = *data.first()?;
    if matches!(first, 0xaa | 0x4f | 0x4b) {
        return Some(Response::Success);
    }
    let second = *data.get(1)?;
    match first {
        0x01 => Some(Response::Credit(second)),
        0x02 if data.len() >= 3 => Some(Response::Mtu(u16::from_le_bytes([data[1], data[2]]))),
        0xff => Some(Response::Status(second)),
        _ => None,
    }
}

pub fn status_text(code: u8) -> String {
    match code {
        0x01 => tr("Kein Papier eingelegt", "No paper loaded").into(),
        0x02 => tr("Deckel ist offen", "Cover is open").into(),
        0x03 => tr("Drucker ist überhitzt", "Printer is overheated").into(),
        0x04 => tr("Akku fast leer", "Battery low").into(),
        0x05 => tr("Deckel geschlossen", "Cover closed").into(),
        other => trf!("Unbekannter Status 0x{other:02X}", "Unknown status 0x{other:02X}"),
    }
}

/// Statuscodes, bei denen der Druck nicht weitergehen kann.
pub fn status_is_fatal(code: u8) -> bool {
    matches!(code, 0x01..=0x03)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_msb_first() {
        let px = [true, false, false, false, false, false, false, true, true];
        let b = pack_bits(&px, 9, 1);
        assert_eq!(b.bytes_per_row, 2);
        assert_eq!(b.data, vec![0b1000_0001, 0b1000_0000]);
    }

    #[test]
    fn rotate_clockwise() {
        // 3x2:
        // a b c
        // d e f
        let (a, b, c, d, e, f) = (true, false, true, false, true, true);
        let src = [a, b, c, d, e, f];
        let (dst, w, h) = rotate_cw(&src, 3, 2);
        assert_eq!((w, h), (2, 3));
        // Im Uhrzeigersinn:
        // d a
        // e b
        // f c
        assert_eq!(dst, vec![d, a, e, b, f, c]);
    }

    #[test]
    fn print_job_bytes() {
        let bmp = pack_bits(&vec![true; 16 * 2], 16, 2);
        let job = build_print_job(&bmp, 2, PaperType::Gap);
        let mut expected_pre = vec![0x10, 0xff, 0x10, 0x00, 0x02];
        expected_pre.extend([0u8; 15]);
        expected_pre.extend([0x10, 0xff, 0xf1, 0x02]);
        assert_eq!(job.preamble, expected_pre);
        assert_eq!(
            job.bulk,
            vec![
                0x1d, 0x76, 0x30, 0x00, 2, 0, 2, 0, 0xff, 0xff, 0xff, 0xff, 0x1d, 0x0c, 0x10,
                0xff, 0xf1, 0x45
            ]
        );
        let job = build_print_job(&bmp, 3, PaperType::Continuous);
        assert!(job.bulk.ends_with(&[0x1b, 0x4a, 100, 0x10, 0xff, 0xf1, 0x45]));
    }

    #[test]
    fn responses() {
        assert_eq!(parse_response(&[0x01, 0x04]), Some(Response::Credit(4)));
        assert_eq!(parse_response(&[0x02, 0xf0, 0x00]), Some(Response::Mtu(240)));
        assert_eq!(parse_response(&[0xff, 0x01]), Some(Response::Status(1)));
        assert_eq!(parse_response(&[0xaa]), Some(Response::Success));
        assert_eq!(parse_response(&[0x4f, 0x4b]), Some(Response::Success));
        assert_eq!(parse_response(&[0x00]), None);
        assert!(is_supported_name("P15_1A2B"));
        assert!(!is_supported_name("Kopfhörer"));
    }
}
