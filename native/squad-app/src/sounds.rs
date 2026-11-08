//! Short sounds for when something needs the person: a bot asking, a job ready to
//! review or finished, a job that failed. The sounds are made here, in one of a few
//! styles the person picks (Bubble unless they pick another), so they are the same on
//! every computer and need no files shipped with the app. They play through what each
//! system already has: `afplay` on a Mac, PlaySound on Windows, and PipeWire,
//! PulseAudio or ALSA's player on Linux.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::settings::AppSettings;

/// What a sound says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Cue {
    /// A bot asks for an OK or a choice. Two notes going up, in every style.
    NeedsYou,
    /// A job stopped with a problem. Two lower notes going down.
    Failed,
    /// A job finished, or its changes are ready to review. The shortest sound.
    Done,
}

impl Cue {
    pub(crate) const ALL: [Cue; 3] = [Cue::NeedsYou, Cue::Failed, Cue::Done];

    #[cfg_attr(windows, allow(dead_code))]
    fn name(self) -> &'static str {
        match self {
            Cue::NeedsYou => "needs-you",
            Cue::Failed => "failed",
            Cue::Done => "done",
        }
    }

    /// Whether the person wants to hear this one.
    pub(crate) fn wanted(self, st: &AppSettings) -> bool {
        st.sounds
            && match self {
                Cue::NeedsYou => st.sound_needs_you,
                Cue::Failed => st.sound_failed,
                Cue::Done => st.sound_done,
            }
    }
}

/// How the sounds sound, picked under Settings > General > Alerts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Style {
    /// Quick blips that slide in pitch.
    Bubble,
    /// Warm, woody notes.
    Marimba,
    /// A soft plucked string.
    Pluck,
    /// Airy bells with a shimmer.
    Glass,
}

impl Style {
    pub(crate) const ALL: [Style; 4] = [Style::Bubble, Style::Marimba, Style::Pluck, Style::Glass];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Style::Bubble => "bubble",
            Style::Marimba => "marimba",
            Style::Pluck => "pluck",
            Style::Glass => "glass",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Style::Bubble => "Bubble",
            Style::Marimba => "Marimba",
            Style::Pluck => "Pluck",
            Style::Glass => "Glass",
        }
    }

    /// The saved style; Bubble for anything unknown.
    pub(crate) fn parse(id: &str) -> Style {
        Style::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .unwrap_or(Style::Bubble)
    }

    /// The cue's notes in this style, as (sound, start in seconds).
    fn notes(self, cue: Cue) -> Vec<(Vec<f32>, f32)> {
        use Cue::*;
        match self {
            Style::Bubble => match cue {
                NeedsYou => vec![
                    (blip(500., 900., 0.12), 0.0),
                    (blip(700., 1300., 0.12), 0.14),
                ],
                Done => vec![(blip(600., 1100., 0.12), 0.0)],
                Failed => vec![
                    (blip(700., 350., 0.16), 0.0),
                    (blip(500., 250., 0.18), 0.17),
                ],
            },
            Style::Marimba => match cue {
                NeedsYou => vec![(marimba(523.25), 0.0), (marimba(783.99), 0.11)],
                Done => vec![(marimba(659.25), 0.0)],
                Failed => vec![(marimba(392.0), 0.0), (marimba(311.13), 0.13)],
            },
            Style::Pluck => match cue {
                NeedsYou => vec![(pluck(587.33), 0.0), (pluck(880.0), 0.10)],
                Done => vec![(pluck(659.25), 0.0), (pluck(987.77), 0.06)],
                Failed => vec![(pluck(293.66), 0.0), (pluck(220.0), 0.14)],
            },
            Style::Glass => match cue {
                NeedsYou => vec![(glass(1046.5), 0.0), (glass(1567.98), 0.15)],
                Done => vec![(glass(1318.5), 0.0)],
                Failed => vec![(glass(659.25), 0.0), (glass(523.25), 0.18)],
            },
        }
    }
}

const RATE: u32 = 44_100;

