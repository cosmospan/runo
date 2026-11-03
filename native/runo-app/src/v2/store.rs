//! What v2 keeps between launches: the folders the crew works in, every job
//! and what happened in it, and each bot's chat. Saved as JSON in Runo's data
//! folder (`~/.runo/v2.json`), apart from v1's state.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::data::{Member, Mode};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub folders: Vec<Folder>,
    /// The folder new jobs and questions go to, by index into `folders`.
    pub current: usize,
    pub jobs: Vec<Job>,
    /// Saved as a list of pairs: a made bot can't be a JSON object key.
    #[serde(with = "chat_list")]
    pub chats: HashMap<Member, Vec<Said>>,
    pub next_id: u64,
    /// Bots the person made in the maker on Crew.
    pub made: Vec<super::data::Made>,
    /// Built-in bots removed from the crew (or never added).
    #[serde(alias = "hidden")]
    pub removed: Vec<Member>,
    /// The first-run walkthrough is done (or was skipped).
    pub set_up: bool,
    /// Bots pinned near the top of the sidebar.
    pub pinned: Vec<Member>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Running,
    Done,
    Failed,
    Stopped,
    /// Runo closed while it ran.
    Interrupted,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Running => "Working",
            Status::Done => "Done",
            Status::Failed => "Stopped with a problem",
            Status::Stopped => "Stopped by you",
            Status::Interrupted => "Interrupted",
        }
    }
}

/// One line in a job's feed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Item {
    /// What the bot said.
    Text(String),
    /// A step it took: "Read src/main.rs", "Run cargo test". `state`: 0 running,
    /// 1 done, 2 failed.
    Tool {
        id: String,
        label: String,
        kind: String,
        state: u8,
    },
    /// What the person added.
    You(String),
    /// Runo speaking: a hint or a warning.
    Note(String),
    Error(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Job {
    pub id: u64,
    pub bot: Member,
    pub mode: Mode,
    pub title: String,
    pub folder: Folder,
    /// "Claude Code · Sonnet 4.5": what ran it.
    pub engine: String,
    pub agent: String,
    pub started: u64,
    pub ended: Option<u64>,
    pub status: Status,
    pub feed: Vec<Item>,
    /// The agent's own todo list: (step, 0 todo / 1 doing / 2 done).
    pub plan: Vec<(String, u8)>,
    /// Files it changed.
    pub files: Vec<String>,
    /// "12s · 3 turns · $0.04".
    pub summary: String,
    /// The agent's conversation id, so a follow-up continues it.
    pub conversation: String,
    /// The person has opened it since it finished.
    pub seen: bool,
}

impl Default for Job {
    fn default() -> Self {
        Job {
            id: 0,
            bot: Member::Builder,
            mode: Mode::Build,
            title: String::new(),
            folder: Folder::default(),
            engine: String::new(),
            agent: String::new(),
            started: 0,
            ended: None,
            status: Status::Done,
            feed: Vec::new(),
            plan: Vec::new(),
            files: Vec::new(),
            summary: String::new(),
            conversation: String::new(),
            seen: true,
        }
    }
}

impl Job {
    /// Seconds it has run, or ran.
    pub fn seconds(&self) -> u64 {
        self.ended.unwrap_or_else(now).saturating_sub(self.started)
    }

    /// Steps done out of steps planned, when the agent keeps a plan.
    pub fn progress(&self) -> Option<(usize, usize)> {
        (!self.plan.is_empty()).then(|| {
            (
                self.plan.iter().filter(|s| s.1 == 2).count(),
                self.plan.len(),
            )
        })
    }

    /// The step it is on now, in words.
    pub fn doing(&self) -> String {
        if self.status != Status::Running {
            return self.status.label().to_string();
        }
        if let Some((step, _)) = self.plan.iter().find(|s| s.1 == 1) {
            return step.clone();
        }
        match self
            .feed
            .iter()
            .rev()
            .find(|i| matches!(i, Item::Tool { .. } | Item::Text(_)))
        {
            Some(Item::Tool { label, .. }) => label.clone(),
            Some(Item::Text(_)) => "Thinking it through".into(),
            _ => "Starting".into(),
        }
    }

    /// The last thing the bot said, for a one-line preview.
    pub fn last_words(&self) -> Option<&str> {
        self.feed.iter().rev().find_map(|i| match i {
            Item::Text(t) if !t.trim().is_empty() => Some(t.trim()),
            _ => None,
        })
    }

    pub fn needs_you(&self) -> bool {
        !self.seen
            && matches!(
                self.status,
                Status::Done | Status::Failed | Status::Interrupted
            )
    }
}

/// A line in a chat with a bot.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Said {
    pub mine: bool,
    pub text: String,
    /// Which engine answered, and how long it took.
    pub via: Option<String>,
    /// Runo speaking, not the bot: a problem or a hint.
    pub note: bool,
}

