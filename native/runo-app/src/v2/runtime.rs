//! The crew itself, apart from any window: the saved store and everything
//! that runs for it. Jobs on their engines, Checker's checks, Shipper's pull
//! requests, goals moving from one job to the next, questions to a bot, and
//! jobs sent from `squad` in a terminal.
//!
//! The window (`Shell`) is one client of a `Runtime`: it draws the store and
//! calls the methods here, then says what happened. Nothing here touches the
//! UI, so the same crew can run with no window at all, such as on a server.
//! What the crew wants said to the person (a merge landed, a job came in from
//! the terminal) goes in `notes` for whichever client is showing it.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use crate::settings::AppSettings;

use super::data::{Member, Mode};
use super::store::{self, Said, Status, Store};
use super::{brain, cli, engine, goal, jobs, ship, trouble, work};

type Landing = Receiver<Result<(), String>>;

/// What starting something opened, so a client can go and show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    Job(u64),
    Race(u64),
    /// The bot is still on another job, which this one waits for.
    Busy(u64),
}

pub struct Runtime {
    pub store: Store,
    /// Jobs' agent processes while they run.
    pub runners: Vec<jobs::Runner>,
    /// A question on its way to a bot's engine.
    pub asking: Option<engine::Ask>,
    /// What the crew wants said, oldest first, until a client takes it.
    pub notes: Vec<String>,
    /// When the store was last kept while jobs ran.
    saved: Instant,
    /// Checker's runs of the checks on finished jobs, by job.
    checking: Vec<(u64, Receiver<Vec<crate::proof::CheckRun>>)>,
    /// Shipper opening pull requests, by job.
    shipping: Vec<(u64, Receiver<ship::Step>)>,
    /// Looks at open pull requests, by job, and when the last round started.
    watching: Vec<(u64, Receiver<Option<crate::github::PrStatus>>)>,
    watched: Instant,
    /// Pull requests being merged (true) or closed (false) on GitHub, by job.
    landing: Vec<(u64, bool, Landing)>,
    /// What Approve does in each folder, worked out once per folder.
    ways: HashMap<String, ship::Way>,
    /// When the queue from `squad` in a terminal was last read.
    queue_at: Instant,
    /// Goals Squad Code is splitting now.
    planning: Vec<(u64, Receiver<crate::agent::Event>)>,
    /// Where jobs can run besides here, for the intelligence layer to choose.
    pub sites: brain::Sites,
    /// Titles and recaps Peek is writing, by job.
    peeking: Vec<(u64, Receiver<brain::tasks::Said>)>,
}

impl Runtime {
    pub fn new(store: Store) -> Self {
        Runtime {
            store,
            runners: Vec::new(),
            asking: None,
            notes: Vec::new(),
            saved: Instant::now(),
            checking: Vec::new(),
            shipping: Vec::new(),
            watching: Vec::new(),
            watched: Instant::now() - Duration::from_secs(3600),
            landing: Vec::new(),
            ways: HashMap::new(),
            queue_at: Instant::now(),
            planning: Vec::new(),
            sites: brain::Sites::default(),
            peeking: Vec::new(),
        }
    }

