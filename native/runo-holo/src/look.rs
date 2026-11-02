//! What a bot looks like: its cube, colours, face, props and motion.
//!
//! The six crew bots and their moods are the presets from the Holo v2 design.
//! A custom bot is just another [`Look`], built from the same parts.

/// An sRGB colour, `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u32);

impl Rgb {
    pub fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn b(self) -> u8 {
        self.0 as u8
    }
}

/// A bot's three colours: the light body tint, the strong accent (rings,
/// props, the projector disk) and a second tint that swirls in the glass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub light: Rgb,
    pub accent: Rgb,
    pub swirl: Rgb,
}

impl Palette {
    pub const ORANGE: Palette = Palette {
        light: Rgb(0xFFB58A),
        accent: Rgb(0xFF6A2B),
        swirl: Rgb(0xFFDB7A),
    };
    pub const GREEN: Palette = Palette {
        light: Rgb(0x8EF0C0),
        accent: Rgb(0x1FC27A),
        swirl: Rgb(0x9AD4FF),
    };
    pub const AMBER: Palette = Palette {
        light: Rgb(0xFFDB7A),
        accent: Rgb(0xFFAA1F),
        swirl: Rgb(0xFFB58A),
    };
    pub const RED: Palette = Palette {
        light: Rgb(0xFF9AA6),
        accent: Rgb(0xF2364F),
        swirl: Rgb(0xC7B4FF),
    };
    pub const BLUE: Palette = Palette {
        light: Rgb(0x9AD4FF),
        accent: Rgb(0x1E95F0),
        swirl: Rgb(0x8EF0C0),
    };
    pub const VIOLET: Palette = Palette {
        light: Rgb(0xC7B4FF),
        accent: Rgb(0x7B4DF5),
        swirl: Rgb(0xFF9AA6),
    };
    pub const PINK: Palette = Palette {
        light: Rgb(0xFFA8C8),
        accent: Rgb(0xE8457E),
        swirl: Rgb(0xFFDB7A),
    };

    /// Every palette a custom bot can pick from.
    pub const ALL: [Palette; 7] = [
        Self::ORANGE,
        Self::GREEN,
        Self::AMBER,
        Self::RED,
        Self::BLUE,
        Self::VIOLET,
        Self::PINK,
    ];
}

/// The eyes. Each carries its own small habit (looking around, blinking,
/// focusing a lens).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eye {
    /// Two round eyes that look around and double-blink.
    Dots,
    /// Checker's single lens: a dashed ring turns, the pupil focuses and scans.
    Lens,
    /// Guard's visor slots, with a glint that slides across.
    Visor,
    /// Closed, sleepy arcs that peek open now and then.
    Sleepy,
    /// One eye hunting about, the other squinting shut.
    Squint,
    /// Smiling arcs.
    Smile,
    /// Big happy arcs, for done.
    Happy,
    /// Round eyes looking up and away, thinking.
    Up,
    /// Round eyes reading left and right, working.
    Read,
    /// Round eyes looking at you with raised brows, hopeful.
    You,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouth {
    Grin,
    O,
    Open,
    Small,
    /// Checker's mouth: a doubtful line that turns into a smile as the tick pops.
    Check,
    Talk,
    Flat,
}

/// How the whole bot moves over its projector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// A slow float up and down.
    Bob,
    /// A quick float, for working.
    BobFast,
    /// A hop with a squash and a little bounce.
    Hop,
    /// A hopeful lift with a lean.
    Hope,
}

/// Small props drawn around the cube.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Props {
    pub antenna: bool,
    pub code: bool,
    pub laser: bool,
    pub lock: bool,
    pub sweat: bool,
    pub stars: bool,
    pub shield: bool,
    pub bolt: bool,
    pub talk_dots: bool,
    pub zz: bool,
    pub think: bool,
    pub sparks: bool,
    pub done: bool,
    pub ask: bool,
}

/// Everything needed to draw one bot. All lengths are in the design's
/// 200 × 200 view box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub palette: Palette,
    /// Half the cube's width.
    pub w: f64,
    /// The height of the top face's slant.
    pub h: f64,
    /// The height of the side faces.
    pub s: f64,
    /// How far the cube turns either way.
    pub turn: f64,
    /// Seconds for one full turn back and forth.
    pub turn_secs: f64,
    pub eye: Eye,
    pub mouth: Mouth,
    pub face_scale: f64,
    pub motion: Motion,
    /// Rock side to side, like Fixer.
    pub tilt: bool,
    /// Breathe deeply, like Keeper.
    pub deep: bool,
    /// Race the scan line, for working.
    pub fast: bool,
    pub props: Props,
}

