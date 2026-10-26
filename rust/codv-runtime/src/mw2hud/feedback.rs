//! MW2's feedback on the player's own fight:
//!
//! * the blood overlay (cg's blood overlay, the `splatter_alt` material): `blood_defocus_color`
//!   over the screen, spread in from the edges by `blood_defocus_mask` as health drops; its
//!   intensity is 1 - health, falling back at 0.3 a second as health returns
//!   (hud_iw4 `blood_overlay_lerp`), rising at once;
//! * the score popup (_rank.gsc scorePopup): "+N" in hudbig, yellow, above the crosshair, the
//!   points of kills close together added up, pulsing on each, fading a second after the last;
//! * the bonus splashes (_awards / splashNotify): "Headshot!", "Longshot!", "First Blood!" and
//!   "Double Kill!" / "Triple Kill!" / "Multi-Kill!" with their "+N", in the killstreak splash's
//!   layout, one after another (a killstreak's own splash goes first).
//!
//! The points are MW2's Team Deathmatch rank events: a kill 100, a head shot 50 more, a long
//! shot 50, first blood 100, a double kill 50, a triple 75, more 100.
use super::*;

const BLOOD_LERP_RATE: f32 = 0.3;
const POPUP_HOLD_MS: i32 = 1000;
const POPUP_FADE_MS: i32 = 750;
const POPUP_PULSE_MS: i32 = 160;
/// Kills this close together count as one multi-kill.
const MULTI_KILL_MS: i32 = 1500;
const KILL_SPLASH_MS: i32 = 1600;
/// At most this many bonus splashes wait (a burst of kills does not queue a minute of them).
const KILL_SPLASH_QUEUE: usize = 3;

pub const POINTS_KILL: u32 = 100;
pub const POINTS_HEADSHOT: u32 = 50;
pub const POINTS_LONGSHOT: u32 = 50;
pub const POINTS_FIRST_BLOOD: u32 = 100;

