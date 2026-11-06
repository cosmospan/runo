//! Peek's tiny router: a word and letter-pattern classifier built into the app.
//!
//! No neural net, no download, no server: about 15,000 weights (370 KB) that
//! `training/train_tiny.py` learns from the same examples the small model is
//! trained on. It answers in microseconds and is used when the keyword rules
//! aren't sure of a job.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::super::data::Mode;

const WEIGHTS: &str = include_str!("tiny.tsv");

struct Tiny {
    kinds: Vec<Mode>,
    w: HashMap<&'static str, Vec<i32>>,
}

fn tiny() -> &'static Tiny {
    static T: OnceLock<Tiny> = OnceLock::new();
    T.get_or_init(|| {
        let mut kinds = Vec::new();
        let mut w = HashMap::new();
        for line in WEIGHTS.lines() {
            let Some((name, rest)) = line.split_once('\t') else {
                continue;
            };
            if name == "#kinds" {
                kinds = rest
                    .split(' ')
                    .filter_map(|k| {
                        Mode::ALL
                            .into_iter()
                            .find(|m| m.label().eq_ignore_ascii_case(k))
                    })
                    .collect();
                continue;
            }
            let v: Vec<i32> = rest.split(' ').filter_map(|n| n.parse().ok()).collect();
            w.insert(name, v);
        }
        Tiny { kinds, w }
    })
}

fn word_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '\''
}

/// The same tokens as `tokens` in train_tiny.py.
fn tokens(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in t.chars() {
        if word_char(ch) {
            cur.push(ch);
            continue;
        }
        if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !ch.is_whitespace() {
            out.push(ch.to_string());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The same features as `features` in train_tiny.py.
pub fn features(text: &str) -> Vec<String> {
    let t = text.to_lowercase();
    let w = tokens(&t);
    let mut f: Vec<String> = w.iter().map(|x| format!("w:{x}")).collect();
    f.extend(w.windows(2).map(|p| format!("b:{}_{}", p[0], p[1])));
    if let Some(first) = w.first() {
        f.push(format!("first:{first}"));
    }
    let s: Vec<char> = format!(" {t} ").chars().collect();
    if s.len() >= 4 {
        f.extend(
            s.windows(4)
                .map(|c| format!("c:{}", c.iter().collect::<String>())),
        );
    }
    if t.trim().ends_with('?') {
        f.push("q?".into());
    }
    f
}

/// The kind of job `text` reads as, and how sure (0 to 1).
pub fn classify(text: &str) -> (Mode, f32) {
    let t = tiny();
    let n = t.kinds.len();
    if n == 0 {
        return (Mode::Build, 0.0);
    }
    let mut z = vec![0f32; n];
    for f in features(text) {
        if let Some(v) = t.w.get(f.as_str()) {
            for (k, x) in v.iter().enumerate().take(n) {
                z[k] += *x as f32 / 100.0;
            }
        }
    }
    let top = z.iter().cloned().fold(f32::MIN, f32::max);
    let e: Vec<f32> = z.iter().map(|v| (v - top).exp()).collect();
    let sum: f32 = e.iter().sum();
    let (k, p) = e
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(k, v)| (k, v / sum))
        .unwrap_or((0, 0.0));
    (t.kinds[k], p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_match_the_trainer() {
        let f = features("Fix: the CSV export?");
        for want in [
            "w:fix",
            "w::",
            "b:fix_:",
            "first:fix",
            "c: fix",
            "c:ort?",
            "q?",
        ] {
            assert!(f.contains(&want.to_string()), "{want} in {f:?}");
        }
    }

    #[test]
    fn it_reads_plain_jobs() {
        assert_eq!(classify("the app crashes when I log out").0, Mode::Fix);
        assert_eq!(classify("write tests for the discount code").0, Mode::Test);
        assert_eq!(classify("how does the auth middleware work").0, Mode::Ask);
    }
}