impl Look {
    fn base(palette: Palette) -> Look {
        Look {
            palette,
            w: 44.0,
            h: 23.0,
            s: 52.0,
            turn: 10.0,
            turn_secs: 7.0,
            eye: Eye::Dots,
            mouth: Mouth::Grin,
            face_scale: 1.2,
            motion: Motion::Bob,
            tilt: false,
            deep: false,
            fast: false,
            props: Props::default(),
        }
    }

    /// Top of the cube in the view box.
    pub fn top(&self) -> f64 {
        (96.0 - self.total_height() / 2.0).round()
    }

    /// The cube's full height, top corner to bottom corner.
    pub fn total_height(&self) -> f64 {
        2.0 * self.h + self.s
    }

    /// Where the face sits.
    pub fn face_y(&self) -> f64 {
        self.top() + 2.0 * self.h + self.s * 0.36
    }
}

/// The crew.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bot {
    Builder,
    Checker,
    Guard,
    Fixer,
    Explainer,
    Keeper,
}

impl Bot {
    pub const ALL: [Bot; 6] = [
        Bot::Builder,
        Bot::Checker,
        Bot::Guard,
        Bot::Fixer,
        Bot::Explainer,
        Bot::Keeper,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Bot::Builder => "Builder",
            Bot::Checker => "Checker",
            Bot::Guard => "Guard",
            Bot::Fixer => "Fixer",
            Bot::Explainer => "Explainer",
            Bot::Keeper => "Keeper",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Bot::Builder => Palette::ORANGE,
            Bot::Checker => Palette::GREEN,
            Bot::Guard => Palette::AMBER,
            Bot::Fixer => Palette::RED,
            Bot::Explainer => Palette::BLUE,
            Bot::Keeper => Palette::VIOLET,
        }
    }

    /// The bot doing what it is known for: Builder hopping with code floating
    /// out, Checker scanning, Guard shielding, and so on.
    pub fn signature(self) -> Look {
        let mut l = Look::base(self.palette());
        match self {
            Bot::Builder => {
                l.props.antenna = true;
                l.props.code = true;
                l.motion = Motion::Hop;
            }
            Bot::Checker => {
                (l.w, l.h, l.s, l.turn) = (34.0, 18.0, 80.0, 7.0);
                l.eye = Eye::Lens;
                l.face_scale = 1.1;
                l.props.laser = true;
            }
            Bot::Guard => {
                (l.w, l.h, l.s, l.turn, l.turn_secs) = (58.0, 28.0, 40.0, 12.0, 9.0);
                l.face_scale = 1.3;
                l.eye = Eye::Visor;
                l.props.shield = true;
                l.props.lock = true;
            }
            Bot::Fixer => {
                l.eye = Eye::Squint;
                l.tilt = true;
                l.props.bolt = true;
                l.props.sweat = true;
            }
            Bot::Explainer => {
                l.eye = Eye::Smile;
                l.props.talk_dots = true;
                l.mouth = Mouth::Talk;
                return l;
            }
            Bot::Keeper => {
                (l.w, l.h, l.s, l.turn_secs, l.turn) = (44.0, 22.0, 48.0, 11.0, 8.0);
                l.eye = Eye::Sleepy;
                l.props.zz = true;
                l.props.stars = true;
                l.deep = true;
            }
        }
        l.mouth = default_mouth(l.eye);
        l
    }

    pub fn look(self, mood: Mood) -> Look {
        mood.apply(self.signature())
    }
}

fn default_mouth(eye: Eye) -> Mouth {
    match eye {
        Eye::Lens => Mouth::Check,
        Eye::Visor => Mouth::Grin,
        Eye::Sleepy => Mouth::Small,
        Eye::Squint => Mouth::Flat,
        Eye::Smile => Mouth::Open,
        _ => Mouth::Grin,
    }
}

