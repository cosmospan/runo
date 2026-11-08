//! An agent plan's usage windows as the agent reports them while it works: Claude Code's
//! `rate_limit_event` on every turn, or the "You've hit your session limit · resets 3pm"
//! line a turn ends with. A window that is used up holds that agent's new turns until it
//! resets, so a turn waits instead of failing, and a turn the limit cut short can carry on
//! where it stopped instead of starting over.

use serde::{Deserialize, Serialize};

/// Where a plan window stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Allowed,
    /// Past a warning threshold: the agent still runs.
    Warning,
    /// Used up: the agent's requests are refused until the window resets.
    Rejected,
}

/// What a turn said about its agent's plan.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanLimit {
    pub status: Status,
    /// The window that limits, as the CLI names it: "five_hour", "seven_day" and the like.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// Unix seconds when that window starts over, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
    /// Percent of that window used, 0 to 100, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used: Option<f32>,
}

/// Claude Code's `rate_limit_info`.
pub fn from_claude(info: &serde_json::Value) -> Option<PlanLimit> {
    let status = match info["status"].as_str()? {
        "rejected" => Status::Rejected,
        "allowed_warning" => Status::Warning,
        "allowed" => Status::Allowed,
        _ => return None,
    };
    Some(PlanLimit {
        status,
        kind: info["rateLimitType"].as_str().unwrap_or("").to_string(),
        resets_at: info["resetsAt"].as_i64().filter(|t| *t > 0),
        used: info["utilization"].as_f64().map(percent),
    })
}

/// A share the CLI gives as 0 to 1, or already as a percent.
fn percent(value: f64) -> f32 {
    let p = if value <= 1.0 { value * 100.0 } else { value };
    p.clamp(0.0, 100.0) as f32
}

/// The line a turn ends with when a plan window is used up, such as "You've hit your
/// session limit · resets 3pm (Asia/Calcutta)" or "Claude usage limit reached.".
pub fn from_text(text: &str, now: chrono::DateTime<chrono::Local>) -> Option<PlanLimit> {
    let lower = text.to_lowercase();
    let hit = lower.contains("hit your") && lower.contains("limit")
        || lower.contains("usage limit reached")
        || lower.contains("usage_limit_reached");
    if !hit {
        return None;
    }
    let kind = if lower.contains("weekly") {
        "seven_day"
    } else if lower.contains("session") || lower.contains("5-hour") {
        "five_hour"
    } else {
        ""
    };
    let resets_at = lower
        .split_once("resets ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|time| next_time(time, now));
    Some(PlanLimit {
        status: Status::Rejected,
        kind: kind.to_string(),
        resets_at,
        used: Some(100.0),
    })
}

/// The next moment the clock reads `time` ("3pm", "1:30am", "15:00"), in local time.
fn next_time(time: &str, now: chrono::DateTime<chrono::Local>) -> Option<i64> {
    use chrono::{NaiveTime, TimeZone};
    let time = time.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    let (clock, pm) = match time.strip_suffix("pm") {
        Some(t) => (t, Some(true)),
        None => match time.strip_suffix("am") {
            Some(t) => (t, Some(false)),
            None => (time, None),
        },
    };
    let (h, m) = match clock.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (clock.parse::<u32>().ok()?, 0),
    };
    let h = match pm {
        Some(true) if h < 12 => h + 12,
        Some(false) if h == 12 => 0,
        Some(_) if h > 12 => return None,
        _ => h,
    };
    let at = NaiveTime::from_hms_opt(h, m, 0)?;
    let mut day = now.date_naive();
    for _ in 0..2 {
        if let Some(t) = chrono::Local
            .from_local_datetime(&day.and_time(at))
            .earliest()
            .filter(|t| *t > now)
        {
            return Some(t.timestamp());
        }
        day = day.succ_opt()?;
    }
    None
}

/// The time of day a window resets, "15:00", with the day when it isn't today.
pub fn reset_clock(at: i64, now: i64) -> String {
    let Some(t) = chrono::DateTime::from_timestamp(at, 0).map(|t| t.with_timezone(&chrono::Local))
    else {
        return String::new();
    };
    let today = chrono::DateTime::from_timestamp(now, 0)
        .map(|n| n.with_timezone(&chrono::Local).date_naive());
    if today == Some(t.date_naive()) {
        t.format("%H:%M").to_string()
    } else {
        t.format("%a %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> chrono::DateTime<chrono::Local> {
        chrono::Local
            .with_ymd_and_hms(2026, 10, 24, h, m, 0)
            .earliest()
            .unwrap()
    }

    #[test]
    fn reads_claudes_rate_limit_event() {
        let info = serde_json::json!({
            "status": "allowed_warning", "resetsAt": 1792840000,
            "rateLimitType": "five_hour", "utilization": 0.86, "isUsingOverage": false
        });
        let limit = from_claude(&info).unwrap();
        assert_eq!(limit.status, Status::Warning);
        assert_eq!(limit.kind, "five_hour");
        assert_eq!(limit.resets_at, Some(1792840000));
        assert!((limit.used.unwrap() - 86.0).abs() < 0.01);
        let rejected = from_claude(&serde_json::json!({"status": "rejected"})).unwrap();
        assert_eq!(rejected.status, Status::Rejected);
        assert_eq!(rejected.resets_at, None);
        assert!(from_claude(&serde_json::json!({"status": "odd"})).is_none());
    }

    #[test]
    fn reads_the_limit_line_and_its_reset_time() {
        let now = at(13, 40);
        let limit = from_text(
            "Agent terminated early due to an API error: You've hit your session limit · resets 3pm (Asia/Calcutta)",
            now,
        )
        .unwrap();
        assert_eq!(limit.status, Status::Rejected);
        assert_eq!(limit.kind, "five_hour");
        assert_eq!(limit.resets_at, Some(at(15, 0).timestamp()));
        // A time already past today is tomorrow's.
        let late = from_text("You've hit your weekly limit · resets 1:30am", at(23, 0)).unwrap();
        assert_eq!(late.kind, "seven_day");
        assert_eq!(
            late.resets_at,
            Some((at(1, 30) + chrono::Duration::days(1)).timestamp())
        );
        assert_eq!(
            from_text("Claude usage limit reached.", now)
                .unwrap()
                .resets_at,
            None
        );
        assert!(from_text("The build hit a rate of 3 per second", now).is_none());
    }
}
