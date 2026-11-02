//! Runo v2: the crew app.
//!
//! A new shell around Runo's bots: Today (the morning view and the one
//! question that waits), Live (watching a bot work), Crew (every bot, and
//! making your own) and Chat (asking the crew in plain words). The bots are
//! the Holo crew from `runo-holo`, drawn every frame from the same rigs as the
//! design.
//!
//! Runo v1 stays one click away on the rail while v2 grows.

mod chat;
mod crew;
mod data;
mod engine;
mod engines;
mod live;
mod today;
mod ui;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{detail_for, Framing, Look, Mood, Renderer};

use data::{Job, Member, Mode};
use ui::c;

/// Where the person is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Today,
    Inbox,
    History,
    Chat,
    Crew,
    Schedule,
    Rules,
    Live(Member),
    Engines,
}

/// The crew's frames, drawn once a tick and shared by every place that shows the
/// same bot at the same size. Small bots move less, so they get fewer ticks: a 24 px
/// avatar redraws 10 times a second, a big one 30. A frame is kept until its bot's
/// next tick, so a still bot (motion off) is drawn once and never again.
struct Holos {
    frame: u64,
    images: HashMap<(u64, u32, bool), Frame>,
    renderers: HashMap<u16, Renderer>,
    /// The fastest rate any bot drawn last frame needed, so the clock can slow down
    /// on screens with only small bots.
    pub asked: u32,
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
fn fps_for(size: f32) -> u32 {
    if size >= 90.0 {
        30
    } else if size >= 40.0 {
        15
    } else {
        10
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
        let mut gone = Vec::new();
        for (k, f) in self.images.iter() {
            if f.used + 2 < frame {
                gone.push(*k);
            }
        }
        for k in gone {
            if let Some(f) = self.images.remove(&k) {
                cx.drop_image(f.image, Some(window));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn image(
        &mut self,
        key: u64,
        look: &Look,
        logical: f32,
        framing: Framing,
        t: f64,
        scale: f32,
        moving: bool,
    ) -> Arc<RenderImage> {
        let device = (logical * scale).round().clamp(8.0, 1024.0) as u16;
        let bust = framing == Framing::Bust;
        let fps = fps_for(logical);
        let tick = if moving { (t * fps as f64) as u64 } else { 0 };
        if moving {
            self.asked = self.asked.max(fps);
        }
        let frame = self.frame;
        let slot = (key, device as u32, bust);
        if let Some(f) = self.images.get_mut(&slot) {
            f.used = frame;
            if f.tick == tick {
                return f.image.clone();
            }
        }
        let r = self
            .renderers
            .entry(device)
            .or_insert_with(|| Renderer::new(device));
        // Draw at the tick's own time, so every size stays in step.
        let at = if moving { tick as f64 / fps as f64 } else { t };
        let premul = r.draw(look, at, framing, detail_for(logical as u32, framing));
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
                // The old frame stays in the atlas until this one has painted.
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

/// A line in a chat.
#[derive(Clone)]
pub(crate) struct Said {
    pub mine: bool,
    pub text: String,
    /// Which engine answered, and how long it took.
    pub via: Option<String>,
    /// Runo speaking, not the bot: a problem or a hint.
    pub note: bool,
}

impl Said {
    fn mine(text: String) -> Self {
        Said {
            mine: true,
            text,
            via: None,
            note: false,
        }
    }
    fn bot(text: String, via: String) -> Self {
        Said {
            mine: false,
            text,
            via: Some(via),
            note: false,
        }
    }
    fn note(text: impl Into<String>) -> Self {
        Said {
            mine: false,
            text: text.into(),
            via: None,
            note: true,
        }
    }
}

/// The v2 window's root.
pub struct Shell {
    place: Place,
    clock: Instant,
    holos: Holos,
    scale: f32,
    mode: Mode,
    jobs: Vec<Job>,
    composer: Entity<InputState>,
    /// The person's answer to Guard's question: `Some(true)` used the fix.
    guard_answer: Option<bool>,
    crew_filter: usize,
    chat_selected: usize,
    chat_input: Entity<InputState>,
    /// What was said in each chat this session, by chat index: (from the person, text).
    chat_log: Vec<Vec<Said>>,
    /// A question on its way to a bot's engine.
    asking: Option<engine::Ask>,
    handed_to_builder: bool,
    live_tab: usize,
    live_input: Entity<InputState>,
    live_extra: Vec<String>,
    live_paused: bool,
    /// The "Make a bot" card cycles through looks.
    maker_seed: u64,
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
    _subs: Vec<Subscription>,
    _pump: Task<()>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx
            .new(|cx| InputState::new(window, cx).placeholder("Tell the crew what you want done…"));
        let chat_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Ask anything about the product…"));
        let live_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Tell Builder something while it works…")
        });
        let subs = vec![
            cx.subscribe_in(&composer, window, |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if !text.is_empty() {
                        input.update(cx, |i, cx| i.set_value("", window, cx));
                        this.drop_job(text, cx);
                    }
                }
            }),
            cx.subscribe_in(&chat_input, window, |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if !text.is_empty() {
                        input.update(cx, |i, cx| i.set_value("", window, cx));
                        this.ask(this.chat_selected, text, cx);
                    }
                }
            }),
            cx.subscribe_in(&live_input, window, |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if !text.is_empty() {
                        input.update(cx, |i, cx| i.set_value("", window, cx));
                        this.live_extra.push(text);
                        cx.notify();
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
                1000.0 / want.min(8) as f64
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
        let start = match std::env::var("RUNO_V2_PLACE").as_deref() {
            Ok("live") => Place::Live(Member::Builder),
            Ok("crew") => Place::Crew,
            Ok("chat") => Place::Chat,
            Ok("inbox") => Place::Inbox,
            Ok("engines") => Place::Engines,
            _ => Place::Today,
        };
        Shell {
            place: start,
            clock: Instant::now(),
            holos: Holos::new(),
            scale: 1.0,
            mode: Mode::Build,
            jobs: data::sample_jobs(),
            composer,
            guard_answer: None,
            crew_filter: 0,
            chat_selected: 0,
            chat_input,
            chat_log: (0..data::CHATS.len()).map(|_| Vec::new()).collect(),
            asking: None,
            handed_to_builder: false,
            live_tab: 0,
            live_input,
            live_extra: Vec::new(),
            live_paused: false,
            maker_seed: 0,
            toast: None,
            entered: Instant::now(),
            motion: true,
            active,
            rate,
            touched: Instant::now(),
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
        }
        cx.notify();
    }

    /// Asks the bot behind chat `chat` for real, on its own engine.
    fn ask(&mut self, chat: usize, text: String, cx: &mut Context<Self>) {
        let who = data::CHATS[chat].who;
        let history: Vec<(bool, String)> = self.chat_log[chat]
            .iter()
            .map(|m| (m.mine, m.text.clone()))
            .collect();
        self.chat_log[chat].push(Said::mine(text.clone()));
        if self.asking.is_some() {
            self.chat_log[chat].push(Said::note(
                "One question at a time: this one waits until the last answer is in.",
            ));
            cx.notify();
            return;
        }
        let st = cx.global::<crate::settings::AppSettings>();
        match engine::Ask::start(st, who, chat, &text, &history) {
            Ok(ask) => self.asking = Some(ask),
            Err(why) => self.chat_log[chat].push(Said::note(why)),
        }
        cx.notify();
    }

    /// Whether `who` is answering a question right now.
    fn thinking(&self, who: Member) -> bool {
        self.asking.as_ref().is_some_and(|a| a.who == who)
    }

    /// Collects an answer that has arrived.
    fn poll_ask(&mut self) {
        let Some(ask) = &self.asking else { return };
        if let Some(reply) = ask.poll() {
            let (chat, engine) = (ask.chat, ask.engine.clone());
            let secs = ask.started.elapsed().as_secs_f32();
            self.chat_log[chat].push(match reply {
                engine::Reply::Answer(text) if !text.is_empty() => {
                    Said::bot(text, format!("{engine} · {secs:.0}s"))
                }
                engine::Reply::Answer(_) => {
                    Said::note("The engine answered with nothing. Try asking again.")
                }
                engine::Reply::Failed(why) => Said::note(why),
            });
            self.asking = None;
        }
    }

    fn say(&mut self, text: impl Into<SharedString>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    /// A sentence from the Today box becomes a job for the bot its mode goes to.
    fn drop_job(&mut self, text: String, cx: &mut Context<Self>) {
        // A question is answered for real, in a chat with Explainer.
        if self.mode == Mode::Ask {
            let chat = data::CHATS
                .iter()
                .position(|c| c.who == Member::Explainer && !c.group)
                .unwrap_or(0);
            self.chat_selected = chat;
            self.go(Place::Chat, cx);
            self.ask(chat, text, cx);
            return;
        }
        let bot = Member::for_mode(self.mode);
        let now = self.now();
        self.jobs.insert(
            0,
            Job {
                title: text,
                bot,
                color: bot.accent(),
                start: 0.04,
                rate: 0.012,
                note: Some("starting"),
                since: now,
            },
        );
        self.jobs.truncate(5);
        self.say(format!(
            "{} has it. It'll ask you once, at the end.",
            bot.name()
        ));
        cx.notify();
    }

    /// A bot drawn at `size` logical pixels, in `mood`.
    fn bot(&mut self, who: Member, mood: Mood, size: f32, framing: Framing) -> Img {
        let look = mood.apply(who.look());
        let key = who.key() * 16 + Mood::ALL.iter().position(|m| *m == mood).unwrap_or(0) as u64;
        self.look_img(key, &look, size, framing)
    }

    fn look_img(&mut self, key: u64, look: &Look, size: f32, framing: Framing) -> Img {
        // With motion off, every bot holds one calm pose.
        let t = if self.motion { self.now() } else { 2.0 };
        let image = self
            .holos
            .image(key, look, size, framing, t, self.scale, self.motion);
        img(image).size(px(size)).flex_none()
    }

    /// A small bust avatar, like the ones beside chat lines.
    fn avatar(&mut self, who: Member, size: f32) -> Img {
        self.bot(who, Mood::Idle, size, Framing::Bust)
    }

    fn rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let place = self.place;
        let nav = [
            (Place::Today, IconName::House, "Today"),
            (Place::Inbox, IconName::Inbox, "Inbox"),
            (Place::History, IconName::Clock, "History"),
            (Place::Chat, IconName::MessageSquare, "Chat"),
            (Place::Crew, IconName::Users, "Crew"),
            (Place::Schedule, IconName::Calendar, "Schedule"),
            (Place::Rules, IconName::Shield, "Rules"),
        ];
        let mut icons = div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.));
        for (i, (p, icon, _label)) in nav.into_iter().enumerate() {
            let on = place == p || (p == Place::Chat && place == Place::Chat);
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
                    .when(on, |d| d.bg(c(0xFFFFFF)).shadow(ui::lift()))
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(Icon::new(icon).size(px(18.)).text_color(c(if on {
                        ui::INK
                    } else {
                        ui::MUTED
                    })))
                    .when(p == Place::Inbox, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(5.))
                                .right(px(5.))
                                .size(px(15.))
                                .rounded_full()
                                .bg(c(0xFF6A2B))
                                .border_2()
                                .border_color(c(ui::RAIL))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(c(0xFFFFFF))
                                .child("2"),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.go(p, cx))),
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
            .gap(px(2.));
        for who in [
            Member::Guard,
            Member::Builder,
            Member::Checker,
            Member::Fixer,
            Member::Explainer,
            Member::Keeper,
            Member::Mochi,
        ] {
            let on = match place {
                Place::Live(m) => m == who,
                Place::Chat => {
                    who == data::CHATS[self.chat_selected.min(data::CHATS.len() - 1)].who
                        && !data::CHATS[self.chat_selected].group
                }
                _ => false,
            };
            let face = self.bot(who, who.mood(), 34., Framing::Bust);
            bots = bots.child(
                div()
                    .id(("rail-bot", who.key() as usize))
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
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(face)
                    .child(
                        div()
                            .absolute()
                            .bottom(px(5.))
                            .right(px(5.))
                            .size(px(9.))
                            .rounded_full()
                            .border_2()
                            .border_color(c(ui::RAIL))
                            .bg(c(who.rail_dot())),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let place = match who {
                            Member::Builder => Place::Live(Member::Builder),
                            other => {
                                if let Some(i) = data::CHATS
                                    .iter()
                                    .position(|ch| ch.who == other && !ch.group)
                                {
                                    this.chat_selected = i;
                                }
                                Place::Chat
                            }
                        };
                        this.go(place, cx);
                    })),
            );
        }
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
                div()
                    .id("nav-engines")
                    .flex_none()
                    .size(px(42.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .cursor_pointer()
                    .when(place == Place::Engines, |d| {
                        d.bg(c(0xFFFFFF)).shadow(ui::lift())
                    })
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(Icon::new(IconName::Cpu).size(px(18.)).text_color(c(
                        if place == Place::Engines {
                            ui::INK
                        } else {
                            ui::MUTED
                        },
                    )))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Engines, cx))),
            )
            .child(
                div()
                    .id("open-v1")
                    .text_size(px(10.))
                    .font_family(ui::MONO)
                    .text_color(c(ui::FAINT))
                    .cursor_pointer()
                    .hover(|s| s.text_color(c(ui::INK)))
                    .child("v1")
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.defer(crate::open_main_window);
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(36.))
                    .rounded_full()
                    .bg(c(0xFFFFFF))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .child("A"),
            )
    }

    /// The places v2 hasn't built yet, with Keeper keeping the spot warm.
    fn soon(&mut self, title: &'static str, line: &'static str) -> AnyElement {
        let keeper = self.bot(Member::Keeper, Mood::Asleep, 180., Framing::Full);
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .child(keeper)
            .child(
                div()
                    .text_size(px(30.))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(c(ui::MUTED))
                    .child(line),
            )
            .into_any_element()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = window.scale_factor();
        // What the last frame needed sets the clock for the next one.
        let busy = self.entered.elapsed() < Duration::from_millis(1000)
            || self
                .toast
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() < Duration::from_millis(3300));
        let resting = self.touched.elapsed() > Duration::from_secs(45);
        let want = if busy {
            FPS as u32
        } else if self.asking.is_some() {
            self.holos.asked.max(10)
        } else {
            self.holos.asked.max(2)
        };
        let want = if resting { want.min(15) } else { want };
        self.rate.store(want, std::sync::atomic::Ordering::Relaxed);
        self.holos.begin(window, cx);
        let body = match self.place {
            Place::Today => self.today(cx),
            Place::Live(who) => self.live(who, cx),
            Place::Crew => self.crew(cx),
            Place::Chat => self.chat(cx),
            Place::Inbox => self.soon(
                "Inbox",
                "Every question the crew has for you, in one place. Coming next.",
            ),
            Place::History => self.soon("History", "What landed, when and why. Coming next."),
            Place::Schedule => self.soon(
                "Schedule",
                "Jobs the crew runs on their own, like Keeper's night shift.",
            ),
            Place::Rules => self.soon("Rules", "The lines Guard keeps for you. Coming next."),
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
        self.poll_ask();
        self.motion = cx.global::<crate::settings::AppSettings>().animations;
        self.active.store(
            window.is_window_active(),
            std::sync::atomic::Ordering::Relaxed,
        );
        let toast = self
            .toast
            .clone()
            .filter(|(_, at)| at.elapsed() < Duration::from_millis(3200));
        let toast_el = toast.map(|(text, at)| {
            let k = (at.elapsed().as_secs_f32() / 0.25).min(1.0);
            let fade = ((3.2 - at.elapsed().as_secs_f32()) / 0.4).clamp(0.0, 1.0);
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
            // ⌘1…⌘6 (Ctrl on Linux and Windows) jump between places.
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = &e.keystroke.modifiers;
                if !(m.platform || m.control) || m.shift || m.alt {
                    return;
                }
                let to = match e.keystroke.key.as_str() {
                    "1" => Place::Today,
                    "2" => Place::Live(Member::Builder),
                    "3" => Place::Crew,
                    "4" => Place::Chat,
                    "5" => Place::Inbox,
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
    let _ = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(window, cx))
    });
}
