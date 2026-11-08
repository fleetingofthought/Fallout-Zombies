//! SUP NVSE, batch 1 (#1008): rounding, points and angles, numbers in words, the
//! Cyrillic test, the actor finders and the topic getters [documented: each page in the
//! wiki export; docs/spec-notes/ext-sup1.md]. SUP is not installed in the probe game, so
//! nothing here is observed.

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use rand_core::RngCore;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// RoundAlt: `x` to `places` decimals (0-4), rounded to the nearest when `round` is set,
/// else cut [documented: the page's examples, 37.55 at 1 cut is 37.5, 37.66 at 1 rounded
/// is 37.7].
pub fn round_to(x: f64, places: f64, round: bool) -> f64 {
    let m = 10f64.powi(places.clamp(0.0, 4.0) as i32);
    let v = x * m;
    // A product a hair below a whole number (37.6 × 10 = 375.99…) is that number.
    let v = if (v - v.round()).abs() < 1e-9 { v.round() } else { v };
    (if round { v.round() } else { v.trunc() }) / m
}

/// Degrees from a facing `angle_z` (degrees, clockwise from +Y) at (x, y) to (tx, ty),
/// in (-180, 180], as GetHeadingAngle (`functions::objects::get_heading_angle`)
/// [documented: GetHeadingAngleBetweenPoints, "equivalent to GetHeadingAngle"].
pub fn heading_between(x: f64, y: f64, angle_z: f64, tx: f64, ty: f64) -> f64 {
    let d = (tx - x).atan2(ty - y).to_degrees() - angle_z;
    // Script-supplied: a non-finite angle gives 0, and a huge one wraps in one step (a
    // loop of 360s would never end, #1363 review).
    if !d.is_finite() {
        return 0.0;
    }
    let d = (d + 180.0).rem_euclid(360.0) - 180.0;
    if d <= -180.0 {
        d + 360.0
    } else {
        d
    }
}

/// A whole number in English words, without "and" or hyphens [documented: NumberToWords,
/// 332323 is "three hundred thirty two thousand three hundred twenty three", 10020 is
/// "ten thousand twenty"; hypothesis: "zero", and "minus" before a negative one].
pub fn number_to_words(n: i64) -> String {
    const ONES: [&str; 20] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen",
        "eighteen", "nineteen",
    ];
    const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    const SCALES: [(u64, &str); 6] = [
        (1_000_000_000_000_000_000, "quintillion"),
        (1_000_000_000_000_000, "quadrillion"),
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1_000, "thousand"),
    ];
    fn below_1000(n: u64, out: &mut Vec<&'static str>) {
        if n >= 100 {
            out.push(ONES[(n / 100) as usize]);
            out.push("hundred");
        }
        let r = n % 100;
        if r >= 20 {
            out.push(TENS[(r / 10) as usize]);
            if !r.is_multiple_of(10) {
                out.push(ONES[(r % 10) as usize]);
            }
        } else if r > 0 {
            out.push(ONES[r as usize]);
        }
    }
    if n == 0 {
        return "zero".into();
    }
    let mut out: Vec<&'static str> = Vec::new();
    if n < 0 {
        out.push("minus");
    }
    let mut rest = n.unsigned_abs();
    for (size, name) in SCALES {
        if rest >= size {
            below_1000(rest / size, &mut out);
            out.push(name);
            rest %= size;
        }
    }
    below_1000(rest, &mut out);
    out.join(" ")
}

