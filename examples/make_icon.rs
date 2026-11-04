//! Erzeugt assets/icon.ico (für die .exe) und assets/icon.png.
//! Aufruf: cargo run --example make_icon

#[path = "../src/icon.rs"]
mod icon;

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::ExtendedColorType;

fn main() {
    std::fs::create_dir_all("assets").unwrap();
    let frames: Vec<IcoFrame> = [256u32, 64, 48, 40, 32, 24, 20, 16]
        .iter()
        .map(|&n| IcoFrame::as_png(&icon::draw_icon(n), n, n, ExtendedColorType::Rgba8).unwrap())
        .collect();
    let f = std::fs::File::create("assets/icon.ico").unwrap();
    IcoEncoder::new(f).encode_images(&frames).unwrap();
    image::RgbaImage::from_raw(256, 256, icon::draw_icon(256)).unwrap().save("assets/icon.png").unwrap();
    println!("assets/icon.ico und assets/icon.png geschrieben");
}