    fn say(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// Something is running or on its way back, so the crew needs polling often.
    pub fn busy(&self) -> bool {
        !self.runners.is_empty()
            || self.asking.is_some()
            || !self.checking.is_empty()
            || !self.shipping.is_empty()
            || !self.landing.is_empty()
            || !self.planning.is_empty()
            || !self.peeking.is_empty()
    }

    /// Whether `who` is answering a question right now.
    pub fn thinking(&self, who: Member) -> bool {
        self.asking.as_ref().is_some_and(|a| a.who == who)
    }

    // ---------- starting work ----------

    /// A new job for `bot`, unless it's still on another one.
    pub fn start_job(
        &mut self,
        st: &AppSettings,
        bot: Member,
        mode: Mode,
        text: String,
        folder: store::Folder,
    ) -> Opened {
        if let Some(busy) = self.store.busy(bot) {
            let (title, id) = (busy.title.clone(), busy.id);
            self.say(format!(
                "{} is still on \u{201c}{title}\u{201d}. This one waits for you to try again.",
                bot.name()
            ));
            return Opened::Busy(id);
        }
        let job = self.make_job(st, bot, mode, &text, folder, None, None);
        let id = job.id;
        self.store.jobs.push(job);
        self.store.save();
        Opened::Job(id)
    }

    /// A new job, on its own branch when it may change files, started on its
    /// bot's engine, or on `pick` (an agent index) for a try in a race. The
    /// caller adds it to the store.
    #[allow(clippy::too_many_arguments)]
    pub fn make_job(
        &mut self,
        st: &AppSettings,
        bot: Member,
        mode: Mode,
        text: &str,
        folder: store::Folder,
        pick: Option<(usize, u64)>,
        part: Option<(u64, String)>,
    ) -> store::Job {
        self.make_placed_job(st, bot, mode, text, folder, pick, part)
            .0
    }

    /// [`Runtime::make_job`], with where the intelligence layer placed it. A
    /// client that can send jobs to a server starts it there when the
    /// placement's site is `Site::Server`.
    #[allow(clippy::too_many_arguments)]
    pub fn make_placed_job(
        &mut self,
        st: &AppSettings,
        bot: Member,
        mode: Mode,
        text: &str,
        folder: store::Folder,
        pick: Option<(usize, u64)>,
        part: Option<(u64, String)>,
    ) -> (store::Job, brain::place::Placement) {
        self.store.next_id += 1;
        let mut job = store::Job {
            id: self.store.next_id,
            bot,
            mode,
            title: text.to_string(),
            folder,
            started: store::now(),
            seen: false,
            ..Default::default()
        };
        if let Some((goal, context)) = part {
            job.goal = goal;
            job.context = context;
        }
        // Peek's short title for the card, when Smart Peek is on.
        if let Some(rx) = brain::tasks::ask(st, &job, false) {
            self.peeking.push((job.id, rx));
        }
        if let Some((i, race)) = pick {
            job.pick = crate::agent::PROVIDERS[i].id.to_string();
            job.race = if race == 0 { job.id } else { race };
        }
        // Where it runs, and why, when there's a choice to make.
        let placed = brain::place(
            st,
            &brain::JobAsk {
                bot,
                mode,
                text: text.to_string(),
                folder: job.folder.clone(),
                race: usize::from(pick.is_some()) * 2,
                goal: job.goal != 0,
            },
            &self.sites,
        );
        if !placed.why.is_empty() {
            job.feed.push(store::Item::Note(placed.why.clone()));
        }
        // A job that may change files gets its own branch and worktree, so your
        // checkout stays as it is and its changes wait for your approval.
        if mode.writes() {
            match work::isolate(&job.folder.path, text, job.id) {
                Ok(Some(iso)) => {
                    job.feed.push(store::Item::Note(format!(
                        "Working on its own branch, {}, so your checkout stays as it is.",
                        iso.branch
                    )));
                    job.branch = iso.branch;
                    job.worktree = iso.worktree;
                    job.base = iso.base;
                    job.base_sha = iso.base_sha;
                }
                Ok(None) => {}
                Err(why) => job.feed.push(store::Item::Note(format!(
                    "Couldn't give it its own branch ({why}), so it works in your folder."
                ))),
            }
        }
        match jobs::start(st, &mut job, text, false) {
            Ok(r) => self.runners.push(r),
            Err(why) => {
                job.status = Status::Failed;
                job.ended = Some(job.started);
                let (i, model) = engine::for_job(st, &job);
                job.engine = engine::label_of(i, &model);
                job.feed.push(store::Item::Error(why));
            }
        }
        (job, placed)
    }

    /// Races one job on up to `n` engines at once, each try on its own branch.
    /// With only one engine ready it runs once.
    pub fn start_race(
        &mut self,
        st: &AppSettings,
        bot: Member,
        mode: Mode,
        text: String,
        folder: store::Folder,
        n: usize,
    ) -> Opened {
        let engines = engine::race_engines(st, bot, n);
        if engines.len() < 2 {
            self.say("Only one engine is ready on this computer, so this runs once. Add another in Settings to race.");
            return self.start_job(st, bot, mode, text, folder);
        }
        let mut race = 0;
        for i in engines {
            let job = self.make_job(st, bot, mode, &text, folder.clone(), Some((i, race)), None);
            race = job.race;
            self.store.jobs.push(job);
        }
        self.store.save();
        Opened::Race(race)
    }

    /// Starts the same job again on other engines, so it races the first try.
    /// The race's id, or None when there was nothing to race it on.
    pub fn race_it(&mut self, st: &AppSettings, id: u64) -> Option<u64> {
        let first = self.store.job(id).cloned()?;
        if !first.mode.writes() {
            return None;
        }
        let race = if first.race == 0 {
            first.id
        } else {
            first.race
        };
        let taken: Vec<String> = self
            .store
            .jobs
            .iter()
            .filter(|j| j.id == id || (j.race != 0 && j.race == race))
            .map(|j| j.agent.clone())
            .collect();
        let engines: Vec<usize> = engine::race_engines(st, first.bot, 8)
            .into_iter()
            .filter(|i| !taken.iter().any(|t| t == crate::agent::PROVIDERS[*i].id))
            .take(3usize.saturating_sub(taken.len()).max(1))
            .collect();
        if engines.is_empty() {
            self.say("No other engine is ready on this computer. Add one in Settings to race.");
            return None;
        }
        if let Some(j) = self.store.job_mut(id) {
            j.race = race;
            if j.pick.is_empty() {
                j.pick = j.agent.clone();
            }
        }
        for i in engines {
            let job = self.make_job(
                st,
                first.bot,
                first.mode,
                &first.title,
                first.folder.clone(),
                Some((i, race)),
                None,
            );
            self.store.jobs.push(job);
        }
        self.store.save();
        Some(race)
    }

    /// Runs a job again from the start, in the same branch, on its own engine or
    /// on `pick` (an agent index) when its own can't run it now.
    pub fn rerun(&mut self, st: &AppSettings, id: u64, pick: Option<usize>) -> bool {
        let Some(job) = self.store.job_mut(id) else {
            return false;
        };
        if job.status == Status::Running {
            return false;
        }
        if let Some(i) = pick {
            job.pick = crate::agent::PROVIDERS[i].id.to_string();
            job.pick_model.clear();
        }
        let (i, model) = engine::for_job(st, job);
        job.feed.push(store::Item::Note(format!(
            "Starting again on {}.",
            engine::label_of(i, &model)
        )));
        job.conflict.clear();
        let text = if job.worktree.is_empty() {
            job.title.clone()
        } else {
            format!(
                "{}\n\nAn earlier run may have left work in the files. Check what is already done before redoing it.",
                job.title
            )
        };
        match jobs::start(st, job, &text, false) {
            Ok(r) => self.runners.push(r),
            Err(why) => {
                job.status = Status::Failed;
                job.feed.push(store::Item::Error(why));
            }
        }
        self.store.save();
        true
    }

    /// More words for a job: while it runs they wait; once it's done they start a
    /// follow-up that continues the same conversation when the engine can.
    pub fn follow_up(&mut self, st: &AppSettings, id: u64, text: String) {
        let Some(job) = self.store.job_mut(id) else {
            return;
        };
        if job.status == Status::Running {
            self.say("It's still working. Add this once it finishes.");
            return;
        }
        job.feed.push(store::Item::You(text.clone()));
        match jobs::start(st, job, &text, true) {
            Ok(r) => self.runners.push(r),
            Err(why) => job.feed.push(store::Item::Error(why)),
        }
        self.store.save();
    }

    /// A stopped or interrupted job picks up where it was.
    pub fn carry_on(&mut self, st: &AppSettings, id: u64) {
        self.follow_up(
            st,
            id,
            "Carry on from where you stopped. Check what is already done in the files first."
                .into(),
        );
    }

    /// Hands a branch that clashes with its base back to its bot to bring up to
    /// date; it comes back for review when it's done.
    pub fn resolve(&mut self, st: &AppSettings, id: u64) {
        let Some(job) = self.store.job(id) else {
            return;
        };
        let text = trouble::resolve_prompt(job);
        if let Some(j) = self.store.job_mut(id) {
            j.conflict.clear();
        }
        self.follow_up(st, id, text);
    }

    pub fn stop(&mut self, id: u64) {
        if let Some(r) = self.runners.iter_mut().find(|r| r.id == id) {
            r.stop();
        }
    }

    // ---------- goals ----------

    /// Has Squad Code split a goal into jobs, to look over before it starts.
    pub fn begin_goal(&mut self, st: &AppSettings, text: String, folder: store::Folder) -> u64 {
        self.store.next_id += 1;
        let id = self.store.next_id;
        let mut g = goal::Goal {
            id,
            text: text.clone(),
            folder: folder.clone(),
            started: store::now(),
            ..Default::default()
        };
        match goal::plan(st, &folder, &text) {
            Ok(rx) => self.planning.push((id, rx)),
            Err(why) => {
                g.state = goal::State::Failed;
                g.note = why;
            }
        }
        self.store.goals.push(g);
        self.store.save();
        id
    }

    /// Starts a goal's jobs, the first one now.
    pub fn start_goal(&mut self, st: &AppSettings, id: u64) {
        if let Some(g) = self.store.goals.iter_mut().find(|g| g.id == id) {
            g.state = goal::State::Running;
        }
        self.advance_goals(st);
        self.store.save();
    }

    /// Goes on past a job that stopped.
    pub fn skip_step(&mut self, st: &AppSettings, id: u64) {
        let jobs = self.store.jobs.clone();
        if let Some(g) = self.store.goals.iter_mut().find(|g| g.id == id) {
            if let Some(k) = g.current(&jobs) {
                g.steps[k].skipped = true;
            }
            g.state = goal::State::Running;
        }
        self.advance_goals(st);
        self.store.save();
    }

    pub fn drop_goal(&mut self, id: u64) {
        self.store.goals.retain(|g| g.id != id);
        self.planning.retain(|(g, _)| *g != id);
        self.store.save();
    }

    /// Splits coming back, and running goals moving on.
    fn poll_goals(&mut self, st: &AppSettings) {
        let mut back = Vec::new();
        self.planning.retain(|(id, rx)| match rx.try_recv() {
            Ok(crate::agent::Event::Output(_, r)) => {
                back.push((*id, r));
                false
            }
            Ok(_) | Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                back.push((*id, Err("it stopped before answering".into())));
                false
            }
        });
        for (id, r) in back {
            if let Some(g) = self.store.goals.iter_mut().find(|g| g.id == id) {
                match r.map(|t| goal::parse_split(&crate::agent::strip_ansi(&t))) {
                    Ok(steps) if !steps.is_empty() => {
                        g.steps = steps;
                        g.state = goal::State::Ready;
                    }
                    Ok(_) => {
                        g.state = goal::State::Failed;
                        g.note = "Squad Code couldn't split this into jobs. Say it another way, or run it as one job.".into();
                    }
                    Err(why) => {
                        g.state = goal::State::Failed;
                        let first = why
                            .lines()
                            .rev()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        g.note = format!("Squad Code couldn't split this into jobs: {first}");
                    }
                }
            }
            self.store.save();
        }
        if self
            .store
            .goals
            .iter()
            .any(|g| g.state == goal::State::Running)
        {
            self.advance_goals(st);
        }
    }

