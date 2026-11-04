//! App-Symbol: violettes Quadrat mit weißem Anhänger-Etikett.
//! Wird für das Fenster und (über examples/make_icon.rs) für die .exe benutzt.

/// Zeichnet das Symbol in n×n Pixeln, RGBA.
pub fn draw_icon(n: u32) -> Vec<u8> {
    let ss = 4; // Unterabtastung für glatte Kanten
    let mut out = vec![0u8; (n * n * 4) as usize];
    for py in 0..n {
        for px in 0..n {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (px as f32 + (sx as f32 + 0.5) / ss as f32) / n as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / ss as f32) / n as f32;
                    let c = sample(x, y);
                    // vormultipliziert aufsummieren
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }
            let cnt = (ss * ss) as f32;
            let a = acc[3] / cnt;
            let i = ((py * n + px) * 4) as usize;
            if a > 0.0 {
                out[i] = (acc[0] / acc[3] * 255.0).round() as u8;
                out[i + 1] = (acc[1] / acc[3] * 255.0).round() as u8;
                out[i + 2] = (acc[2] / acc[3] * 255.0).round() as u8;
                out[i + 3] = (a * 255.0).round() as u8;
            }
        }
    }
    out
}

fn rounded(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Farbe an Position (x, y) im Einheitsquadrat, RGBA 0..1.
fn sample(x: f32, y: f32) -> [f32; 4] {
    let top = [118.0 / 255.0, 108.0 / 255.0, 1.0];
    let bottom = [70.0 / 255.0, 58.0 / 255.0, 210.0 / 255.0];
    if !rounded(x, y, 0.04, 0.04, 0.96, 0.96, 0.22) {
        return [0.0; 4];
    }
    let bg = mix(top, bottom, (x * 0.35 + y * 0.65).clamp(0.0, 1.0));

    // Etikett leicht gedreht
    let (cx, cy) = (0.5, 0.52);
    let ang: f32 = -0.18;
    let (s, c) = ang.sin_cos();
    let (dx, dy) = (x - cx, y - cy);
    let (lx, ly) = (dx * c - dy * s + cx, dx * s + dy * c + cy);

    // Schatten des Etiketts
    let in_label = |lx: f32, ly: f32| rounded(lx, ly, 0.16, 0.33, 0.86, 0.71, 0.07);
    let hole = ((lx - 0.27).powi(2) + (ly - 0.52).powi(2)).sqrt() < 0.05;
    if in_label(lx, ly) && !hole {
        // Schrift-Balken
        if rounded(lx, ly, 0.39, 0.42, 0.76, 0.49, 0.035) {
            return [0.12, 0.12, 0.16, 1.0];
        }
        if rounded(lx, ly, 0.39, 0.555, 0.64, 0.615, 0.03) {
            return [0.62, 0.60, 0.95, 1.0];
        }
        return [1.0, 1.0, 1.0, 1.0];
    }
    // weicher Schatten rechts unten
    let (sx, sy) = (lx - 0.012, ly - 0.03);
    if in_label(sx, sy) && !hole {
        let d = mix(bg, [0.1, 0.05, 0.3], 0.35);
        return [d[0], d[1], d[2], 1.0];
    }
    [bg[0], bg[1], bg[2], 1.0]
}
