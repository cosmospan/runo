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
mod home;
mod jobs;
mod live;
mod palette;
mod review;
mod ship;
mod sidebar;
mod store;
mod today;
mod ui;
mod usage;
mod welcome;
mod work;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
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
    /// What the crew cost and did lately.
    Usage,
    /// The first-run walkthrough.
    Welcome,
}

/// The crew's frames, drawn once a tick and shared by every place that shows the
/// same bot at the same size. Small bots move less, so they get fewer ticks: a 24 px
/// avatar redraws 10 times a second, a big lively one 30. New frames are drawn on
/// the painter thread while the last one stays on screen, so the window itself
/// never waits for a bot. A still bot (motion off) is drawn once and never again.
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
    /// Frames asked of the painter and not back yet, one per bot at most.
    pending: std::collections::HashSet<Slot>,
    painter: Painters,
    painted: std::sync::mpsc::Receiver<Painted>,
    /// Time spent drawing first frames on the main thread this window frame.
    /// Past [`FIRST_BUDGET`] the rest go to the painters, so a screen full of
    /// new bots never stalls a frame.
    spent: Duration,
    /// Shown for the one frame a new bot waits on the painters.
    blank: Arc<RenderImage>,
}

/// Main-thread time a window frame may spend on first frames of new bots.
const FIRST_BUDGET: Duration = Duration::from_millis(5);

struct Frame {
    tick: u64,
    used: u64,
    image: Arc<RenderImage>,
}

/// Frames a second while anything moves: the painters draw at the display's
/// rate, so motion is never stepped.
const FPS: f64 = 60.0;

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
        (true, _, true) => 60,
        (true, false, false) => 30,
        (false, true, _) => 60,
        (false, false, true) => 30,
        _ => 20,
    }
}

impl Holos {
    fn new() -> Self {
        let (painter, painted) = painter();
        Holos {
            pending: Default::default(),
            painter,
            painted,
            frame: 0,
            images: HashMap::new(),
            renderers: HashMap::new(),
            asked: 0,
            retired: Vec::new(),
            spent: Duration::ZERO,
            blank: blank_image(),
        }
    }

    /// Starts a window frame: frames replaced or not shown last time are released
    /// from the GPU atlas.
    fn begin(&mut self, window: &mut Window, cx: &mut App) {
        self.frame += 1;
        let frame = self.frame;
        self.asked = 0;
        self.spent = Duration::ZERO;
        self.collect();
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
        // A frame on screen already: keep showing it, and have the painter draw
        // the next one off the main thread, so the window never waits on a bot.
        if let Some(f) = self.images.get_mut(&slot) {
            f.used = frame;
            if f.tick != tick && self.pending.insert(slot) {
                let at = if moving { tick as f64 / fps as f64 } else { t };
                let _ = self.painter.send(Paint {
                    slot,
                    tick,
                    at,
                    pose,
                    device,
                    framing,
                });
            }
            return f.image.clone();
        }
        // A bot's first frame is drawn right away, so it never shows up blank,
        // as long as this frame has time left; otherwise a painter draws it and
        // it appears on the next frame.
        let at = if moving { tick as f64 / fps as f64 } else { t };
        if self.spent >= FIRST_BUDGET {
            if self.pending.insert(slot) {
                let _ = self.painter.send(Paint {
                    slot,
                    tick,
                    at,
                    pose,
                    device,
                    framing,
                });
            }
            return self.blank.clone();
        }
        let began = Instant::now();
        let r = self
            .renderers
            .entry(device)
            .or_insert_with(|| BubbleRenderer::new(device));
        let image = to_image(r.draw(&pose, at, framing), device);
        self.spent += began.elapsed();
        self.images.insert(
            slot,
            Frame {
                tick,
                used: frame,
                image: image.clone(),
            },
        );
        image
    }

    /// Takes the frames the painter has finished, in place of the ones showing.
    fn collect(&mut self) {
        while let Ok(done) = self.painted.try_recv() {
            self.pending.remove(&done.slot);
            if let Some(f) = self.images.get_mut(&done.slot) {
                self.retired
                    .push(std::mem::replace(&mut f.image, done.image));
                f.tick = done.tick;
            } else {
                // A first frame the painters drew while the window was busy.
                self.images.insert(
                    done.slot,
                    Frame {
                        tick: done.tick,
                        used: self.frame,
                        image: done.image,
                    },
                );
            }
        }
    }
}

type Slot = (Pose, u32, bool);