/// Sounds closer together than this are one sound: a burst of jobs ending at once
/// chimes once.
const GAP: Duration = Duration::from_millis(1500);

/// The most wanted of `cues` (asks before failures before finished work), if the
/// person wants any of them.
pub(crate) fn pick(cues: impl IntoIterator<Item = Cue>, st: &AppSettings) -> Option<Cue> {
    cues.into_iter().filter(|c| c.wanted(st)).min()
}

/// Play `cue` in the person's style, unless another sound played a moment ago. It
/// never waits for the sound.
pub(crate) fn play(cue: Cue, st: &AppSettings) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|at| at.elapsed() < GAP) {
            return;
        }
        *last = Some(Instant::now());
    }
    preview(Style::parse(&st.sound_style), cue);
}

/// Play `cue` in `style` now, as Settings does when one is picked.
pub(crate) fn preview(style: Style, cue: Cue) {
    std::thread::Builder::new()
        .name("sound".into())
        .spawn(move || out(style, cue))
        .ok();
}

/// The sound as a WAV file, made once.
fn wav(style: Style, cue: Cue) -> &'static [u8] {
    type Made = HashMap<(Style, Cue), &'static [u8]>;
    static MADE: Mutex<Option<Made>> = Mutex::new(None);
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    made.get_or_insert_with(HashMap::new)
        .entry((style, cue))
        .or_insert_with(|| Box::leak(encode(&mix(&style.notes(cue))).into_boxed_slice()))
}

/// Lays the notes out in time and brings the loudest point to a soft level.
fn mix(notes: &[(Vec<f32>, f32)]) -> Vec<i16> {
    let len = notes
        .iter()
        .map(|(s, at)| (at * RATE as f32) as usize + s.len())
        .max()
        .unwrap_or(0);
    let mut out = vec![0f32; len];
    for (sound, at) in notes {
        let from = (at * RATE as f32) as usize;
        for (i, x) in sound.iter().enumerate() {
            out[from + i] += x;
        }
    }
    // Trailing near-silence is cut.
    while out.len() > 1000 && out.last().is_some_and(|x| x.abs() < 1e-4) {
        out.pop();
    }
    let peak = out.iter().fold(1e-9f32, |m, x| m.max(x.abs()));
    let gain = 0.45 / peak;
    out.iter()
        .map(|s| ((s * gain).clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

fn samples(secs: f32) -> impl Iterator<Item = f32> {
    (0..(secs * RATE as f32) as usize).map(|i| i as f32 / RATE as f32)
}

fn sine(freq: f32, t: f32) -> f32 {
    (std::f32::consts::TAU * freq * t).sin()
}

/// A short blip whose pitch slides from `from` to `to`.
fn blip(from: f32, to: f32, secs: f32) -> Vec<f32> {
    let mut phase = 0f32;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            let freq = from * (to / from).powf(t / secs);
            phase += std::f32::consts::TAU * freq / RATE as f32;
            let env = (std::f32::consts::PI * (t / secs).min(1.0)).sin().powf(1.5);
            env * phase.sin()
        })
        .collect();
    out.extend(std::iter::repeat_n(0.0, (0.05 * RATE as f32) as usize));
    out
}

/// A struck wooden bar: the note with quickly fading overtones.
fn marimba(freq: f32) -> Vec<f32> {
    samples(0.5)
        .map(|t| {
            let rise = (t / 0.002).min(1.0);
            rise * ((-t * 9.0).exp() * sine(freq, t)
                + 0.35 * (-t * 40.0).exp() * sine(freq * 3.9, t)
                + 0.15 * (-t * 70.0).exp() * sine(freq * 9.2, t))
        })
        .collect()
}

/// A plucked string (Karplus-Strong), softened so it doesn't twang.
fn pluck(freq: f32) -> Vec<f32> {
    let n = (RATE as f32 / freq) as usize;
    let mut seed = freq as u32 | 1;
    let mut line: Vec<f32> = (0..n)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32 * 2.0 - 1.0
        })
        .collect();
    for _ in 0..3 {
        let prev = line.clone();
        for i in 0..n {
            line[i] = (prev[i] + prev[(i + n - 1) % n]) / 2.0;
        }
    }
    (0..(0.7 * RATE as f32) as usize)
        .map(|i| {
            let x = line[i % n];
            line[i % n] = 0.996 * 0.5 * (x + line[(i + 1) % n]);
            x
        })
        .collect()
}

