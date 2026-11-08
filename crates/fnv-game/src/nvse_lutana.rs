//! Lutana NVSE (LU, merged into JIP LN), batch 1 (#1009): challenges, casinos, recipes,
//! actor AI data, perks, cells and water, encounter zone flags, activators, terminals and
//! locks, form flags and playability, weapon fields, load screen types, and a few misc
//! getters [documented: each page in the wiki export; docs/spec-notes/ext-lutana.md].
//! Setters change the record for the session, read by the getters; where a game system
//! reads the same value through one accessor (aggro radius, services, casino max
//! winnings, encounter zone flags) it sees the change too.

use crate::casino::CasinoDef;
use crate::challenges::ChallengeDef;
use crate::crafting::RecipeDef;
use crate::game::{Game, RunCtx};
use crate::nvse::{field_number, record_value, F32, U32, U8};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::HashMap;

/// What this batch's setters changed, for the session.
#[derive(Debug, Default)]
pub struct LutanaState {
    /// Whole copies of the records a setter changed, read by the getters (and by
    /// [`challenge`], [`casino`], [`recipe`]).
    pub challenges: HashMap<FormId, ChallengeDef>,
    pub casinos: HashMap<FormId, CasinoDef>,
    pub recipes: HashMap<FormId, RecipeDef>,
    /// SetServiceFlag's flags and LNSetAggroRadius's radius, by the actor record changed
    /// (`Game::services`, `Game::aggro_radius`).
    pub services: HashMap<FormId, u32>,
    pub aggro_radius: HashMap<FormId, u32>,
    /// SetRadius on a reference (its XRDS).
    pub ref_radius: HashMap<FormId, f32>,
    /// SetDefaultMessageTime's seconds.
    pub message_time: Option<f64>,
}

/// A challenge as it stands now: a script's changes, else the record's (CHAL).
pub fn challenge<'a>(g: &'a Game, c: FormId) -> Option<&'a ChallengeDef> {
    g.other.lutana.challenges.get(&c).or_else(|| g.data.challenge_defs.get(&c))
}

/// A casino as it stands now: a script's changes, else the record's (CSNO).
pub fn casino<'a>(g: &'a Game, c: FormId) -> Option<&'a CasinoDef> {
    g.other.lutana.casinos.get(&c).or_else(|| g.data.casinos.get(&c))
}

/// A recipe as it stands now: a script's changes, else the record's (RCPE).
pub fn recipe<'a>(g: &'a Game, r: FormId) -> Option<&'a RecipeDef> {
    g.other.lutana.recipes.get(&r).or_else(|| g.data.recipes.get(&r))
}

/// The default display time of the corner messages: SetDefaultMessageTime's, else 2
/// seconds [documented: GetMessageDisplayTime, "defaults to 2.0"].
pub fn message_time(g: &Game) -> f64 {
    g.other.lutana.message_time.unwrap_or(2.0)
}

/// The record copied into `map` on the first change, then changed there.
fn copy_on_write<'a, T: Clone>(map: &'a mut HashMap<FormId, T>, from: Option<&T>, f: FormId) -> Option<&'a mut T> {
    match map.entry(f) {
        std::collections::hash_map::Entry::Occupied(e) => Some(e.into_mut()),
        std::collections::hash_map::Entry::Vacant(e) => Some(e.insert(from?.clone())),
    }
}

fn challenge_mut<'a>(g: &'a mut Game, c: FormId) -> Option<&'a mut ChallengeDef> {
    let from = g.data.challenge_defs.get(&c);
    copy_on_write(&mut g.other.lutana.challenges, from, c)
}

fn casino_mut<'a>(g: &'a mut Game, c: FormId) -> Option<&'a mut CasinoDef> {
    let from = g.data.casinos.get(&c);
    copy_on_write(&mut g.other.lutana.casinos, from, c)
}

fn recipe_mut<'a>(g: &'a mut Game, r: FormId) -> Option<&'a mut RecipeDef> {
    let from = g.data.recipes.get(&r);
    copy_on_write(&mut g.other.lutana.recipes, from, r)
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k) {
        Some(XVal::Num(x)) => Some(*x),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn reff(f: Option<FormId>) -> XVal {
    XVal::Ref(f.map_or(0, |f| f.0))
}

fn record_ref(g: &Game, f: FormId, sub: &str) -> Option<FormId> {
    match record_value(g, f, sub, "")? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(r)),
        _ => None,
    }
}

fn record_num(g: &Game, f: FormId, sub: &str, path: &str) -> Option<f64> {
    match record_value(g, f, sub, path)? {
        XVal::Num(x) => Some(x),
        _ => None,
    }
}

