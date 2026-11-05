//! CODV's kill counter in MW2's score bar art (`scorebar_hd`), raised above GTA's radar:
//!
//! * the big number in the bar's icon square: hostile kills since the mode was switched on
//!   (the session), "Kills" under it;
//! * the top (green) bar: this life's streak, filled towards the next killstreak picked (full
//!   past the last one), "Streak N" beside it;
//! * the bottom (red) bar: the session's kills over kills and deaths, "Deaths N" beside it;
//! * on a death, MW2's "Round Over" in the middle of the screen with that life's kills and the
//!   session's total, for a few seconds once the HUD is back;
//! * CODV's round: "Round N" in small under "Kills", one more each time a Tactical Nuke ends the run
//!   (its streak starts over).
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
    /// Runs a Tactical Nuke ended since the mode was switched on: the round shown is one more.
    pub nukes: u32,
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

    /// The Tactical Nuke over: this life's streak starts again (the session's kills stay) and a new
    /// round begins.
    pub fn start_over(&mut self) {
        self.life = 0;
        self.nukes += 1;
    }

    /// The round under way, from 1.
    pub fn round(&self) -> u32 {
        self.nukes + 1
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

/// A game mode's own count in the score bar's frame (cv_mw2_scorebar): the big number and its caption, the
/// small gold line under them, each bar's share (0..1) and its row of text. Sent every frame; gone when not
/// sent again for a moment (the score bar's own count is back).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostBoard {
    pub big: u32,
    pub caption: String,
    pub under: String,
    pub top_share: f32,
    pub top_row: String,
    pub bottom_share: f32,
    pub bottom_row: String,
    /// Over the bars: a title (a mode's state: "GET READY", "WAVE 3 CLEARED") and a line under it (empty: none).
    pub title: String,
    pub line: String,
}

/// A mode's board not sent again within this long is dropped (ms).
const HOST_BOARD_STALE_MS: i32 = 500;

/// What the score bar shows this frame: CODV's count, or a game mode's.
struct Board {
    big: String,
    caption: String,
    under: String,
    top: f32,
    bottom: f32,
    top_row: String,
    bottom_row: String,
    title: String,
    line: String,
}

impl Mw2Hud {
    /// A notice over the score bar's bars for `ms` (the title in the big font, a line under it). False when the
    /// score bar does not show (the caller says it another way).
    pub fn board_notice(&mut self, title: &str, line: &str, ms: i32) -> bool {
        let fresh =
            self.host_board.as_ref().is_some_and(|(_, at)| self.time_ms.wrapping_sub(*at) <= HOST_BOARD_STALE_MS);
        if !self.scorebar_on && !fresh {
            return false;
        }
        self.board_notice = Some((title.to_owned(), line.to_owned(), self.time_ms.wrapping_add(ms.clamp(500, 15000))));
        true
    }

    /// A game mode's count for the score bar's frames to come (None: the score bar's own count).
    pub fn set_host_board(&mut self, board: Option<HostBoard>) {
        self.host_board = board.map(|b| (b, self.time_ms));
    }

    fn board(&mut self) -> Option<Board> {
        if let Some((b, at)) = &self.host_board {
            if self.time_ms.wrapping_sub(*at) <= HOST_BOARD_STALE_MS {
                return Some(Board {
                    big: b.big.to_string(),
                    caption: b.caption.clone(),
                    under: b.under.clone(),
                    top: b.top_share.clamp(0.0, 1.0),
                    bottom: b.bottom_share.clamp(0.0, 1.0),
                    top_row: b.top_row.clone(),
                    bottom_row: b.bottom_row.clone(),
                    title: b.title.clone(),
                    line: b.line.clone(),
                });
            }
            self.host_board = None;
        }
        if !self.scorebar_on {
            return None;
        }
        let streak = self.data.string("MENU_STREAK").unwrap_or("Streak").to_owned();
        let deaths = self.data.string("CGAME_SB_DEATHS").unwrap_or("Deaths").to_owned();
        Some(Board {
            big: self.score.total.to_string(),
            caption: self.data.string("CGAME_SB_KILLS").unwrap_or("Kills").to_owned(),
            under: format!("Round {}", self.score.round()),
            top: self.score.streak_share(),
            bottom: self.score.kd_share(),
            top_row: format!("{streak} {}", self.score.life),
            bottom_row: format!("{deaths} {}", self.score.deaths),
            title: String::new(),
            line: String::new(),
        })
    }

