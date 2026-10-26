//! F5c: the killstreaks' own HUDs, from MW2's menus in the import:
//!
//! * the Predator Missile's camera (`missilecam_hud_hd`: the 105 mm frame and the steer / boost
//!   hints) and the static when it hits (_remotemissile.gsc staticEffect: white and
//!   `ac130_overlay_grain`);
//! * the AC-130 (`ac130_hud_hd`): the gun's frame, the three gun labels (the one in use
//!   flickering: alpha 1 - sin(ms / 90) * 0.65 as the menu's expression), the instrument text
//!   blocks, the time left, "Reloading", the hints and the film grain;
//! * the Chopper Gunner (`remote_chopper_overlay_hd`): its overlay, night-vision goggles and
//!   grain, the 25 mm label, the heading tape (owner draw 145 scrolled by the camera's heading),
//!   the turret's reticle (owner draw 190: cobra_player_minigun_mp's centre reticle), the seeker
//!   box wandering around the centre and the locking / locked boxes on the hostile held near the
//!   crosshair (CODV's lock: 0.5 s on it);
//! * the airstrike's target picking (`targetmap_fullscreen`'s scan lines and confirm hint, with
//!   MW2's "><" crosshair: CODV picks the target with a camera above it instead of MW2's map);
//! * the thermal FOF boxes on hostiles (cg ThermalVisionFOFOverlay: `hud_fofbox_hostile`
//!   sized as the target's height x FoFIconScale 1.3, clamped to FoFIconMinSize 30 ..
//!   FoFIconMaxSize 640);
//! * notices on any view: the Tactical Nuke's countdown and a crate's use hint and capture bar.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewKind {
    #[default]
    None,
    Predator,
    Ac130,
    Chopper,
    TargetMap,
}

/// The killstreak view of this frame, as the host describes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreakView {
    pub kind: ViewKind,
    /// The AC-130's gun (0 105 mm, 1 40 mm, 2 25 mm).
    pub weapon: u32,
    pub reloading: bool,
    /// The Predator's static after the hit.
    pub static_noise: bool,
    pub seconds_left: f32,
    /// The Tactical Nuke's countdown (None: no nuke inbound).
    pub nuke_seconds: Option<f32>,
    /// A localize key shown at the bottom centre (a crate's "Press and hold ...") and its bar.
    pub hint: String,
    pub hint_progress: Option<f32>,
    /// Hostiles in the thermal view: (base x, base y, head x, head y), backbuffer pixels.
    pub targets: Vec<[f32; 4]>,
    /// The camera's heading (GTA degrees: 0 north, growing to the west): the heading tape.
    pub heading: f32,
}

/// cg_fofIcon* defaults (virtual units, scale).
const FOF_ICON_SCALE: f32 = 1.3;
const FOF_ICON_MIN: f32 = 30.0;
const FOF_ICON_MAX: f32 = 640.0;
const AC130_WEAPONS: [&str; 3] = ["AC130_HUD_WEAPON_105MM", "AC130_HUD_WEAPON_40MM", "AC130_HUD_WEAPON_25MM"];
const AC130_OVERLAYS: [&str; 3] = ["ac130_overlay_105mm", "ac130_overlay_40mm", "ac130_overlay_25mm"];
/// compassTickertapeStretch's default: the heading tape shows half its image at once.
const TICKER_STRETCH: f32 = 0.5;
/// The Chopper Gunner's lock: a hostile this close to the crosshair (share of the screen's
/// height) is tracked, and locked after CHOPPER_LOCK_MS on it.
const CHOPPER_LOCK_RADIUS: f32 = 0.09;
const CHOPPER_LOCK_MS: i32 = 500;

/// The Chopper Gunner's lock state: the target's screen position, and whether it is complete.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChopperLock {
    pub at: [f32; 2],
    pub locked: bool,
}

/// The menus' flicker: 1 - sin(milliseconds / 90) * 0.65 (clamped), MW2's blinking text.
pub fn flicker(ms: i32) -> f32 {
    (1.0 - (ms as f32 / 90.0).sin() * 0.65).clamp(0.0, 1.0)
}