impl Said {
    pub fn mine(text: String) -> Self {
        Said {
            mine: true,
            text,
            ..Default::default()
        }
    }
    pub fn bot(text: String, via: String) -> Self {
        Said {
            text,
            via: Some(via),
            ..Default::default()
        }
    }
    pub fn note(text: impl Into<String>) -> Self {
        Said {
            text: text.into(),
            note: true,
            ..Default::default()
        }
    }
}

mod chat_list {
    use std::collections::HashMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{Member, Said};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Pairs(Vec<(Member, Vec<Said>)>),
        // Stores from before made bots: a map keyed by the bot's name.
        Map(HashMap<Member, Vec<Said>>),
    }

    pub fn serialize<S: Serializer>(
        m: &HashMap<Member, Vec<Said>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut v: Vec<(&Member, &Vec<Said>)> = m.iter().collect();
        v.sort_by_key(|(k, _)| k.key());
        v.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<HashMap<Member, Vec<Said>>, D::Error> {
        Ok(match Saved::deserialize(d)? {
            Saved::Pairs(v) => v.into_iter().collect(),
            Saved::Map(m) => m,
        })
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn path() -> PathBuf {
    crate::store::data_dir().join("v2.json")
}

impl Store {
    /// The saved store, or a fresh one that starts from v1's projects.
    pub fn load() -> Store {
        let mut s: Store = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| {
                let v1 = crate::store::State::load();
                Store {
                    folders: v1
                        .projects
                        .iter()
                        .rev()
                        .filter(|p| std::path::Path::new(&p.path).is_dir())
                        .map(|p| Folder {
                            name: p.name.clone(),
                            path: p.path.clone(),
                        })
                        .collect(),
                    ..Default::default()
                }
            });
        for j in &mut s.jobs {
            if j.status == Status::Running {
                j.status = Status::Interrupted;
                j.ended = Some(j.ended.unwrap_or(j.started));
                j.feed
                    .push(Item::Note("Runo closed while this was running.".into()));
            }
        }
        s.folders.retain(|f| std::path::Path::new(&f.path).is_dir());
        // People who used v2 before the walkthrough existed don't need it.
        if !s.set_up && (!s.jobs.is_empty() || !s.chats.is_empty()) {
            s.set_up = true;
        }
        super::data::set_made(&s.made);
        super::data::set_removed(&s.removed);
        if s.current >= s.folders.len() {
            s.current = 0;
        }
        s
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(tmp, p);
            }
        }
    }

    pub fn folder(&self) -> Option<&Folder> {
        self.folders.get(self.current)
    }

    /// Adds a folder (or picks it if it's there already) and makes it current.
    pub fn add_folder(&mut self, path: PathBuf) {
        let p = path.to_string_lossy().into_owned();
        if let Some(i) = self.folders.iter().position(|f| f.path == p) {
            self.current = i;
            return;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.clone());
        self.folders.insert(0, Folder { name, path: p });
        self.current = 0;
    }

    pub fn job(&self, id: u64) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    pub fn job_mut(&mut self, id: u64) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn running(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter().filter(|j| j.status == Status::Running)
    }

    /// The job a bot is on right now.
    pub fn busy(&self, who: Member) -> Option<&Job> {
        self.running().find(|j| j.bot == who)
    }

    pub fn chat(&self, who: Member) -> &[Said] {
        self.chats.get(&who).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// "just now", "4m", "2h", "3d".
pub fn ago(at: u64) -> String {
    let s = now().saturating_sub(at);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// "45s", "3m 20s", "1h 5m".
pub fn span(s: u64) -> String {
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chats_with_made_bots_save_and_load() {
        let mut s = Store::default();
        s.chats
            .insert(Member::Explainer, vec![Said::mine("hi".into())]);
        s.chats
            .insert(Member::Made(3), vec![Said::mine("yo".into())]);
        let text = serde_json::to_string(&s).unwrap();
        let back: Store = serde_json::from_str(&text).unwrap();
        assert_eq!(back.chat(Member::Made(3))[0].text, "yo");
        assert_eq!(back.chat(Member::Explainer)[0].text, "hi");
    }

    #[test]
    fn old_stores_still_load() {
        let old = r#"{"chats":{"Explainer":[{"mine":true,"text":"hi"}]},"next_id":4}"#;
        let back: Store = serde_json::from_str(old).unwrap();
        assert_eq!(back.chat(Member::Explainer)[0].text, "hi");
        assert_eq!(back.next_id, 4);
    }
}
