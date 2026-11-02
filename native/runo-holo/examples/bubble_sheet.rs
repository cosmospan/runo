//! A contact sheet of the bubble crew: `cargo run --release -p runo-holo --example bubble_sheet out.png [t]`.
use runo_holo::{Bot, BubbleRenderer, Framing, Mood};
use std::time::Instant;

fn main() {
    let out = std::env::args().nth(1).unwrap_or("bubbles.png".into());
    let t: f64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.3);
    let cell = 170usize;
    let rows: Vec<(Bot, Mood)> = Bot::ALL
        .iter()
        .map(|b| (*b, Mood::Signature))
        .chain(
            [
                Mood::Idle,
                Mood::Thinking,
                Mood::Working,
                Mood::Done,
                Mood::NeedsYou,
                Mood::Asleep,
            ]
            .map(|m| (Bot::Builder, m)),
        )
        .collect();
    let cols = 6;
    let (w, h) = (cols * cell, rows.len().div_ceil(cols) * cell + 60);
    let mut img = vec![0xEEu8; w * h * 4];
    let mut r = BubbleRenderer::new(cell as u16);
    for (i, (b, m)) in rows.iter().enumerate() {
        let px = r.draw(&b.bubble().pose(*m), t, Framing::Full).to_vec();
        let (ox, oy) = ((i % cols) * cell, (i / cols) * cell);
        blit(&mut img, w, &px, cell, ox, oy);
    }
    // Small busts along the bottom.
    for (size, x0) in [(24usize, 10usize), (42, 200), (56, 480)] {
        let mut r = BubbleRenderer::new(size as u16);
        for (i, b) in Bot::ALL.iter().enumerate() {
            let px = r
                .draw(&b.bubble().pose(Mood::Idle), t, Framing::Bust)
                .to_vec();
            blit(&mut img, w, &px, size, x0 + i * (size + 2), h - 58);
        }
    }
    let f = std::fs::File::create(&out).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&img).unwrap();
    for (size, framing) in [
        (170u16, Framing::Full),
        (96, Framing::Full),
        (42, Framing::Bust),
        (24, Framing::Bust),
    ] {
        let mut r = BubbleRenderer::new(size);
        let poses: Vec<_> = Bot::ALL
            .iter()
            .map(|b| b.bubble().pose(Mood::Signature))
            .collect();
        let n = 60;
        let s = Instant::now();
        for i in 0..n {
            for p in &poses {
                r.draw(p, i as f64 / 30.0, framing);
            }
        }
        println!(
            "{size:>4}px: {:.3} ms a bot a frame",
            s.elapsed().as_secs_f64() * 1000.0 / (n * poses.len()) as f64
        );
    }
}

fn blit(img: &mut [u8], w: usize, px: &[u8], n: usize, ox: usize, oy: usize) {
    for y in 0..n {
        for x in 0..n {
            let s = (y * n + x) * 4;
            let d = ((oy + y) * w + ox + x) * 4;
            let a = px[s + 3] as u32;
            for c in 0..3 {
                img[d + c] =
                    (px[s + c] as u32 + img[d + c] as u32 * (255 - a) / 255).min(255) as u8;
            }
            img[d + 3] = 255;
        }
    }
}