/// The actors the finders look through: those in the loaded cells, not the player
/// (and not `besides`), the dead only with `dead`, the player's teammates only with
/// `teammates`, within `distance` of `at` when it is above 0 [documented: FindClosestActor
/// and the others' notes: the loaded interior or the exterior cells around the player's,
/// excluding the player], with their distance.
fn candidates(g: &Game, at: [f64; 3], besides: Option<FormId>, dead: bool, teammates: bool, distance: f64) -> Vec<(FormId, f64)> {
    let mut out = Vec::new();
    let mut cells: Vec<FormId> = g.loaded_cells.iter().copied().collect();
    cells.sort();
    for c in cells {
        for r in g.refs_in_cell(c, false) {
            if r == FormId::PLAYER_REF || Some(r) == besides || !g.present(r) {
                continue;
            }
            let base = crate::functions::base_of(g, r);
            if !matches!(base.and_then(|b| g.data.sig(b)).as_ref(), Some(b"NPC_") | Some(b"CREA")) {
                continue;
            }
            if (!dead && g.is_dead(r)) || (!teammates && g.is_teammate(r)) {
                continue;
            }
            let Some(s) = g.world.peek(r) else { continue };
            let d = ((f64::from(s.pos[0]) - at[0]).powi(2) + (f64::from(s.pos[1]) - at[1]).powi(2) + (f64::from(s.pos[2]) - at[2]).powi(2)).sqrt();
            if distance > 0.0 && d > distance {
                continue;
            }
            out.push((r, d));
        }
    }
    out
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // `x`: to the nearest whole number [documented: Round, 37.4 is 37, 37.8 is 38;
        // hypothesis: a half away from zero].
        "round" => XVal::Num(num(args, 0, 0.0).round()),
        "roundalt" => XVal::Num(round_to(num(args, 0, 0.0), num(args, 1, 0.0), num(args, 2, 0.0) != 0.0)),
        // `[x]`: 1 for a NaN; with no number, a NaN [documented: IsNumberNAN's two forms].
        "isnumbernan" => match args.first() {
            Some(XVal::Num(x)) => yes(x.is_nan()),
            _ => XVal::Num(f64::NAN),
        },
        // `x1 y1 z1 x2 y2 z2` [documented: GetDistanceBetweenPoints].
        "getdistancebetweenpoints" => {
            let p: Vec<f64> = (0..6).map(|k| num(args, k, 0.0)).collect();
            XVal::Num(((p[3] - p[0]).powi(2) + (p[4] - p[1]).powi(2) + (p[5] - p[2]).powi(2)).sqrt())
        }
        // `x y angleZ tx ty` [documented: GetHeadingAngleBetweenPoints]; `x y angleZ
        // ref`, the reference's position [documented: GetHeadingAngleAlt].
        "getheadinganglebetweenpoints" => XVal::Num(heading_between(num(args, 0, 0.0), num(args, 1, 0.0), num(args, 2, 0.0), num(args, 3, 0.0), num(args, 4, 0.0))),
        "getheadinganglealt" => {
            let Some(to) = form(args, 3).and_then(|r| g.world.get(r).ok()).map(|s| s.pos) else { return Some(Ok(zero)) };
            XVal::Num(heading_between(num(args, 0, 0.0), num(args, 1, 0.0), num(args, 2, 0.0), f64::from(to[0]), f64::from(to[1])))
        }
        "numbertowords" => XVal::Str(number_to_words(num(args, 0, 0.0).trunc() as i64)),
        // `text`: 1 when it holds a Cyrillic letter (U+0400-U+04FF) [documented:
        // StringContainsCyrillic; hypothesis: the block].
        "stringcontainscyrillic" => yes(match args.first() {
            Some(XVal::Str(s)) => s.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)),
            _ => false,
        }),
        // Always -1 [documented: GetSUPVersion's note].
        "getsupversion" => XVal::Num(-1.0),
        // Carrying more than the capacity, as ShowOff's GetIsPlayerOverencumbered
        // [documented: IsPlayerOverencumbered points there].
        "isplayeroverencumbered" => {
            let w = g.world.carried_weight(FormId::PLAYER_REF).unwrap_or(0.0);
            let cap = crate::functions::actor::actor_value(g, FormId::PLAYER_REF, crate::actor_values::CARRY_WEIGHT).unwrap_or(0.0);
            yes(w > cap)
        }
        // `x y z [dead] [teammates] [distance]`, or on a reference `[dead] [teammates]
        // [distance]`: the closest of the candidates, or a random one; none (0) when
        // there is none [documented: FindClosestActor, FindRandomActor and the FromRef
        // forms; hypothesis: the calling reference is not its own find].
        "findclosestactor" | "findrandomactor" | "findclosestactorfromref" | "findrandomactorfromref" => {
            let from_ref = name.ends_with("fromref");
            let (at, k) = if from_ref {
                let Some(p) = subject.and_then(|r| g.world.get(r).ok()).map(|s| s.pos) else { return Some(Ok(XVal::Ref(0))) };
                ([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])], 0)
            } else {
                ([num(args, 0, 0.0), num(args, 1, 0.0), num(args, 2, 0.0)], 3)
            };
            let found = candidates(g, at, subject.filter(|_| from_ref), num(args, k, 0.0) != 0.0, num(args, k + 1, 0.0) != 0.0, num(args, k + 2, 0.0));
            let pick = if name.starts_with("findclosest") {
                found.iter().min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0))).map(|p| p.0)
            } else if found.is_empty() {
                None
            } else {
                Some(found[g.rng.next_u32() as usize % found.len()].0)
            };
            XVal::Ref(pick.map_or(0, |r| r.0))
        }
        // `topic`: the topic's type, 0 Topic … 7 Radio (DATA) [documented: TopicGetType].
        "topicgettype" => XVal::Num(form(args, 0).and_then(|t| g.data.topic_defs.get(&t)).map_or(0.0, |t| f64::from(t.topic_type))),
        // `topic [quest]`: the topic's lines, in topic order, only that quest's when one
        // is given [documented: TopicGetAllTopicInfos].
        "topicgetalltopicinfos" => {
            let quest = form(args, 1);
            let infos: Vec<XVal> = form(args, 0)
                .and_then(|t| g.data.topics.get(&t))
                .into_iter()
                .flatten()
                .filter(|i| quest.is_none() || g.data.infos.get(i).is_some_and(|d| d.quest == quest))
                .map(|i| XVal::Ref(i.0))
                .collect();
            list(g, infos)
        }
        // `info`: its choices (TCLT) [documented: TopicInfoGetAllChoices, …ChoiceCount];
        // `info topic`: 1 when the topic is one [documented: TopicInfoHasChoice].
        "topicinfogetallchoices" => {
            let choices: Vec<XVal> = form(args, 0).and_then(|i| g.dialogue_view().info(&i).map(|d| d.choices.iter().map(|c| XVal::Ref(c.0)).collect())).unwrap_or_default();
            list(g, choices)
        }
        "topicinfogetchoicecount" => XVal::Num(form(args, 0).and_then(|i| g.dialogue_view().info(&i).map(|d| d.choices.len() as f64)).unwrap_or(0.0)),
        "topicinfohaschoice" => {
            let t = form(args, 1);
            yes(t.is_some() && form(args, 0).is_some_and(|i| g.dialogue_view().info(&i).is_some_and(|d| d.choices.iter().any(|c| Some(*c) == t))))
        }
        _ => return None,
    };
    let _ = ctx;
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_examples() {
        // [documented: RoundAlt].
        assert_eq!(round_to(37.55, 0.0, false), 37.0);
        assert_eq!(round_to(37.55, 1.0, false), 37.5);
        assert_eq!(round_to(37.6, 1.0, false), 37.6);
        assert_eq!(round_to(37.66, 1.0, true), 37.7);
        assert_eq!(round_to(37.665454, 3.0, true), 37.665);
        assert_eq!(round_to(37.665654, 3.0, true), 37.666);
        // [documented: NumberToWords].
        assert_eq!(number_to_words(10), "ten");
        assert_eq!(number_to_words(332323), "three hundred thirty two thousand three hundred twenty three");
        assert_eq!(number_to_words(10020), "ten thousand twenty");
        assert_eq!(number_to_words(0), "zero");
        assert_eq!(number_to_words(-1_000_015), "minus one million fifteen");
        // Facing north (0) at the origin: east is 90, south 180, west -90.
        assert_eq!(heading_between(0.0, 0.0, 0.0, 10.0, 0.0), 90.0);
        assert_eq!(heading_between(0.0, 0.0, 0.0, 0.0, -10.0), 180.0);
        assert_eq!(heading_between(0.0, 0.0, 90.0, 0.0, 10.0), -90.0);
        // In (-180, 180]: due south from a facing of 0 is 180, not -180; a facing past a
        // turn wraps; an endless or huge angle does not hang (#1363 review).
        assert_eq!(heading_between(0.0, 0.0, 0.0, 0.0, -10.0), 180.0);
        assert_eq!(heading_between(0.0, 0.0, 720.0 + 45.0, 0.0, 10.0), -45.0);
        assert_eq!(heading_between(0.0, 0.0, f64::INFINITY, 0.0, 10.0), 0.0);
        assert_eq!(heading_between(0.0, 0.0, f64::NAN, 0.0, 10.0), 0.0);
        let h = heading_between(0.0, 0.0, 1e30, 0.0, 10.0);
        assert!(h > -180.0 && h <= 180.0, "{h}");
    }
}