fn list(g: &mut Game, items: impl Iterator<Item = XVal>) -> XVal {
    let entries = items.enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

fn string_map(g: &mut Game, items: Vec<(&str, f64)>) -> XVal {
    let entries = items.into_iter().map(|(k, v)| (XKey::Str(k.to_string()), XVal::Num(v))).collect();
    XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
}

fn base(g: &Game, f: FormId) -> FormId {
    crate::functions::base_of(g, f).unwrap_or(f)
}

/// The actor record whose AI data `f` (an actor reference or base) uses, following its
/// template while "Use AI Data" is set [documented: xEdit template flags; as
/// `Game::services` and `Game::aggro_radius` read it].
fn ai_source(g: &Game, f: FormId) -> Option<FormId> {
    if g.world.placed.get(f).is_some() || g.world.peek(f).is_some() {
        let b = crate::functions::base_of(g, f)?;
        return g.template_source(b, f, |d| d.template_ai).map(|(id, _)| id);
    }
    g.data.actors.contains_key(&f).then(|| g.data.base_template_source(f, |d| d.template_ai))
}

/// The cell `args[k]` names, else the player's [documented: GetCellWaterForm, "If no
/// cell is specified, the player's current cell is used"].
fn cell_arg(g: &Game, args: &[XVal], k: usize) -> Option<FormId> {
    form(args, k).or_else(|| g.cell_of(FormId::PLAYER_REF))
}

/// A cell's water form: its XCWT, else for an exterior cell its worldspace's NAM2
/// [documented: GetCellWaterForm, "the parent worldspace's water form for exterior
/// cells"].
fn cell_water(g: &Game, cell: FormId) -> Option<FormId> {
    record_ref(g, cell, "XCWT").or_else(|| g.world.placed.cell_world.get(&cell).and_then(|&w| record_ref(g, w, "NAM2")))
}

/// The reference the lock functions act on: the argument, else the calling reference.
fn lockable(args: &[XVal], k: usize, subject: Option<FormId>) -> Option<FormId> {
    form(args, k).or(subject)
}

fn is_terminal(g: &Game, r: FormId) -> bool {
    g.data.sig(base(g, r)) == Some(*b"TERM")
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- challenges (CHAL) [documented: each Get/SetChallenge… page] ----
        "getchallengeform1" | "getchallengeform2" | "getchallengeinterval" | "getchallengethreshold" | "getchallengetype" | "getchallengevalue1"
        | "getchallengevalue2" | "getchallengevalue3" | "getchallengeflags" => {
            let Some(d) = form(args, 0).and_then(|c| challenge(g, c)) else {
                return Some(Ok(if name.starts_with("getchallengeform") { XVal::Ref(0) } else { zero }));
            };
            match name {
                "getchallengeform1" => reff(d.form1),
                "getchallengeform2" => reff(d.form2),
                "getchallengeinterval" => XVal::Num(f64::from(d.interval)),
                "getchallengethreshold" => XVal::Num(f64::from(d.threshold)),
                // The DATA type (0 KillInFormList … 13 Scripted) [documented:
                // GetChallengeType's table].
                "getchallengetype" => XVal::Num(d.kind as f64),
                "getchallengevalue1" => XVal::Num(f64::from(d.value1)),
                "getchallengevalue2" => XVal::Num(f64::from(d.value2)),
                "getchallengevalue3" => XVal::Num(f64::from(d.value3)),
                _ => XVal::Num(f64::from(d.flags)),
            }
        }
        "setchallengeform1" | "setchallengeform2" => {
            let f = form(args, 1);
            if let Some(d) = form(args, 0).and_then(|c| challenge_mut(g, c)) {
                if name == "setchallengeform1" {
                    d.form1 = f;
                } else {
                    d.form2 = f;
                }
            }
            zero
        }
        // The numbers are u32 fields: kept in range, NaN refused (`nvse::field_number`).
        "setchallengeinterval" | "setchallengethreshold" | "setchallengevalue1" | "setchallengevalue2" | "setchallengevalue3" | "setchallengeflags" => {
            let v = num(args, 1).and_then(|v| field_number(v.trunc(), U32.0, U32.1));
            if let (Some(v), Some(d)) = (v, form(args, 0).and_then(|c| challenge_mut(g, c))) {
                let v = v as u32;
                match name {
                    "setchallengeinterval" => d.interval = v,
                    "setchallengethreshold" => d.threshold = v,
                    "setchallengevalue1" => d.value1 = v,
                    "setchallengevalue2" => d.value2 = v,
                    "setchallengevalue3" => d.value3 = v,
                    _ => d.flags = v,
                }
            }
            zero
        }
        // ---- casinos (CSNO) [documented: each Get/SetCasino… page] ----
        "getcasinobj17stand" | "getcasinobjpayout" | "getcasinomaxwinnings" | "getcasinonumdecks" | "getcasinoshufflepercent" | "getcasinoreelstops" => {
            let Some(c) = form(args, 0).and_then(|c| casino(g, c)) else { return Some(Ok(zero)) };
            XVal::Num(match name {
                "getcasinobj17stand" => f64::from(u8::from(c.stay_soft17)),
                "getcasinobjpayout" => crate::nvse::record_float(f64::from(c.payout)),
                "getcasinomaxwinnings" => f64::from(c.max_winnings),
                "getcasinonumdecks" => f64::from(c.decks),
                // A share, 1.0 all of the shoe [documented: "1.0 is 100%"].
                "getcasinoshufflepercent" => crate::nvse::record_float(f64::from(c.shuffle_share)),
                // `casino symbol`: symbols 1-7, 7 the W [documented]; another symbol 0
                // [hypothesis].
                _ => match num(args, 1).map(|s| s as i64) {
                    Some(s @ 1..=7) => f64::from(c.reel_stops[(s - 1) as usize]),
                    _ => 0.0,
                },
            })
        }
        "setcasinobj17stand" | "setcasinobjpayout" | "setcasinomaxwinnings" | "setcasinonumdecks" | "setcasinoshufflepercent" => {
            let v = num(args, 1).filter(|v| v.is_finite());
            if let (Some(v), Some(c)) = (v, form(args, 0).and_then(|c| casino_mut(g, c))) {
                match name {
                    "setcasinobj17stand" => c.stay_soft17 = v != 0.0,
                    "setcasinobjpayout" => c.payout = v.clamp(F32.0, F32.1) as f32,
                    "setcasinomaxwinnings" => c.max_winnings = v.trunc().clamp(U32.0, U32.1) as u32,
                    // A shoe of no decks cannot be dealt from: at least 1 [hypothesis].
                    "setcasinonumdecks" => c.decks = v.trunc().clamp(1.0, U32.1) as u32,
                    // 0-1 [hypothesis: a share outside it means nothing].
                    _ => c.shuffle_share = v.clamp(0.0, 1.0) as f32,
                }
            }
            zero
        }
        // `casino symbol stops`: the count as given; the page's "must equal 14" is the
        // record's rule, not checked here [hypothesis].
        "setcasinoreelstops" => {
            let (s, v) = (num(args, 1).map(|s| s as i64), num(args, 2).and_then(|v| field_number(v.trunc(), U32.0, U32.1)));
            if let (Some(s @ 1..=7), Some(v)) = (s, v) {
                if let Some(c) = form(args, 0).and_then(|c| casino_mut(g, c)) {
                    c.reel_stops[(s - 1) as usize] = v as u32;
                }
            }
            zero
        }
        // ---- recipes (RCPE) [documented: each page] ----
        "getrecipeinputcount" | "getrecipeoutputcount" => {
            let (Some(r), Some(item)) = (form(args, 0).and_then(|r| recipe(g, r)), form(args, 1)) else { return Some(Ok(zero)) };
            let entries = if name == "getrecipeinputcount" { &r.ingredients } else { &r.outputs };
            XVal::Num(entries.iter().filter(|e| e.0 == item).map(|e| f64::from(e.1)).sum())
        }
        // The actor value code; a recipe with no skill -1 [hypothesis].
        "getreciperequiredskill" => XVal::Num(form(args, 0).and_then(|r| recipe(g, r)).and_then(|r| r.skill).map_or(-1.0, |s| f64::from(s.0))),
        "getreciperequiredskilllevel" => XVal::Num(form(args, 0).and_then(|r| recipe(g, r)).and_then(|r| r.skill).map_or(0.0, |s| f64::from(s.1))),
        // `recipe code`: a code below 0 takes the skill away, and a recipe without one
        // gains it at level 0 [hypothesis].
        "setreciperequiredskill" => {
            let code = num(args, 1).filter(|c| c.is_finite()).map(f64::trunc);
            if let (Some(code), Some(r)) = (code, form(args, 0).and_then(|r| recipe_mut(g, r))) {
                r.skill = (code >= 0.0).then(|| (code.min(f64::from(u16::MAX)) as u16, r.skill.map_or(0, |s| s.1)));
            }
            zero
        }
        // `recipe level`: only a recipe with a skill has a level [hypothesis].
        "setreciperequiredskilllevel" => {
            let level = num(args, 1).and_then(|v| field_number(v.trunc(), U32.0, U32.1));
            if let (Some(level), Some(r)) = (level, form(args, 0).and_then(|r| recipe_mut(g, r))) {
                if let Some(s) = r.skill.as_mut() {
                    s.1 = level as u32;
                }
            }
            zero
        }
        // `recipe item count`: a new entry at the end; an item already listed has the
        // count added to its entry; a count below 1 adds nothing [hypothesis].
        "addrecipeinputform" | "addrecipeoutputform" => {
            let (item, count) = (form(args, 1), num(args, 2).and_then(|v| field_number(v.trunc(), 0.0, U32.1)));
            if let (Some(item), Some(count @ 1.0..)) = (item, count) {
                if let Some(r) = form(args, 0).and_then(|r| recipe_mut(g, r)) {
                    let entries = if name == "addrecipeinputform" { &mut r.ingredients } else { &mut r.outputs };
                    match entries.iter_mut().find(|e| e.0 == item) {
                        Some(e) => e.1 = e.1.saturating_add(count as u32),
                        None => entries.push((item, count as u32)),
                    }
                }
            }
            zero
        }
        // `recipe item`: every entry of the item removed [hypothesis: a list names an
        // item once].
        "removerecipeinputform" | "removerecipeoutputform" => {
            if let (Some(item), Some(r)) = (form(args, 1), form(args, 0).and_then(|r| recipe_mut(g, r))) {
                let entries = if name == "removerecipeinputform" { &mut r.ingredients } else { &mut r.outputs };
                entries.retain(|e| e.0 != item);
            }
            zero
        }
        // `recipe old new`: the entry keeps its count [documented: the examples].
        "replacerecipeinputform" | "replacerecipeoutputform" => {
            if let (Some(old), Some(new), Some(r)) = (form(args, 1), form(args, 2), form(args, 0).and_then(|r| recipe_mut(g, r))) {
                let entries = if name == "replacerecipeinputform" { &mut r.ingredients } else { &mut r.outputs };
                for e in entries.iter_mut().filter(|e| e.0 == old) {
                    e.0 = new;
                }
            }
            zero
        }
        // `recipe item count`: a listed item's count; an item not listed is not added
        // [hypothesis]; a count below 1 is refused [hypothesis].
        "setrecipeinputcount" | "setrecipeoutputcount" => {
            let (item, count) = (form(args, 1), num(args, 2).and_then(|v| field_number(v.trunc(), 0.0, U32.1)));
            if let (Some(item), Some(count @ 1.0..)) = (item, count) {
                if let Some(r) = form(args, 0).and_then(|r| recipe_mut(g, r)) {
                    let entries = if name == "setrecipeinputcount" { &mut r.ingredients } else { &mut r.outputs };
                    for e in entries.iter_mut().filter(|e| e.0 == item) {
                        e.1 = count as u32;
                    }
                }
            }
            zero
        }
        // `item [category]`: the recipes that take the item, in FormID order, those of
        // the category (or subcategory) only when one is given [documented:
        // GetFormRecipes; hypothesis: the order]; GetFormRecipeOutputs their outputs,
        // each once, in that order [documented: GetFormRecipeOutputs].
        "getformrecipes" | "getformrecipeoutputs" => {
            let (Some(item), cat) = (form(args, 0), form(args, 1)) else { return Some(Ok(list(g, std::iter::empty()))) };
            let mut ids: Vec<FormId> = g.data.recipes.keys().copied().collect();
            ids.sort();
            let using: Vec<&RecipeDef> = ids.iter().filter_map(|&id| recipe(g, id)).filter(|r| r.ingredients.iter().any(|e| e.0 == item) && cat.is_none_or(|c| r.listed_under(c))).collect();
            let out: Vec<FormId> = if name == "getformrecipes" {
                using.iter().map(|r| r.form).collect()
            } else {
                let mut seen = std::collections::HashSet::new();
                using.iter().flat_map(|r| r.outputs.iter().map(|e| e.0)).filter(|o| seen.insert(*o)).collect()
            };
            list(g, out.into_iter().map(|f| XVal::Ref(f.0)))
        }
        // ---- actors ----
        // `actor.ActorHasEffect effect`: an active effect of that base effect on the actor
        // [documented: ActorHasEffect], as HasMagicEffect reads it.
        "actorhaseffect" => {
            let (Some(r), Some(m)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            g.sync_actor_effects(r);
            yes(g.effects.iter().any(|e| e.target == r && e.effect == m))
        }
        // `npc flag`: the AI data's Buys/Sells and Services flags (1 Weapons … 131072
        // Repair) [documented: GetServiceFlag's table; AIDT services, xEdit]; the NPC
        // form's own AI data, following its template [hypothesis: as the vendor reads it].
        "getserviceflag" => {
            let Some(src) = form(args, 0).and_then(|f| ai_source(g, f)) else { return Some(Ok(zero)) };
            let flags = g.other.lutana.services.get(&src).copied().unwrap_or_else(|| g.data.actors.get(&src).map_or(0, |d| d.services));
            let bit = num(args, 1).map_or(0, |b| b as i64 as u32);
            yes(bit != 0 && flags & bit == bit)
        }
        // `npc flag on`: changes the AI data's flags, which the vendor's services read
        // unless "Autocalc Service" takes the class's (`Game::services`).
        "setserviceflag" => {
            let (Some(src), Some(bit)) = (form(args, 0).and_then(|f| ai_source(g, f)), num(args, 1).map(|b| b as i64 as u32)) else { return Some(Ok(zero)) };
            let now = g.other.lutana.services.get(&src).copied().unwrap_or_else(|| g.data.actors.get(&src).map_or(0, |d| d.services));
            let new = if num(args, 2).unwrap_or(0.0) != 0.0 { now | bit } else { now & !bit };
            g.other.lutana.services.insert(src, new);
            zero
        }
        // `[actorBase]`: the AI data's aggro radius [documented: LNGetAggroRadius], the
        // calling reference's when no base is given.
        "lngetaggroradius" => {
            let Some(src) = form(args, 0).or(subject).and_then(|f| ai_source(g, f)) else { return Some(Ok(zero)) };
            XVal::Num(f64::from(g.other.lutana.aggro_radius.get(&src).copied().unwrap_or_else(|| g.data.actors.get(&src).map_or(0, |d| d.aggro_radius))))
        }
        // `radius [actorBase]`: read by the aggro checks too (`Game::aggro_radius`),
        // which still need the record's "Aggro Radius Behavior" on.
        "lnsetaggroradius" => {
            let r = num(args, 0).and_then(|v| field_number(v.trunc(), U32.0, U32.1));
            if let (Some(r), Some(src)) = (r, form(args, 1).or(subject).and_then(|f| ai_source(g, f))) {
                g.other.lutana.aggro_radius.insert(src, r as u32);
            }
            zero
        }
        // `[actorBase]`: the combat style the actor fights with (`Game::combat_style_id`:
        // the package's, SetCombatStyle's, the AI data's ZNAM) [documented:
        // GetCombatStyle, "the current Combat Style"]; for a base form its AI data's ZNAM;
        // none 0 [hypothesis: no fall back to DefaultCombatstyle here].
        "getcombatstyle" => {
            if let Some(b) = form(args, 0) {
                let src = ai_source(g, b);
                reff(src.and_then(|s| g.data.actors.get(&s)).and_then(|d| d.combat_style))
            } else {
                reff(subject.and_then(|r| g.combat_style_id(r)))
            }
        }
        // ---- perks ----
        // [documented: IsTrait] PERK DATA Trait, SetPerkFlag's change first (`nvse_gaps`).
        "istrait" => {
            let Some(p) = form(args, 0) else { return Some(Ok(zero)) };
            let rec = g.data.perks.get(&p).is_some_and(|d| d.is_trait);
            yes(g.gaps.perk_flags.get(&(p, 0)).copied().unwrap_or(rec))
        }
        // `perk index`: an ability entry's spell, a quest stage entry's quest [documented:
        // "Ability for type 5, Quest for type 6"]; an entry point 0 (its leveled list is
        // not kept decoded) [hypothesis], a negative index or past the entries 0.
        "getnthperkentryform" => {
            let e = form(args, 0).and_then(|p| g.data.perks.get(&p)).and_then(|p| num(args, 1).filter(|i| *i >= 0.0).and_then(|i| p.entries.get(i as usize)));
            reff(match e.map(|e| &e.effect) {
                Some(crate::data::PerkEffect::Ability(s)) => Some(*s),
                Some(crate::data::PerkEffect::QuestStage { quest, .. }) => Some(*quest),
                _ => None,
            })
        }
        // ---- worldspaces, cells, water ----
        // [documented: GetWorldspaceParentWorldspace] WRLD WNAM.
        "getworldspaceparentworldspace" | "getworldparentworld" => reff(form(args, 0).and_then(|w| record_ref(g, w, "WNAM"))),
        "getcellwaterform" => reff(cell_arg(g, args, 0).and_then(|c| cell_water(g, c))),
        // `[cell]`: CELL XCLW, SetCellWaterHeight's first; an exterior cell without one its
        // worldspace's default water height (DNAM) [hypothesis]; else 0.
        "getcellwaterheight" => {
            let Some(c) = cell_arg(g, args, 0) else { return Some(Ok(zero)) };
            let own = record_num(g, c, "XCLW", "");
            let world = || g.world.placed.cell_world.get(&c).and_then(|&w| record_num(g, w, "DNAM", "default_water_height"));
            XVal::Num(own.or_else(world).unwrap_or(0.0))
        }
        // `height [cell]`: for the session, read by GetCellWaterHeight; the water drawn
        // keeps the record's height (ext-lutana.md, follow-ups).
        "setcellwaterheight" => {
            let h = num(args, 0).and_then(|h| field_number(h, F32.0, F32.1));
            if let (Some(h), Some(c)) = (h, cell_arg(g, args, 1)) {
                g.field_overrides.insert((c, "XCLW/".into()), XVal::Num(h));
            }
            zero
        }
        // `[cell]`: a string map of "x" and "y", the exterior cell's grid (XCLC)
        // [documented: GetCellCoords]; an interior cell gives an empty map [hypothesis].
        "getcellcoords" => {
            let c = cell_arg(g, args, 0);
            let xy = c.and_then(|c| Some((record_num(g, c, "XCLC", "x")?, record_num(g, c, "XCLC", "y")?)));
            string_map(g, xy.map_or_else(Vec::new, |(x, y)| vec![("x", x), ("y", y)]))
        }
        // `water`: the water type's actor effect (WATR XNAM) [documented:
        // GetWaterFormEffect]; with no water, the player's cell's [hypothesis].
        "getwaterformeffect" => {
            let w = form(args, 0).or_else(|| g.cell_of(FormId::PLAYER_REF).and_then(|c| cell_water(g, c)));
            // SetWaterFormEffect's change first (`crate::nvse_jip8`).
            reff(w.and_then(|w| g.water_effect(w)))
        }
        // `ref.GetZone`: the encounter zone of the reference's parent cell
        // [documented: GetZone], SetCellEncounterZone's and SetWorldspaceEncounterZone's
        // changes first (`nvse_jg5::cell_zone`).
        "getzone" => {
            let c = subject.and_then(|r| g.cell_of(r));
            reff(c.and_then(|c| crate::nvse_jg5::cell_zone(g, c)))
        }
        // `zone flag`: 1 Never Resets, 2 Match PC Below Minimum Level [documented: the
        // pages' table]; 1 when every bit of `flag` is set.
        "getzoneflag" => {
            let (Some(d), Some(bit)) = (form(args, 0).and_then(|z| g.zone(z)), num(args, 1).map(|b| b as i64 as u8)) else { return Some(Ok(zero)) };
            let flags = u8::from(d.never_resets) | u8::from(d.match_pc_below_min) << 1;
            yes(bit != 0 && flags & bit == bit)
        }
        // `zone flag on`: read wherever the zone's flags are read through `Game::zone`.
        "setzoneflag" => {
            if let (Some(z), Some(bit)) = (form(args, 0).filter(|z| g.world.forms.zones.contains_key(z)), num(args, 1).map(|b| b as i64 as u8)) {
                let on = num(args, 2).unwrap_or(0.0) != 0.0;
                let c = g.other.zone_changes.entry(z).or_default();
                if bit & 1 != 0 {
                    c.never_resets = Some(on);
                }
                if bit & 2 != 0 {
                    c.match_pc_below_min = Some(on);
                }
            }
            zero
        }
        // `list`: LVLG, the global giving the chance none [documented:
        // GetChanceNoneGlobal], SetChanceNoneGlobal's first (`nvse_leveled`).
        "getchancenoneglobal" => reff(form(args, 0).and_then(|l| g.lev_list(l)).and_then(|l| l.global)),
        // ---- activators (ACTI) [documented: each page; the fields: xEdit ACTI] ----
        "getactivatorradiostation" => reff(form(args, 0).or(subject).map(|f| base(g, f)).and_then(|a| record_ref(g, a, "RNAM"))),
        // `form [activator]`: RNAM, VNAM, SNAM for the session, read by their getters
        // (GetActivatorSoundActivate/Looping, `nvse_plugins`).
        "setactivatorradiostation" | "setactivatorsoundactivate" | "setactivatorsoundlooping" => {
            let sub = match name {
                "setactivatorradiostation" => "RNAM/",
                "setactivatorsoundactivate" => "VNAM/",
                _ => "SNAM/",
            };
            let a = form(args, 1).or(subject).map(|f| base(g, f)).filter(|&a| g.data.sig(a) == Some(*b"ACTI"));
            if let Some(a) = a {
                g.field_overrides.insert((a, sub.into()), XVal::Ref(form(args, 0).map_or(0, |f| f.0)));
            }
            zero
        }
        // ---- terminals and locks ----
        // `[terminal]`: the base's DNAM difficulty, the number GetLockLevel gives a
        // terminal (0 Very Easy … 4 Very Hard, 5 Requires Key) [documented:
        // GetTerminalLock, "regardless of locked state"; `terminal::lock_level`].
        "getterminallock" => {
            let t = form(args, 0).or(subject).map(|f| base(g, f));
            XVal::Num(t.and_then(|t| g.data.terminals.get(&t)).map_or(0.0, |d| f64::from(d.difficulty)))
        }
        // `[terminal]`: TERM PNAM [documented: GetPasswordNote], SetPasswordNote's first.
        "getpasswordnote" => reff(form(args, 0).or(subject).map(|f| base(g, f)).and_then(|t| record_ref(g, t, "PNAM"))),
        "setpasswordnote" => {
            let t = form(args, 1).or(subject).map(|f| base(g, f)).filter(|&t| g.data.sig(t) == Some(*b"TERM"));
            if let Some(t) = t {
                g.field_overrides.insert((t, "PNAM/".into()), XVal::Ref(form(args, 0).map_or(0, |f| f.0)));
            }
            zero
        }
        // `[ref]`: 0 not broken, 1 broken (a forced lock, a terminal locked out), 2
        // broken again on Infiltrator's or Computer Whiz's try, 3 an inaccessible door
        // [documented: GetLockedOut's table; `lockpick::Locks`, `terminal::Terminals`].
        "getlockedout" => {
            let Some(r) = lockable(args, 0, subject) else { return Some(Ok(zero)) };
            let (broken, again) = if is_terminal(g, r) {
                (g.terminals.access.get(&r) == Some(&crate::terminal::Access::LockedOut), g.terminals.retried.contains(&r))
            } else {
                (g.locks.broken.contains(&r), g.locks.retried.contains(&r))
            };
            let door = g.data.sig(base(g, r)) == Some(*b"DOOR");
            XVal::Num(if again {
                2.0
            } else if broken {
                1.0
            } else if door && crate::nvse_other::record_flags_of(g, r) & fnv_sim::refs::INACCESSIBLE != 0 {
                3.0
            } else {
                0.0
            })
        }
        // `state [ref]`: 0 mends the lock, 1 breaks it, 2 breaks it with the perk's try
        // spent [documented: SetLockedOut's table]; another state changes nothing
        // [hypothesis].
        "setlockedout" => {
            let (Some(state @ 0..=2), Some(r)) = (num(args, 0).map(|s| s as i64), lockable(args, 1, subject)) else { return Some(Ok(zero)) };
            if is_terminal(g, r) {
                let t = &mut g.terminals;
                match state {
                    0 => {
                        if t.access.get(&r) == Some(&crate::terminal::Access::LockedOut) {
                            t.access.remove(&r);
                        }
                        t.retried.remove(&r);
                    }
                    _ => {
                        t.access.insert(r, crate::terminal::Access::LockedOut);
                        if state == 2 {
                            t.retried.insert(r);
                        } else {
                            t.retried.remove(&r);
                        }
                    }
                }
            } else {
                let l = &mut g.locks;
                if state == 0 {
                    l.broken.remove(&r);
                } else {
                    l.broken.insert(r);
                }
                if state == 2 {
                    l.retried.insert(r);
                } else {
                    l.retried.remove(&r);
                }
            }
            zero
        }
        // `ref.PlayerHasKey`: a door's or container's key carried, a terminal's password
        // note among the player's notes or carried [documented: PlayerHasKey]; a load
        // door with no lock of its own reads the other side's [hypothesis: as GetLockLevel].
        "playerhaskey" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let key = if is_terminal(g, r) {
                let b = base(g, r);
                let note = record_ref(g, b, "PNAM");
                if note.is_some_and(|n| g.player.notes.contains(&n)) {
                    return Some(Ok(XVal::Num(1.0)));
                }
                note
            } else {
                let own = g.world.get(r).ok().and_then(|s| s.lock).and_then(|l| l.key);
                let door = g.world.placed.get(r).and_then(|p| p.teleport.as_ref()).map(|t| t.door);
                own.or_else(|| door.and_then(|d| g.world.get(d).ok()).and_then(|s| s.lock).and_then(|l| l.key))
            };
            yes(key.is_some_and(|k| g.world.item_count(FormId::PLAYER_REF, k).is_ok_and(|n| n > 0)))
        }
        // `[ref]`: XAPD "Parent Activate Only" [documented: IsParentActivateOnly; the
        // flag: `PlacedRef::parent_activate_only`].
        "isparentactivateonly" => yes(form(args, 0).or(subject).and_then(|r| g.world.placed.get(r)).is_some_and(|p| p.parent_activate_only)),
        // ---- form flags and playability ----
        // `form flag`: 1 when every bit of `flag` is among the form's record flags
        // [documented: GetFormFlag's table], as GetFlagsLow/High read them.
        "getformflag" => {
            let (Some(f), Some(bit)) = (form(args, 0), num(args, 1).map(|b| b as i64 as u32)) else { return Some(Ok(zero)) };
            yes(bit != 0 && crate::nvse_other::record_flags_of(g, f) & bit == bit)
        }
        // `form flag on`: as SetFlagsLow/High store them, read by the same getters.
        "setformflag" => {
            if let (Some(f), Some(bit)) = (form(args, 0), num(args, 1).map(|b| b as i64 as u32)) {
                let now = crate::nvse_other::record_flags_of(g, f);
                let new = if num(args, 2).unwrap_or(0.0) != 0.0 { now | bit } else { now & !bit };
                g.other.flags.insert(f, new);
            }
            zero
        }
        // `[form]`: weapons, armour and ammo as IsPlayable; races (DATA flags bit 0),
        // hair and eyes (DATA bit 0) and perks (DATA Playable) [documented:
        // LNIsPlayable's form types; the fields: xEdit]; the calling reference's base
        // when no form is given.
        "lnisplayable" => {
            let Some(f) = form(args, 0).or(subject).map(|f| base(g, f)) else { return Some(Ok(zero)) };
            yes(playable(g, f).unwrap_or(false))
        }
        "lnsetisplayable" => {
            let on = num(args, 0).unwrap_or(0.0) != 0.0;
            if let Some(f) = form(args, 1).or(subject).map(|f| base(g, f)) {
                set_playable(g, f, on);
            }
            zero
        }
        // ---- weapons (WEAP) [documented: each page; the fields: xEdit WEAP] ----
        // `weapon value`: CRDT flags, DNAM kill impulse and impulse distance, VATS effect,
        // for the session, read through the record getters (`nvse::field`).
        "setweaponcritflags" | "setweaponkillimpulse" | "setweaponimpulsedistance" => {
            let w = form(args, 0).filter(|&w| g.data.sig(w) == Some(*b"WEAP"));
            let (key, range) = match name {
                "setweaponcritflags" => ("CRDT/flags", U8),
                "setweaponkillimpulse" => ("DNAM/kill_impulse", F32),
                _ => ("DNAM/impulse_dist", F32),
            };
            let v = num(args, 1).map(|v| if range == U8 { v.trunc() } else { v }).and_then(|v| field_number(v, range.0, range.1));
            if let (Some(w), Some(v)) = (w, v) {
                g.field_overrides.insert((w, key.into()), XVal::Num(v));
            }
            zero
        }
        "setweaponvatseffect" => {
            if let Some(w) = form(args, 0).filter(|&w| g.data.sig(w) == Some(*b"WEAP")) {
                g.field_overrides.insert((w, "VATS/effect".into()), XVal::Ref(form(args, 1).map_or(0, |f| f.0)));
            }
            zero
        }
        // ---- load screens (LSCR) ----
        // `screen`: WMI1, the load screen type [documented: GetLoadScreenType;
        // SetLoadScreenType, for the session].
        "getloadscreentype" => reff(form(args, 0).and_then(|s| record_ref(g, s, "WMI1"))),
        "setloadscreentype" => {
            if let (Some(s), Some(t)) = (form(args, 0).filter(|&s| g.data.sig(s) == Some(*b"LSCR")), form(args, 1).filter(|&t| g.data.sig(t) == Some(*b"LSCT"))) {
                g.field_overrides.insert((s, "WMI1/".into()), XVal::Ref(t.0));
            }
            zero
        }
        // ---- misc ----
        // [documented: GetPCCanUsePowerArmor] SetPCCanUsePowerArmor's state.
        "getpccanusepowerarmor" => yes(g.player.power_armor),
        // [documented: GetMessageDisplayTime, alias GetMDT; SetDefaultMessageTime, alias
        // SetDMT]: seconds; a time that is not a positive number is refused [hypothesis].
        "getmessagedisplaytime" | "getmdt" => XVal::Num(message_time(g)),
        "setdefaultmessagetime" | "setdmt" => {
            if let Some(t) = num(args, 0).filter(|t| t.is_finite() && *t > 0.0) {
                g.other.lutana.message_time = Some(t);
            }
            zero
        }
        // `note`: AddNote without the corner message [documented: AddNoteNS].
        "addnotens" => {
            if let Some(n) = form(args, 0) {
                if g.player.notes.insert(n) {
                    g.jip_event("noteadded", None, vec![XVal::Ref(n.0)]);
                }
            }
            zero
        }
        // The computer's local time as "HH-mm-SS" [documented: GetTimeStamp].
        "gettimestamp" => XVal::Str(chrono::Local::now().format("%H-%M-%S").to_string()),
        // `list`: 1 when the reference under the crosshair is in the list, 2 when its base
        // is, else 0 [documented: CrosshairRefInList].
        "crosshairrefinlist" => {
            let (Some(r), Some(l)) = (g.senses.crosshair_ref, form(args, 0)) else { return Some(Ok(zero)) };
            let members = crate::functions::refs::list_members(g, l);
            XVal::Num(if members.contains(&r) {
                1.0
            } else if members.contains(&base(g, r)) {
                2.0
            } else {
                0.0
            })
        }
        // `[light]`: a light base's radius (LIGH DATA); else the calling reference's XRDS
        // (SetRadius's first), else its light base's radius [hypothesis], else 0
        // [documented: GetRadius, "a reference (any type) or Light BaseForm"].
        "getradius" => {
            if let Some(l) = form(args, 0) {
                XVal::Num(record_num(g, l, "DATA", "radius").unwrap_or(0.0))
            } else if let Some(r) = subject {
                let own = g.other.lutana.ref_radius.get(&r).copied().or_else(|| g.world.placed.get(r).and_then(|p| p.radius));
                let b = base(g, r);
                let light = || (g.data.sig(b) == Some(*b"LIGH")).then(|| record_num(g, b, "DATA", "radius")).flatten();
                XVal::Num(own.map(|x| crate::nvse::record_float(f64::from(x))).or_else(light).unwrap_or(0.0))
            } else {
                zero
            }
        }
        // `radius [light]`: the light base's radius (an integer field, so whole and not
        // below 0), else the calling reference's own [documented: SetRadius].
        "setradius" => {
            let Some(v) = num(args, 0).filter(|v| v.is_finite()) else { return Some(Ok(zero)) };
            if let Some(l) = form(args, 1).filter(|&l| g.data.sig(l) == Some(*b"LIGH")) {
                g.field_overrides.insert((l, "DATA/radius".into()), XVal::Num(v.trunc().clamp(U32.0, U32.1)));
            } else if let Some(r) = subject.filter(|_| form(args, 1).is_none()) {
                g.other.lutana.ref_radius.insert(r, v.clamp(0.0, F32.1) as f32);
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// Whether a form is playable, for the form types LNIsPlayable names; `None` for another
/// type.
fn playable(g: &Game, f: FormId) -> Option<bool> {
    Some(match &g.data.sig(f)? {
        b"WEAP" | b"ARMO" | b"AMMO" => g.playable_overrides.get(&f).copied().unwrap_or_else(|| g.world.forms.item(f).is_some_and(|i| !i.non_playable)),
        b"RACE" => record_num(g, f, "DATA", "flags").is_some_and(|x| x as u64 & 1 != 0),
        b"HAIR" | b"EYES" => record_num(g, f, "DATA", "").is_some_and(|x| x as u64 & 1 != 0),
        b"PERK" => g.gaps.perk_flags.get(&(f, 1)).copied().unwrap_or_else(|| g.data.perks.get(&f).is_some_and(|p| p.playable)),
        _ => return None,
    })
}

/// LNSetIsPlayable: each type where its getter reads it.
fn set_playable(g: &mut Game, f: FormId, on: bool) {
    let bit = |x: Option<f64>| {
        let now = x.map_or(0, |x| x as u64);
        (if on { now | 1 } else { now & !1 }) as f64
    };
    match g.data.sig(f).as_ref() {
        Some(b"WEAP" | b"ARMO" | b"AMMO") => {
            g.playable_overrides.insert(f, on);
        }
        Some(b"RACE") => {
            let v = bit(record_num(g, f, "DATA", "flags"));
            g.field_overrides.insert((f, "DATA/flags".into()), XVal::Num(v));
        }
        Some(b"HAIR" | b"EYES") => {
            let v = bit(record_num(g, f, "DATA", ""));
            g.field_overrides.insert((f, "DATA/".into()), XVal::Num(v));
        }
        Some(b"PERK") => {
            g.gaps.perk_flags.insert((f, 1), on);
        }
        _ => {}
    }
}