    /// Starts the next job of each running goal once the one before it shipped,
    /// and pauses a goal whose job stopped.
    fn advance_goals(&mut self, st: &AppSettings) {
        let mut start = Vec::new();
        let jobs = self.store.jobs.clone();
        let mut said = Vec::new();
        for g in &mut self.store.goals {
            if g.state != goal::State::Running {
                continue;
            }
            match g.current(&jobs) {
                None => {
                    g.state = goal::State::Done;
                    said.push(format!("Goal done: \u{201c}{}\u{201d}.", g.text));
                }
                Some(k) if g.steps[k].job == 0 => start.push((g.id, k)),
                Some(k) => {
                    if jobs
                        .iter()
                        .find(|j| j.id == g.steps[k].job)
                        .is_some_and(goal::step_stuck)
                    {
                        g.state = goal::State::Paused;
                    }
                }
            }
        }
        for m in said {
            self.say(m);
        }
        for (gid, k) in start {
            let Some(g) = self.store.goals.iter().find(|g| g.id == gid).cloned() else {
                continue;
            };
            let step = g.steps[k].clone();
            let bot = Member::doer_for(step.mode).unwrap_or(Member::for_mode(step.mode));
            let part = Some((gid, goal::context(&g, k)));
            let job = self.make_job(
                st,
                bot,
                step.mode,
                &step.title,
                g.folder.clone(),
                None,
                part,
            );
            let jid = job.id;
            self.store.jobs.push(job);
            if let Some(g) = self.store.goals.iter_mut().find(|g| g.id == gid) {
                g.steps[k].job = jid;
            }
            self.store.save();
        }
    }

