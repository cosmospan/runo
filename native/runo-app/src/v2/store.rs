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
    /// Goals split into jobs.
    pub goals: Vec<super::goal::Goal>,
    /// A made-up store for looking at every screen (RUNO_V2_DEMO): never saved.
    #[serde(skip)]
    pub demo: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    pub name: String,
    pub path: String,
    /// What Approve does here: "pr", "merge", or "" to decide from the remote.
    pub ship: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Running,
    Done,
    Failed,
    Stopped,
    /// Runo closed while it ran.
    Interrupted,
    /// It changed files in its own worktree and waits for you to approve them.
    Review,
    /// Approved in a GitHub project: Shipper has (or is opening) a pull request
    /// and watches it.
    Shipping,
    /// Approved and landed: merged here, or its pull request merged.
    Shipped,
    /// You threw its changes away.
    Discarded,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Running => "Working",
            Status::Done => "Done",
            Status::Failed => "Stopped with a problem",
            Status::Stopped => "Stopped by you",
            Status::Interrupted => "Interrupted",
            Status::Review => "Ready for review",
            Status::Shipping => "Pull request open",
            Status::Shipped => "Shipped",
            Status::Discarded => "Discarded",
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
    /// Another bot on the crew speaking about this job, such as Shipper
    /// reporting a step.
    By(Member, String),
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
    /// The branch and worktree a writing job works in, apart from your own
    /// checkout, and the branch it started from. Empty when it worked in the folder.
    pub branch: String,
    pub worktree: String,
    pub base: String,
    /// Where `base` was when the job started, for its diff.
    pub base_sha: String,
    /// What it changed, once it finished.
    pub changes: Vec<Changed>,
    /// Checker's run of the project's checks on those changes.
    pub checks: Vec<crate::proof::CheckRun>,
    /// Checker is running them now.
    pub checking: bool,
    /// What happened when it was approved: "Merged into main as 3f2a1c9", or the
    /// pull request's address.
    pub shipped: String,
    /// Its pull request's address, once Shipper opened one.
    pub pr: String,
    pub pr_number: u64,
    /// "Checks running: 3 of 5 done", from the last look at the pull request.
    pub pr_line: String,
    /// OPEN, MERGED or CLOSED.
    pub pr_state: String,
    /// Failed checks already reported, so each failure is told once.
    pub pr_failed: Vec<String>,
    /// Merge the pull request as soon as its checks pass.
    pub auto_merge: bool,
    /// Shipper is pushing and opening the pull request now.
    pub shipping: bool,
    /// The race this job is one try in: the id of its first try. 0 when it
    /// isn't racing.
    pub race: u64,
    /// The engine this try runs on, when a race picked it: an agent id and a
    /// model ("" for the agent's own). Empty means the bot's engine.
    pub pick: String,
    pub pick_model: String,
    /// Files that clash with the base branch, when approving it stopped on them.
    pub conflict: Vec<String>,
    /// The goal this job is one part of, and what the bot is told about it.
    pub goal: u64,
    pub context: String,
}

/// One file a job changed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Changed {
    pub path: String,
    /// Git's letter: A added, M modified, D deleted.
    pub status: String,
    pub added: usize,
    pub removed: usize,
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
            branch: String::new(),
            worktree: String::new(),
            base: String::new(),
            base_sha: String::new(),
            changes: Vec::new(),
            checks: Vec::new(),
            checking: false,
            shipped: String::new(),
            pr: String::new(),
            pr_number: 0,
            pr_line: String::new(),
            pr_state: String::new(),
            pr_failed: Vec::new(),
            auto_merge: true,
            shipping: false,
            race: 0,
            pick: String::new(),
            pick_model: String::new(),
            conflict: Vec::new(),
            goal: 0,
            context: String::new(),
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

    /// Lines added and removed across its changes.
    pub fn lines(&self) -> (usize, usize) {
        self.changes
            .iter()
            .fold((0, 0), |a, c| (a.0 + c.added, a.1 + c.removed))
    }

    /// The folder it runs in: its worktree, or the project folder.
    pub fn dir(&self) -> &str {
        if self.worktree.is_empty() {
            &self.folder.path
        } else {
            &self.worktree
        }
    }

    pub fn needs_you(&self) -> bool {
        // Changes waiting for approval stay in Needs you until you decide.
        if self.status == Status::Review {
            return true;
        }
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
                            ship: String::new(),
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
            if j.checking {
                j.checking = false;
            }
            // Runo closed while Shipper was opening the pull request.
            if j.shipping {
                j.shipping = false;
                if j.pr.is_empty() && j.status == Status::Shipping {
                    j.status = Status::Review;
                }
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
        if self.demo {
            return;
        }
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
        self.folders.insert(
            0,
            Folder {
                name,
                path: p,
                ship: String::new(),
            },
        );
        self.current = 0;
    }

    /// Jobs newest first, with each race shown once: by the try that matters
    /// most now (kept, then waiting for review, then still working).
    pub fn listed(&self) -> Vec<&Job> {
        let rank = |j: &Job| match j.status {
            Status::Shipping | Status::Shipped => 0,
            Status::Review => 1,
            Status::Running => 2,
            Status::Done => 3,
            _ => 4,
        };
        let mut face: HashMap<u64, &Job> = HashMap::new();
        for j in self.jobs.iter().filter(|j| j.race != 0) {
            let e = face.entry(j.race).or_insert(j);
            if (rank(j), j.id) < (rank(e), e.id) {
                *e = j;
            }
        }
        self.jobs
            .iter()
            .rev()
            .filter(|j| j.race == 0 || face.get(&j.race).is_some_and(|f| f.id == j.id))
            .collect()
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