/// The kill feedback's state (cleared with the session).
#[derive(Clone, Debug, Default)]
pub struct Feedback {
    /// The popup's running total and when points were last added.
    popup: Option<(u32, i32)>,
    /// The times of the last kills (the multi-kill window).
    recent: Vec<i32>,
    first_blood_done: bool,
    /// Bonus splashes waiting (splash key, points) and the one showing (with its start).
    queue: std::collections::VecDeque<(&'static str, u32)>,
    showing: Option<(&'static str, u32, i32)>,
    /// The blood overlay's intensity and the HUD time it was last stepped at.
    blood: f32,
    blood_ms: Option<i32>,
}

impl Feedback {
    pub fn reset(&mut self) {
        let blood = (self.blood, self.blood_ms);
        *self = Feedback::default();
        (self.blood, self.blood_ms) = blood;
    }

    /// The blood intensity after `ms`: 1 - health at once when worse, easing back at 0.3 / s.
    pub fn step_blood(&mut self, health: f32, now: i32) -> f32 {
        let dt = self.blood_ms.map_or(0, |t| now.wrapping_sub(t).clamp(0, 1000));
        self.blood_ms = Some(now);
        let target = (1.0 - health).clamp(0.0, 1.0);
        if target < self.blood {
            self.blood = (self.blood - dt as f32 / 1000.0 * BLOOD_LERP_RATE).max(target);
        } else {
            self.blood = target;
        }
        self.blood
    }

    /// `kills` hostile kills at `now`, `headshots` / `longshots` of them: the points they add.
    pub fn kills(&mut self, kills: u32, headshots: u32, longshots: u32, now: i32) -> u32 {
        if kills == 0 {
            return 0;
        }
        let mut points = POINTS_KILL * kills;
        let splash = |key: &'static str, p: u32, q: &mut std::collections::VecDeque<_>| {
            if q.len() < KILL_SPLASH_QUEUE {
                q.push_back((key, p));
            }
        };
        if !self.first_blood_done {
            self.first_blood_done = true;
            points += POINTS_FIRST_BLOOD;
            splash("SPLASHES_FIRSTBLOOD", POINTS_FIRST_BLOOD, &mut self.queue);
        }
        for _ in 0..headshots.min(kills) {
            points += POINTS_HEADSHOT;
            splash("SPLASHES_HEADSHOT", POINTS_HEADSHOT, &mut self.queue);
        }
        for _ in 0..longshots.min(kills) {
            points += POINTS_LONGSHOT;
            splash("SPLASHES_LONGSHOT", POINTS_LONGSHOT, &mut self.queue);
        }
        self.recent.retain(|&t| now.wrapping_sub(t) <= MULTI_KILL_MS);
        let before = self.recent.len();
        self.recent.extend(std::iter::repeat_n(now, kills as usize));
        let after = self.recent.len();
        // One splash for the multi-kill as it grows: the latest step replaces a waiting one.
        let multi = |n: usize| match n {
            2 => Some(("SPLASHES_DOUBLEKILL", 50)),
            3 => Some(("SPLASHES_TRIPLEKILL", 75)),
            n if n >= 4 => Some(("SPLASHES_MULTIKILL", 100)),
            _ => None,
        };
        if after >= 2 && multi(after) != multi(before) {
            let (key, p) = multi(after).unwrap();
            points += p;
            self.queue.retain(|(k, _)| !k.ends_with("KILL"));
            splash(key, p, &mut self.queue);
        }
        let total = match self.popup {
            Some((t, at)) if now.wrapping_sub(at) < POPUP_HOLD_MS + POPUP_FADE_MS => t + points,
            _ => points,
        };
        self.popup = Some((total, now));
        points
    }
}

impl Mw2Hud {
    /// Hostile kills from the host (cv_killstreak_step): the popup and the bonus splashes.
    pub fn kill_feedback(&mut self, kills: u32, headshots: u32, longshots: u32) {
        if self.feedback_on {
            self.feedback.kills(kills, headshots, longshots, self.time_ms);
        }
    }

    /// MW2's blood over the whole screen, under the rest of the HUD.
    pub(super) fn blood_overlay(&mut self, out: &mut Vec<Quad>, view: View) {
        let intensity = self.feedback.step_blood(self.health, self.time_ms);
        if !self.blood_on || intensity <= 0.001 {
            return;
        }
        let (Some(color), Some(mask)) = (self.image("blood_defocus_color"), self.image("blood_defocus_mask")) else {
            return;
        };
        // The colour's alpha carries the intensity; the host spreads it by the mask (QUAD_SPLATTER).
        let mut q = rect_quad(color, 0.0, 0.0, view.width, view.height, [1.0, 1.0, 1.0, intensity]);
        q.mask = Some(mask);
        q.flags |= QUAD_SPLATTER;
        out.push(q);
    }

    /// The "+N" popup above the crosshair.
    pub(super) fn score_popup(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some((total, at)) = self.feedback.popup else { return };
        if !self.feedback_on {
            return;
        }
        let age = self.time_ms.wrapping_sub(at);
        if age >= POPUP_HOLD_MS + POPUP_FADE_MS {
            self.feedback.popup = None;
            return;
        }
        let alpha = if age <= POPUP_HOLD_MS {
            0.85
        } else {
            0.85 * (1.0 - (age - POPUP_HOLD_MS) as f32 / POPUP_FADE_MS as f32)
        };
        // fontPulse: a quick swell on each addition.
        let pulse = if age < POPUP_PULSE_MS { 1.0 + 0.35 * (1.0 - age as f32 / POPUP_PULSE_MS as f32) } else { 1.0 };
        let color = [1.0, 1.0, 0.5, alpha];
        let item = Item {
            rect: [0.0, -38.0, 0.0, 0.0],
            align: [h::ALIGN_CENTER, h::ALIGN_CENTER],
            color,
            text_scale: 0.42 * pulse,
            font: 9,
            text_align: 1 | 8,
            text_style: 3,
            ..Default::default()
        };
        self.item_text(out, sp, &item, &format!("+{total}"), color);
    }