/// How a bot is feeling. The turn speed, the scan and the little extras carry
/// the mood; the cube's shape, colours and own props stay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mood {
    /// Doing its own thing (the bot's signature look).
    Signature,
    /// Swaying slowly, looking around.
    Idle,
    /// Eyes up, thoughts drifting out.
    Thinking,
    /// Spinning fast, the scan racing.
    Working,
    /// Hopping, rings bursting, beaming.
    Done,
    /// Looking at you, hopeful, with a question mark.
    NeedsYou,
    /// Eyes closed, breathing deeply.
    Asleep,
}

impl Mood {
    pub const ALL: [Mood; 7] = [
        Mood::Signature,
        Mood::Idle,
        Mood::Thinking,
        Mood::Working,
        Mood::Done,
        Mood::NeedsYou,
        Mood::Asleep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mood::Signature => "Signature",
            Mood::Idle => "Idle",
            Mood::Thinking => "Thinking",
            Mood::Working => "Working",
            Mood::Done => "Done",
            Mood::NeedsYou => "Needs you",
            Mood::Asleep => "Asleep",
        }
    }

    pub fn apply(self, mut l: Look) -> Look {
        // Props that belong to the bot's body stay; the mood's extras replace
        // the signature's floating extras.
        let keep = Props {
            antenna: l.props.antenna,
            shield: l.props.shield && matches!(self, Mood::Signature | Mood::NeedsYou | Mood::Idle),
            lock: l.props.lock
                && matches!(
                    self,
                    Mood::Signature | Mood::NeedsYou | Mood::Idle | Mood::Thinking
                ),
            ..Props::default()
        };
        let own_eye = matches!(l.eye, Eye::Lens | Eye::Visor);
        match self {
            Mood::Signature => return l,
            Mood::Idle => {
                l.props = keep;
                l.turn_secs = 9.0;
                if !own_eye {
                    l.eye = Eye::Dots;
                }
                l.motion = Motion::Bob;
            }
            Mood::Thinking => {
                l.props = keep;
                l.props.think = true;
                l.turn_secs = 12.0;
                l.turn = 6.0;
                if !own_eye {
                    l.eye = Eye::Up;
                }
                l.mouth = Mouth::O;
                l.motion = Motion::Bob;
            }
            Mood::Working => {
                l.props = keep;
                l.props.sparks = true;
                l.turn_secs = 2.2;
                l.turn = 14.0;
                l.fast = true;
                if l.eye != Eye::Lens {
                    l.eye = Eye::Read;
                }
                l.motion = Motion::BobFast;
                l.mouth = default_mouth(l.eye);
                if l.eye == Eye::Lens {
                    l.props.laser = true;
                }
            }
            Mood::Done => {
                l.props = keep;
                l.props.done = true;
                l.eye = Eye::Happy;
                l.mouth = Mouth::Open;
                l.motion = Motion::Hop;
                l.tilt = false;
            }
            Mood::NeedsYou => {
                l.props = keep;
                l.props.ask = true;
                if l.eye != Eye::Visor {
                    l.eye = Eye::You;
                }
                l.mouth = Mouth::Small;
                l.motion = Motion::Hope;
                l.tilt = false;
            }
            Mood::Asleep => {
                l.props = keep;
                l.props.zz = true;
                l.eye = Eye::Sleepy;
                l.mouth = Mouth::Small;
                l.deep = true;
                l.turn_secs = 11.0;
                l.turn = 6.0;
                l.motion = Motion::Bob;
                l.tilt = false;
            }
        }
        l
    }
}

/// The shape a custom bot can take, from the crew screen's "Make a bot".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Cube,
    Tall,
    Wide,
    Soft,
}

/// Builds a custom bot's look from the parts a person picks.
pub fn custom(palette: Palette, shape: Shape, eye: Eye) -> Look {
    let mut l = Look::base(palette);
    match shape {
        Shape::Cube => {}
        Shape::Tall => (l.w, l.h, l.s, l.turn) = (34.0, 18.0, 72.0, 7.0),
        Shape::Wide => (l.w, l.h, l.s, l.turn, l.face_scale) = (54.0, 27.0, 38.0, 11.0, 1.25),
        Shape::Soft => (l.w, l.h, l.s, l.turn_secs, l.deep) = (44.0, 22.0, 48.0, 10.0, true),
    }
    l.eye = eye;
    l.mouth = default_mouth(eye);
    if eye == Eye::Sleepy {
        l.props.stars = true;
    }
    l
}