/// "m:ss" as MW2's secondsAsCountdown.
pub fn countdown(seconds: f32) -> String {
    let s = seconds.max(0.0).ceil() as i32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// MW2's key references in its strings ("[{+attack}]") as CODV's PC keys.
pub fn bind_keys(text: &str, action_slot: &str) -> String {
    text.replace("{+attack}", "Left Mouse")
        .replace("{+speed_throw}", "Right Mouse")
        .replace("{+activate}", "E")
        .replace("{+usereload}", "E")
        .replace("{weapnext}", "Tab")
        .replace("{+actionslot 4}", action_slot)
}

/// A solid fill ("$white", "white", "$black" in MW2's menus): the host draws a missing image white.
fn solid(background: &str) -> Option<[f32; 3]> {
    match background {
        "$white" | "white" => Some([1.0, 1.0, 1.0]),
        "$black" | "black" => Some([0.0, 0.0, 0.0]),
        _ => None,
    }
}

impl Mw2Hud {
    /// An item's image (or solid fill) over its rectangle.
    pub(super) fn menu_pic(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, item: &Item, alpha: f32) {
        let mut color = item.color;
        color[3] *= alpha;
        if color[3] <= 0.0 {
            return;
        }
        let (name, tint) = match solid(&item.background) {
            Some(rgb) => ("$white".to_owned(), Some(rgb)),
            None => (item.background.clone(), None),
        };
        if let Some(rgb) = tint {
            color = [color[0] * rgb[0], color[1] * rgb[1], color[2] * rgb[2], color[3]];
        }
        let Some(img) = self.image(&name) else { return };
        // A zero-sized fullscreen item (targetmap_fullscreen) covers the screen.
        let rect = if item.rect[2] <= 0.0 && item.rect[3] <= 0.0 { [0.0, 0.0, 640.0, 480.0] } else { item.rect };
        let align = if item.rect[2] <= 0.0 && item.rect[3] <= 0.0 { [h::ALIGN_FULLSCREEN; 2] } else { item.align };
        let mut q = pic(sp, img, rect, align, color);
        if tint.is_some() {
            q.flags |= QUAD_SOLID;
        }
        out.push(q);
    }

    /// An item's text: its own "@KEY" (or `text`), keys bound, at `alpha`.
    pub(super) fn menu_text(
        &mut self,
        out: &mut Vec<Quad>,
        sp: &h::ScreenPlacement,
        item: &Item,
        text: Option<&str>,
        alpha: f32,
    ) {
        let text = match text {
            Some(t) => t.to_owned(),
            None => {
                let Some(key) = item.text.strip_prefix('@') else {
                    if item.text.is_empty() {
                        return;
                    }
                    return self.menu_text(out, sp, item, Some(&item.text.clone()), alpha);
                };
                match self.data.string(key) {
                    Some(t) => t.to_owned(),
                    None => return,
                }
            }
        };
        let text = bind_keys(&text, &self.key_label.clone());
        let mut color = item.color;
        color[3] *= alpha;
        if color[3] > 0.0 {
            self.item_text(out, sp, item, &text, color);
        }
    }

    pub(super) fn streak_overlay(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, view: View) {
        let v = self.streak_view.clone();
        let ms = self.time_ms;
        match v.kind {
            ViewKind::None => {}
            ViewKind::Predator => {
                if v.static_noise {
                    // _remotemissile.gsc staticEffect: white, then ac130_overlay_grain over it.
                    let white = Item {
                        background: "$white".into(),
                        rect: [0.0, 0.0, 640.0, 480.0],
                        align: [4, 4],
                        color: [1.0; 4],
                        ..Default::default()
                    };
                    self.menu_pic(out, sp, &white, 1.0);
                    if let Some(grain) = self.data.image("ac130_overlay_grain").map(str::to_owned) {
                        let item = Item { background: grain, ..white };
                        self.menu_pic(out, sp, &item, 1.0);
                    }
                    return;
                }
                self.fof_boxes(out, sp, &v.targets);
                for item in self.data.menu("missilecam_hud_hd").to_vec() {
                    if !item.background.is_empty() {
                        self.menu_pic(out, sp, &item, 1.0);
                    } else {
                        self.menu_text(out, sp, &item, None, 1.0);
                    }
                }
            }
            ViewKind::Ac130 => {
                self.fof_boxes(out, sp, &v.targets);
                let weapon = (v.weapon as usize).min(2);
                for item in self.data.menu("ac130_hud_hd").to_vec() {
                    if let Some(i) = AC130_OVERLAYS.iter().position(|o| item.background.eq_ignore_ascii_case(o)) {
                        if i == weapon {
                            // The frame flickers while the gun reloads.
                            self.menu_pic(out, sp, &item, if v.reloading { flicker(ms) } else { 1.0 });
                        }
                        continue;
                    }
                    if !item.background.is_empty() {
                        self.menu_pic(out, sp, &item, 1.0);
                        continue;
                    }
                    let key = item.text.strip_prefix('@').unwrap_or("");
                    if let Some(i) = AC130_WEAPONS.iter().position(|k| k.eq_ignore_ascii_case(key)) {
                        self.menu_text(out, sp, &item, None, if i == weapon { flicker(ms) } else { 1.0 });
                    } else if key.eq_ignore_ascii_case("AC130_RELOADING") {
                        if v.reloading {
                            self.menu_text(out, sp, &item, None, flicker(ms));
                        }
                    } else if item.text.is_empty() {
                        // The time left (ui_ac130timer as secondsAsCountdown).
                        if v.seconds_left > 0.0 {
                            self.menu_text(out, sp, &item, Some(&countdown(v.seconds_left)), 1.0);
                        }
                    } else {
                        self.menu_text(out, sp, &item, None, 1.0);
                    }
                }
            }
            ViewKind::Chopper => {
                self.fof_boxes(out, sp, &v.targets);
                let lock = self.chopper_track(&v.targets, view);
                let blink = |rate: f32| match lock {
                    None => false,
                    Some(l) if l.locked => true,
                    Some(_) => rate <= 0.0 || ((ms.max(0) as f32 * rate / 1000.0) as i64) % 2 == 0,
                };
                for item in self.data.menu("remote_chopper_overlay_hd").to_vec() {
                    match item.owner_draw {
                        // The heading tape (cg paint_heading_tape: the image's s window around the heading).
                        145 => {
                            let Some(img) = self.image(&item.background) else { continue };
                            // -(yaw - north_yaw) / 360: IW4 yaws and GTA headings both grow counter-clockwise.
                            let centre = -v.heading / 360.0;
                            let a = sp.apply_rect(
                                item.rect[0],
                                item.rect[1],
                                item.rect[2],
                                item.rect[3],
                                item.align[0],
                                item.align[1],
                            );
                            // The host clamps texture coordinates: a window across the image's seam is two quads.
                            let s0 = (centre - TICKER_STRETCH * 0.5).rem_euclid(1.0);
                            let s1 = s0 + TICKER_STRETCH;
                            if s1 <= 1.0 {
                                out.push(st_quad(img, a.x, a.y, a.w, a.h, [s0, 0.0, s1, 1.0], item.color));
                            } else {
                                let w0 = a.w * (1.0 - s0) / TICKER_STRETCH;
                                out.push(st_quad(img, a.x, a.y, w0, a.h, [s0, 0.0, 1.0, 1.0], item.color));
                                out.push(st_quad(
                                    img,
                                    a.x + w0,
                                    a.y,
                                    a.w - w0,
                                    a.h,
                                    [0.0, 0.0, s1 - 1.0, 1.0],
                                    item.color,
                                ));
                            }
                            continue;
                        }
                        // The turret's reticle (its weapon's centre reticle at reticle_center_size).
                        190 => {
                            if let Some((img, size)) = self.vehicle_reticle.clone() {
                                let reticle = Item {
                                    background: img,
                                    rect: [-size * 0.5, -size * 0.5, size, size],
                                    align: [h::ALIGN_CENTER, h::ALIGN_CENTER],
                                    color: [1.0; 4],
                                    ..Default::default()
                                };
                                self.menu_pic(out, sp, &reticle, 1.0);
                            }
                            continue;
                        }
                        0 => {}
                        _ => continue, // 160 / 161: owner draws iw4 leaves blank here
                    }
                    let bg = item.background.to_ascii_lowercase();
                    if bg == "viper_locking_box" || (bg == "viper_locked_box" && item.align[0] == h::ALIGN_NOSCALE) {
                        // At the lock (weaplockscreenpos - (32, 42) px): the locking box while there is a
                        // target (weaplockblink(0)), the locked box once complete (weaplockblink(5) && weaplocked).
                        let Some(l) = lock else { continue };
                        let show = if bg == "viper_locking_box" { blink(0.0) } else { l.locked && blink(5.0) };
                        if show {
                            let mut it = item.clone();
                            it.rect[0] = l.at[0] - 32.0;
                            it.rect[1] = l.at[1] - 42.0;
                            self.menu_pic(out, sp, &it, 1.0);
                        }
                        continue;
                    }
                    if bg == "viper_locked_box" {
                        // The seeker box wandering around the centre while nothing is locked
                        // (-32 + sin(ms / 420) * 100, -32 + sin(ms / 490) * 100; !weaplockblink(11)).
                        if !blink(11.0) {
                            let mut it = item.clone();
                            it.rect[0] = -32.0 + (ms as f32 / 420.0).sin() * 100.0;
                            it.rect[1] = -32.0 + (ms as f32 / 490.0).sin() * 100.0;
                            self.menu_pic(out, sp, &it, 1.0);
                        }
                        continue;
                    }
                    if !item.background.is_empty() {
                        self.menu_pic(out, sp, &item, 1.0);
                    } else {
                        let alpha = if item.text.contains("WEAPON") { flicker(ms) } else { 1.0 };
                        self.menu_text(out, sp, &item, None, alpha);
                    }
                }
                if self.vehicle_reticle.is_none() {
                    self.centre_crosshair(out, sp);
                }
            }
            ViewKind::TargetMap => {
                for item in self.data.menu("targetmap_fullscreen").to_vec() {
                    if item.background.contains("scanlines") {
                        self.menu_pic(out, sp, &item, 1.0);
                    } else if item.text.contains("CONFIRM_TARGET") {
                        self.menu_text(out, sp, &item, None, 1.0);
                    }
                }
                self.centre_crosshair(out, sp);
            }
        }
        let _ = view;
    }

    /// The hostile nearest the crosshair (within CHOPPER_LOCK_RADIUS of the screen's height), held
    /// while it stays the nearest; locked after CHOPPER_LOCK_MS. None: no target near the centre.
    pub(super) fn chopper_track(&mut self, targets: &[[f32; 4]], view: View) -> Option<ChopperLock> {
        let (cx, cy) = (view.width * 0.5, view.height * 0.5);
        let reach = view.height * CHOPPER_LOCK_RADIUS;
        let near = targets
            .iter()
            .map(|t| [(t[0] + t[2]) * 0.5, (t[1] + t[3]) * 0.5])
            .map(|p| (p, (p[0] - cx).hypot(p[1] - cy)))
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| p);
        let Some(at) = near else {
            self.chopper_lock = None;
            return None;
        };
        let since = match self.chopper_lock {
            // The same target if it moved less than the reach since the last frame.
            Some((was, since)) if (was[0] - at[0]).hypot(was[1] - at[1]) < reach => since,
            _ => self.time_ms,
        };
        self.chopper_lock = Some((at, since));
        Some(ChopperLock { at, locked: self.time_ms.wrapping_sub(since) >= CHOPPER_LOCK_MS })
    }

    /// MW2's airstrike crosshair string ("><") at the centre of the screen.
    fn centre_crosshair(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some(text) = self.data.string("MP_WAR_AIRSTRIKE_CROSSHAIR").map(str::to_owned) else { return };
        let item = Item {
            rect: [0.0, 6.0, 0.0, 0.0],
            align: [h::ALIGN_CENTER, h::ALIGN_CENTER],
            color: [1.0, 1.0, 1.0, 0.8],
            text_scale: 0.4,
            font: 3,
            text_align: 1,
            ..Default::default()
        };
        self.item_text(out, sp, &item, &text, item.color);
    }

    /// Thermal FOF boxes on the hostiles the host found in view.
    fn fof_boxes(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, targets: &[[f32; 4]]) {
        let Some(img) = self.data.image("hud_fofbox_hostile").map(str::to_owned).and_then(|i| self.image(&i)) else {
            return;
        };
        let k = sp.scale_virtual_to_real[1];
        for t in targets {
            let (cx, cy) = ((t[0] + t[2]) * 0.5, (t[1] + t[3]) * 0.5);
            let height = ((t[0] - t[2]).powi(2) + (t[1] - t[3]).powi(2)).sqrt();
            let size = (height * FOF_ICON_SCALE).clamp(FOF_ICON_MIN * k, FOF_ICON_MAX * k);
            // The box image is red on black: MW2 adds it to the frame.
            let mut q = rect_quad(img, cx - size * 0.5, cy - size * 0.5, size, size, [1.0; 4]);
            q.flags |= QUAD_ADDITIVE;
            out.push(q);
        }
    }

    /// The nuke's countdown (top centre) and a crate's use hint with its capture bar (bottom centre).
    pub(super) fn streak_notices(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let v = self.streak_view.clone();
        let text_item = |y: f32, scale: f32, font: i32, color: [f32; 4]| Item {
            rect: [0.0, y, 0.0, 0.0],
            align: [h::ALIGN_CENTER, h::ALIGN_CENTER],
            color,
            text_scale: scale,
            font,
            text_align: 1,
            text_style: 3,
            ..Default::default()
        };
        if let Some(s) = v.nuke_seconds {
            let title = self.data.string("MP_TACTICAL_NUKE").unwrap_or("Tactical Nuke").to_owned();
            let item = text_item(-180.0, 0.5, 9, [1.0, 0.3, 0.2, 1.0]);
            self.item_text(out, sp, &item, &title, item.color);
            let item = text_item(-150.0, 0.6, 9, [1.0, 1.0, 1.0, 1.0]);
            self.item_text(out, sp, &item, &countdown(s), item.color);
        }
        if !v.hint.is_empty() {
            let text = self.data.string(&v.hint).map(|t| bind_keys(t, &self.key_label.clone()));
            if let Some(text) = text {
                let item = text_item(70.0, 0.375, 0, [1.0; 4]);
                self.item_text(out, sp, &item, &text, item.color);
            }
            if let Some(p) = v.hint_progress {
                // A use bar under the hint: black backing, white fill.
                let (w, hgt, y) = (120.0, 6.0, 80.0);
                if let Some(img) = self.image("$white") {
                    let back = [0.0, 0.0, 0.0, 0.5];
                    let mut q = pic(sp, img, [-w * 0.5, y, w, hgt], [h::ALIGN_CENTER, h::ALIGN_CENTER], back);
                    q.flags |= QUAD_SOLID;
                    out.push(q);
                    let mut q = pic(
                        sp,
                        img,
                        [-w * 0.5, y, w * p.clamp(0.0, 1.0), hgt],
                        [h::ALIGN_CENTER, h::ALIGN_CENTER],
                        [1.0; 4],
                    );
                    q.flags |= QUAD_SOLID;
                    out.push(q);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_helpers() {
        assert_eq!(countdown(35.2), "0:36");
        assert_eq!(countdown(-1.0), "0:00");
        assert_eq!(bind_keys("^3 [{+activate}] ^7 Toggle Thermal", "4"), "^3 [E] ^7 Toggle Thermal");
        assert!((flicker(0) - 1.0).abs() < 1e-6);
        assert!(flicker(141) < 0.4, "sin(pi/2) -> 0.35");
        assert_eq!(solid("$black"), Some([0.0; 3]));
    }
}
