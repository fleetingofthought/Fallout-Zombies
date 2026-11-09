//! The level-up menu's state: the two sheets, the page, the chosen row, the scroll and
//! what the pointer is on; [`LevelUp::view`] reads what to draw from it and the game.

use bevy::prelude::*;
use fnv_game::chargen;
use fnv_game::levelup::{self as rules, PerkSheet, SkillSheet};
use fnv_game::Game;

/// The skill list shows fifteen rows [documented: levelup_menu.xml LUM_SkillList
/// `_number_of_visible_items`].
pub const SKILL_ROWS: usize = 15;
/// The perk list is 676 high [documented: LUM_PerkList], thirteen rows of 52.
pub const PERK_ROWS: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Skills,
    Perks,
}

/// A clickable part besides the rows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Reset,
    Continue,
    Back,
    ScrollUp,
    ScrollDown,
    /// A skill row's "+" and "−".
    Plus(usize),
    Minus(usize),
}

/// A clickable row (its index in the list).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct LevelUp {
    pub level: i32,
    pub skills: SkillSheet,
    /// The level brings a perk: Continue leads to the perks page [documented: the menu's
    /// `_EndPage`, the button "Continue" before the last page and "Done" on it].
    pub perk_due: bool,
    /// The perks page's sheet, while it is shown.
    pub perks: Option<PerkSheet>,
    pub page: Page,
    pub selected: usize,
    pub scroll: usize,
    /// The part under the pointer (drawn lit).
    pub hover: Option<Part>,
    /// ShowPerkMenu's or ShowSkillMenu's title in place of the level's
    /// (`fnv_game::nvse_showoff5::LevelUpOpen`).
    pub title: Option<String>,
    /// ShowPerkMenu: the perks page alone, without Back.
    pub perks_only: bool,
}

/// One row as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct RowView {
    pub text: String,
    pub value: Option<String>,
    pub minus: bool,
    pub plus: bool,
    /// A perk taken: the small square.
    pub marked: bool,
    /// A perk that cannot be taken now: drawn faint [documented: the perk row's
    /// `_TextAlpha`; observed: about half as bright on the owner's screenshots].
    pub dim: bool,
}

/// What the menu shows, read from its state and the game.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub page: Page,
    pub title: String,
    pub rows: Vec<RowView>,
    pub selected: usize,
    pub scroll: usize,
    pub icon: Option<String>,
    pub info: String,
    pub counter: String,
    /// "Continue" or "Done", and whether it may be pressed.
    pub next: &'static str,
    pub next_on: bool,
    pub back: bool,
    pub hover: Option<Part>,
    /// The names the hints use (`crate::pad::labels`).
    pub labels: crate::pad::Labels,
}

impl LevelUp {
    pub fn new(level: i32, skills: SkillSheet, perk_due: bool) -> LevelUp {
        LevelUp { level, skills, perk_due, perks: None, page: Page::Skills, selected: 0, scroll: 0, hover: None, title: None, perks_only: false }
    }

    /// The menu for `level`, from the player as the game holds them.
    pub fn open(g: &mut Game, level: i32) -> LevelUp {
        LevelUp::new(level, SkillSheet::of_player(g, level), rules::perk_due(g, level))
    }

    pub fn rows(&self) -> usize {
        match (&self.page, &self.perks) {
            (Page::Perks, Some(p)) => p.perks.len(),
            _ => self.skills.skills.len(),
        }
    }

    pub fn visible(&self) -> usize {
        match self.page {
            Page::Skills => SKILL_ROWS,
            Page::Perks => PERK_ROWS,
        }
    }

    /// Choose row `i` (pointer or keys), scrolling it into the list.
    pub fn select(&mut self, i: usize) {
        if i >= self.rows() {
            return;
        }
        self.selected = i;
        let v = self.visible();
        if i < self.scroll {
            self.scroll = i;
        } else if i >= self.scroll + v {
            self.scroll = i + 1 - v;
        }
    }

    /// Move the choice by `by` rows, stopping at the ends.
    pub fn step(&mut self, by: i32) {
        let n = self.rows() as i32;
        if n > 0 {
            self.select((self.selected as i32 + by).clamp(0, n - 1) as usize);
        }
    }

    /// Scroll the list by `by` rows.
    pub fn scroll_by(&mut self, by: i32) {
        let most = self.rows().saturating_sub(self.visible()) as i32;
        self.scroll = (self.scroll as i32 + by).clamp(0, most) as usize;
    }

    /// Reset: the page's choices back [documented: LUM_ResetButton; hypothesis: on the
    /// perks page it gives the perk back].
    pub fn reset(&mut self) {
        match (&self.page, &mut self.perks) {
            (Page::Perks, Some(p)) => p.chosen.clear(),
            _ => self.skills.reset(),
        }
    }

