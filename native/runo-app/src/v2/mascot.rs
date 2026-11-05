//! Fin, the crew's mascot, and how each bot wears it.
//!
//! Every bot is drawn as Fin by default, in its own colour and dressed for its
//! job: the Builder in a hard hat with a wrench, the Guard as security with a
//! shield and baton, and so on. Settings → Mascot changes any of it, or goes
//! back to the bubble crew. Choices live in `AppSettings` (`mascot`,
//! `fin_material`, `fin_looks`); a parsed copy is kept here so drawing a bot
//! never has to reach for the settings.

use std::collections::HashMap;
use std::sync::RwLock;

use runo_holo::fin::{Badge, Fin, Hat, Held, Material, Outfit, Tool, ROLES};
use runo_holo::{Figure, Mood, Tint};

use crate::settings::AppSettings;

use super::data::Member;
use super::engine::bot_key;

/// The colours a Fin can be, with the names they're saved under.
pub const COLOURS: [(&str, &str, Tint); 7] = [
    ("orange", "Orange", Tint::ORANGE),
    ("green", "Green", Tint::GREEN),
    ("amber", "Amber", Tint::AMBER),
    ("red", "Red", Tint::RED),
    ("blue", "Blue", Tint::BLUE),
    ("violet", "Violet", Tint::VIOLET),
    ("teal", "Teal", Tint::TEAL),
];

pub fn colour_id(t: Tint) -> &'static str {
    COLOURS
        .iter()
        .find(|c| c.2 == t)
        .map(|c| c.0)
        .unwrap_or("violet")
}

fn colour(id: &str) -> Option<Tint> {
    COLOURS.iter().find(|c| c.0 == id).map(|c| c.2)
}

/// What a bot wears when the person hasn't changed it: a kit that fits its job.
pub fn default_look(who: Member) -> (Tint, Outfit) {
    let kit = |hat, left, right, badge| Outfit {
        hat,
        left,
        right,
        badge,
    };
    match who {
        Member::Builder => (
            Tint::ORANGE,
            kit(Hat::HardHat, Held::None, Tool::Wrench, Badge::Id),
        ),
        Member::Checker => (
            Tint::GREEN,
            kit(Hat::Grad, Held::Clipboard, Tool::Magnifier, Badge::Id),
        ),
        Member::Guard => (
            Tint::AMBER,
            kit(Hat::Security, Held::Shield, Tool::Baton, Badge::Star),
        ),
        Member::Fixer => (
            Tint::RED,
            kit(Hat::Bandana, Held::None, Tool::Magnifier, Badge::Gear),
        ),
        Member::Explainer => (
            Tint::BLUE,
            kit(Hat::Headset, Held::Coffee, Tool::None, Badge::NameTag),
        ),
        Member::Keeper => (
            Tint::VIOLET,
            kit(Hat::None, Held::None, Tool::Broom, Badge::Gear),
        ),
        Member::Shipper => (
            Tint::TEAL,
            kit(Hat::Cap, Held::Parcel, Tool::None, Badge::NameTag),
        ),
        Member::Fin => (
            Tint::VIOLET,
            kit(Hat::Crown, Held::Clipboard, Tool::None, Badge::Star),
        ),
        Member::Made(_) => (who.bubble().tint, Outfit::PLAIN),
    }
}

/// `colour.hat.left.right.badge`, the saved form of a look.
pub fn code(tint: Tint, outfit: Outfit) -> String {
    format!("{}.{}", colour_id(tint), Fin::outfit_code(outfit))
}

fn parse(code: &str) -> Option<(Tint, Outfit)> {
    let (c, rest) = code.split_once('.')?;
    Some((colour(c)?, Fin::outfit_from_code(rest)))
}

#[derive(Default)]
struct Looks {
    on: bool,
    material: Option<Material>,
    saved: HashMap<String, String>,
    parsed: HashMap<String, (Tint, Outfit)>,
}

static LOOKS: std::sync::LazyLock<RwLock<Looks>> = std::sync::LazyLock::new(|| {
    RwLock::new(Looks {
        on: true,
        ..Looks::default()
    })
});

/// Takes the person's choices from the settings; cheap when nothing changed.
pub fn sync(st: &AppSettings) {
    let on = st.mascot != "bubble";
    let material = Material::from_id(&st.fin_material);
    {
        let l = LOOKS.read().unwrap_or_else(|e| e.into_inner());
        if l.on == on && l.material == Some(material) && l.saved == st.fin_looks {
            return;
        }
    }
    let mut l = LOOKS.write().unwrap_or_else(|e| e.into_inner());
    l.on = on;
    l.material = Some(material);
    l.saved = st.fin_looks.clone();
    l.parsed = st
        .fin_looks
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), parse(v)?)))
        .collect();
}

/// The bot's look as saved, or its default.
pub fn look_in(st: &AppSettings, who: Member) -> (Tint, Outfit) {
    st.fin_looks
        .get(&bot_key(who))
        .and_then(|c| parse(c))
        .unwrap_or_else(|| default_look(who))
}

/// The bot's Fin as the settings have it.
pub fn fin_in(st: &AppSettings, who: Member) -> Fin {
    let (tint, outfit) = look_in(st, who);
    Fin {
        tint,
        material: Material::from_id(&st.fin_material),
        outfit,
    }
}

/// Saves a bot's look; the default look is stored as no entry at all.
pub fn set_look(st: &mut AppSettings, who: Member, tint: Tint, outfit: Outfit) {
    let key = bot_key(who);
    if (tint, outfit) == default_look(who) {
        st.fin_looks.remove(&key);
    } else {
        st.fin_looks.insert(key, code(tint, outfit));
    }
}

/// The role kit a look matches, by outfit alone (any colour).
pub fn role_of(outfit: Outfit) -> Option<&'static runo_holo::fin::Role> {
    ROLES.iter().find(|r| r.outfit == outfit)
}

/// How `who` is drawn in `mood`, as the last [`sync`] left things.
pub fn figure(who: Member, mood: Mood) -> Figure {
    let l = LOOKS.read().unwrap_or_else(|e| e.into_inner());
    if !l.on {
        return Figure::Bubble(who.bubble().pose(mood));
    }
    let (tint, outfit) = l
        .parsed
        .get(&bot_key(who))
        .copied()
        .unwrap_or_else(|| default_look(who));
    Figure::Fin(
        Fin {
            tint,
            material: l.material.unwrap_or(Material::Glass),
            outfit,
        }
        .pose(mood),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_round_trip_and_defaults_are_not_saved() {
        let mut st = AppSettings::default();
        let (t, o) = default_look(Member::Guard);
        set_look(&mut st, Member::Guard, t, o);
        assert!(st.fin_looks.is_empty());
        set_look(&mut st, Member::Guard, Tint::RED, ROLES[6].outfit);
        assert_eq!(look_in(&st, Member::Guard), (Tint::RED, ROLES[6].outfit));
    }

    #[test]
    fn every_built_in_bot_wears_something() {
        for who in std::iter::once(Member::Fin).chain(Member::CREW) {
            assert_ne!(default_look(who).1, Outfit::PLAIN, "{who:?}");
        }
    }
}
