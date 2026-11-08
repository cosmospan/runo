//! Short chimes for when something needs the person: a bot asking, a job ready to
//! review or finished, a job that failed. The sounds are made here, a few soft bell
//! notes, so they are the same on every computer and need no files shipped with the
//! app. They play through what each system already has: `afplay` on a Mac, PlaySound
//! on Windows, and PipeWire, PulseAudio or ALSA's player on Linux.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::settings::AppSettings;

/// What a sound says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Cue {
    /// A bot asks for an OK or a choice. Two notes going up.
    NeedsYou,
    /// A job stopped with a problem. Two notes going down, lower.
    Failed,
    /// A job finished, or its changes are ready to review. One soft note.
    Done,
}

impl Cue {
    pub(crate) const ALL: [Cue; 3] = [Cue::NeedsYou, Cue::Failed, Cue::Done];

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

    /// The notes, as (pitch in Hz, start in seconds, length in seconds), and how loud.
    fn notes(self) -> (&'static [(f32, f32, f32)], f32) {
        match self {
            // E5 then B5.
            Cue::NeedsYou => (&[(659.25, 0.0, 0.45), (987.77, 0.12, 0.55)], 0.20),
            // A4 then F4.
            Cue::Failed => (&[(440.0, 0.0, 0.40), (349.23, 0.14, 0.55)], 0.20),
            // G5.
            Cue::Done => (&[(783.99, 0.0, 0.60)], 0.16),
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

/// Play `cue`, unless another sound played a moment ago. It never waits for the sound.
pub(crate) fn play(cue: Cue) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|at| at.elapsed() < GAP) {
            return;
        }
        *last = Some(Instant::now());
    }
    std::thread::Builder::new()
        .name("sound".into())
        .spawn(move || out(cue))
        .ok();
}

/// The cue's sound as a WAV file, made once.
fn wav(cue: Cue) -> &'static [u8] {
    static MADE: [OnceLock<Vec<u8>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let at = Cue::ALL.iter().position(|c| *c == cue).unwrap_or(0);
    MADE[at].get_or_init(|| {
        let (notes, loud) = cue.notes();
        encode(&synth(notes, loud))
    })
}

/// Soft bell notes: a sine with a little of its octave, a quick rise and a long fade.
fn synth(notes: &[(f32, f32, f32)], loud: f32) -> Vec<i16> {
    let end = notes
        .iter()
        .map(|(_, start, len)| start + len)
        .fold(0.0f32, f32::max)
        + 0.05;
    let n = (end * RATE as f32) as usize;
    let mut out = vec![0f32; n];
    for &(pitch, start, len) in notes {
        let from = (start * RATE as f32) as usize;
        let count = (len * RATE as f32) as usize;
        // Fades to about a hundredth by the end of the note.
        let fade = 4.6 / len;
        for i in 0..count.min(n.saturating_sub(from)) {
            let t = i as f32 / RATE as f32;
            let rise = (t / 0.006).min(1.0);
            let env = rise * (-fade * t).exp();
            let phase = std::f32::consts::TAU * pitch * t;
            let tone = phase.sin() + 0.18 * (2.0 * phase).sin() * (-fade * t).exp();
            out[from + i] += loud * env * tone;
        }
    }
    out.iter()
        .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
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
fn out(cue: Cue) {
    #[link(name = "winmm")]
    extern "system" {
        fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;
    let bytes = wav(cue);
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
fn out(cue: Cue) {
    let Some(path) = file(cue) else {
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
fn file(cue: Cue) -> Option<std::path::PathBuf> {
    let dir = crate::store::data_dir().join("sounds");
    let path = dir.join(format!("{}.wav", cue.name()));
    let bytes = wav(cue);
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
        for cue in Cue::ALL {
            let bytes = wav(cue);
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            assert_eq!(riff + 8, bytes.len());
            let secs = (bytes.len() - 44) as f32 / (RATE * 2) as f32;
            assert!(secs > 0.3 && secs < 1.0, "{cue:?} lasts {secs}s");
            // Soft: never near full scale.
            let peak = bytes[44..]
                .chunks(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
                .max()
                .unwrap();
            assert!(peak < i16::MAX as u16 / 2, "{cue:?} peaks at {peak}");
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
}