    pub fn can_continue(&self) -> bool {
        match (&self.page, &self.perks) {
            (Page::Perks, Some(p)) => p.complete(),
            _ => self.skills.complete(),
        }
    }

    /// On to the perks page with its sheet.
    pub fn to_perks(&mut self, perks: PerkSheet) {
        self.perks = Some(perks);
        self.page = Page::Perks;
        self.selected = 0;
        self.scroll = 0;
        self.hover = None;
    }

    /// Back to the skills page.
    pub fn back(&mut self) {
        self.perks = None;
        self.page = Page::Skills;
        self.selected = 0;
        self.scroll = 0;
        self.hover = None;
    }

    /// The row named `name` on the page shown (not case sensitive).
    pub fn find(&self, g: &Game, name: &str) -> Option<usize> {
        match (&self.page, &self.perks) {
            (Page::Perks, Some(p)) => p.perks.iter().position(|id| g.data.perks.get(id).is_some_and(|d| d.name.eq_ignore_ascii_case(name))),
            _ => self.skills.skills.iter().position(|&s| chargen::skill_name(g, s).eq_ignore_ascii_case(name)),
        }
    }

    pub fn view(&self, g: &mut Game) -> View {
        let skills = &self.skills;
        let (rows, icon, info, counter) = match (&self.page, &self.perks) {
            (Page::Perks, Some(p)) => {
                let rows = p
                    .perks
                    .iter()
                    .enumerate()
                    .map(|(i, id)| RowView {
                        text: rules::perk_row(&g.data.perks.get(id).map(|d| d.name.clone()).unwrap_or_else(|| id.to_string()), g.player.perks.get(id).copied().unwrap_or(0)),
                        value: None,
                        minus: false,
                        plus: false,
                        marked: p.chosen.contains(id),
                        dim: !p.available[i],
                    })
                    .collect();
                let id = p.perks.get(self.selected).copied();
                let d = id.and_then(|id| g.data.perks.get(&id));
                let info = match (id, d) {
                    (Some(id), Some(d)) => rules::perk_info(&chargen::requirement_line(g, id), d.ranks.max(1), &d.description),
                    _ => String::new(),
                };
                (rows, d.and_then(|d| d.icon.clone()), info, rules::perk_counter(p.left()))
            }
            _ => {
                let rows = (0..skills.skills.len())
                    .map(|i| RowView {
                        text: chargen::skill_name(g, skills.skills[i]),
                        value: Some(skills.value(i).to_string()),
                        minus: skills.can_lower(i),
                        plus: skills.can_raise(i),
                        marked: false,
                        dim: false,
                    })
                    .collect();
                let a = skills.skills.get(self.selected).and_then(|&s| g.data.actor_value_info(s));
                (rows, a.and_then(|a| a.icon.clone()), a.map(|a| a.description.clone()).unwrap_or_default(), rules::skill_counter(skills.remaining()))
            }
        };
        let last = self.page == Page::Perks || !self.perk_due;
        View {
            page: self.page,
            title: self.title.clone().unwrap_or_else(|| rules::title(self.level)),
            rows,
            selected: self.selected,
            scroll: self.scroll,
            icon,
            info,
            counter,
            next: if last { "Done" } else { "Continue" },
            next_on: self.can_continue(),
            back: self.page == Page::Perks && !self.perks_only,
            hover: self.hover,
            labels: crate::pad::labels(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_sim::FormId;

    fn menu(perk_due: bool) -> LevelUp {
        LevelUp::new(3, SkillSheet::new(vec![32, 34, 40], vec![17, 17, 17], vec![1, 1, 1], 12), perk_due)
    }

    #[test]
    fn continue_waits_for_every_point() {
        let mut m = menu(false);
        assert!(!m.can_continue());
        for _ in 0..12 {
            assert!(m.skills.raise(2));
        }
        assert!(m.can_continue());
        m.reset();
        assert_eq!(m.skills.remaining(), 12);
    }

    #[test]
    fn the_perks_page_and_back() {
        let mut m = menu(true);
        m.select(2);
        let perks = PerkSheet::new((1..=20).map(FormId).collect());
        m.to_perks(perks);
        assert_eq!((m.page, m.selected, m.rows()), (Page::Perks, 0, 20));
        assert!(!m.can_continue());
        // Thirteen rows shown: the list scrolls with the choice.
        m.step(15);
        assert_eq!((m.selected, m.scroll), (15, 3));
        m.scroll_by(10);
        assert_eq!(m.scroll, 7);
        m.back();
        assert_eq!((m.page, m.rows(), m.perks.is_none()), (Page::Skills, 3, true));
        m.step(10);
        assert_eq!((m.selected, m.scroll), (2, 0), "fifteen skill rows fit");
    }
}