/// A glass bell, with a slightly detuned twin for a shimmer.
fn glass(freq: f32) -> Vec<f32> {
    samples(1.0)
        .map(|t| {
            let rise = (t / 0.004).min(1.0);
            rise * (sine(freq, t) * (-t * 3.5).exp()
                + 0.5 * sine(freq * 2.76, t) * (-t * 6.0).exp()
                + 0.25 * sine(freq * 5.4, t) * (-t * 10.0).exp()
                + 0.6 * sine(freq + 3.0, t) * (-t * 3.5).exp())
        })
        .collect()
}

/// 16-bit mono PCM samples as a WAV file.
fn encode(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(windows)]
fn out(style: Style, cue: Cue) {
    #[link(name = "winmm")]
    extern "system" {
        fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;
    let bytes = wav(style, cue);
    // SAFETY: with SND_MEMORY the first argument is a whole WAV file in memory; it is
    // kept for the life of the app, so it outlives the asynchronous playback.
    unsafe {
        PlaySoundW(
            bytes.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

#[cfg(not(windows))]
fn out(style: Style, cue: Cue) {
    let Some(path) = file(style, cue) else {
        return;
    };
    let players: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("afplay", &[])]
    } else {
        &[("pw-play", &[]), ("paplay", &[]), ("aplay", &["-q"])]
    };
    for (player, args) in players {
        let child = std::process::Command::new(player)
            .args(*args)
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match child {
            Ok(mut child) => {
                let _ = child.wait();
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return,
        }
    }
}

/// The cue's WAV file on disk, for players that take a path; written when missing
/// or out of date.
#[cfg(not(windows))]
fn file(style: Style, cue: Cue) -> Option<std::path::PathBuf> {
    let dir = crate::store::data_dir().join("sounds");
    let path = dir.join(format!("{}-{}.wav", style.id(), cue.name()));
    let bytes = wav(style, cue);
    let same = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64)
        && std::fs::read(&path).is_ok_and(|b| b == bytes);
    if !same {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_short_whole_wav_files() {
        for (style, cue) in Style::ALL
            .into_iter()
            .flat_map(|s| Cue::ALL.map(|c| (s, c)))
        {
            let bytes = wav(style, cue);
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            assert_eq!(riff + 8, bytes.len());
            let secs = (bytes.len() - 44) as f32 / (RATE * 2) as f32;
            assert!(secs > 0.1 && secs < 1.3, "{style:?} {cue:?} lasts {secs}s");
            // Soft: never near full scale.
            let peak = bytes[44..]
                .chunks(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
                .max()
                .unwrap();
            assert!(
                peak > 1000 && peak < i16::MAX as u16 / 2,
                "{style:?} {cue:?} peaks at {peak}"
            );
        }
    }

    #[test]
    fn asks_win_and_settings_are_heard() {
        let mut st = AppSettings::default();
        assert_eq!(
            pick([Cue::Done, Cue::NeedsYou, Cue::Failed], &st),
            Some(Cue::NeedsYou)
        );
        st.sound_needs_you = false;
        assert_eq!(pick([Cue::Done, Cue::NeedsYou], &st), Some(Cue::Done));
        st.sounds = false;
        assert_eq!(pick([Cue::Done, Cue::Failed], &st), None);
    }
    #[test]
    fn styles_round_trip_and_bubble_is_the_default() {
        for style in Style::ALL {
            assert_eq!(Style::parse(style.id()), style);
        }
        assert_eq!(Style::parse(""), Style::Bubble);
        assert_eq!(
            Style::parse(&AppSettings::default().sound_style),
            Style::Bubble
        );
    }
}
