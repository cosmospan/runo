//! Frame times for the CPU path: `cargo run --release -p runo-holo --example holo_bench`.
use runo_holo::{detail_for, Bot, Framing, Mood, Renderer};
use std::time::Instant;

fn main() {
    for (px, framing) in [
        (290u16, Framing::Full),
        (120, Framing::Full),
        (48, Framing::Bust),
        (24, Framing::Bust),
    ] {
        let mut r = Renderer::new(px);
        let looks: Vec<_> = Bot::ALL.iter().map(|b| b.look(Mood::Signature)).collect();
        let n = 120;
        let start = Instant::now();
        for i in 0..n {
            for l in &looks {
                r.draw(l, i as f64 / 60.0, framing, detail_for(px as u32, framing));
            }
        }
        let per = start.elapsed().as_secs_f64() * 1000.0 / (n * looks.len()) as f64;
        println!("{px:>4}px {framing:?}: {per:.3} ms a bot a frame");
    }
}
