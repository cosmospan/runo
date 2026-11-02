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
use ui::c;

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
    saved: Instant,
    _subs: Vec<Subscription>,
    _pump: Task<()>,
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
        let subs = vec![
            cx.subscribe_in(&composer, window, |this, input, event, window, cx| {
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
            self.place = place;
            self.entered = Instant::now();
            self.folder_menu = false;
        }
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

    fn start_job(&mut self, mode: Mode, text: String, cx: &mut Context<Self>) {
        let Some(folder) = self.store.folder().cloned() else {
            return;
        };
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
        let mut icons = div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.));
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
                    .when(on, |d| d.bg(c(0xFFFFFF)).shadow(ui::lift()))
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(Icon::new(icon).size(px(18.)).text_color(c(if on {
                        ui::INK
                    } else {
                        ui::MUTED
                    })))
                    .when(p == Place::Inbox && inbox > 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(5.))
                                .right(px(4.))
                                .min_w(px(15.))
                                .h(px(15.))
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
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(who.name()).build(window, cx)
                    })
                    .when(on, |d| {
                        d.border_2().border_color(c(ui::INK)).bg(c(0xFFFFFF))
                    })
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(face)
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
            .child(
                div()
                    .flex_none()
                    .size(px(40.))
                    .rounded(px(12.))
                    .bg(c(ui::INK))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::ArrowUpRight)
                            .size(px(17.))
                            .text_color(c(0xFFFFFF)),
                    ),
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
        // What the last frame needed sets the clock for the next one.
        let busy = self.entered.elapsed() < Duration::from_millis(1000)
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
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, _| {
                this.touched = Instant::now();
            }))
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, _| {
                this.touched = Instant::now();
            }))
            // ⌘1…⌘6 (Ctrl on Linux and Windows) jump between places, even from a text box.
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = &e.keystroke.modifiers;
                if !(m.platform || m.control) || m.shift || m.alt {
                    return;
                }
                let to = match e.keystroke.key.as_str() {
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
