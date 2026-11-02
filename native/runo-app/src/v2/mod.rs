//! Runo v2: the crew app.
//!
//! A shell around Runo's bots: Today (hand the crew a job and see what is
//! moving), Live (watch one job as the bot works), Chat (ask a bot in plain
//! words), Crew, Inbox and History. Every job runs for real on the bot's
//! engine in a folder the person picked, and is kept between launches. The
//! bots are the bubble crew from `runo-holo`, drawn as their moods change.
//!
//! Runo v1 stays one click away on the rail while v2 grows.

mod chat;
mod crew;
mod data;
mod engine;
mod engines;
mod history;
mod jobs;
mod live;
mod palette;
mod store;
mod today;
mod ui;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{BubbleRenderer, Framing, Mood, Pose};

use data::{Member, Mode};
use store::{Said, Status, Store};
use ui::{c, ca};

/// Where the person is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Today,
    Inbox,
    History,
    Chat(Member),
    Crew,
    /// One job, by id.
    Live(u64),
    Engines,
}

/// The crew's frames, drawn once a tick and shared by every place that shows the
/// same bot at the same size. Small bots move less, so they get fewer ticks: a 24 px
/// avatar redraws 10 times a second, a big one 30. A frame is kept until its bot's
/// next tick, so a still bot (motion off) is drawn once and never again.
struct Holos {
    frame: u64,
    images: HashMap<(Pose, u32, bool), Frame>,
    renderers: HashMap<u16, BubbleRenderer>,
    /// The fastest rate any bot drawn this frame needed, so the clock can slow
    /// down on screens with only small bots.
    asked: u32,
    /// Replaced frames, released at the start of the next window frame, once the
    /// new ones have painted.
    retired: Vec<Arc<RenderImage>>,
}

struct Frame {
    tick: u64,
    used: u64,
    image: Arc<RenderImage>,
}

/// Frames a second for a big bot. Smooth enough for the slow, soft motion and half
/// the work of 60.
const FPS: f64 = 30.0;

/// Frames a second for a bot this many logical pixels tall.
/// How often a bot of this size needs a new frame. A calm bubble drifts slowly,
/// so it needs far fewer than one that is busy, celebrating or asking.
fn fps_for(size: f32, pose: &Pose) -> u32 {
    use runo_holo::Layout;
    let lively = pose.sparks
        || pose.ask
        || pose.shine
        || matches!(pose.layout, Layout::Think | Layout::Busy | Layout::Splash);
    match (lively, size >= 90.0, size >= 40.0) {
        (true, true, _) => 24,
        (true, false, true) => 15,
        (false, true, _) => 12,
        _ => 8,
    }
}

impl Holos {
    fn new() -> Self {
        Holos {
            frame: 0,
            images: HashMap::new(),
            renderers: HashMap::new(),
            asked: 0,
            retired: Vec::new(),
        }
    }

    /// Starts a window frame: frames replaced or not shown last time are released
    /// from the GPU atlas.
    fn begin(&mut self, window: &mut Window, cx: &mut App) {
        self.frame += 1;
        let frame = self.frame;
        self.asked = 0;
        for old in self.retired.drain(..) {
            cx.drop_image(old, Some(window));
        }
        let gone: Vec<_> = self
            .images
            .iter()
            .filter(|(_, f)| f.used + 2 < frame)
            .map(|(k, _)| *k)
            .collect();
        for k in gone {
            if let Some(f) = self.images.remove(&k) {
                cx.drop_image(f.image, Some(window));
            }
        }
    }

    fn image(
        &mut self,
        pose: Pose,
        logical: f32,
        framing: Framing,
        t: f64,
        scale: f32,
        moving: bool,
    ) -> Arc<RenderImage> {
        let device = (logical * scale).round().clamp(8.0, 1024.0) as u16;
        let bust = framing == Framing::Bust;
        let fps = fps_for(logical, &pose);
        let tick = if moving { (t * fps as f64) as u64 } else { 0 };
        if moving {
            self.asked = self.asked.max(fps);
        }
        let frame = self.frame;
        let slot = (pose, device as u32, bust);
        if let Some(f) = self.images.get_mut(&slot) {
            f.used = frame;
            if f.tick == tick {
                return f.image.clone();
            }
        }
        let r = self
            .renderers
            .entry(device)
            .or_insert_with(|| BubbleRenderer::new(device));
        // Draw at the tick's own time, so every size stays in step.
        let at = if moving { tick as f64 / fps as f64 } else { t };
        let premul = r.draw(&pose, at, framing);
        // Straight-alpha BGRA, as GPUI's image path expects.
        let mut px = Vec::with_capacity(premul.len());
        for p in premul.as_chunks::<4>().0 {
            let a = p[3];
            let un = |v: u8| {
                if a == 0 {
                    0
                } else {
                    ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8
                }
            };
            px.extend_from_slice(&[un(p[2]), un(p[1]), un(p[0]), a]);
        }
        let buf = image::RgbaImage::from_raw(device as u32, device as u32, px).expect("frame size");
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(buf)]));
        match self.images.get_mut(&slot) {
            Some(f) => {
                self.retired
                    .push(std::mem::replace(&mut f.image, image.clone()));
                f.tick = tick;
            }
            None => {
                self.images.insert(
                    slot,
                    Frame {
                        tick,
                        used: frame,
                        image: image.clone(),
                    },
                );
            }
        }
        image
    }
}

