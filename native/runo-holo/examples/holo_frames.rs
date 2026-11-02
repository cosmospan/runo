//! Writes PNG frames of every bot and mood, for checking the rigs against
//! the design: `cargo run -p runo-holo --example holo_frames -- out_dir [t]`.

use runo_holo::{render_png, Bot, Framing, Mood};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "holo-frames".into());
    let t: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    std::fs::create_dir_all(&out).expect("out dir");
    for bot in Bot::ALL {
        for mood in Mood::ALL {
            let png = render_png(&bot.look(mood), t, 290, Framing::Full);
            std::fs::write(
                format!(
                    "{out}/{}-{}.png",
                    bot.name().to_lowercase(),
                    mood.label().to_lowercase().replace(' ', "-")
                ),
                png,
            )
            .unwrap();
        }
        let png = render_png(&bot.signature(), t, 48, Framing::Bust);
        std::fs::write(
            format!("{out}/{}-bust48.png", bot.name().to_lowercase()),
            png,
        )
        .unwrap();
    }
    // A time strip of each signature, for judging motion.
    for bot in Bot::ALL {
        for i in 0..10 {
            let png = render_png(&bot.signature(), i as f64 * 0.5, 160, Framing::Full);
            std::fs::write(
                format!("{out}/strip-{}-{i:02}.png", bot.name().to_lowercase()),
                png,
            )
            .unwrap();
        }
    }
}