    /// The bonus splashes, after any killstreak splash.
    pub(super) fn kill_splash(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        if !self.feedback_on || self.splash.is_some() || !self.splash_queue.is_empty() {
            return;
        }
        if self.feedback.showing.is_none() {
            let Some((key, points)) = self.feedback.queue.pop_front() else { return };
            self.feedback.showing = Some((key, points, self.time_ms));
        }
        let Some((key, points, start)) = self.feedback.showing else { return };
        let age = self.time_ms.wrapping_sub(start);
        let Some((dx, alpha)) = splash_motion(age, KILL_SPLASH_MS) else {
            self.feedback.showing = None;
            return;
        };
        let Some(title) = self.data.string(key).map(str::to_owned) else {
            self.feedback.showing = None;
            return;
        };
        let desc = self
            .data
            .string(&format!("{key}_DESC"))
            .map(|d| strip_colour_codes(&h::splash_replace_optional(d, points as i32)))
            .unwrap_or_else(|| format!("+{points}"));
        let items = self.data.splash_items.clone();
        for item in &items {
            let mut item = item.clone();
            item.rect[0] += dx;
            let mut color = item.color;
            color[3] *= alpha;
            match item.name.as_str() {
                "killstreak_title" => {
                    let glow = [0.9, 0.55, 0.1, 0.6 * alpha];
                    self.item_text_glow(out, sp, &item, &title, color, Some(glow));
                }
                "killstreak_description" => {
                    let yellow = [1.0, 1.0, 0.5, color[3]];
                    self.item_text(out, sp, &item, &desc, yellow);
                }
                _ => {}
            }
        }
    }
}

/// MW2's "^N" colour codes out of a string (the line is drawn in one colour).
pub fn strip_colour_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(|n| n.is_ascii_digit()) {
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kills_add_mw2s_points_and_bonuses() {
        let mut f = Feedback::default();
        // The first kill of the session: 100 + first blood.
        assert_eq!(f.kills(1, 0, 0, 1000), 200);
        assert_eq!(f.popup, Some((200, 1000)));
        // A head shot 400 ms later: 100 + 50, and a double kill (+50); the popup adds up.
        assert_eq!(f.kills(1, 1, 0, 1400), 200);
        assert_eq!(f.popup, Some((400, 1400)));
        let keys: Vec<&str> = f.queue.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, ["SPLASHES_FIRSTBLOOD", "SPLASHES_HEADSHOT", "SPLASHES_DOUBLEKILL"]);
        // Long after: a plain long shot, the popup starts again.
        f.queue.clear();
        assert_eq!(f.kills(1, 0, 1, 9000), 150);
        assert_eq!(f.popup, Some((150, 9000)));
        // Three at once: a triple kill (+75), its splash alone.
        f.queue.clear();
        assert_eq!(f.kills(3, 0, 0, 20000), 375);
        assert_eq!(f.queue.iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["SPLASHES_TRIPLEKILL"]);
        f.reset();
        assert_eq!(f.kills(1, 0, 0, 30000), 200, "a new session: first blood again");
    }

    #[test]
    fn the_blood_rises_at_once_and_eases_back() {
        let mut f = Feedback::default();
        assert_eq!(f.step_blood(1.0, 0), 0.0);
        assert!((f.step_blood(0.4, 16) - 0.6).abs() < 1e-6);
        // Health back to full: 0.3 a second down.
        assert!((f.step_blood(1.0, 1016) - 0.3).abs() < 1e-4);
        assert!(f.step_blood(1.0, 3016) == 0.0);
    }

    #[test]
    fn colour_codes_are_stripped() {
        assert_eq!(strip_colour_codes("You got the first kill. (^3+100^7)"), "You got the first kill. (+100)");
    }
}
