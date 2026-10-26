//! The Javelin's CLU view (`javelin_overlay_hd`), shown while the Javelin is fully aimed
//! (hud `ads_javelin`: the weapon's overlayInterface 1 and fWeaponPosFrac 1). The view model
//! hides behind it, as the snipers' behind their scope. Items and their visibility, from the
//! menu's own expressions:
//!
//! * the film grain and the CLU frame (`hud_javelin_bg`), always;
//! * the target lines through the lock's screen position (`weaplockblink(15)`: blinking while
//!   the lock builds, steady once locked; hidden without a target);
//! * the seeker box at the centre while there is no lock (`!weaplocking && !weaplocked`);
//! * the lamps: LOCK (`weaplockblink(4.5)`), CLU, TOP (CODV's rockets always top-attack) and
//!   ROCKET / NO ROCKET from the clip.
use super::*;

/// The lock the host's launcher holds (gunplay's lock-on).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Lock {
    #[default]
    None,
    Locking,
    Locked,
}

/// The CLU's lock state this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clu {
    pub lock: Lock,
    /// The target's screen position (backbuffer pixels).
    pub at: [f32; 2],
    pub has_rocket: bool,
    /// The target is too close to lock or fire at (the CLU's warning).
    pub too_close: bool,
}

/// MW2's weaplockblink(rate): steady once locked, a square wave at `rate` Hz while
/// locking (rate 0: steady), off without a lock.
pub fn lock_blink(lock: Lock, rate: f32, ms: i32) -> bool {
    match lock {
        Lock::None => false,
        Lock::Locked => true,
        Lock::Locking => rate <= 0.0 || ((ms.max(0) as f32 * rate / 1000.0) as i64) % 2 == 0,
    }
}

impl Mw2Hud {
    pub(super) fn javelin_view(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, clu: Clu) {
        let ms = self.time_ms;
        let items = self.data.menu("javelin_overlay_hd").to_vec();
        for item in &items {
            let name = item.name.to_ascii_lowercase();
            let bg = item.background.to_ascii_lowercase();
            if item.background.is_empty() {
                // The too-close warnings (MP_TOO_CLOSE_NOLOCK before the lock, NOFIRE once locked);
                // the other prompts are owner text CODV leaves out.
                let show = clu.too_close
                    && match name.as_str() {
                        "prompt_tooclosewarningprelock" => clu.lock != Lock::Locked,
                        "prompt_tooclosewarning" => clu.lock == Lock::Locked,
                        _ => false,
                    };
                if show {
                    self.menu_text(out, sp, item, None, 1.0);
                }
                continue;
            }
            if name.starts_with("target_line") {
                if !lock_blink(clu.lock, 15.0, ms) {
                    continue;
                }
                // Pixels (ALIGN_NOSCALE): the lines cross at the target.
                let mut it = item.clone();
                let (x, y) = (clu.at[0], clu.at[1]);
                match name.as_str() {
                    "target_line_horz_bg" => it.rect[1] = y - 1.0,
                    "target_line_vert_bg" => it.rect[0] = x - 2.0,
                    "target_line_horz" => it.rect[1] = y,
                    "target_line_vert" => it.rect[0] = x,
                    _ => {}
                }
                self.menu_pic(out, sp, &it, 1.0);
                continue;
            }
            let show = match bg.as_str() {
                "hud_javelin_lock_box" => clu.lock == Lock::None,
                "hud_javelin_lock_on" => lock_blink(clu.lock, 4.5, ms),
                "hud_javelin_top_on" => clu.lock != Lock::None,
                "hud_javelin_dir_on" => false,
                "hud_javelin_rocket_on" => clu.has_rocket,
                "hud_javelin_norocket_on" => !clu.has_rocket,
                _ => true, // grain, frame, CLU lamp
            };
            if show {
                self.menu_pic(out, sp, item, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_lamp_blinks_while_locking_and_stays_lit_once_locked() {
        assert!(!lock_blink(Lock::None, 4.5, 0));
        assert!(lock_blink(Lock::Locked, 4.5, 123));
        // 4.5 Hz: on for the first 222 ms, off for the next.
        assert!(lock_blink(Lock::Locking, 4.5, 100));
        assert!(!lock_blink(Lock::Locking, 4.5, 300));
        assert!(lock_blink(Lock::Locking, 0.0, 300));
    }
}