/// The v2 window's root.
pub struct Shell {
    place: Place,
    clock: Instant,
    holos: Holos,
    scale: f32,
    mode: Mode,
    store: Store,
    runners: Vec<jobs::Runner>,
    composer: Entity<TextareaState>,
    chat_input: Entity<InputState>,
    live_input: Entity<InputState>,
    /// Live's feed scrolls itself to the newest line while the job runs.
    live_scroll: ScrollHandle,
    live_len: usize,
    chat_scroll: ScrollHandle,
    chat_len: usize,
    /// A question on its way to a bot's engine.
    asking: Option<engine::Ask>,
    /// The bubble in the "make your own" playground on Crew.
    maker: runo_holo::Bubble,
    /// Which jobs History shows: 0 all, 1 running, 2 done, 3 problems.
    history_filter: usize,
    folder_menu: bool,
    toast: Option<(SharedString, Instant)>,
    /// Motion is on (Settings → Animations); off, the crew holds still.
    motion: bool,
    /// When the current place was opened, for its entrance.
    entered: Instant,
    /// Whether the window is in front; behind, the crew slows to save power.
    active: Arc<std::sync::atomic::AtomicBool>,
    /// Redraws a second the screen needs: 30 with a big bot or an entrance
    /// playing, fewer with only small bots, a trickle when nothing moves.
    rate: Arc<std::sync::atomic::AtomicU32>,
    /// The last mouse move or key press. Left alone for a while, the crew keeps
    /// moving at half the frame rate.
    touched: Instant,
    /// The bot under the pointer: it gets the light sweep.
    hovered: Option<Member>,
    hover_at: Instant,
    /// The rail bot under the pointer, which shows its card.
    rail_hover: Option<Member>,
    /// The command palette (⌘K), its box, the picked row and when it opened.
    palette: bool,
    /// The window's own focus, so shortcuts work before any box is clicked.
    focus: FocusHandle,
    /// When focus was last moved to a box on purpose.
    focus_moved: Instant,
    palette_input: Entity<InputState>,
    /// History's search box.
    history_input: Entity<InputState>,
    palette_sel: usize,
    palette_at: Instant,
    /// Where the person has been, newest last, for Back (⌘[).
    trail: Vec<Place>,
    /// The rail's highlight glides between items: from, to, and since when.
    nav_from: f32,
    nav_to: f32,
    nav_at: Instant,
    /// Progress bars' shown values, by job, so they glide.
    bars: HashMap<u64, f32>,
    /// Today's mode tabs: the highlight glides from one to the next.
    mode_from: f32,
    mode_at: Instant,
    /// The Inbox count last frame, and when it last went up, for the badge's pop.
    inbox_seen: usize,
    inbox_at: Instant,
    /// When the newest chat line arrived, so it can slide in.
    chat_at: Instant,
    saved: Instant,
    _subs: Vec<Subscription>,
    _pump: Task<()>,
}

/// Width of one of Today's mode tabs plus the gap after it.
const MODE_STEP: f32 = 66.;