    // ---------- the queue from `squad` in a terminal ----------

    /// Jobs and decisions sent from `squad` in a terminal, once a second.
    fn poll_queue(&mut self, st: &AppSettings) {
        if self.queue_at.elapsed() < Duration::from_secs(1) || self.store.demo {
            return;
        }
        self.queue_at = Instant::now();
        for ask in cli::take_queue() {
            self.handle(st, ask);
        }
    }

    /// Fin suggests a bot worth making, once per idea, when the same kind of
    /// job keeps coming up and no bot is for it.
    fn offer_idea(&mut self) {
        let crew = Member::all();
        let Some(idea) = brain::ideas::next(&self.store, &crew, &self.store.offered, store::now())
        else {
            return;
        };
        self.store.offered.push(idea.key.to_string());
        self.say(brain::voice::line(&brain::voice::Moment::Idea {
            name: idea.name.to_string(),
            why: idea.why,
        }));
    }

    /// Does one thing asked from the command line (or any other client).
    pub fn handle(&mut self, st: &AppSettings, ask: cli::Ask) {
        match ask {
            cli::Ask::New {
                text,
                mode,
                bot,
                dir,
            } => {
                let folder = self.folder_for(&dir);
                let who = Member::all()
                    .into_iter()
                    .find(|m| m.name() == bot)
                    .or_else(|| Member::doer_for(mode))
                    .unwrap_or(Member::for_mode(mode));
                let text = if text.is_empty() {
                    mode.default_job().unwrap_or("").to_string()
                } else {
                    text
                };
                let job = self.make_job(st, who, mode, &text, folder, None, None);
                self.say(format!(
                    "{} took a job from the terminal: \u{201c}{}\u{201d}.",
                    who.name(),
                    job.title
                ));
                self.store.jobs.push(job);
                self.store.save();
            }
            cli::Ask::Approve { id } => self.approve(id),
            cli::Ask::Discard { id } => self.discard(id),
            cli::Ask::Stop { id } => self.stop(id),
        }
    }

    /// The project a terminal's folder belongs to: a known one it's inside,
    /// the project of a job's worktree it's in, or a new one.
    pub fn folder_for(&mut self, dir: &str) -> store::Folder {
        let inside = |root: &str| {
            !root.is_empty() && std::path::Path::new(dir).starts_with(std::path::Path::new(root))
        };
        if let Some(j) = self.store.jobs.iter().find(|j| inside(&j.worktree)) {
            return j.folder.clone();
        }
        if let Some(f) = self
            .store
            .folders
            .iter()
            .filter(|f| inside(&f.path))
            .max_by_key(|f| f.path.len())
        {
            return f.clone();
        }
        // Added at the top: the folder you had picked moves down one.
        let current = self.store.current;
        let before = self.store.folders.len();
        self.store.add_folder(std::path::PathBuf::from(dir));
        let f = self.store.folder().cloned().unwrap_or_default();
        if self.store.folders.len() > before && before > 0 {
            self.store.current = current + 1;
        }
        f
    }

    // ---------- review and shipping ----------

    /// The tries in a race, first one first.
    pub fn tries(&self, race: u64) -> Vec<store::Job> {
        let mut v: Vec<_> = self
            .store
            .jobs
            .iter()
            .filter(|j| j.race == race && race != 0)
            .cloned()
            .collect();
        v.sort_by_key(|j| j.id);
        v
    }

    /// Keeps one try of a race: approves it and throws the others away.
    pub fn keep_try(&mut self, id: u64) {
        let Some(race) = self.store.job(id).map(|j| j.race) else {
            return;
        };
        for other in self.tries(race) {
            if other.id == id {
                continue;
            }
            if let Some(pos) = self.runners.iter().position(|r| r.id == other.id) {
                let mut r = self.runners.remove(pos);
                r.stop();
            }
            self.checking.retain(|(j, _)| *j != other.id);
            if matches!(
                other.status,
                Status::Shipped | Status::Shipping | Status::Discarded
            ) {
                continue;
            }
            work::discard(&other);
            if let Some(j) = self.store.job_mut(other.id) {
                j.status = Status::Discarded;
                j.ended.get_or_insert_with(store::now);
                j.worktree.clear();
                j.seen = true;
                j.feed.push(store::Item::Note(
                    "Another try won the race, so this one was thrown away.".into(),
                ));
            }
        }
        self.approve_one(id);
        self.store.save();
    }