    pub(super) fn scorebar(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some(mut board) = self.board() else { return };
        // A timed notice over the bars (a Juggernaut down), where a mode's own title is not.
        if let Some((title, line, until)) = &self.board_notice {
            if until.wrapping_sub(self.time_ms) > 0 {
                if board.title.is_empty() {
                    board.title = title.clone();
                    board.line = line.clone();
                }
            } else {
                self.board_notice = None;
            }
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
        let (top, bottom) = (board.top, board.bottom);
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
        // The icon square: the big count (the session's kills), its caption, a small gold line under that
        // (the round, a mode's wave).
        let side = 54.667;
        let square = [bg.rect[0] + 5.333, bg.rect[1] + 3.0, side, side * 0.52];
        self.item_text(out, sp, &text(square, 1 | 8, 0.5, [1.0; 4]), &board.big, [1.0; 4]);
        let under = [square[0], square[1] + square[3], side, side * 0.22];
        let grey = [0.85, 0.85, 0.85, 0.9];
        self.item_text(out, sp, &text(under, 1 | 8, 0.22, grey), &board.caption, grey);
        let gold = [1.0, 0.8, 0.4, 0.95];
        let low = [square[0], under[1] + under[3], side, side * 0.2];
        self.item_text(out, sp, &text(low, 1 | 8, 0.17, gold), &board.under, gold);
        // Beside the bars: their rows ("Streak N" and "Deaths N", or a mode's).
        let x = bg.rect[0] + 232.0;
        let rows = [(bg.rect[1] + 18.0, 12.0, board.top_row), (bg.rect[1] + 31.333, 10.667, board.bottom_row)];
        for (y, h, line) in rows {
            self.item_text(out, sp, &text([x, y, 100.0, h], 8, 0.3333, [1.0; 4]), &line, [1.0; 4]);
        }
        // Over the bars: a mode's state in the big title font (blood red, a dark glow) and its line under it.
        let left = bg.rect[0] + 70.0;
        if !board.title.is_empty() {
            let red = [0.85, 0.1, 0.06, 1.0];
            let title = Item { font: 9, ..text([left, bg.rect[1] - 30.0, 200.0, 16.0], 8, 0.42, red) };
            self.item_text_glow(out, sp, &title, &board.title, red, Some([0.35, 0.0, 0.0, 0.8]));
        }
        if !board.line.is_empty() {
            let white = [0.93, 0.93, 0.88, 1.0];
            self.item_text(out, sp, &text([left, bg.rect[1] - 12.0, 220.0, 11.0], 8, 0.28, white), &board.line, white);
        }
    }

    /// "Round Over" with the last life's kills and the session's total, centred, once the HUD is
    /// back after a death.
    pub(super) fn round_over(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some((kills, start)) = self.score.round_over else { return };
        // A game mode with its own board (Survival) says its own end in the score bar's frame: no centred
        // "Round Over" over it.
        let host_board =
            matches!(&self.host_board, Some((_, at)) if self.time_ms.wrapping_sub(*at) <= HOST_BOARD_STALE_MS);
        if !self.scorebar_on || host_board {
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
        assert_eq!(s.round(), 1);
        s.kills(35);
        s.start_over();
        assert_eq!((s.round(), s.life, s.total), (2, 0, 38), "a nuke: a new round, the session's kills kept");
        s.start_over();
        assert_eq!(s.round(), 3);
        s.reset();
        assert_eq!(s, Score::default());
    }

    #[test]
    fn a_modes_board_takes_the_score_bar_while_sent() {
        let mut hud = Mw2Hud::new(Hud::default());
        hud.scorebar_on = false;
        assert!(hud.board().is_none(), "the score bar off: nothing");
        hud.set_host_board(Some(HostBoard {
            big: 42,
            under: "Wave 3/15".into(),
            top_share: 2.0,
            ..Default::default()
        }));
        let b = hud.board().expect("a mode's board shows");
        assert_eq!((b.big.as_str(), b.under.as_str(), b.top), ("42", "Wave 3/15", 1.0));
        hud.time_ms += HOST_BOARD_STALE_MS + 1;
        assert!(hud.board().is_none(), "not sent again: gone");
        assert!(!hud.board_notice("JUGGERNAUT DOWN", "", 3000), "no score bar: said another way");
        hud.scorebar_on = true;
        assert!(hud.board_notice("JUGGERNAUT DOWN", "Enemy Juggernaut killed", 3000));
    }
}