/// A bot frame for the painter to draw.
struct Paint {
    slot: Slot,
    tick: u64,
    at: f64,
    pose: Pose,
    device: u16,
    framing: Framing,
}

struct Painted {
    slot: Slot,
    tick: u64,
    image: Arc<RenderImage>,
}

/// The painters: threads that draw bot frames, so the window's own frames stay
/// quick. Each bot always goes to the same painter, and when one falls behind,
/// only the newest ask for each bot is drawn.
fn painter() -> (Painters, std::sync::mpsc::Receiver<Painted>) {
    let (done, dones) = std::sync::mpsc::channel::<Painted>();
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 3);
    let mut workers = Vec::with_capacity(n);
    for i in 0..n {
        let (ask, asks) = std::sync::mpsc::channel::<Paint>();
        let done = done.clone();
        let _ = std::thread::Builder::new()
            .name(format!("runo-bots-{i}"))
            .spawn(move || {
                let mut renderers: HashMap<u16, BubbleRenderer> = HashMap::new();
                while let Ok(first) = asks.recv() {
                    let mut todo: Vec<Paint> = vec![first];
                    todo.extend(asks.try_iter());
                    // The newest ask per bot wins.
                    let mut newest: HashMap<Slot, Paint> = HashMap::new();
                    for p in todo {
                        newest.insert(p.slot, p);
                    }
                    for (_, p) in newest {
                        let r = renderers
                            .entry(p.device)
                            .or_insert_with(|| BubbleRenderer::new(p.device));
                        let image = to_image(r.draw(&p.pose, p.at, p.framing), p.device);
                        if done
                            .send(Painted {
                                slot: p.slot,
                                tick: p.tick,
                                image,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            });
        workers.push(ask);
    }
    (Painters(workers), dones)
}

/// Where bot frames are asked for: one queue per painter thread.
struct Painters(Vec<std::sync::mpsc::Sender<Paint>>);

impl Painters {
    fn send(&self, p: Paint) -> Result<(), std::sync::mpsc::SendError<Paint>> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        p.slot.hash(&mut h);
        self.0[(h.finish() % self.0.len() as u64) as usize].send(p)
    }
}

/// A clear 1 × 1 frame, for a bot whose first frame is still being drawn.
fn blank_image() -> Arc<RenderImage> {
    let buf = image::RgbaImage::from_raw(1, 1, vec![0; 4]).expect("blank");
    Arc::new(RenderImage::new(vec![image::Frame::new(buf)]))
}

/// A drawn bot (premultiplied RGBA) as an image GPUI can paint.
fn to_image(premul: &[u8], device: u16) -> Arc<RenderImage> {
    // Straight-alpha BGRA, as GPUI's image path expects.
    let mut px = Vec::with_capacity(premul.len());
    for p in premul.as_chunks::<4>().0 {
        let a = p[3];
        if a == 255 {
            px.extend_from_slice(&[p[2], p[1], p[0], 255]);
            continue;
        }
        if a == 0 {
            px.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
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
    Arc::new(RenderImage::new(vec![image::Frame::new(buf)]))
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
    /// Nobody has touched the app for a while and nothing is running: the crew
    /// holds still and the window stops redrawing until the pointer moves.
    dozing: Arc<std::sync::atomic::AtomicBool>,
    /// The last mouse move or key press. Left alone for a while, the crew keeps
    /// moving at half the frame rate.
    touched: Instant,
    /// The bot under the pointer: it gets the light sweep.
    hovered: Option<Member>,
    /// The maker's name and job boxes, and the built-in bot its work is like.
    maker_name: Entity<InputState>,
    maker_role: Entity<InputState>,
    maker_like: Member,
    /// The walkthrough's step and the crew picked in it.
    setup_step: usize,
    /// Home's table tab: active, needs you or finished.
    home_tab: u8,
    /// The job Home's inspector shows; the newest active one when unset.
    home_pick: Option<u64>,
    /// The Inbox item open on the right.
    inbox_pick: Option<u64>,
    setup_crew: Vec<Member>,
    /// A made bot chosen for Today's next job (its mode must match).
    target: Option<Member>,
    hover_at: Instant,
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
    /// Checker's runs of the checks on finished jobs, by job.
    checking: Vec<(u64, std::sync::mpsc::Receiver<Vec<crate::proof::CheckRun>>)>,
    /// The changed file whose diff is open on a job's review, if any.
    diff_open: Option<(u64, String)>,
    diff_text: String,
    /// Shipper opening pull requests, by job.
    shipping: Vec<(u64, std::sync::mpsc::Receiver<ship::Step>)>,
    /// Looks at open pull requests, by job, and when the last round started.
    watching: Vec<(
        u64,
        std::sync::mpsc::Receiver<Option<crate::github::PrStatus>>,
    )>,
    watched: Instant,
    /// Pull requests being merged (true) or closed (false) on GitHub, by job.
    landing: Vec<(u64, bool, std::sync::mpsc::Receiver<Result<(), String>>)>,
    /// What Approve does in each folder, worked out once per folder.
    ways: HashMap<String, ship::Way>,
    _subs: Vec<Subscription>,
    _pump: Task<()>,
}

/// The folders behind dropped paths: a folder itself, or the folder a file is in,
/// each once.
fn folders_from(paths: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for p in paths {
        let dir = if p.is_dir() {
            p.clone()
        } else {
            match p.parent() {
                Some(d) if d.is_dir() => d.to_path_buf(),
                _ => continue,
            }
        };
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

/// How long the app waits, untouched, before the crew dozes (five minutes;
/// `RUNO_DOZE_SECS` shortens it for testing).
fn doze_after() -> Duration {
    let secs = std::env::var("RUNO_DOZE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    Duration::from_secs(secs)
}

/// Extra room at the bottom of a page (the crew is in the sidebar now).
pub(super) const DOCK_ROOM: f32 = 16.;

/// The page ground: a quiet neutral grey under white surfaces.
pub(super) const PAGE: u32 = ui::BG;

/// Width of one of Today's mode tabs plus the gap after it.
const MODE_STEP: f32 = 78.;

/// The rail's place for each part of the app, as a y offset of its highlight.
fn nav_slot(place: Place) -> Option<f32> {
    let i = match place {
        Place::Today => 0,
        Place::Inbox => 1,
        Place::History | Place::Live(_) => 2,
        Place::Chat(_) => 3,
        Place::Crew => 4,
        Place::Engines | Place::Usage | Place::Welcome => return None,
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
        let maker_name = cx.new(|cx| InputState::new(window, cx).placeholder("Scout"));
        let maker_role =
            cx.new(|cx| InputState::new(window, cx).placeholder("Checks the docs match the code"));
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
        let dozing = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let asleep = dozing.clone();
        let mut slept = 0u32;
        let pump = cx.spawn(async move |this, cx| loop {
            // Dozing (nobody around, nothing running): no redraws at all, apart from
            // one every half minute so times like "4m ago" stay true.
            if asleep.load(std::sync::atomic::Ordering::Relaxed) {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                slept += 1;
                if slept < 60 {
                    continue;
                }
                slept = 0;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
                continue;
            }
            slept = 0;
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
            Ok("usage") => Place::Usage,
            Ok("live") => store
                .jobs
                .last()
                .map(|j| Place::Live(j.id))
                .unwrap_or(Place::Today),
            Ok("welcome") => Place::Welcome,
            _ if !store.set_up => Place::Welcome,
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
            dozing,
            touched: Instant::now(),
            hovered: None,
            target: None,
            setup_step: 0,
            home_tab: 0,
            home_pick: None,
            inbox_pick: None,
            setup_crew: data::STARTER_CREW.to_vec(),
            maker_name,
            maker_role,
            maker_like: Member::Checker,
            hover_at: Instant::now(),
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
            checking: Vec::new(),
            diff_open: None,
            diff_text: String::new(),
            shipping: Vec::new(),
            watching: Vec::new(),
            watched: Instant::now() - Duration::from_secs(3600),
            landing: Vec::new(),
            ways: HashMap::new(),
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
        let to = Mode::ALL
            .into_iter()
            .filter(|m| Member::doer_for(*m).is_some())
            .position(|m| m == self.mode)
            .unwrap_or(0) as f32
            * MODE_STEP;
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
        let doer = self.doer();
        if self.mode == Mode::Ask {
            self.go(Place::Chat(doer), cx);
            self.ask(doer, text, cx);
            return;
        }
        let Some(folder) = self.store.folder().cloned() else {
            return;
        };
        let mode = self.mode;
        self.start_job_in(doer, mode, text, folder, cx);
    }

    /// The bot a job from Today's box goes to: a made bot picked for it, else the
    /// crew member for the mode.
    fn doer(&self) -> Member {
        self.target
            .filter(|t| t.mode() == self.mode && Member::all().contains(t))
            .or_else(|| Member::doer_for(self.mode))
            .unwrap_or(Member::for_mode(self.mode))
    }

    /// Adds the bot in the maker to the crew.
    fn save_made(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.maker_name.read(cx).value().trim().to_string();
        let role = self.maker_role.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.say("Give your bot a name first.");
            cx.notify();
            return;
        }
        if Member::all()
            .iter()
            .any(|m| m.name().eq_ignore_ascii_case(&name))
        {
            self.say(format!("There's already a bot called {name}."));
            cx.notify();
            return;
        }
        let id = self.store.made.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let like = self.maker_like;
        let role = if role.is_empty() {
            like.role().to_string()
        } else {
            role
        };
        self.store.made.push(data::Made {
            id,
            name: name.clone(),
            role,
            like,
            look: self.maker.code(),
        });
        data::set_made(&self.store.made);
        self.store.save();
        for input in [&self.maker_name, &self.maker_role] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        self.entered = Instant::now();
        self.say(format!("{name} joined the crew."));
        cx.notify();
    }

    /// Adds a built-in bot to the crew, or removes it (with its chat; its past
    /// jobs stay in History).
    fn set_on_crew(&mut self, who: Member, on: bool, cx: &mut Context<Self>) {
        if who.is_made() {
            return;
        }
        if !on && Member::all().len() <= 1 {
            self.say("Keep at least one bot on the crew.");
            cx.notify();
            return;
        }
        if !on && self.store.busy(who).is_some() {
            self.say(format!("{} is working. Stop the job first.", who.name()));
            cx.notify();
            return;
        }
        if !on {
            self.store.chats.remove(&who);
            if self.target == Some(who) {
                self.target = None;
            }
        }
        self.store.removed.retain(|m| *m != who);
        if !on {
            self.store.removed.push(who);
        }
        data::set_removed(&self.store.removed);
        self.store.save();
        if !on && self.place == Place::Chat(who) {
            self.place = Place::Crew;
        }
        self.say(if on {
            format!("Added {} to your crew.", who.name())
        } else {
            format!("Removed {} from your crew.", who.name())
        });
        cx.notify();
    }

    /// Takes a made bot off the crew. Its past jobs stay in History.
    fn remove_made(&mut self, who: Member, cx: &mut Context<Self>) {
        let Member::Made(id) = who else { return };
        if self.store.busy(who).is_some() {
            self.say(format!("{} is working. Stop the job first.", who.name()));
            cx.notify();
            return;
        }
        let name = who.name();
        self.store.made.retain(|m| m.id != id);
        self.store.chats.remove(&who);
        data::set_made(&self.store.made);
        if self.target == Some(who) {
            self.target = None;
        }
        if self.place == Place::Chat(who) {
            self.place = Place::Crew;
        }
        self.store.save();
        self.say(format!("{name} left the crew."));
        cx.notify();
    }

    /// The person is back: the crew wakes up.
    fn wake(&mut self, cx: &mut Context<Self>) {
        self.touched = Instant::now();
        if self
            .dozing
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            cx.notify();
        }
    }

    /// Folders dropped on the window join the list; the last one becomes current.
    fn drop_paths(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        let mut added = Vec::new();
        for dir in folders_from(paths) {
            self.store.add_folder(dir);
            if let Some(f) = self.store.folder() {
                added.push(f.name.clone());
            }
        }
        if added.is_empty() {
            return;
        }
        self.store.save();
        self.say(match added.as_slice() {
            [one] => format!("The crew now works in {one}."),
            many => format!(
                "Added {} folders. The crew now works in {}.",
                many.len(),
                many.last().cloned().unwrap_or_default()
            ),
        });
        cx.notify();
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
        let (bot, mode, title, folder) = (j.bot, j.mode, j.title.clone(), j.folder.clone());
        self.start_job_in(bot, mode, title, folder, cx);
    }

    fn start_job_in(
        &mut self,
        bot: Member,
        mode: Mode,
        text: String,
        folder: store::Folder,
        cx: &mut Context<Self>,
    ) {
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
        // A job that may change files gets its own branch and worktree, so your
        // checkout stays as it is and its changes wait for your approval.
        if mode.writes() {
            match work::isolate(&job.folder.path, &text, job.id) {
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
    pub(super) fn recheck(&mut self, id: u64, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    /// What Approve does for this job's project.
    pub(super) fn way(&mut self, job: &store::Job) -> ship::Way {
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

    /// The Approve button's words for this job.
    pub(super) fn approve_label(&mut self, job: &store::Job) -> String {
        match self.way(job) {
            ship::Way::PullRequest => "Approve & open PR".into(),
            ship::Way::Merge => format!("Approve & merge into {}", job.base),
        }
    }

    /// Switches what Approve does in a job's project: pull request or local merge.
    pub(super) fn set_way(&mut self, id: u64, way: ship::Way, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    /// Approves a job's changes: a pull request from Shipper on GitHub projects,
    /// otherwise merged into the branch it started from.
    pub(super) fn approve(&mut self, id: u64, cx: &mut Context<Self>) {
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
            cx.notify();
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
                if let Some(j) = self.store.job_mut(id) {
                    j.feed
                        .push(store::Item::Error(format!("Couldn't merge it: {why}")));
                }
                self.say("Couldn't merge it. The reason is on the job.");
            }
        }
        self.store.save();
        cx.notify();
    }

    /// Throws a job's changes away, with its branch and worktree.
    pub(super) fn discard_job(&mut self, id: u64, cx: &mut Context<Self>) {
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
        self.diff_open = None;
        self.say("Discarded. Your checkout was never touched.");
        self.store.save();
        cx.notify();
    }

    /// Merges (or closes) a job's pull request on GitHub now.
    pub(super) fn land_pr(&mut self, id: u64, merge: bool, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    /// Turns "merge when checks pass" on or off for a job's pull request.
    pub(super) fn set_auto_merge(&mut self, id: u64, on: bool, cx: &mut Context<Self>) {
        if let Some(j) = self.store.job_mut(id) {
            j.auto_merge = on;
        }
        self.store.save();
        cx.notify();
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
                Err(std::sync::mpsc::TryRecvError::Empty) => break true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break false,
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
                    j.feed.push(store::Item::Error(format!(
                        "{} moved while this was waiting, and these files now clash: {}. Send it back and ask it to bring the branch up to date with {}.",
                        j.base,
                        files.join(", "),
                        j.base
                    )));
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
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
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
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
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

    pub(super) fn stop_job(&mut self, id: u64, cx: &mut Context<Self>) {
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
        self.poll_ship();
        // Checker's runs of the checks coming back.
        let mut back = Vec::new();
        self.checking.retain(|(id, rx)| match rx.try_recv() {
            Ok(results) => {
                back.push((*id, results));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                back.push((*id, Vec::new()));
                false
            }
        });
        for (id, results) in back {
            if let Some(job) = self.store.job_mut(id) {
                job.checks = results;
                job.checking = false;
            }
            self.store.save();
        }
        if !finished.is_empty() {
            self.runners.retain(|r| !finished.contains(&r.id));
            for id in &finished {
                self.begin_review(*id);
            }
            for id in &finished {
                if let Some(j) = self.store.job(*id).cloned() {
                    if self.place != Place::Live(*id) {
                        let msg = match j.status {
                            Status::Done => {
                                format!("{} finished \u{201c}{}\u{201d}.", j.bot.name(), j.title)
                            }
                            _ => format!("{} stopped: \u{201c}{}\u{201d}.", j.bot.name(), j.title),
                        };
                        self.say(msg.clone());
                    }
                    // Runo is behind other windows: tell the person on the desktop.
                    if !self.active.load(std::sync::atomic::Ordering::Relaxed) {
                        let title = match j.status {
                            Status::Done => format!("{} finished", j.bot.name()),
                            _ => format!("{} stopped", j.bot.name()),
                        };
                        crate::settings::notify(&title, &j.title);
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
        // With motion off, or while dozing, every bot holds one calm pose.
        let moving = self.motion && !self.dozing.load(std::sync::atomic::Ordering::Relaxed);
        let t = if moving { self.now() } else { 2.0 };
        let image = self.holos.image(pose, size, framing, t, self.scale, moving);
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

    /// The bot whose chat was open last, else Explainer.
    fn last_chat(&self) -> Member {
        Member::all()
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
        crate::framestats::begin();
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
        // While something on screen is gliding, draw every frame the display
        // shows (60 or 120 a second), not on the crew's slower clock.
        if busy && self.motion {
            window.request_animation_frame();
        }
        let want = if busy {
            FPS as u32
        } else if working && idle < Duration::from_secs(20) {
            // Progress sweeps and breathing dots stay smooth while you watch.
            FPS as u32
        } else if working {
            self.holos.asked.max(4)
        } else {
            self.holos.asked.max(2)
        };
        let want = if idle > Duration::from_secs(120) {
            want.min(15)
        } else if idle > Duration::from_secs(20) {
            want.min(30)
        } else {
            want
        };
        // Bots moving at full rate follow the display's own refresh, so every
        // frame lands on a vsync instead of a timer that drifts against it.
        if self.motion && want >= FPS as u32 && window.is_window_active() {
            window.request_animation_frame();
        }
        // The first frame on screen takes focus: asking before the window is
        // up is ignored on recent macOS.
        static FRONT: std::sync::Once = std::sync::Once::new();
        FRONT.call_once(|| {
            window.activate_window();
            cx.activate(true);
        });
        self.rate.store(want, std::sync::atomic::Ordering::Relaxed);
        // Five minutes with nobody around and nothing running: doze.
        let doze = self.motion
            && idle > doze_after()
            && !working
            && !self.palette
            && self
                .toast
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed() > Duration::from_secs(4));
        self.dozing
            .store(doze, std::sync::atomic::Ordering::Relaxed);
        self.holos.begin(window, cx);
        self.motion = cx.global::<crate::settings::AppSettings>().animations;
        // The mode in Today's box must have a bot on the crew to take it.
        if Member::doer_for(self.mode).is_none() {
            if let Some(m) = Mode::ALL
                .into_iter()
                .find(|m| Member::doer_for(*m).is_some())
            {
                self.set_mode(m, window, cx);
            }
        }
        let body = match self.place {
            Place::Today => self.today(cx),
            Place::Live(id) => self.live(id, cx),
            Place::Crew => self.crew(cx),
            Place::Chat(who) => self.chat(who, cx),
            Place::Inbox => self.inbox(cx),
            Place::History => self.history(cx),
            Place::Engines => self.engines(cx),
            Place::Usage => self.usage(cx),
            Place::Welcome => self.welcome(cx),
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
                        .bg(c(ui::FILL))
                        .text_color(c(ui::ON_FILL))
                        .text_size(px(13.))
                        .shadow(ui::lift())
                        .child(text),
                )
        });
        let setup = self.place == Place::Welcome;
        let root = div()
            .size_full()
            .relative()
            .flex()
            .track_focus(&self.focus)
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                this.wake(cx);
            }))
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, cx| {
                this.wake(cx);
            }))
            // Drop a folder (or a file in it) anywhere on the window to work in it.
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(ca(ui::ORANGE, 0.06)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.drop_paths(paths.paths(), cx);
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
                    "7" => Place::Usage,
                    _ => return,
                };
                this.go(to, cx);
                cx.stop_propagation();
            }))
            .font_family("Geist")
            .text_color(c(ui::INK))
            .bg(c(PAGE))
            // The walkthrough has the whole window; the sidebar comes after it.
            .when(!setup, |d| d.child(self.sidebar(cx)))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .when(!setup, |d| d.child(self.topbar(cx)))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .relative()
                            .overflow_hidden()
                            .child(div().size_full().child(body)),
                    ),
            )
            .children(toast_el)
            // Deferred, so it paints over the crew's frames too.
            .when(self.palette, |d| d.child(self.palette_view(cx)))
            // Painted last: with RUNO_FRAME_STATS set, the frame's timing is noted.
            .child(
                canvas(|_, _, _| (), |_, _, _, _| crate::framestats::painted())
                    .absolute()
                    .size_0(),
            );
        crate::framestats::rendered();
        root
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
        // Solid: the colour wash is drawn by the app, so there's nothing to see
        // through, and blending what's behind the window costs every frame.
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        // v2 is dark throughout, so text boxes and menus from the kit follow.
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(window), cx);
        gpui_kit::component::Theme::update(cx, |t| {
            t.background = ui::c(ui::BG);
            t.foreground = ui::c(ui::INK);
            t.border = ui::c(ui::LINE);
            t.input = ui::c(ui::LINE);
            t.muted_foreground = ui::c(ui::MUTED);
        });
        cx.new(|cx| Shell::new(window, cx))
    });
    // Come to the front when opened, not behind whatever had focus (the first
    // frame asks again, once the window is showing).
    cx.activate(true);
    if let Ok((handle, _)) = opened {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
    cx.activate(true);
}

#[cfg(test)]
mod drop_tests {
    use super::folders_from;

    #[test]
    fn dropped_files_mean_their_folder() {
        let dir = std::env::temp_dir().join(format!("runo-drop-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let got = folders_from(&[
            dir.join("a.txt"),
            dir.clone(),
            dir.join("sub"),
            dir.join("missing/nope.txt"),
        ]);
        assert_eq!(got, vec![dir.clone(), dir.join("sub")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