    /// A writing job finished in its own worktree: list what it changed and have
    /// Checker run the checks. With nothing changed there's nothing to review.
    fn begin_review(&mut self, id: u64) {
        let Some(job) = self.store.job_mut(id) else {
            return;
        };
        if job.status != Status::Done || job.worktree.is_empty() {
            return;
        }
        job.changes = work::changed(job);
        if job.changes.is_empty() {
            work::discard(job);
            job.worktree.clear();
            job.feed
                .push(store::Item::Note("It didn't change any files.".into()));
            return;
        }
        job.status = Status::Review;
        job.seen = false;
        job.checks.clear();
        job.checking = true;
        let dir = job.worktree.clone();
        self.checking.push((id, work::run_checks(dir)));
    }

    /// Runs the checks on a job's changes again.
    pub fn recheck(&mut self, id: u64) {
        let Some(job) = self.store.job_mut(id) else {
            return;
        };
        if job.worktree.is_empty() || job.checking {
            return;
        }
        job.checks.clear();
        job.checking = true;
        let dir = job.worktree.clone();
        self.checking.retain(|(j, _)| *j != id);
        self.checking.push((id, work::run_checks(dir)));
    }

    /// What Approve does for this job's project.
    pub fn way(&mut self, job: &store::Job) -> ship::Way {
        let key = format!("{}\u{0}{}", job.folder.path, job.folder.ship);
        if let Some(w) = self.ways.get(&key) {
            return *w;
        }
        let ship_pref = self
            .store
            .folders
            .iter()
            .find(|f| f.path == job.folder.path)
            .map(|f| f.ship.clone())
            .unwrap_or_default();
        let w = ship::way_for(&job.folder.path, &ship_pref);
        self.ways.insert(key, w);
        w
    }

    /// Switches what Approve does in a job's project: pull request or local merge.
    pub fn set_way(&mut self, id: u64, way: ship::Way) {
        let Some(path) = self.store.job(id).map(|j| j.folder.path.clone()) else {
            return;
        };
        let pref = match way {
            ship::Way::PullRequest => "pr",
            ship::Way::Merge => "merge",
        };
        for f in &mut self.store.folders {
            if f.path == path {
                f.ship = pref.into();
            }
        }
        for j in &mut self.store.jobs {
            if j.folder.path == path {
                j.folder.ship = pref.into();
            }
        }
        self.ways.clear();
        self.store.save();
    }

    /// Approves a job's changes: a pull request from Shipper on GitHub projects,
    /// otherwise merged into the branch it started from. Approving one try of a
    /// race keeps it and throws the others away.
    pub fn approve(&mut self, id: u64) {
        let race = self.store.job(id).map(|j| j.race).unwrap_or(0);
        if race != 0 && self.tries(race).len() > 1 {
            self.keep_try(id);
        } else {
            self.approve_one(id);
        }
    }

    fn approve_one(&mut self, id: u64) {
        let Some(job) = self.store.job(id).cloned() else {
            return;
        };
        if job.status != Status::Review {
            return;
        }
        if self.way(&job) == ship::Way::PullRequest {
            if let Some(j) = self.store.job_mut(id) {
                j.status = Status::Shipping;
                j.shipping = true;
                j.seen = true;
                j.feed.push(store::Item::By(
                    Member::Shipper,
                    "Approved. Opening a pull request.".into(),
                ));
            }
            self.shipping.push((id, ship::open_pr(job)));
            self.store.save();
            return;
        }
        match work::merge(&job) {
            Ok(hash) => {
                let msg = format!("Merged into {} as {hash}.", job.base);
                if let Some(j) = self.store.job_mut(id) {
                    j.status = Status::Shipped;
                    j.shipped = msg.clone();
                    j.worktree.clear();
                    j.seen = true;
                    j.feed.push(store::Item::Note(msg.clone()));
                }
                self.say(msg);
            }
            Err(why) => {
                let files = trouble::conflict_files(&why);
                if let Some(j) = self.store.job_mut(id) {
                    if files.is_empty() {
                        j.feed
                            .push(store::Item::Error(format!("Couldn't merge it: {why}")));
                    } else {
                        j.feed.push(store::Item::Note(format!(
                            "{} moved on and {} now clash, so nothing was merged.",
                            j.base,
                            files.join(", ")
                        )));
                        j.conflict = files;
                    }
                }
                self.say("Couldn't merge it. The reason is on the job.");
            }
        }
        self.store.save();
    }

    /// Throws a job's changes away, with its branch and worktree.
    pub fn discard(&mut self, id: u64) {
        let Some(job) = self.store.job(id).cloned() else {
            return;
        };
        work::discard(&job);
        if let Some(j) = self.store.job_mut(id) {
            j.status = Status::Discarded;
            j.worktree.clear();
            j.seen = true;
            j.feed
                .push(store::Item::Note("You discarded its changes.".into()));
        }
        self.say("Discarded. Your checkout was never touched.");
        self.store.save();
    }

