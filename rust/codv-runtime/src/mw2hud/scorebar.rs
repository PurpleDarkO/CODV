//! CODV's kill counter in MW2's score bar art (`scorebar_hd`), raised above GTA's radar:
//!
//! * the big number in the bar's icon square: hostile kills since the mode was switched on
//!   (the session), "Kills" under it;
//! * the top (green) bar: this life's streak, filled towards the next killstreak picked (full
//!   past the last one), "Streak N" beside it;
//! * the bottom (red) bar: the session's kills over kills and deaths, "Deaths N" beside it;
//! * on a death, MW2's "Round Over" in the middle of the screen with that life's kills and the
//!   session's total, for a few seconds once the HUD is back.
//!
//! The bar fills follow the menu's own expressions: width 20 / 1.5 + share x 172 / 1.5, the cap
//! at (132 + 20) / 1.5 + share x 172 / 1.5 - 6 / 1.5.
use super::*;

/// Up from MW2's spot (bottom left) to clear GTA's radar, virtual units.
const RAISE: f32 = 92.0;
/// The whole bar at this size of MW2's (its art, bars and text alike), from its bottom left
/// corner: no larger than the weapon bar.
const SIZE: f32 = 0.68;
const ROUND_OVER_MS: i32 = 4000;
const ROUND_OVER_FADE_MS: i32 = 500;

/// The session's count, as cv_killstreak_step reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    /// Hostile kills since the mode was switched on, and in this life.
    pub total: u32,
    pub life: u32,
    pub deaths: u32,
    /// The next killstreak picked: (kills it needs); None past the last one.
    pub next: Option<i32>,
    /// The last life's kills, waiting to show "Round Over" (start: when the HUD drew it first).
    round_over: Option<(u32, Option<i32>)>,
}

impl Score {
    pub fn kills(&mut self, n: u32) {
        self.total += n;
        self.life += n;
    }

    pub fn died(&mut self) {
        self.deaths += 1;
        self.round_over = Some((self.life, None));
        self.life = 0;
    }

    /// The mode switched off: a new session.
    pub fn reset(&mut self) {
        *self = Score::default();
    }