/// The rail's place for each part of the app, as a y offset of its highlight.
fn nav_slot(place: Place) -> Option<f32> {
    let i = match place {
        Place::Today => 0,
        Place::Inbox => 1,
        Place::History | Place::Live(_) => 2,
        Place::Chat(_) => 3,
        Place::Crew => 4,
        Place::Engines => return None,
    };
    Some(i as f32 * 46.)
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A long job wraps and the box grows with it; Shift+Enter starts a new line.
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(Mode::Build.placeholder())
                .submit_on_enter(true)
                .auto_grow(1, 6)
        });
        let chat_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Ask anything about the code…"));
        let live_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add to the job…"));
        let palette_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Jump to a place, a bot, a job or a folder…")
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let history_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search jobs, bots, folders…"));
        let subs = vec![
            cx.subscribe_in(&history_input, window, |_, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&palette_input, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.palette_sel = 0;
                    cx.notify();
                }
            }),
            cx.subscribe_in(&composer, window, |this, input, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
                if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                    let text = input.read(cx).value().trim().to_string();
                    input.update(cx, |i, cx| i.set_value("", window, cx));
                    this.hand_over(text, cx);
                }
            }),
            cx.subscribe_in(&chat_input, window, |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if let (false, Place::Chat(who)) = (text.is_empty(), this.place) {
                        input.update(cx, |i, cx| i.set_value("", window, cx));
                        this.ask(who, text, cx);
                    }
                }
            }),
            cx.subscribe_in(&live_input, window, |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if let (false, Place::Live(id)) = (text.is_empty(), this.place) {
                        input.update(cx, |i, cx| i.set_value("", window, cx));
                        this.follow_up(id, text, cx);
                    }
                }
            }),
        ];
        // The crew's clock: as many redraws as the screen needs while the window is
        // in front (see `rate`), a few a second behind other windows.
        let active = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let rate = Arc::new(std::sync::atomic::AtomicU32::new(FPS as u32));
        let (seen, wanted) = (active.clone(), rate.clone());
        let pump = cx.spawn(async move |this, cx| loop {
            let front = seen.load(std::sync::atomic::Ordering::Relaxed);
            let want = wanted
                .load(std::sync::atomic::Ordering::Relaxed)
                .clamp(2, FPS as u32);
            let wait = if front {
                1000.0 / want as f64
            } else {
                1000.0 / want.min(4) as f64
            };
            cx.background_executor()
                .timer(Duration::from_millis(wait as u64))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });
        // Learn which agents are installed and the models they offer.
        crate::caps::refresh();
        let store = Store::load();
        let start = match std::env::var("RUNO_V2_PLACE").as_deref() {
            Ok("crew") => Place::Crew,
            Ok("chat") => Place::Chat(Member::Explainer),
            Ok("inbox") => Place::Inbox,
            Ok("history") => Place::History,
            Ok("engines") => Place::Engines,
            Ok("live") => store
                .jobs
                .last()
                .map(|j| Place::Live(j.id))
                .unwrap_or(Place::Today),
            _ => Place::Today,
        };
        Shell {
            place: start,
            clock: Instant::now(),
            holos: Holos::new(),
            scale: 1.0,
            mode: Mode::Build,
            store,
            runners: Vec::new(),
            composer,
            chat_input,
            live_input,
            live_scroll: ScrollHandle::new(),
            live_len: 0,
            chat_scroll: ScrollHandle::new(),
            chat_len: 0,
            asking: None,
            maker: Member::Fixer.bubble(),
            history_filter: 0,
            folder_menu: false,
            toast: None,
            entered: Instant::now(),
            motion: true,
            active,
            rate,
            touched: Instant::now(),
            hovered: None,
            hover_at: Instant::now(),
            rail_hover: None,
            palette: false,
            focus,
            focus_moved: Instant::now(),
            palette_input,
            history_input,
            palette_sel: 0,
            palette_at: Instant::now(),
            trail: Vec::new(),
            nav_from: nav_slot(start).unwrap_or(0.),
            nav_to: nav_slot(start).unwrap_or(0.),
            nav_at: Instant::now(),
            bars: HashMap::new(),
            mode_from: MODE_STEP,
            mode_at: Instant::now(),
            inbox_seen: usize::MAX,
            inbox_at: Instant::now(),
            chat_at: Instant::now(),
            saved: Instant::now(),
            _subs: subs,
            _pump: pump,
        }
    }

    fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    fn go(&mut self, place: Place, cx: &mut Context<Self>) {
        if self.place != place {
            self.trail.push(self.place);
            if self.trail.len() > 40 {
                self.trail.remove(0);
            }
            self.arrive(place);
        }
        self.visit(place, cx);
    }

    /// Back to the place before this one.
    fn back(&mut self, cx: &mut Context<Self>) {
        while let Some(p) = self.trail.pop() {
            // Skip jobs that are gone and repeats of where we are.
            let ok = match p {
                Place::Live(id) => self.store.job(id).is_some(),
                _ => true,
            };
            if ok && p != self.place {
                self.arrive(p);
                self.visit(p, cx);
                return;
            }
        }
    }

    /// Switches place and starts its entrance and the rail's glide.
    fn arrive(&mut self, place: Place) {
        {
            if let Some(to) = nav_slot(place) {
                let now = self.nav_y();
                self.nav_from = if nav_slot(self.place).is_some() {
                    now
                } else {
                    to
                };
                self.nav_to = to;
                self.nav_at = Instant::now();
            }
            self.place = place;
            self.entered = Instant::now();
            self.folder_menu = false;
        }
    }

    /// Where Today's mode highlight is now, in pixels from the first tab.
    fn mode_x(&self) -> f32 {
        let to = Mode::ALL.iter().position(|m| *m == self.mode).unwrap_or(0) as f32 * MODE_STEP;
        let k = runo_holo::anim::spring(self.mode_at.elapsed().as_secs_f64(), 0.3) as f32;
        self.mode_from + (to - self.mode_from) * k
    }

    /// How far item `i` of the current place has eased in, 0 → 1; each item
    /// starts a beat after the one before.
    fn stagger(&self, i: usize) -> f32 {
        if !self.motion {
            return 1.0;
        }
        let t = self.entered.elapsed().as_secs_f64() - 0.045 * i.min(12) as f64;
        runo_holo::anim::spring(t.max(0.0), 0.36) as f32
    }

    /// A progress bar's shown value, gliding toward `to` instead of jumping.
    fn glide_bar(&mut self, id: u64, to: f32) -> f32 {
        let v = self.bars.entry(id).or_insert(to);
        *v += (to - *v) * 0.18;
        if (to - *v).abs() < 0.002 {
            *v = to;
        }
        *v
    }

    /// Where the rail's highlight is now, between its last two stops.
    fn nav_y(&self) -> f32 {
        let k = runo_holo::anim::spring(self.nav_at.elapsed().as_secs_f64(), 0.34) as f32;
        self.nav_from + (self.nav_to - self.nav_from) * k
    }

    /// Marks a finished job seen once it's opened.
    fn visit(&mut self, place: Place, cx: &mut Context<Self>) {
        if let Place::Live(id) = place {
            if let Some(j) = self.store.job_mut(id) {
                if j.status != Status::Running && !j.seen {
                    j.seen = true;
                    self.store.save();
                }
            }
        }
        cx.notify();
    }

    fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if mode != self.mode {
            self.mode_from = self.mode_x();
            self.mode_at = Instant::now();
        }
        self.mode = mode;
        self.composer.update(cx, |i, cx| {
            i.set_placeholder(mode.placeholder(), window, cx)
        });
        cx.notify();
    }

    fn say(&mut self, text: impl Into<SharedString>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    /// Asks for a folder and makes it the one the crew works in.
    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        self.folder_menu = false;
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a project folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.store.add_folder(path);
                this.store.save();
                let name = this
                    .store
                    .folder()
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                this.say(format!("The crew now works in {name}."));
                cx.notify();
            });
        })
        .detach();
    }

    /// The text from Today's box becomes a job for the bot its mode goes to, or a
    /// question for Explainer.
    fn hand_over(&mut self, text: String, cx: &mut Context<Self>) {
        let text = match (text.is_empty(), self.mode.default_job()) {
            (false, _) => text,
            (true, Some(d)) => d.to_string(),
            (true, None) => return,
        };
        if self.store.folder().is_none() {
            self.say("Pick a folder first, so the crew knows where to work.");
            self.pick_folder(cx);
            return;
        }
        if self.mode == Mode::Ask {
            self.go(Place::Chat(Member::Explainer), cx);
            self.ask(Member::Explainer, text, cx);
            return;
        }
        let mode = self.mode;
        self.start_job(mode, text, cx);
    }

    /// Takes a folder off the crew's list. Its files and past jobs stay.
    fn forget_folder(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.store.folders.len() {
            return;
        }
        let gone = self.store.folders.remove(i);
        if self.store.current >= self.store.folders.len() || self.store.current > i {
            self.store.current = self.store.current.saturating_sub(1);
        }
        if self.store.folders.is_empty() {
            self.folder_menu = false;
        }
        self.store.save();
        self.say(format!(
            "Removed {} from the list. Its files are untouched.",
            gone.name
        ));
        cx.notify();
    }

    /// Starts a stopped or failed job over, as a new job in the same folder.
    fn retry_job(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(j) = self.store.job(id) else { return };
        let (mode, title, folder) = (j.mode, j.title.clone(), j.folder.clone());
        self.start_job_in(mode, title, folder, cx);
    }

    fn start_job(&mut self, mode: Mode, text: String, cx: &mut Context<Self>) {
        let Some(folder) = self.store.folder().cloned() else {
            return;
        };
        self.start_job_in(mode, text, folder, cx);
    }

    fn start_job_in(
        &mut self,
        mode: Mode,
        text: String,
        folder: store::Folder,
        cx: &mut Context<Self>,
    ) {
        let bot = Member::for_mode(mode);
        if let Some(busy) = self.store.busy(bot) {
            let (title, id) = (busy.title.clone(), busy.id);
            self.say(format!(
                "{} is still on \u{201c}{title}\u{201d}. This one waits for you to try again.",
                bot.name()
            ));
            self.go(Place::Live(id), cx);
            return;
        }
        self.store.next_id += 1;
        let mut job = store::Job {
            id: self.store.next_id,
            bot,
            mode,
            title: text.clone(),
            folder,
            started: store::now(),
            seen: false,
            ..Default::default()
        };
        let st = cx.global::<crate::settings::AppSettings>();
        match jobs::start(st, &mut job, &text, false) {
            Ok(r) => self.runners.push(r),
            Err(why) => {
                job.status = Status::Failed;
                job.ended = Some(job.started);
                job.engine = engine::label(st, bot);
                job.feed.push(store::Item::Error(why));
            }
        }
        let id = job.id;
        self.store.jobs.push(job);
        self.store.save();
        self.go(Place::Live(id), cx);
    }

    /// More words for a job: while it runs they wait; once it's done they start a
    /// follow-up that continues the same conversation when the engine can.
    fn follow_up(&mut self, id: u64, text: String, cx: &mut Context<Self>) {
        let st = cx.global::<crate::settings::AppSettings>().clone();
        let Some(job) = self.store.job_mut(id) else {
            return;
        };
        if job.status == Status::Running {
            self.say("It's still working. Add this once it finishes.");
            cx.notify();
            return;
        }
        job.feed.push(store::Item::You(text.clone()));
        match jobs::start(&st, job, &text, true) {
            Ok(r) => self.runners.push(r),
            Err(why) => job.feed.push(store::Item::Error(why)),
        }
        self.store.save();
        cx.notify();
    }

    fn stop_job(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(r) = self.runners.iter_mut().find(|r| r.id == id) {
            r.stop();
        }
        cx.notify();
    }

    /// Asks `who` for real, on its own engine, in the current folder.
    fn ask(&mut self, who: Member, text: String, cx: &mut Context<Self>) {
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
            cx.notify();
            return;
        }
        let Some(folder) = self.store.folders.get(self.store.current).cloned() else {
            log.push(Said::note(
                "Pick a folder first (on Today), so the crew knows which code to read.",
            ));
            cx.notify();
            return;
        };
        let st = cx.global::<crate::settings::AppSettings>();
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
        cx.notify();
    }

    /// Whether `who` is answering a question right now.
    fn thinking(&self, who: Member) -> bool {
        self.asking.as_ref().is_some_and(|a| a.who == who)
    }

    /// Collects answers and job output that have arrived.
    fn poll(&mut self) {
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
                    if self.place == Place::Live(job.id) {
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
        if !finished.is_empty() {
            self.runners.retain(|r| !finished.contains(&r.id));
            for id in &finished {
                if let Some(j) = self.store.job(*id) {
                    if self.place != Place::Live(*id) {
                        let msg = match j.status {
                            Status::Done => {
                                format!("{} finished \u{201c}{}\u{201d}.", j.bot.name(), j.title)
                            }
                            _ => format!("{} stopped: \u{201c}{}\u{201d}.", j.bot.name(), j.title),
                        };
                        self.say(msg);
                    }
                }
            }
            self.store.save();
        }
    }

    /// How a bot feels right now, from what it is really doing.
    fn mood(&self, who: Member) -> Mood {
        if self.thinking(who) {
            return Mood::Thinking;
        }
        if self.store.busy(who).is_some() {
            return Mood::Working;
        }
        let recent = self
            .store
            .jobs
            .iter()
            .rev()
            .find(|j| j.bot == who && j.ended.is_some());
        if let Some(j) = recent {
            let fresh = store::now().saturating_sub(j.ended.unwrap_or(0)) < 8;
            match j.status {
                Status::Done if fresh => return Mood::Done,
                Status::Failed if !j.seen => return Mood::NeedsYou,
                _ => {}
            }
        }
        Mood::Signature
    }

    /// One word for what a bot is doing, and its colour.
    fn status_of(&self, who: Member) -> (String, u32) {
        if self.thinking(who) {
            return ("answering".into(), who.accent());
        }
        if let Some(j) = self.store.busy(who) {
            return (
                match j.progress() {
                    Some((d, n)) => format!("working {d}/{n}"),
                    None => "working".into(),
                },
                who.accent(),
            );
        }
        if self
            .store
            .jobs
            .iter()
            .any(|j| j.bot == who && j.needs_you())
        {
            return ("needs you".into(), ui::AMBER);
        }
        (who.role().to_lowercase(), ui::FAINT)
    }

    /// A bot drawn at `size` logical pixels, in `mood`.
    fn bot(&mut self, who: Member, mood: Mood, size: f32, framing: Framing) -> Img {
        let mut pose = who.bubble().pose(mood);
        pose.shine |= self.hovered == Some(who);
        self.pose_img(pose, size, framing)
    }

    fn hover_bot(&mut self, who: Member, on: bool, cx: &mut Context<Self>) {
        let was = self.hovered;
        if on {
            if was != Some(who) {
                self.hover_at = Instant::now();
            }
            self.hovered = Some(who);
        } else if self.hovered == Some(who) {
            self.hovered = None;
        }
        if was != self.hovered {
            cx.notify();
        }
    }

    fn pose_img(&mut self, pose: Pose, size: f32, framing: Framing) -> Img {
        // With motion off, every bot holds one calm pose.
        let t = if self.motion { self.now() } else { 2.0 };
        let image = self
            .holos
            .image(pose, size, framing, t, self.scale, self.motion);
        img(image).size(px(size)).flex_none()
    }

    /// A small avatar, like the ones beside chat lines: still, so it costs nothing.
    fn avatar(&mut self, who: Member, size: f32) -> Img {
        let image = self.holos.image(
            who.bubble().pose(Mood::Idle),
            size,
            Framing::Bust,
            2.0,
            self.scale,
            false,
        );
        img(image).size(px(size)).flex_none()
    }

    fn inbox_count(&self) -> usize {
        self.store.jobs.iter().filter(|j| j.needs_you()).count()
    }

    fn rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let place = self.place;
        let inbox = self.inbox_count();
        // The Inbox badge pops when something new lands in it.
        if inbox > self.inbox_seen {
            self.inbox_at = Instant::now();
        }
        self.inbox_seen = inbox;
        let pop = {
            let t = self.inbox_at.elapsed().as_secs_f32();
            if t < 0.6 && self.motion {
                3.0 * (t / 0.6 * std::f32::consts::PI).sin() * (1.0 - t / 0.6)
            } else {
                0.0
            }
        };
        let nav = [
            (Place::Today, IconName::House, "Today"),
            (Place::Inbox, IconName::Inbox, "Inbox"),
            (Place::History, IconName::Clock, "History"),
            (
                Place::Chat(Member::Explainer),
                IconName::MessageSquare,
                "Chat",
            ),
            (Place::Crew, IconName::Users, "Crew"),
        ];
        // One white highlight glides between the items as the place changes.
        let glide = nav_slot(place).map(|_| self.nav_y());
        let mut icons = div()
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .when_some(glide, |d, y| {
                d.child(
                    div()
                        .absolute()
                        .top(px(y))
                        .left(px(0.))
                        .size(px(42.))
                        .rounded(px(12.))
                        .bg(c(0xFFFFFF))
                        .shadow(ui::lift()),
                )
            });
        for (i, (p, icon, label)) in nav.into_iter().enumerate() {
            let on = match (place, p) {
                (Place::Chat(_), Place::Chat(_)) => true,
                (Place::Live(_), Place::History) => true,
                _ => place == p,
            };
            icons = icons.child(
                div()
                    .id(("nav", i))
                    .relative()
                    .flex_none()
                    .size(px(42.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .cursor_pointer()
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                    })
                    .when(!on, |d| d.hover(|s| s.bg(ca(0xFFFFFF, 0.7))))
                    .child(Icon::new(icon).size(px(18.)).text_color(c(if on {
                        ui::INK
                    } else {
                        ui::MUTED
                    })))
                    .when(p == Place::Inbox && inbox > 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(5. - pop))
                                .right(px(4. - pop))
                                .min_w(px(15. + 2. * pop))
                                .h(px(15. + 2. * pop))
                                .px(px(3.))
                                .rounded_full()
                                .bg(c(ui::ORANGE))
                                .border_2()
                                .border_color(c(ui::RAIL))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(c(0xFFFFFF))
                                .child(inbox.min(99).to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let to = match p {
                            Place::Chat(_) => Place::Chat(this.last_chat()),
                            other => other,
                        };
                        this.go(to, cx)
                    })),
            );
        }
        // The crew takes the rail's spare height and clips gracefully in short windows.
        let mut bots = div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.));
        for who in Member::CREW {
            let on = place == Place::Chat(who)
                || matches!(place, Place::Live(id) if self.store.job(id).is_some_and(|j| j.bot == who));
            let mood = self.mood(who);
            // Small and slow (a few frames a second), so the rail stays alive cheaply.
            let face = self.bot(who, mood, 34., Framing::Bust);
            let (_, dot) = self.status_of(who);
            let busy = self.store.busy(who).map(|j| j.id);
            bots = bots.child(
                div()
                    .id(("rail-bot", who.key() as usize))
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                        if *on {
                            this.rail_hover = Some(who);
                        } else if this.rail_hover == Some(who) {
                            this.rail_hover = None;
                        }
                        this.hover_bot(who, *on, cx);
                    }))
                    .relative()
                    .flex_none()
                    .size(px(42.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(14.))
                    .cursor_pointer()
                    .when(on, |d| {
                        d.border_2().border_color(c(ui::INK)).bg(c(0xFFFFFF))
                    })
                    // Hovered, a card slides out beside the rail: who, what it's
                    // doing, and what a click does.
                    .when(self.rail_hover == Some(who), |d| {
                        let k = runo_holo::anim::spring(self.hover_at.elapsed().as_secs_f64(), 0.22)
                            as f32;
                        let (line, color) = self.status_of(who);
                        let tip = if busy.is_some() {
                            "Click to watch the job"
                        } else {
                            "Click to chat"
                        };
                        d.child(
                            deferred(
                                div()
                                    .absolute()
                                    .left(px(52. + 8. * (1. - k)))
                                    .top(px(-6.))
                                    .w(px(210.))
                                    .opacity(k)
                                    .p(px(12.))
                                    .rounded(px(14.))
                                    .bg(c(0xFFFFFF))
                                    .border_1()
                                    .border_color(c(ui::LINE))
                                    .shadow(ui::lift())
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(who.name()),
                                    )
                                    .child(ui::status(color, line))
                                    .child(
                                        div()
                                            .mt(px(4.))
                                            .text_size(px(11.))
                                            .text_color(c(ui::FAINT))
                                            .child(tip),
                                    ),
                            )
                            .with_priority(2),
                        )
                    })
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(face)
                    // A working bot's dot breathes: a soft ring swells and fades.
                    .when(dot != ui::FAINT && busy.is_some() && self.motion, |d| {
                        let p = runo_holo::anim::phase(self.now(), 1.6, 0.0) as f32;
                        let r = 9. + 10. * p;
                        d.child(
                            div()
                                .absolute()
                                .bottom(px(9.5 - r / 2.))
                                .right(px(9.5 - r / 2.))
                                .size(px(r))
                                .rounded_full()
                                .bg(ca(dot, 0.35 * (1. - p))),
                        )
                    })
                    .when(dot != ui::FAINT, |d| {
                        d.child(
                            div()
                                .absolute()
                                .bottom(px(5.))
                                .right(px(5.))
                                .size(px(9.))
                                .rounded_full()
                                .border_2()
                                .border_color(c(ui::RAIL))
                                .bg(c(dot)),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let to = busy.map(Place::Live).unwrap_or(Place::Chat(who));
                        this.go(to, cx);
                    })),
            );
        }
        let nav_button = |id: &'static str, icon: IconName, on: bool, label: &'static str| {
            div()
                .id(id)
                .flex_none()
                .size(px(42.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.))
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                })
                .when(on, |d| d.bg(c(0xFFFFFF)).shadow(ui::lift()))
                .hover(|s| s.bg(c(0xFFFFFF)))
                .child(Icon::new(icon).size(px(18.)).text_color(c(if on {
                    ui::INK
                } else {
                    ui::MUTED
                })))
        };
        div()
            .w(px(76.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            // On a Mac the window buttons sit in the top-left corner, over the rail.
            .pt(px(if cfg!(target_os = "macos") { 46. } else { 20. }))
            .pb(px(16.))
            .gap(px(14.))
            .bg(c(ui::RAIL))
            .border_r_1()
            .border_color(c(ui::LINE))
            // The top button finds anything: places, bots, jobs, folders (⌘K).
            .child(
                div()
                    .id("open-palette")
                    .flex_none()
                    .size(px(40.))
                    .rounded(px(12.))
                    .bg(c(ui::INK))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0x2C2A38)))
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new("Find anything (⌘K)")
                            .build(window, cx)
                    })
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(17.))
                            .text_color(c(0xFFFFFF)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
            )
            .child(icons)
            .child(div().w(px(32.)).h(px(1.)).bg(c(ui::LINE)))
            .child(bots)
            .child(
                nav_button(
                    "nav-engines",
                    IconName::Cpu,
                    place == Place::Engines,
                    "Engines",
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Engines, cx))),
            )
            .child(
                div()
                    .id("open-v1")
                    .px(px(8.))
                    .py(px(3.))
                    .rounded_full()
                    .text_size(px(10.))
                    .font_family(ui::MONO)
                    .text_color(c(ui::FAINT))
                    .cursor_pointer()
                    .hover(|s| s.text_color(c(ui::INK)).bg(c(0xFFFFFF)))
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new("Open Runo v1").build(window, cx)
                    })
                    .child("v1")
                    .on_click(cx.listener(|_, _, _, cx| cx.defer(crate::open_main_window))),
            )
    }

    /// The bot whose chat was open last, else Explainer.
    fn last_chat(&self) -> Member {
        Member::CREW
            .into_iter()
            .max_by_key(|m| {
                (
                    self.store.chat(*m).len().min(1),
                    (*m == Member::Explainer) as u8,
                )
            })
            .unwrap_or(Member::Explainer)
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = window.scale_factor();
        self.poll();
        // A box that had focus may be gone (a new place opened): take focus back,
        // so shortcuts keep working without a click.
        // (Not just after focus moved on purpose: the new box may not be drawn yet.)
        if self.focus_moved.elapsed() > Duration::from_millis(300)
            && !self.focus.contains_focused(window, cx)
        {
            self.focus.focus(window, cx);
        }
        // What the last frame needed sets the clock for the next one.
        let busy = self.entered.elapsed() < Duration::from_millis(1000)
            || (self.palette && self.palette_at.elapsed() < Duration::from_millis(600))
            || self.mode_at.elapsed() < Duration::from_millis(500)
            || self.inbox_at.elapsed() < Duration::from_millis(700)
            || self.chat_at.elapsed() < Duration::from_millis(500)
            || self.nav_at.elapsed() < Duration::from_millis(600)
            || (self.hovered.is_some() && self.hover_at.elapsed() < Duration::from_millis(400))
            || self
                .toast
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() < Duration::from_millis(3600));
        // Nobody at the keyboard: the crew slows down, then nearly stops.
        let idle = self.touched.elapsed();
        let working = self.asking.is_some() || !self.runners.is_empty();
        let want = if busy {
            FPS as u32
        } else if working {
            self.holos.asked.max(4)
        } else {
            self.holos.asked.max(2)
        };
        let want = if idle > Duration::from_secs(120) {
            want.min(4)
        } else if idle > Duration::from_secs(20) {
            want.min(8)
        } else {
            want
        };
        self.rate.store(want, std::sync::atomic::Ordering::Relaxed);
        self.holos.begin(window, cx);
        self.motion = cx.global::<crate::settings::AppSettings>().animations;
        let body = match self.place {
            Place::Today => self.today(cx),
            Place::Live(id) => self.live(id, cx),
            Place::Crew => self.crew(cx),
            Place::Chat(who) => self.chat(who, cx),
            Place::Inbox => self.inbox(cx),
            Place::History => self.history(cx),
            Place::Engines => self.engines(cx),
        };
        // Each place eases in: a short fade and a lift of a few pixels.
        let k = runo_holo::anim::spring(self.entered.elapsed().as_secs_f64(), 0.32) as f32;
        let body = div()
            .size_full()
            .opacity(0.35 + 0.65 * k)
            .relative()
            .top(px(10. * (1. - k)))
            .child(body);
        self.active.store(
            window.is_window_active(),
            std::sync::atomic::Ordering::Relaxed,
        );
        let toast = self
            .toast
            .clone()
            .filter(|(_, at)| at.elapsed() < Duration::from_millis(3500));
        let toast_el = toast.map(|(text, at)| {
            let k = (at.elapsed().as_secs_f32() / 0.25).min(1.0);
            let fade = ((3.5 - at.elapsed().as_secs_f32()) / 0.4).clamp(0.0, 1.0);
            div()
                .absolute()
                .bottom(px(28. + 10. * (1. - k)))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .opacity(k.min(fade))
                .child(
                    div()
                        .px(px(16.))
                        .py(px(10.))
                        .rounded_full()
                        .bg(c(ui::INK))
                        .text_color(c(0xFFFFFF))
                        .text_size(px(13.))
                        .shadow(ui::lift())
                        .child(text),
                )
        });
        div()
            .size_full()
            .relative()
            .flex()
            .track_focus(&self.focus)
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, _| {
                this.touched = Instant::now();
            }))
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, _| {
                this.touched = Instant::now();
            }))
            // ⌘1…⌘6 (Ctrl on Linux and Windows) jump between places, even from a text box;
            // ⌘K opens the palette and ⌘[ goes back.
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                let m = &e.keystroke.modifiers;
                let key = e.keystroke.key.as_str();
                if this.palette && !m.platform && !m.control {
                    if this.palette_key(key, window, cx) {
                        cx.stop_propagation();
                    }
                    return;
                }
                if !(m.platform || m.control) || m.shift || m.alt {
                    return;
                }
                match key {
                    "k" => {
                        if this.palette {
                            this.close_palette(window, cx);
                        } else {
                            this.open_palette(window, cx);
                        }
                        cx.stop_propagation();
                        return;
                    }
                    "[" => {
                        this.palette = false;
                        this.back(cx);
                        cx.stop_propagation();
                        return;
                    }
                    _ => {}
                }
                this.palette = false;
                let to = match key {
                    "1" => Place::Today,
                    "2" => Place::Inbox,
                    "3" => Place::History,
                    "4" => Place::Chat(this.last_chat()),
                    "5" => Place::Crew,
                    "6" => Place::Engines,
                    _ => return,
                };
                this.go(to, cx);
                cx.stop_propagation();
            }))
            .font_family("Geist")
            .text_color(c(ui::INK))
            .bg(linear_gradient(
                180.,
                linear_color_stop(c(0xFBF6F3), 0.0),
                linear_color_stop(c(0xF3F4FA), 0.35),
            ))
            .child(self.rail(cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(body),
            )
            .children(toast_el)
            // Deferred, so it paints over the crew's frames too.
            .when(self.palette, |d| d.child(self.palette_view(cx)))
    }
}

/// Opens the v2 window.
pub fn open_window(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1440.), px(900.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Runo".into()),
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: Some(point(px(16.), px(16.))),
        }),
        app_id: Some("runo".into()),
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(window, cx))
    });
    // Come to the front when opened, not behind whatever had focus.
    if let Ok((handle, _)) = opened {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
    cx.activate(true);
}