    /// Merges (or closes) a job's pull request on GitHub now.
    pub fn land_pr(&mut self, id: u64, merge: bool) {
        let Some(job) = self.store.job(id) else {
            return;
        };
        if job.pr.is_empty() || self.landing.iter().any(|(j, ..)| *j == id) {
            return;
        }
        let (dir, url) = (job.folder.path.clone(), job.pr.clone());
        let rx = if merge {
            ship::merge_pr(dir, url)
        } else {
            ship::close_pr(dir, url)
        };
        self.landing.push((id, merge, rx));
        if let Some(j) = self.store.job_mut(id) {
            j.feed.push(store::Item::By(
                Member::Shipper,
                if merge {
                    "Merging the pull request.".into()
                } else {
                    "Closing the pull request.".into()
                },
            ));
        }
    }

    /// Turns "merge when checks pass" on or off for a job's pull request.
    pub fn set_auto_merge(&mut self, id: u64, on: bool) {
        if let Some(j) = self.store.job_mut(id) {
            j.auto_merge = on;
        }
        self.store.save();
    }

    /// Shipper's side of the poll: steps while opening pull requests, a look at
    /// open ones every half minute, and merges or closes coming back.
    fn poll_ship(&mut self) {
        let mut steps = Vec::new();
        self.shipping.retain(|(id, rx)| loop {
            match rx.try_recv() {
                Ok(step) => {
                    let end = !matches!(step, ship::Step::Did(_));
                    steps.push((*id, step));
                    if end {
                        break false;
                    }
                }
                Err(TryRecvError::Empty) => break true,
                Err(TryRecvError::Disconnected) => break false,
            }
        });
        let mut look_now = false;
        for (id, step) in steps {
            let Some(j) = self.store.job_mut(id) else {
                continue;
            };
            match step {
                ship::Step::Did(what) => j.feed.push(store::Item::By(Member::Shipper, what)),
                ship::Step::Opened(url) => {
                    j.shipping = false;
                    j.pr_number = url
                        .rsplit('/')
                        .next()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                    j.pr = url.clone();
                    j.pr_state = "OPEN".into();
                    j.feed.push(store::Item::By(
                        Member::Shipper,
                        format!("Opened the pull request: {url}"),
                    ));
                    look_now = true;
                }
                ship::Step::Conflict(files) => {
                    j.shipping = false;
                    j.status = Status::Review;
                    j.seen = false;
                    j.feed.push(store::Item::By(
                        Member::Shipper,
                        format!(
                            "{} moved while this was waiting, and {} now clash with it, so I stopped before pushing.",
                            j.base,
                            files.join(", ")
                        ),
                    ));
                    j.conflict = files;
                }
                ship::Step::Failed(why) => {
                    j.shipping = false;
                    j.status = Status::Review;
                    j.seen = false;
                    j.feed.push(store::Item::Error(format!(
                        "Shipper couldn't open the pull request: {why}"
                    )));
                }
            }
            self.store.save();
        }
        // A look at every open pull request every 30 seconds.
        if look_now || self.watched.elapsed() > Duration::from_secs(30) {
            self.watched = Instant::now();
            let open: Vec<(u64, String, String)> = self
                .store
                .jobs
                .iter()
                .filter(|j| j.status == Status::Shipping && !j.pr.is_empty())
                .map(|j| (j.id, j.folder.path.clone(), j.pr.clone()))
                .collect();
            for (id, dir, url) in open {
                if !self.watching.iter().any(|(j, _)| *j == id) {
                    self.watching.push((id, ship::watch(dir, url)));
                }
            }
        }
        let mut seen = Vec::new();
        self.watching.retain(|(id, rx)| match rx.try_recv() {
            Ok(pr) => {
                seen.push((*id, pr));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        let mut merge_now = Vec::new();
        let mut landed = Vec::new();
        for (id, pr) in seen {
            let Some(pr) = pr else { continue };
            let busy = self.landing.iter().any(|(j, ..)| *j == id);
            let Some(j) = self.store.job_mut(id) else {
                continue;
            };
            j.pr_number = pr.number;
            j.pr_line = ship::checks_line(&pr);
            j.pr_state = pr.state.clone();
            for name in &pr.failed {
                if !j.pr_failed.contains(name) {
                    j.pr_failed.push(name.clone());
                    j.seen = false;
                    j.feed.push(store::Item::By(
                        Member::Shipper,
                        format!("{name} failed on GitHub. Write below to send it back and fix it."),
                    ));
                }
            }
            match pr.state.as_str() {
                "MERGED" => landed.push((id, true)),
                "CLOSED" => landed.push((id, false)),
                _ => {
                    let green = pr.failed.is_empty() && pr.pending == 0;
                    if j.auto_merge && green && pr.mergeable != "CONFLICTING" && !busy {
                        merge_now.push(id);
                    }
                }
            }
        }
        for id in merge_now {
            if let Some(j) = self.store.job_mut(id) {
                j.feed.push(store::Item::By(
                    Member::Shipper,
                    "Checks pass. Merging, as you asked.".into(),
                ));
                let (dir, url) = (j.folder.path.clone(), j.pr.clone());
                self.landing.push((id, true, ship::merge_pr(dir, url)));
            }
        }
        let mut came_back = Vec::new();
        self.landing.retain(|(id, merge, rx)| match rx.try_recv() {
            Ok(r) => {
                came_back.push((*id, *merge, r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for (id, merge, r) in came_back {
            match r {
                Ok(()) => landed.push((id, merge)),
                Err(why) => {
                    if let Some(j) = self.store.job_mut(id) {
                        j.feed
                            .push(store::Item::Error(format!("GitHub said no: {why}")));
                    }
                }
            }
        }
        for (id, merged) in landed {
            let Some(job) = self.store.job(id).cloned() else {
                continue;
            };
            if !matches!(job.status, Status::Shipping) {
                continue;
            }
            // The branch lives on GitHub now (or is gone): the local copy can go.
            work::discard(&job);
            let msg = if merged {
                format!(
                    "Merged #{} into {}. Cleaned up the branch and worktree.",
                    job.pr_number, job.base
                )
            } else {
                format!(
                    "Closed #{} without merging. Cleaned up the branch and worktree.",
                    job.pr_number
                )
            };
            if let Some(j) = self.store.job_mut(id) {
                j.status = if merged {
                    Status::Shipped
                } else {
                    Status::Discarded
                };
                j.pr_state = if merged { "MERGED" } else { "CLOSED" }.into();
                j.worktree.clear();
                j.shipped = msg.clone();
                j.feed.push(store::Item::By(Member::Shipper, msg.clone()));
            }
            self.say(msg);
            self.store.save();
        }
    }

    // ---------- questions ----------

    /// Asks `who` for real, on its own engine, in the current folder.
    pub fn ask(&mut self, st: &AppSettings, who: Member, text: String) {
        let history: Vec<(bool, String)> = self
            .store
            .chat(who)
            .iter()
            .filter(|m| !m.note)
            .map(|m| (m.mine, m.text.clone()))
            .collect();
        let log = self.store.chats.entry(who).or_default();
        log.push(Said::mine(text.clone()));
        if self.asking.is_some() {
            log.push(Said::note(
                "One question at a time: ask again when the last answer is in.",
            ));
            return;
        }
        let Some(folder) = self.store.folders.get(self.store.current).cloned() else {
            log.push(Said::note(
                "Pick a folder first (on Today), so the crew knows which code to read.",
            ));
            return;
        };
        match engine::Ask::start(st, who, &folder, &text, &history) {
            Ok(ask) => self.asking = Some(ask),
            Err(why) => self
                .store
                .chats
                .entry(who)
                .or_default()
                .push(Said::note(why)),
        }
        self.store.save();
    }

    // ---------- the poll ----------

    /// Moves everything along: answers and job output that have arrived, checks
    /// and pull requests coming back, goals, and the terminal's queue. `viewing`
    /// is the job a client has open, which counts as seen when it ends. Returns
    /// the jobs whose runs ended, for the client to say so.
    pub fn poll(&mut self, st: &AppSettings, viewing: Option<u64>) -> Vec<u64> {
        if let Some(ask) = &self.asking {
            if let Some(reply) = ask.poll() {
                let (who, engine) = (ask.who, ask.engine.clone());
                let secs = ask.started.elapsed().as_secs_f32();
                self.store.chats.entry(who).or_default().push(match reply {
                    engine::Reply::Answer(text) if !text.is_empty() => {
                        Said::bot(text, format!("{engine} · {secs:.0}s"))
                    }
                    engine::Reply::Answer(_) => {
                        Said::note("The engine answered with nothing. Try asking again.")
                    }
                    engine::Reply::Failed(why) => Said::note(why),
                });
                self.asking = None;
                self.store.save();
            }
        }
        let mut finished = Vec::new();
        for r in &mut self.runners {
            if let Some(job) = self.store.jobs.iter_mut().find(|j| j.id == r.id) {
                if r.pump(job) {
                    finished.push(r.id);
                    if viewing == Some(job.id) {
                        job.seen = true;
                    }
                }
            } else {
                finished.push(r.id);
            }
        }
        // While jobs run, what they've done so far is kept every few seconds.
        if !self.runners.is_empty() && self.saved.elapsed() > Duration::from_secs(5) {
            self.saved = Instant::now();
            self.store.save();
        }
        self.poll_ship();
        // Checker's runs of the checks coming back.
        let mut back = Vec::new();
        self.checking.retain(|(id, rx)| match rx.try_recv() {
            Ok(results) => {
                back.push((*id, results));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                back.push((*id, Vec::new()));
                false
            }
        });
        for (id, results) in back {
            let mut retry = None;
            if let Some(job) = self.store.job_mut(id) {
                job.checks = results;
                job.checking = false;
                retry = fix_turn(job);
            }
            if let Some(prompt) = retry {
                self.follow_up(st, id, prompt);
            }
            self.store.save();
        }
        if !finished.is_empty() {
            self.runners.retain(|r| !finished.contains(&r.id));
            for id in &finished {
                self.begin_review(*id);
                self.peek(st, *id, true);
            }
            self.offer_idea();
            self.store.save();
        }
        self.poll_peek();
        self.poll_goals(st);
        self.poll_queue(st);
        finished
    }

    /// Asks Peek for a job's title, or its recap once it ended, when Smart
    /// Peek is on.
    fn peek(&mut self, st: &AppSettings, id: u64, recap: bool) {
        let Some(job) = self.store.job(id) else {
            return;
        };
        if let Some(rx) = brain::tasks::ask(st, job, recap) {
            self.peeking.push((id, rx));
        }
    }

    /// Titles and recaps coming back from Peek.
    fn poll_peek(&mut self) {
        let mut back = Vec::new();
        self.peeking.retain(|(id, rx)| match rx.try_recv() {
            Ok(said) => {
                back.push((*id, said));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for (id, said) in back {
            let Some(job) = self.store.job_mut(id) else {
                continue;
            };
            match said {
                brain::tasks::Said::Title(t) => job.short = t,
                brain::tasks::Said::Recap(r, _) => {
                    job.feed.push(store::Item::Note(r.clone()));
                    job.recap = r;
                }
            }
            self.store.save();
        }
    }
}

/// How many times a job's failing checks go back to its bot before the
/// person is asked.
pub const FIX_TURNS: u8 = 1;

/// When a job's checks failed and it hasn't used its fix turns, notes that in
/// its feed and gives the words to send its bot. Checks the computer couldn't
/// run (a full disk, a timeout) aren't the code's fault and send nothing back.
fn fix_turn(job: &mut store::Job) -> Option<String> {
    let failed: Vec<&crate::proof::CheckRun> = job
        .checks
        .iter()
        .filter(|c| !c.ok && c.infra.is_empty() && !c.timed_out)
        .collect();
    if failed.is_empty() || job.fix_turns >= FIX_TURNS || job.status != Status::Review {
        return None;
    }
    let names: Vec<String> = failed.iter().map(|c| format!("`{}`", c.command)).collect();
    job.fix_turns += 1;
    job.feed.push(store::Item::Note(format!(
        "{} failed, so {} is fixing it before it comes to you.",
        names.join(" and "),
        job.bot.name()
    )));
    Some(crate::proof::fix_prompt(&job.checks).replace("Runo ran", "Squad Code ran"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failing_checks_go_back_to_the_bot_once() {
        let failing = crate::proof::CheckRun {
            command: "cargo test".into(),
            ok: false,
            tail: "test parse ... FAILED".into(),
            ..Default::default()
        };
        let mut job = store::Job {
            bot: Member::Fixer,
            status: Status::Review,
            checks: vec![failing.clone()],
            ..Default::default()
        };
        let prompt = fix_turn(&mut job).expect("one fix turn");
        assert!(prompt.contains("cargo test") && prompt.contains("FAILED"));
        assert!(prompt.starts_with("Squad Code ran"));
        assert_eq!(job.fix_turns, 1);
        // The second failure is the person's to see.
        assert!(fix_turn(&mut job).is_none());
        // A check the computer couldn't run sends nothing back.
        let mut job = store::Job {
            status: Status::Review,
            checks: vec![crate::proof::CheckRun {
                infra: "disk full".into(),
                ..failing
            }],
            ..Default::default()
        };
        assert!(fix_turn(&mut job).is_none());
    }

    fn quiet_store() -> Store {
        // A demo store is never saved, so tests leave ~/.runo alone.
        Store {
            demo: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_fresh_crew_is_idle() {
        let rt = Runtime::new(quiet_store());
        assert!(!rt.busy());
        assert!(rt.notes.is_empty());
        assert!(!rt.thinking(Member::Explainer));
    }

    #[test]
    fn a_job_that_cant_start_fails_without_a_window() {
        // No agent can run here: the job is made, fails with a reason, and
        // nothing is left running.
        let mut st = AppSettings::default();
        for p in crate::agent::PROVIDERS.iter() {
            let mut a = st.agent(p.id);
            a.binary = "/nonexistent/squad-test-agent".into();
            st.agents.insert(p.id.to_string(), a);
        }
        let dir = std::env::temp_dir().join(format!("squad-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut rt = Runtime::new(quiet_store());
        let folder = store::Folder {
            name: "t".into(),
            path: dir.to_string_lossy().into_owned(),
            ship: String::new(),
        };
        let opened = rt.start_job(&st, Member::Builder, Mode::Ask, "Read me".into(), folder);
        let Opened::Job(id) = opened else {
            panic!("expected a job, got {opened:?}");
        };
        let job = rt.store.job(id).unwrap();
        assert_eq!(job.status, Status::Failed);
        assert!(matches!(job.feed.last(), Some(store::Item::Error(_))));
        assert!(rt.runners.is_empty());
        assert!(rt.poll(&st, None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_busy_bot_says_so_and_points_at_its_job() {
        let mut rt = Runtime::new(quiet_store());
        rt.store.jobs.push(store::Job {
            id: 7,
            bot: Member::Builder,
            title: "Already going".into(),
            status: Status::Running,
            ..Default::default()
        });
        let st = AppSettings::default();
        let opened = rt.start_job(
            &st,
            Member::Builder,
            Mode::Build,
            "Another".into(),
            store::Folder::default(),
        );
        assert_eq!(opened, Opened::Busy(7));
        assert_eq!(rt.store.jobs.len(), 1);
        assert!(rt.notes[0].contains("Already going"));
    }

    #[test]
    fn a_dropped_goal_is_gone() {
        let mut rt = Runtime::new(quiet_store());
        rt.store.goals.push(goal::Goal {
            id: 3,
            text: "Big thing".into(),
            ..Default::default()
        });
        rt.drop_goal(3);
        assert!(rt.store.goals.is_empty());
    }
}