    /// The top bar's share: this life's streak towards the next killstreak.
    pub fn streak_share(&self) -> f32 {
        match self.next {
            Some(k) if k > 0 => (self.life as f32 / k as f32).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// The bottom bar's share: kills over kills and deaths.
    pub fn kd_share(&self) -> f32 {
        let all = self.total + self.deaths;
        if all == 0 { 0.0 } else { self.total as f32 / all as f32 }
    }
}

impl Mw2Hud {
    pub(super) fn scorebar(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        if !self.scorebar_on {
            return;
        }
        let items = self.data.menu("scorebar_hd").to_vec();
        let Some(bg) = items.iter().find(|i| i.background.eq_ignore_ascii_case("hud_scorebar")).cloned() else {
            return;
        };
        // Raised over the radar, then shrunk towards the bar's bottom left corner.
        let anchor = [bg.rect[0], bg.rect[1] - RAISE + bg.rect[3]];
        let fit = |r: [f32; 4]| {
            [anchor[0] + (r[0] - anchor[0]) * SIZE, anchor[1] + (r[1] - anchor[1]) * SIZE, r[2] * SIZE, r[3] * SIZE]
        };
        let raise = |mut it: Item| {
            it.rect[1] -= RAISE;
            it
        };
        let (top, bottom) = (self.score.streak_share(), self.score.kd_share());
        for it in &items {
            let name = it.background.to_ascii_lowercase();
            if !name.starts_with("hud_scorebar") {
                continue;
            }
            let mut it = raise(it.clone());
            let share = if name.contains("top") { top } else { bottom };
            if name.ends_with("bar") && !name.ends_with("_bg") && name != "hud_scorebar" {
                it.rect[2] = 20.0 / 1.5 + share * 172.0 / 1.5;
            } else if name.ends_with("cap") {
                it.rect[0] = (132.0 + 20.0) / 1.5 + share * 172.0 / 1.5 - 6.0 / 1.5;
            }
            it.rect = fit(it.rect);
            self.menu_pic(out, sp, &it, 1.0);
        }
        let bg = raise(bg);
        let text = |rect: [f32; 4], align: i32, scale: f32, color: [f32; 4]| Item {
            rect: fit(rect),
            align: bg.align,
            color,
            text_scale: scale * SIZE,
            font: 10,
            text_align: align,
            text_style: 3,
            ..Default::default()
        };
        // The icon square: the session's kills, "Kills" under them.
        let square = [bg.rect[0] + 5.333, bg.rect[1] + 4.0, 54.667, 54.667 * 0.62];
        let total = self.score.total.to_string();
        self.item_text(out, sp, &text(square, 1 | 8, 0.5, [1.0; 4]), &total, [1.0; 4]);
        let caption = self.data.string("CGAME_SB_KILLS").unwrap_or("Kills").to_owned();
        let under = [square[0], square[1] + square[3], square[2], 54.667 * 0.3];
        let grey = [0.85, 0.85, 0.85, 0.9];
        self.item_text(out, sp, &text(under, 1 | 8, 0.25, grey), &caption, grey);
        // Beside the bars: "Streak N" (top), "Deaths N" (bottom).
        let x = bg.rect[0] + 232.0;
        let streak = self.data.string("MENU_STREAK").unwrap_or("Streak").to_owned();
        let deaths = self.data.string("CGAME_SB_DEATHS").unwrap_or("Deaths").to_owned();
        let rows = [
            (bg.rect[1] + 18.0, 12.0, format!("{streak} {}", self.score.life)),
            (bg.rect[1] + 31.333, 10.667, format!("{deaths} {}", self.score.deaths)),
        ];
        for (y, h, line) in rows {
            self.item_text(out, sp, &text([x, y, 100.0, h], 8, 0.3333, [1.0; 4]), &line, [1.0; 4]);
        }
    }

    /// "Round Over" with the last life's kills and the session's total, centred, once the HUD is
    /// back after a death.
    pub(super) fn round_over(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some((kills, start)) = self.score.round_over else { return };
        if !self.scorebar_on {
            self.score.round_over = None;
            return;
        }
        let start = start.unwrap_or(self.time_ms);
        self.score.round_over = Some((kills, Some(start)));
        let age = self.time_ms.wrapping_sub(start);
        if age >= ROUND_OVER_MS {
            self.score.round_over = None;
            return;
        }
        let alpha = ((ROUND_OVER_MS - age) as f32 / ROUND_OVER_FADE_MS as f32).clamp(0.0, 1.0);
        let line = |y: f32, scale: f32, font: i32, color: [f32; 4]| Item {
            rect: [0.0, y, 0.0, 0.0],
            align: [h::ALIGN_CENTER, h::ALIGN_CENTER],
            color,
            text_scale: scale,
            font,
            text_align: 1,
            text_style: 3,
            ..Default::default()
        };
        let title = self.data.string("MP_ROUNDEND").unwrap_or("Round Over").to_owned();
        let gold = [1.0, 0.8, 0.4, alpha];
        self.item_text_glow(out, sp, &line(-90.0, 0.5, 9, gold), &title, gold, Some([0.6, 0.3, 0.0, alpha * 0.6]));
        let kills_word = self.data.string("CGAME_SB_KILLS").unwrap_or("Kills").to_owned();
        let white = [1.0, 1.0, 1.0, alpha];
        self.item_text(out, sp, &line(-62.0, 0.4, 10, white), &format!("{kills_word} {kills}"), white);
        let grey = [0.85, 0.85, 0.85, alpha * 0.9];
        let total = format!("{kills_word} (total) {}", self.score.total);
        self.item_text(out, sp, &line(-44.0, 0.3, 10, grey), &total, grey);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_score_counts_the_session_the_life_and_the_deaths() {
        let mut s = Score { next: Some(5), ..Default::default() };
        s.kills(3);
        assert_eq!((s.total, s.life), (3, 3));
        assert!((s.streak_share() - 0.6).abs() < 1e-6);
        assert_eq!(s.kd_share(), 1.0);
        s.died();
        assert_eq!((s.total, s.life, s.deaths), (3, 0, 1));
        assert!((s.kd_share() - 0.75).abs() < 1e-6);
        assert_eq!(s.round_over, Some((3, None)));
        s.next = None;
        assert_eq!(s.streak_share(), 1.0, "past the last killstreak: full");
        s.reset();
        assert_eq!(s, Score::default());
    }
}
