//! MW2's HUD (F5b part 1) as textured quads for the host's 2D pass, from the
//! import's `mw2/hud/hud.json` (MW2's own weapon bar layout, fonts and
//! strings) and iw4L's HUD logic (`hud_iw4`):
//!
//! * the weapon bar (`weaponbar_hd`): its background, the spare rounds in
//!   MW2's HUD font, the frag icons, the clip's rounds (`clip_pip_*`), the
//!   weapon's name fading after a switch, and the low ammo warning
//!   ("Reload", "Low Ammo", "No Ammo") pulsing at the centre;
//! * the hit marker (`damage_feedback`, as maps/mp/gametypes/
//!   _damagefeedback.gsc places and fades it);
//! * the damage direction arrows (iw4L hud/src/damage_direction.rs with the
//!   cg_hudDamageIcon* defaults);
//! * the grenade indicator near live grenades (`hud_grenadeicon` and its
//!   pointer, the cg_hudGrenade* defaults);
//! * F5c: the killstreak splash (MW2's `killstreak` menu: "3 Kill Streak!",
//!   the earn hint and the reward's icon, sliding in from the left and out to
//!   the right as its OnOpen / OnCloseRequest lerps do) and the held reward
//!   in action slot 4 (`dpad_hd`: its icon and the key that uses it).
//!
//! Positions follow MW2's 640 x 480 virtual screen and its alignments
//! (`hud_iw4::ScreenPlacement`), in the backbuffer's pixels.
use codv_engine::Mw2Player;
use codv_engine::arms::GunFrame;
use codv_mw2data::hudfile::{self, Font, Hud, Item};
use codv_mw2data::table::Row;
use hud_iw4 as h;
use std::path::Path;

mod feedback;
mod javelin;
mod scorebar;
mod streaks;
pub use javelin::{Clu, Lock};
pub use scorebar::Score;
pub use streaks::{StreakView, ViewKind};

/// One textured quad: corners top-left, top-right, bottom-right, bottom-left (pixels).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Quad {
    pub image: u32,
    pub xy: [[f32; 2]; 4],
    pub st: [[f32; 2]; 4],
    pub color: [f32; 4],
    /// An alpha mask over the quad's rectangle (unrotated: corners (0,0), (1,0), (1,1), (0,1)):
    /// the compass letters fade with MW2's `compass_alpha`, as its multipass menu masks them.
    pub mask: Option<u32>,
    /// QUAD_SOLID (a plain fill: no image) / QUAD_ADDITIVE (added to the frame, as MW2's
    /// additive HUD materials: the thermal FOF box).
    pub flags: u32,
}

pub const QUAD_SOLID: u32 = 2;
pub const QUAD_ADDITIVE: u32 = 4;
/// MW2's splatter_alt: the image's alpha spread by the mask image's value against the quad's
/// alpha (the blood overlay's intensity): low mask values show first.
pub const QUAD_SPLATTER: u32 = 8;

/// What the weapon bar shows this frame.
#[derive(Clone, Debug, Default)]
pub struct Weapon<'a> {
    pub frame: Option<&'a GunFrame>,
    pub row: Option<&'a Row>,
    /// The frag's HUD icon image ("" none).
    pub frag_icon: String,
}

/// The host's view of the frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct View {
    pub width: f32,
    pub height: f32,
    /// The sniper scope covers the view.
    pub scoped: bool,
    /// The Javelin is fully aimed: its CLU view replaces the weapon bar and compass.
    pub javelin: Option<Clu>,
    /// GTA has the player in a vehicle: no weapon bar, sprint meter or compass (the gun is away and
    /// GTA's radar shows the way); the kill counter, killstreaks and notices stay.
    pub vehicle: bool,
    /// A cinematic has the view (the host's camera): the splashes only.
    pub cinematic: bool,
}

/// A splash waiting or showing: a killstreak earned (its name and the streak) or the host's own
/// (a title, a line under it and the title's glow), drawn in the same MW2 splash.
#[derive(Clone, Debug, PartialEq)]
pub enum Splash {
    Killstreak { name: String, streak: i32 },
    Custom { title: String, description: String, glow: [f32; 4] },
}

/// How long the host's own splash shows (MW2's killstreak splash: 2.5 s).
const CUSTOM_SPLASH_MS: i32 = 3000;

#[derive(Clone, Copy, Default)]
struct Hit {
    time: i32,
    yaw: f32,
    stun: bool,
}

/// cg_hudDamageIcon* (iw4L's defaults).
const DAMAGE_ICON_TIME_MS: i32 = 6000;
const DAMAGE_ICON_WIDTH: f32 = 128.0;
const DAMAGE_ICON_HEIGHT: f32 = 64.0;
const DAMAGE_ICON_OFFSET: f32 = 128.0;
/// cg_hudGrenade* (IW4's defaults): range in units, sizes in virtual units.
pub const GRENADE_ICON_RANGE: f32 = 250.0;
const GRENADE_ICON_OFFSET: f32 = 50.0;
const GRENADE_ICON_SIZE: f32 = 25.0;
const GRENADE_POINTER_WIDTH: f32 = 25.0;
const GRENADE_POINTER_HEIGHT: f32 = 12.0;
/// _damagefeedback.gsc: a 24 x 48 marker at (-12, -12) from the centre, fading over a second.
const HIT_MARKER_FADE_MS: i32 = 1000;
/// The Scavenger pickup icon's fade (feedbackDurationOverride 2.5).
const SCAVENGER_FADE_MS: i32 = 2500;

const OD_STOCK: i32 = 119;
const OD_FRAG: i32 = 103;
const OD_CLIP: i32 = 117;
const OD_WEAPON_NAME: i32 = 81;
const OD_LOW_AMMO: i32 = 120;
const OD_COMPASS_RING: i32 = 166;
/// Where north lies in IW4 yaw: GTA's north is +Y, IW4 yaw 90 (MW2 maps author theirs).
pub const NORTH_YAW: f32 = 90.0;
const WEAPON_NAME_RIGHT_INSET: f32 = 28.0;
/// The weapon icon over the name (virtual units): its height and the gap above the name's baseline.
const WEAPON_ICON_HEIGHT: f32 = 22.0;
const WEAPON_ICON_GAP: f32 = 16.0;
/// The killstreak menu's lerps: x from -640 and alpha from 0 over 0.15 s on open, the reverse to +640 on close.
const SPLASH_LERP_MS: i32 = 150;
const SPLASH_SLIDE: f32 = 640.0;
/// The held reward's action slot owner draw (dpad_hd).
const OD_ACTION_SLOT_4: i32 = 174;

pub struct Mw2Hud {
    data: Hud,
    /// Image names by quad image index (append-only, so indices stay valid for the host).
    images: Vec<String>,
    pub time_ms: i32,
    hits_seen: (u32, u32),
    hit_time: Option<i32>,
    damage: [Hit; 12],
    rng: u32,
    select: (u32, i32),
    /// F5c: killstreak splashes waiting (reference, streak) and the one showing (with its start).
    splash_queue: std::collections::VecDeque<Splash>,
    splash: Option<(Splash, i32)>,
    /// The held reward (its reference) and the key label that uses it ("4").
    pub held: Option<String>,
    pub key_label: String,
    /// The killstreak view the host is in this frame (cv_killstreak_view).
    pub streak_view: streaks::StreakView,
    /// When MW2's Scavenger last topped up the guns (its pickup icon fades over 2.5 s).
    scavenger_time: Option<i32>,
    /// CODV's sprint meter under the crosshair (MW2 has none): shown while it is not full.
    pub stamina_bar: bool,
    /// Since when the meter is full (it fades out then).
    stamina_full: Option<i32>,
    /// The Chopper Gunner's turret reticle (image, reticle_center_size): VehicleWeaponReticle.
    pub vehicle_reticle: Option<(String, f32)>,
    /// The Chopper Gunner's target lock: where the target was (pixels) and since when.
    chopper_lock: Option<([f32; 2], i32)>,
    /// CODV's kill counter (MW2's score bar art) and whether it shows.
    pub score: Score,
    pub scorebar_on: bool,
    /// The player's health (0..1) for the blood overlay, and the kill feedback's state.
    pub health: f32,
    pub blood_on: bool,
    pub feedback_on: bool,
    pub feedback: feedback::Feedback,
}

/// The sprint meter: segments under the weapon bar's line (virtual units from the bottom right,
/// as the weapon bar's items), fading out once full.
const STAMINA_RECT: [f32; 4] = [-232.0, -46.0, 120.0, 2.5];
const STAMINA_SEGMENTS: usize = 20;
const STAMINA_FADE_MS: i32 = 600;

impl Mw2Hud {
    /// None when the setup imported no HUD.
    pub fn load(root: Option<&Path>, log: &dyn Fn(&str)) -> Option<Mw2Hud> {
        let path = root?.join("mw2").join("hud").join("hud.json");
        let text = std::fs::read_to_string(&path).ok()?;
        match hudfile::from_json(&text) {
            Ok(data) => {
                log(&format!(
                    "mw2: HUD with {} fonts, {} weapon bar items, {} strings",
                    data.fonts.len(),
                    data.items.len(),
                    data.strings.len()
                ));
                Some(Mw2Hud::new(data))
            }
            Err(e) => {
                log(&format!("mw2: {}: {e}; the host's HUD stays", path.display()));
                None
            }
        }
    }

    pub fn new(data: Hud) -> Mw2Hud {
        Mw2Hud {
            data,
            images: Vec::new(),
            time_ms: 1,
            hits_seen: (0, 0),
            hit_time: None,
            damage: [Hit::default(); 12],
            rng: 0x1234_5678,
            select: (0, 0),
            splash_queue: std::collections::VecDeque::new(),
            splash: None,
            held: None,
            key_label: "4".into(),
            streak_view: streaks::StreakView::default(),
            scavenger_time: None,
            stamina_bar: true,
            stamina_full: None,
            vehicle_reticle: None,
            chopper_lock: None,
            score: Score::default(),
            scorebar_on: true,
            health: 1.0,
            blood_on: true,
            feedback_on: true,
            feedback: feedback::Feedback::default(),
        }
    }

    /// The killstreaks the import read: (reference, kills).
    pub fn killstreak_table(&self) -> Vec<(String, i32)> {
        self.data.killstreaks.iter().map(|k| (k.name.clone(), k.kills)).collect()
    }

    /// A killstreak's sound aliases: (earn jingle, earn dialog, use dialog).
    pub fn killstreak_sounds(&self, name: &str) -> Option<(String, String, String)> {
        let k = self.data.killstreak(name)?;
        Some((k.sound.clone(), k.earn_dialog.clone(), k.use_dialog.clone()))
    }

    /// `name` earned at `streak` kills: its splash (_hud_message.gsc killstreakSplashNotify), queued.
    pub fn killstreak_earned(&mut self, name: &str, streak: i32) {
        if self.data.killstreak(name).is_some() {
            self.splash_queue.push_back(Splash::Killstreak { name: name.to_owned(), streak });
        }
    }

    /// The host's own splash (a Juggernaut's entrance): MW2's killstreak splash with `title` (in
    /// `glow`) and `description`, queued after any killstreak's. False without the import's splash.
    pub fn custom_splash(&mut self, title: &str, description: &str, glow: [f32; 4]) -> bool {
        if self.data.splash_items.is_empty() {
            return false;
        }
        self.splash_queue.push_back(Splash::Custom {
            title: title.to_owned(),
            description: description.to_owned(),
            glow,
        });
        true
    }

    pub fn image_name(&self, index: u32) -> Option<&str> {
        self.images.get(index as usize).map(String::as_str)
    }

    /// Every image the HUD can draw (file stems), for the host to read ahead.
    pub fn known_images(&self) -> Vec<String> {
        let d = &self.data;
        let items =
            d.items.iter().chain(&d.splash_items).chain(&d.dpad_items).chain(d.menus.iter().flat_map(|(_, m)| m));
        let names = d
            .images
            .iter()
            .map(|(_, i)| i.as_str())
            .chain(d.fonts.iter().map(|f| f.image.as_str()))
            .chain(items.map(|i| i.background.as_str()))
            .chain(self.vehicle_reticle.iter().map(|(i, _)| i.as_str()));
        let mut out: Vec<String> = Vec::new();
        for n in names {
            let stem = codv_mw2data::vm::file_stem(&n.to_ascii_lowercase());
            if !stem.is_empty() && !stem.starts_with('_') && !out.contains(&stem) {
                out.push(stem);
            }
        }
        out
    }

    pub fn advance(&mut self, msec: u32) {
        self.time_ms = self.time_ms.wrapping_add(msec.min(1000) as i32).max(1);
    }

    /// MW2's Scavenger pickup: _damagefeedback.gsc's "scavenger_pickup" icon.
    pub fn scavenged(&mut self) {
        self.scavenger_time = Some(self.time_ms);
    }

    /// The hit counters from the weapon; a change starts the hit marker.
    pub fn hits(&mut self, hits: u32, kills: u32) {
        if (hits, kills) != self.hits_seen {
            self.hits_seen = (hits, kills);
            self.hit_time = Some(self.time_ms);
        }
    }

    /// Damage taken, from the direction a bullet travels (IW4 yaw, degrees).
    pub fn damaged(&mut self, yaw: f32, stun: bool) {
        let slot = self.damage.iter().enumerate().min_by_key(|(_, h)| h.time).map_or(0, |(i, _)| i);
        self.rng = self.rng.wrapping_mul(214_013).wrapping_add(2_531_011);
        let jitter = (((self.rng >> 16) & 0x7fff) as f32 / 32768.0 - 0.5) * 20.0;
        self.damage[slot] = Hit { time: self.time_ms, yaw: yaw + jitter, stun };
    }

    fn image(&mut self, name: &str) -> Option<u32> {
        if name.is_empty() {
            return None;
        }
        let name = codv_mw2data::vm::file_stem(&name.to_ascii_lowercase());
        if let Some(i) = self.images.iter().position(|n| *n == name) {
            return Some(i as u32);
        }
        self.images.push(name);
        Some(self.images.len() as u32 - 1)
    }

    /// This frame's quads. `grenades`: live grenades relative to the eye
    /// (IW4 units, x/y/z in the world's axes).
    pub fn draw(&mut self, player: &Mw2Player, weapon: &Weapon, view: View, grenades: &[[f32; 3]]) -> Vec<Quad> {
        let mut out = Vec::new();
        if view.width < 8.0 || view.height < 8.0 {
            return out;
        }
        let sp = h::ScreenPlacement::setup_fullscreen(view.width, view.height, 1.0);
        let ps = &player.ps;
        // A host cinematic has the view: only the splashes (a Juggernaut's title) over it.
        if view.cinematic {
            self.killstreak_splash(&mut out, &sp);
            self.kill_splash(&mut out, &sp);
            return out;
        }
        // F5c: a killstreak that takes the view (Predator, AC-130, Chopper Gunner, the airstrike's
        // map) shows its own overlay instead of the weapon bar and the crosshair-area cues.
        if self.streak_view.kind != streaks::ViewKind::None {
            self.streak_overlay(&mut out, &sp, view);
            self.streak_notices(&mut out, &sp);
            self.killstreak_splash(&mut out, &sp);
            self.score_popup(&mut out, &sp);
            self.kill_splash(&mut out, &sp);
            return out;
        }
        self.blood_overlay(&mut out, view);
        if let Some(clu) = view.javelin {
            self.javelin_view(&mut out, &sp, clu);
            self.hit_marker(&mut out, &sp);
            self.grenade_indicators(&mut out, &sp, ps.viewangles[1], grenades);
            self.streak_notices(&mut out, &sp);
            self.killstreak_splash(&mut out, &sp);
            self.score_popup(&mut out, &sp);
            self.kill_splash(&mut out, &sp);
            return out;
        }
        if !view.vehicle {
            self.weapon_bar(&mut out, &sp, weapon);
            self.compass(&mut out, &sp, ps.viewangles[1]);
        }
        self.scorebar(&mut out, &sp);
        self.round_over(&mut out, &sp);
        self.hit_marker(&mut out, &sp);
        self.scavenger_icon(&mut out, &sp);
        // The sprint meter and the cues pointed by MW2's view (its yaw stands still in a vehicle).
        if !view.vehicle {
            self.stamina(&mut out, &sp, player.sprint_left());
            let mantling = ps.pm_flags & playerstate_iw4::pm_flags::MANTLE != 0;
            if !view.scoped && !mantling {
                self.damage_directions(&mut out, &sp, ps.viewangles[1]);
            }
            self.grenade_indicators(&mut out, &sp, ps.viewangles[1], grenades);
        }
        self.reward_slot(&mut out, &sp);
        self.streak_notices(&mut out, &sp);
        self.killstreak_splash(&mut out, &sp);
        self.score_popup(&mut out, &sp);
        self.kill_splash(&mut out, &sp);
        out
    }

    // ---- killstreaks (F5c) ------------------------------------------------------

    /// The splash showing now: MW2's `killstreak` menu items (title with the
    /// splashTable glow, earn hint, icon), offset by the open / close slide.
    fn killstreak_splash(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        if self.splash.is_none() {
            let Some(next) = self.splash_queue.pop_front() else { return };
            self.splash = Some((next, self.time_ms));
        }
        let Some((splash, start)) = self.splash.clone() else { return };
        // What it shows: the title (and its glow), the line under it, the icon.
        let (title, hint, glow, icon, hold) = match &splash {
            Splash::Killstreak { name, streak } => {
                let Some(k) = self.data.killstreak(name).cloned() else {
                    self.splash = None;
                    return;
                };
                // A care package's reward (streak 0): its name as the title (splashTable "<name>_pickup").
                let title = if *streak <= 0 {
                    self.data.string(&k.title).map(str::to_owned)
                } else {
                    self.data.string(&k.splash_title).map(|t| h::splash_replace_optional(t, *streak))
                };
                let hint = self.data.string(&k.earned).map(|t| t.replace("{+actionslot 4}", &self.key_label));
                (title, hint, k.glow, self.data.image(&k.icon).map(str::to_owned), k.splash_ms)
            }
            Splash::Custom { title, description, glow } => {
                (Some(title.clone()), Some(description.clone()), *glow, None, CUSTOM_SPLASH_MS)
            }
        };
        let age = self.time_ms.wrapping_sub(start);
        let hold = hold.max(SPLASH_LERP_MS);
        let Some((dx, alpha)) = splash_motion(age, hold) else {
            self.splash = None;
            return;
        };
        let items = self.data.splash_items.clone();
        for item in &items {
            let mut item = item.clone();
            item.rect[0] += dx;
            let mut color = item.color;
            color[3] *= alpha;
            match item.name.as_str() {
                "killstreak_title" => {
                    if let Some(t) = &title {
                        let mut glow = glow;
                        glow[3] *= alpha;
                        self.item_text_glow(out, sp, &item, t, color, Some(glow));
                    }
                }
                "killstreak_description" => {
                    if let Some(t) = &hint {
                        self.item_text(out, sp, &item, t, color);
                    }
                }
                "killstreak_icon" => {
                    if let Some(img) = icon.as_ref().and_then(|i| self.image(i)) {
                        out.push(pic(sp, img, item.rect, item.align, color));
                    }
                }
                _ => {}
            }
        }
    }

    /// The held reward in action slot 4: its dpad icon and, under it, the key that uses it.
    fn reward_slot(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some(name) = self.held.clone() else { return };
        let Some(k) = self.data.killstreak(&name).cloned() else { return };
        let items = self.data.dpad_items.clone();
        for item in &items {
            if item.owner_draw == OD_ACTION_SLOT_4 {
                if let Some(img) = self.data.image(&k.dpad_icon).map(str::to_owned).and_then(|i| self.image(&i)) {
                    let mut q = pic(sp, img, item.rect, item.align, item.color);
                    // The killstreak dpad icons are flipbooks (MW2's dish turns, the jet banks...).
                    if let Some(a) = self.data.atlas(&k.dpad_icon) {
                        let st = atlas_frame(a, self.time_ms);
                        q.st = [[st[0], st[1]], [st[2], st[1]], [st[2], st[3]], [st[0], st[3]]];
                    }
                    out.push(q);
                }
            } else if item.owner_draw == 0 && !self.key_label.is_empty() {
                // dpad_hd: "^3" + KeyBinding("+actionslot 4"), in the item's yellow.
                let label = self.key_label.clone();
                self.item_text(out, sp, item, &label, item.color);
            }
        }
    }

    // ---- weapon bar -----------------------------------------------------------

    fn weapon_bar(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, weapon: &Weapon) {
        let (Some(f), Some(row)) = (weapon.frame, weapon.row) else { return };
        let clip_size = row.i("clip_size");
        // Older imports carry no ammo counter kind: a magazine.
        let counter = if row.num("ammo_counter_clip").is_some() { row.i("ammo_counter_clip") } else { 1 };
        let kind = h::ammo_counter_clip_kind(counter).unwrap_or(h::AmmoCounterClipKind::Magazine);
        let stock = f.stock.max(0);
        let frags = f.frags.max(0);
        if f.weapon != self.select.0 {
            self.select = (f.weapon, self.time_ms);
        }
        let items = self.data.items.clone();
        for item in &items {
            let value = |var: &str| match var {
                "stockAmmo" => stock as f32,
                "fragAmmo" => frags as f32,
                "clipAmmo" => f.clip as f32,
                _ => 1.0,
            };
            if let Some(t) = &item.visible
                && !t.passes(value(&t.variable))
            {
                continue;
            }
            match item.owner_draw {
                0 if is_compass_mask(&item.background) => {}
                0 => {
                    if let Some(img) = self.image(&item.background) {
                        out.push(pic(sp, img, item.rect, item.align, item.color));
                    }
                }
                OD_STOCK => {
                    if !matches!(kind, h::AmmoCounterClipKind::None | h::AmmoCounterClipKind::AltWeapon) {
                        self.item_text(out, sp, item, &format!("{stock:3}"), item.color);
                    }
                }
                OD_FRAG => {
                    if frags > 0
                        && let Some(img) = self.image(&weapon.frag_icon)
                    {
                        out.push(pic(sp, img, item.rect, item.align, item.color));
                    }
                }
                OD_CLIP => self.clip_rounds(out, sp, item, kind, f.clip, clip_size),
                OD_WEAPON_NAME => {
                    let Some(alpha) = h::fade_color(
                        self.time_ms,
                        self.select.1,
                        h::WEAPON_NAME_FADE_DURATION_MS,
                        h::WEAPON_NAME_FADE_TAIL_MS,
                    ) else {
                        continue;
                    };
                    let name = self.data.string(&row.display_name).unwrap_or(&row.display_name).to_owned();
                    let mut color = item.color;
                    color[3] *= alpha;
                    // CODV: the weapon's HUD icon over its name while the name shows (MW2's
                    // hud_icon_image, hud_icon_ratio 0 / 1 / 2: 1:1, 2:1, 4:1).
                    let icon = row.s("hud_icon_image").to_owned();
                    if let Some(img) = self.image(&icon) {
                        let h = WEAPON_ICON_HEIGHT;
                        let w = h * match row.num("hud_icon_ratio").unwrap_or(1.0) as i32 {
                            0 => 1.0,
                            2 => 4.0,
                            _ => 2.0,
                        };
                        let right = item.rect[0] + item.rect[2] - WEAPON_NAME_RIGHT_INSET;
                        out.push(pic(
                            sp,
                            img,
                            [right - w, item.rect[1] - WEAPON_ICON_GAP - h, w, h],
                            item.align,
                            color,
                        ));
                    }
                    let font = self.font_for(sp, item).cloned();
                    if let Some(font) = font {
                        let width = text_width(&font, &name, item.text_scale).trunc();
                        let x = item.rect[0] + item.rect[2] - width - WEAPON_NAME_RIGHT_INSET;
                        self.text(
                            out,
                            sp,
                            &font,
                            &name,
                            [x, item.rect[1]],
                            item.text_scale,
                            item.align,
                            color,
                            item.text_style,
                        );
                    }
                }
                OD_LOW_AMMO => {
                    let q = h::LowAmmoWarningQuery {
                        pm_type: 0,
                        e_flags: 0,
                        weapon: f.weapon,
                        ammo_counter_clip: counter,
                        weaponstate: [f.weaponstate, 0],
                        hands: 1,
                        clip: [f.clip, 0],
                        clip_size,
                        stock,
                        threshold: if row.num("low_ammo_warning_threshold").is_some() {
                            row.f("low_ammo_warning_threshold")
                        } else {
                            0.33
                        },
                        clip_only: row.b("clip_only"),
                    };
                    let Some(warning) = h::draw_player_weapon_low_ammo_warning(q) else { continue };
                    let Some(text) = self.data.string(warning.loc_key()).map(str::to_owned) else { continue };
                    let (c1, c2) = h::low_ammo_warning_color_pair(warning);
                    let c = h::vec4_lerp(c1, c2, h::low_ammo_warning_pulse_frac(self.time_ms));
                    self.item_text(out, sp, item, &text, c.map(|x| x.clamp(0.0, 1.0)));
                }
                _ => {}
            }
        }
    }

    /// MW2's compass (weaponbar_hd's compass passes): each `compass_alpha`
    /// item masks the letter rings after it, which turn with the view
    /// (iw4L paint_compass_ring: the ring's texture coordinates rotated by
    /// -(yaw - north) around their centre).
    fn compass(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, yaw: f32) {
        let items = self.data.items.clone();
        let mut mask = None;
        for item in &items {
            match item.owner_draw {
                0 if is_compass_mask(&item.background) => mask = self.image(&item.background),
                OD_COMPASS_RING => {
                    let Some(img) = self.image(&item.background) else { continue };
                    let mut q = pic(sp, img, item.rect, item.align, item.color);
                    q.st = rotate_st_corners(0.5, 0.5, 0.5, -(yaw - NORTH_YAW));
                    q.mask = mask;
                    out.push(q);
                }
                _ => {}
            }
        }
    }

    fn clip_rounds(
        &mut self,
        out: &mut Vec<Quad>,
        sp: &h::ScreenPlacement,
        item: &Item,
        kind: h::AmmoCounterClipKind,
        clip: i32,
        clip_size: i32,
    ) {
        let Some(m) = h::clip_pip_metrics(kind) else { return };
        let Some(img) = self.image(self.data.image(m.image).unwrap_or(m.image).to_owned().as_str()) else { return };
        let s = sp.scale_virtual_to_real;
        let m = h::ClipPipMetrics {
            width: m.width * s[0],
            height: m.height * s[1],
            step_x: m.step_x * s[0],
            step_y: m.step_y * s[1],
            ..m
        };
        let a = sp.apply_rect(item.rect[0], item.rect[1], 0.0, 0.0, item.align[0], item.align[1]);
        let base = [a.x, a.y];
        let empty = [
            h::CLIP_PIP_EMPTY_RGB,
            h::CLIP_PIP_EMPTY_RGB,
            h::CLIP_PIP_EMPTY_RGB,
            h::CLIP_PIP_EMPTY_ALPHA * item.color[3],
        ];
        for i in 0..clip_size.clamp(0, 100) {
            let [x, y] = if kind == h::AmmoCounterClipKind::Beltfed {
                h::clip_pip_belt_xy(m, base, i, clip_size, item.align[0])
            } else {
                h::clip_pip_grid_xy(m, base, i, item.align[0])
            };
            out.push(rect_quad(img, x, y, m.width, m.height, if i < clip { item.color } else { empty }));
        }
    }

    fn font_for(&self, sp: &h::ScreenPlacement, item: &Item) -> Option<&Font> {
        self.data.font(h::ui_get_font_handle(item.font, sp.scale_virtual_to_real[1], item.text_scale))
    }

    fn item_text(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, item: &Item, text: &str, color: [f32; 4]) {
        self.item_text_glow(out, sp, item, text, color, None);
    }

    /// An item's text; `glow`: the menu item's glowColor, a soft halo under the letters
    /// (IW4 draws it with the font's glow material; CODV spreads the glyphs a little).
    fn item_text_glow(
        &mut self,
        out: &mut Vec<Quad>,
        sp: &h::ScreenPlacement,
        item: &Item,
        text: &str,
        color: [f32; 4],
        glow: Option<[f32; 4]>,
    ) {
        let Some(font) = self.font_for(sp, item).cloned() else { return };
        let w = text_width(&font, text, item.text_scale);
        let (x, y) = h::item_text_origin(
            item.rect[0],
            item.rect[1],
            item.rect[2],
            item.rect[3],
            item.text_align,
            item.text_align_xy[0],
            item.text_align_xy[1],
            w,
            h::ui_text_height(item.text_scale),
        );
        if let Some(g) = glow.filter(|g| g[3] > 0.0) {
            let c = [g[0], g[1], g[2], g[3] * GLOW_ALPHA];
            for (ox, oy) in GLOW_OFFSETS {
                self.text(out, sp, &font, text, [x + ox, y + oy], item.text_scale, item.align, c, 0);
            }
        }
        self.text(out, sp, &font, text, [x, y], item.text_scale, item.align, color, item.text_style);
    }

    /// One line of text from its baseline origin (virtual units), as iw4L's draw2d lays glyphs.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        out: &mut Vec<Quad>,
        sp: &h::ScreenPlacement,
        font: &Font,
        text: &str,
        at: [f32; 2],
        text_scale: f32,
        align: [i32; 2],
        color: [f32; 4],
        style: i32,
    ) {
        let Some(img) = self.image(&font.image) else { return };
        let scale = h::normalized_text_scale(font.pixel_height, text_scale);
        let a = sp.apply_rect(at[0], at[1], scale, scale, align[0], align[1]);
        let (xs, ys) = (a.w, a.h);
        let shadow = h::text_drop_shadow_offset(h::draw_text_render_flags(style));
        let mut cursor = a.x;
        let mut line = a.y;
        let mut chars = text.chars().peekable();
        while let Some(letter) = h::next_letter(&mut chars) {
            // MW2's multi-line strings (the AC-130's side blocks): the next line one font height down.
            if letter == u32::from('\n') {
                cursor = a.x;
                line += font.pixel_height as f32 * ys;
                continue;
            }
            let Some(g) = font.glyph(letter) else { continue };
            let gx = cursor + f32::from(g.x0) * xs;
            let gy = line + f32::from(g.y0) * ys;
            let (gw, gh) = (f32::from(g.width) * xs, f32::from(g.height) * ys);
            if gw > 0.0 && gh > 0.0 {
                if let Some(off) = shadow {
                    out.push(st_quad(img, gx + off, gy + off, gw, gh, g.st, [0.0, 0.0, 0.0, color[3]]));
                }
                out.push(st_quad(img, gx, gy, gw, gh, g.st, color));
            }
            cursor += f32::from(g.dx) * xs;
        }
    }

    // ---- centre ---------------------------------------------------------------

    fn hit_marker(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some(t) = self.hit_time else { return };
        let age = self.time_ms.wrapping_sub(t);
        if !(0..HIT_MARKER_FADE_MS).contains(&age) {
            self.hit_time = None;
            return;
        }
        let Some(img) = self.data.image("damage_feedback").map(str::to_owned).and_then(|i| self.image(&i)) else {
            return;
        };
        let alpha = 1.0 - age as f32 / HIT_MARKER_FADE_MS as f32;
        out.push(pic(sp, img, [-12.0, -12.0, 24.0, 48.0], [h::ALIGN_CENTER, h::ALIGN_CENTER], [1.0, 1.0, 1.0, alpha]));
    }

    /// _damagefeedback.gsc updateDamageFeedback( "scavenger" ): a 64 x 32 icon at (-36, 32) from
    /// the centre, fading over 2.5 s.
    fn scavenger_icon(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement) {
        let Some(t) = self.scavenger_time else { return };
        let age = self.time_ms.wrapping_sub(t);
        if !(0..SCAVENGER_FADE_MS).contains(&age) {
            self.scavenger_time = None;
            return;
        }
        let Some(img) = self.data.image("scavenger_pickup").map(str::to_owned).and_then(|i| self.image(&i)) else {
            return;
        };
        let alpha = 1.0 - age as f32 / SCAVENGER_FADE_MS as f32;
        out.push(pic(sp, img, [-36.0, 32.0, 64.0, 32.0], [h::ALIGN_CENTER, h::ALIGN_CENTER], [1.0, 1.0, 1.0, alpha]));
    }

    /// CODV's sprint meter in the weapon bar's style: a row of segments under its line, right
    /// aligned with it (as the clip graphic's bullets), lit for the sprint left, red when nearly spent.
    fn stamina(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, left: f32) {
        if !self.stamina_bar {
            return;
        }
        let alpha = if left >= 0.999 {
            let since = *self.stamina_full.get_or_insert(self.time_ms);
            1.0 - self.time_ms.wrapping_sub(since) as f32 / STAMINA_FADE_MS as f32
        } else {
            self.stamina_full = None;
            1.0
        };
        if alpha <= 0.0 {
            return;
        }
        let Some(img) = self.image("$white") else { return };
        let [x, y, w, hgt] = STAMINA_RECT;
        let step = w / STAMINA_SEGMENTS as f32;
        let lit = (left * STAMINA_SEGMENTS as f32).ceil() as usize;
        let on = if left < 0.2 { [1.0, 0.35, 0.3, 0.85 * alpha] } else { [1.0, 1.0, 1.0, 0.7 * alpha] };
        let off = [1.0, 1.0, 1.0, 0.15 * alpha];
        for i in 0..STAMINA_SEGMENTS {
            let color = if i < lit { on } else { off };
            let mut q = pic(
                sp,
                img,
                [x + i as f32 * step, y, step - 1.0, hgt],
                [10, 10], /* the weapon bar items' bottom-right alignment */
                color,
            );
            q.flags |= QUAD_SOLID;
            out.push(q);
        }
    }

    fn damage_directions(&mut self, out: &mut Vec<Quad>, sp: &h::ScreenPlacement, view_yaw: f32) {
        let s = sp.scale_virtual_to_real;
        let half_width = DAMAGE_ICON_WIDTH * s[0] * 0.5;
        let offset = DAMAGE_ICON_OFFSET * s[1];
        let outer = offset + DAMAGE_ICON_HEIGHT * s[1];
        let centre = [sp.real_viewport_size[0] * 0.5, sp.real_viewport_size[1] * 0.5];
        for hit in self.damage {
            let age = self.time_ms.wrapping_sub(hit.time);
            if hit.time == 0 || age <= 0 || age >= DAMAGE_ICON_TIME_MS {
                continue;
            }
            let name = if hit.stun { "hit_direction_stun" } else { "hit_direction" };
            let Some(img) = self.data.image(name).map(str::to_owned).and_then(|i| self.image(&i)) else { continue };
            let (sin, cos) = (view_yaw - hit.yaw).to_radians().sin_cos();
            let xy = [[half_width, outer], [-half_width, outer], [-half_width, offset], [half_width, offset]]
                .map(|[x, y]| [centre[0] + cos * x - sin * y, centre[1] + sin * x + cos * y]);
            out.push(Quad {
                image: img,
                xy,
                st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                color: [1.0, 1.0, 1.0, (2.0 - 2.0 * age as f32 / DAMAGE_ICON_TIME_MS as f32).min(1.0)],
                mask: None,
                flags: 0,
            });
        }
    }

    fn grenade_indicators(
        &mut self,
        out: &mut Vec<Quad>,
        sp: &h::ScreenPlacement,
        view_yaw: f32,
        grenades: &[[f32; 3]],
    ) {
        let s = sp.scale_virtual_to_real;
        let centre = [sp.real_viewport_size[0] * 0.5, sp.real_viewport_size[1] * 0.5];
        let icon = self.data.image("hud_grenadeicon").map(str::to_owned).and_then(|i| self.image(&i));
        let pointer = self.data.image("hud_grenadepointer").map(str::to_owned).and_then(|i| self.image(&i));
        for g in grenades {
            let dist = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
            if !(1.0..GRENADE_ICON_RANGE).contains(&dist) {
                continue;
            }
            // Screen direction: ahead is up, left is left (IW4 yaw grows to the left).
            let rel = (g[1].atan2(g[0]).to_degrees() - view_yaw).to_radians();
            let dir = [-rel.sin(), -rel.cos()];
            let r = GRENADE_ICON_OFFSET * s[1];
            let at = [centre[0] + dir[0] * r, centre[1] + dir[1] * r];
            if let Some(img) = icon {
                let (w, hh) = (GRENADE_ICON_SIZE * s[0], GRENADE_ICON_SIZE * s[1]);
                out.push(rect_quad(img, at[0] - w * 0.5, at[1] - hh * 0.5, w, hh, [1.0; 4]));
            }
            if let Some(img) = pointer {
                // The arrow just beyond the icon, pointing away from the centre.
                let (hw, ph) = (GRENADE_POINTER_WIDTH * s[0] * 0.5, GRENADE_POINTER_HEIGHT * s[1]);
                let r0 = r + GRENADE_ICON_SIZE * s[1] * 0.5;
                let (sin, cos) = (rel.sin(), rel.cos());
                // Local frame: x across, y outwards (screen up when ahead).
                let xy = [[-hw, -(r0 + ph)], [hw, -(r0 + ph)], [hw, -r0], [-hw, -r0]]
                    .map(|[x, y]| [centre[0] + cos * x + sin * y, centre[1] - sin * x + cos * y]);
                out.push(Quad {
                    image: img,
                    xy,
                    st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    color: [1.0; 4],
                    mask: None,
                    flags: 0,
                });
            }
        }
    }
}

/// The glow halo: copies of the text around it (virtual units) at this fraction of the glow's alpha.
const GLOW_OFFSETS: [(f32, f32); 8] =
    [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7), (0.7, 0.7)];
const GLOW_ALPHA: f32 = 0.3;

/// A flipbook's frame rate on the HUD (frames per second).
const ATLAS_FPS: i32 = 20;

/// The texture rectangle (s0, t0, s1, t1) of a (rows, columns) flipbook's frame at `time_ms`.
pub fn atlas_frame(atlas: [u8; 2], time_ms: i32) -> [f32; 4] {
    let (rows, cols) = (i32::from(atlas[0].max(1)), i32::from(atlas[1].max(1)));
    let frame = (time_ms.max(0) / (1000 / ATLAS_FPS)) % (rows * cols);
    let (r, c) = (frame / cols, frame % cols);
    let (w, h) = (1.0 / cols as f32, 1.0 / rows as f32);
    [c as f32 * w, r as f32 * h, (c + 1) as f32 * w, (r + 1) as f32 * h]
}

/// The killstreak splash at `age` ms of a `hold` ms showing: its x offset
/// (virtual units) and alpha, None once it has slid out.
pub fn splash_motion(age: i32, hold: i32) -> Option<(f32, f32)> {
    let lerp = SPLASH_LERP_MS as f32;
    if age < 0 {
        return Some((-SPLASH_SLIDE, 0.0));
    }
    if age < SPLASH_LERP_MS {
        let t = age as f32 / lerp;
        return Some((-SPLASH_SLIDE * (1.0 - t), t));
    }
    if age < hold {
        return Some((0.0, 1.0));
    }
    let t = (age - hold) as f32 / lerp;
    (t < 1.0).then_some((SPLASH_SLIDE * t, 1.0 - t))
}

fn is_compass_mask(image: &str) -> bool {
    image.eq_ignore_ascii_case("compass_alpha")
}

/// iw4L hud draw2d `rotate_st_corners` (scale_final 1): texture corners
/// turned `deg` degrees around (`cs`, `ct`).
pub fn rotate_st_corners(cs: f32, ct: f32, r: f32, deg: f32) -> [[f32; 2]; 4] {
    let (sin, cos) = deg.to_radians().sin_cos();
    let step_s = [r * cos, r * sin];
    let step_t = [-r * sin, r * cos];
    [
        [cs - step_s[0] - step_t[0], ct - step_s[1] - step_t[1]],
        [cs + step_s[0] - step_t[0], ct + step_s[1] - step_t[1]],
        [cs + step_s[0] + step_t[0], ct + step_s[1] + step_t[1]],
        [cs - step_s[0] + step_t[0], ct - step_s[1] + step_t[1]],
    ]
}

/// The width of `text` in virtual units at `text_scale`.
pub fn text_width(font: &Font, text: &str, text_scale: f32) -> f32 {
    let scale = h::normalized_text_scale(font.pixel_height, text_scale);
    let mut chars = text.chars().peekable();
    let mut w = 0.0;
    while let Some(letter) = h::next_letter(&mut chars) {
        if let Some(g) = font.glyph(letter) {
            w += f32::from(g.dx);
        }
    }
    w * scale
}

fn st_quad(image: u32, x: f32, y: f32, w: f32, h: f32, st: [f32; 4], color: [f32; 4]) -> Quad {
    Quad {
        image,
        xy: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        st: [[st[0], st[1]], [st[2], st[1]], [st[2], st[3]], [st[0], st[3]]],
        color,
        mask: None,
        flags: 0,
    }
}

fn rect_quad(image: u32, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Quad {
    st_quad(image, x, y, w, h, [0.0, 0.0, 1.0, 1.0], color)
}

fn pic(sp: &h::ScreenPlacement, image: u32, rect: [f32; 4], align: [i32; 2], color: [f32; 4]) -> Quad {
    let a = sp.apply_rect(rect[0], rect[1], rect[2], rect[3], align[0], align[1]);
    rect_quad(image, a.x, a.y, a.w, a.h, color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codv_mw2data::hudfile::{Glyph, Test};

    fn hud() -> Hud {
        let glyphs: Vec<Glyph> = (32u16..127)
            .map(|c| Glyph { letter: c, x0: 0, y0: -30, dx: 20, width: 20, height: 30, st: [0.0, 0.0, 0.1, 0.1] })
            .collect();
        let font = |name: &str| Font {
            name: name.into(),
            pixel_height: 42,
            image: "gamefonts_pc".into(),
            glyphs: glyphs.clone(),
        };
        let item = |od: i32, rect: [f32; 4], font: i32, scale: f32| Item {
            owner_draw: od,
            rect,
            align: [10, 10],
            color: [1.0, 1.0, 1.0, 1.0],
            text_scale: scale,
            font,
            ..Default::default()
        };
        let mut low = item(OD_LOW_AMMO, [-50.0, -40.0, 100.0, 100.0], 9, 0.3333);
        low.align = [2, 2];
        low.text_align = 9;
        let mut stock = item(OD_STOCK, [-136.0, -52.67, 37.33, 0.67], 9, 0.5833);
        stock.visible = Some(Test { variable: "stockAmmo".into(), op: "<".into(), value: 100.0 });
        let mut bg = item(0, [-349.33, -96.0, 349.33, 56.0], 0, 0.55);
        bg.background = "hud_weaponbar".into();
        Hud {
            fonts: vec![font("fonts/hudbigfont"), font("fonts/hudsmallfont")],
            strings: vec![("PLATFORM_RELOAD".into(), "Reload".into()), ("WEAPON_M4_CARBINE".into(), "M4A1".into())],
            items: vec![
                bg,
                stock,
                item(OD_CLIP, [-130.0, -62.67, 0.0, 0.0], 0, 0.55),
                item(OD_WEAPON_NAME, [-102.67, -72.67, 37.33, 0.67], 10, 0.3333),
                low,
            ],
            images: vec![
                ("ammo_counter_bullet_mp".into(), "ammo_counter_bullet_mp".into()),
                ("damage_feedback".into(), "damage_feedback".into()),
                ("hit_direction".into(), "hit_direction".into()),
                ("hud_grenadeicon".into(), "grenadeicon_white".into()),
                ("hud_grenadepointer".into(), "grenadepointer_white".into()),
            ],
            ..Default::default()
        }
    }

    fn row() -> Row {
        Row {
            name: "m4_mp".into(),
            display_name: "WEAPON_M4_CARBINE".into(),
            numbers: vec![
                ("clip_size".into(), 30.0),
                ("ammo_counter_clip".into(), 1.0),
                ("low_ammo_warning_threshold".into(), 0.33),
            ],
            ..Default::default()
        }
    }

    fn view() -> View {
        View { width: 1920.0, height: 1080.0, ..Default::default() }
    }

    fn images(h: &Mw2Hud, quads: &[Quad], name: &str) -> usize {
        quads.iter().filter(|q| h.image_name(q.image) == Some(name)).count()
    }

    /// The Javelin fully aimed: MW2's CLU frame instead of the weapon bar; the seeker box
    /// without a lock, the target lines and LOCK lamp once locked.
    #[test]
    fn the_javelin_shows_its_clu_instead_of_the_weapon_bar() {
        let pic = |name: &str, bg: &str, rect: [f32; 4], align: i32| Item {
            name: name.into(),
            rect,
            align: [align, align],
            color: [1.0; 4],
            background: bg.into(),
            ..Default::default()
        };
        let mut data = hud();
        data.menus = vec![(
            "javelin_overlay_hd".into(),
            vec![
                pic("target_line_horz", "white", [0.0, 239.0, 4000.0, 2.0], 5),
                pic("target_line_vert", "white", [319.0, 0.0, 2.0, 4000.0], 5),
                pic("javelinlockbox", "hud_javelin_lock_box", [-40.0, -20.0, 80.0, 40.0], 2),
                pic("WeaponJavelinBG_mp", "hud_javelin_bg", [-427.0, -240.0, 854.0, 480.0], 2),
                pic("javelinlockon", "hud_javelin_lock_on", [154.0, -198.0, 64.0, 64.0], 2),
                pic("javelinrocket_on", "hud_javelin_rocket_on", [30.0, 144.0, 64.0, 64.0], 2),
                pic("javelinnorocket_on", "hud_javelin_norocket_on", [154.0, 144.0, 64.0, 64.0], 2),
            ],
        )];
        let mut hud = Mw2Hud::new(data);
        let player = Mw2Player::new([0.0; 3], 0.0);
        let frame = GunFrame { weapon: 3, clip: 1, stock: 2, ..Default::default() };
        let r = row();
        let w = Weapon { frame: Some(&frame), row: Some(&r), frag_icon: String::new() };
        let clu = |lock| View {
            javelin: Some(Clu { lock, at: [1200.0, 400.0], has_rocket: true, too_close: false }),
            ..view()
        };
        let idle = hud.draw(&player, &w, clu(Lock::None), &[]);
        assert_eq!(images(&hud, &idle, "hud_weaponbar"), 0, "no weapon bar in the CLU");
        for (img, n) in [("hud_javelin_bg", 1), ("hud_javelin_lock_box", 1), ("hud_javelin_lock_on", 0)] {
            assert_eq!(images(&hud, &idle, img), n, "{img}");
        }
        assert_eq!(images(&hud, &idle, "hud_javelin_rocket_on"), 1);
        assert_eq!(images(&hud, &idle, "hud_javelin_norocket_on"), 0);
        let locked = hud.draw(&player, &w, clu(Lock::Locked), &[]);
        assert_eq!(images(&hud, &locked, "hud_javelin_lock_box"), 0);
        assert_eq!(images(&hud, &locked, "hud_javelin_lock_on"), 1);
        // The lines cross at the target.
        let lines: Vec<&Quad> = locked.iter().filter(|q| q.flags & QUAD_SOLID != 0).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|q| (q.xy[0][1] - 400.0).abs() < 1.0), "{:?}", lines[0].xy);
        assert!(lines.iter().any(|q| (q.xy[0][0] - 1200.0).abs() < 1.0), "{:?}", lines[1].xy);
    }

    /// The Chopper Gunner: the heading tape scrolls with the camera (two quads across the image's
    /// seam), the turret's reticle sits at the centre, and a hostile held near the crosshair gets
    /// the locking box, then the locked box.
    #[test]
    fn the_chopper_gunner_draws_its_tape_reticle_and_lock() {
        let pic = |bg: &str, od: i32, rect: [f32; 4], align: i32| Item {
            owner_draw: od,
            rect,
            align: [align, align],
            color: [1.0; 4],
            background: bg.into(),
            ..Default::default()
        };
        let mut data = hud();
        data.menus = vec![(
            "remote_chopper_overlay_hd".into(),
            vec![
                pic("viper_heading_mp", 145, [-200.0, -190.0, 400.0, 20.0], 2),
                pic("viper_locked_box", 0, [-32.0, -32.0, 40.0, 40.0], 2),
                pic("viper_locking_box", 0, [-20.0, -20.0, 64.0, 64.0], 5),
                pic("viper_locked_box", 0, [-20.0, -20.0, 64.0, 64.0], 5),
                pic("", 190, [-320.0, -240.0, 640.0, 480.0], 0),
            ],
        )];
        let mut hud = Mw2Hud::new(data);
        hud.vehicle_reticle = Some(("m203_reticle".into(), 128.0));
        let player = Mw2Player::new([0.0; 3], 0.0);
        let w = Weapon::default();
        let view_at = |hud: &mut Mw2Hud, heading: f32, targets: Vec<[f32; 4]>| {
            hud.streak_view = StreakView { kind: ViewKind::Chopper, heading, targets, ..Default::default() };
            hud.draw(&player, &w, view(), &[])
        };
        // North: the window [-0.25, 0.25] crosses the seam.
        let q = view_at(&mut hud, 0.0, vec![]);
        let tape: Vec<&Quad> = q.iter().filter(|x| hud.image_name(x.image) == Some("viper_heading_mp")).collect();
        assert_eq!(tape.len(), 2);
        assert!((tape[0].st[0][0] - 0.75).abs() < 1e-4 && (tape[1].st[1][0] - 0.25).abs() < 1e-4);
        let q = view_at(&mut hud, -90.0, vec![]);
        let tape: Vec<&Quad> = q.iter().filter(|x| hud.image_name(x.image) == Some("viper_heading_mp")).collect();
        assert_eq!(tape.len(), 1);
        assert!((tape[0].st[0][0] - 0.0).abs() < 1e-4 && (tape[0].st[1][0] - 0.5).abs() < 1e-4);
        assert_eq!(images(&hud, &q, "m203_reticle"), 1);
        assert_eq!(images(&hud, &q, "viper_locking_box"), 0, "no target: no lock");
        // A hostile just under the crosshair: locking, then locked after half a second.
        let near = vec![[965.0, 580.0, 965.0, 520.0]];
        let q = view_at(&mut hud, 0.0, near.clone());
        assert_eq!(images(&hud, &q, "viper_locking_box"), 1);
        hud.advance(600);
        let q = view_at(&mut hud, 0.0, near);
        assert_eq!(images(&hud, &q, "viper_locking_box"), 1);
        assert!(images(&hud, &q, "viper_locked_box") >= 1);
        // Far from the crosshair: no lock.
        let q = view_at(&mut hud, 0.0, vec![[200.0, 900.0, 200.0, 840.0]]);
        assert_eq!(images(&hud, &q, "viper_locking_box"), 0);
    }

    /// In a vehicle: the kill counter stays, the weapon bar and the sprint meter go.
    #[test]
    fn a_vehicle_keeps_the_counter_but_not_the_guns_hud() {
        let mut data = hud();
        data.menus = vec![(
            "scorebar_hd".into(),
            vec![Item {
                rect: [0.0, -82.67, 341.33, 42.67],
                align: [8, 10],
                color: [1.0; 4],
                background: "hud_scorebar".into(),
                ..Default::default()
            }],
        )];
        let mut hud = Mw2Hud::new(data);
        let player = Mw2Player::new([0.0; 3], 0.0);
        let frame = GunFrame { weapon: 3, clip: 20, stock: 60, ..Default::default() };
        let r = row();
        let w = Weapon { frame: Some(&frame), row: Some(&r), frag_icon: String::new() };
        let on_foot = hud.draw(&player, &w, view(), &[]);
        assert_eq!(images(&hud, &on_foot, "hud_weaponbar"), 1);
        assert_eq!(images(&hud, &on_foot, "hud_scorebar"), 1);
        let seat = hud.draw(&player, &w, View { vehicle: true, ..view() }, &[]);
        assert_eq!(images(&hud, &seat, "hud_weaponbar"), 0);
        assert_eq!(images(&hud, &seat, "ammo_counter_bullet_mp"), 0);
        assert_eq!(images(&hud, &seat, "hud_scorebar"), 1);
    }

    #[test]
    fn the_sprint_meter_shows_until_full_then_fades() {
        let mut hud = Mw2Hud::new(hud());
        let sp = h::ScreenPlacement::setup_fullscreen(1920.0, 1080.0, 1.0);
        let mut out = Vec::new();
        hud.stamina(&mut out, &sp, 0.5);
        assert_eq!(out.len(), STAMINA_SEGMENTS);
        assert!(out.iter().all(|q| q.flags & QUAD_SOLID != 0));
        let lit = out.iter().filter(|q| q.color[3] > 0.5).count();
        assert_eq!(lit, STAMINA_SEGMENTS / 2);
        // Bottom right, under the weapon bar.
        assert!(out.iter().all(|q| q.xy[0][0] > 1920.0 * 0.6 && q.xy[0][1] > 1080.0 * 0.85));
        out.clear();
        hud.stamina(&mut out, &sp, 0.1);
        assert!(out[0].color[1] < 0.5, "nearly spent: red");
        out.clear();
        hud.stamina(&mut out, &sp, 1.0);
        assert_eq!(out.len(), STAMINA_SEGMENTS, "full: still there, fading");
        hud.advance(700);
        out.clear();
        hud.stamina(&mut out, &sp, 1.0);
        assert!(out.is_empty(), "faded out");
        hud.stamina_bar = false;
        hud.stamina(&mut out, &sp, 0.2);
        assert!(out.is_empty());
    }

    #[test]
    fn the_weapon_bar_shows_rounds_spare_ammo_and_the_name() {
        let mut hud = Mw2Hud::new(hud());
        let player = Mw2Player::new([0.0; 3], 0.0);
        let frame = GunFrame { weapon: 3, clip: 20, stock: 60, ..Default::default() };
        let r = row();
        let w = Weapon { frame: Some(&frame), row: Some(&r), frag_icon: String::new() };
        let q = hud.draw(&player, &w, view(), &[]);
        assert_eq!(images(&hud, &q, "hud_weaponbar"), 1);
        // 30 rounds, 20 full: bottom right, right to left.
        let rounds: Vec<&Quad> =
            q.iter().filter(|x| hud.image_name(x.image) == Some("ammo_counter_bullet_mp")).collect();
        assert_eq!(rounds.len(), 30);
        assert_eq!(rounds.iter().filter(|r| r.color[3] == 1.0).count(), 20);
        assert!(rounds.iter().all(|r| r.xy[0][0] > 1300.0 && r.xy[0][1] > 900.0), "{:?}", rounds[0].xy);
        assert!(rounds[1].xy[0][0] < rounds[0].xy[0][0]);
        // " 60" and "M4A1" (with shadows) in the font atlas; no warning at 20 / 30.
        let glyphs = images(&hud, &q, "gamefonts_pc");
        assert!(glyphs >= 2 + 4, "{glyphs}");
        // Reload warning once the clip is low; it pulses in the centre.
        let frame = GunFrame { weapon: 3, clip: 5, stock: 60, ..Default::default() };
        let w = Weapon { frame: Some(&frame), ..w };
        let before = images(&hud, &q, "gamefonts_pc");
        let q2 = hud.draw(&player, &w, view(), &[]);
        assert!(images(&hud, &q2, "gamefonts_pc") > before);
        let centre: Vec<&Quad> = q2
            .iter()
            .filter(|x| hud.image_name(x.image) == Some("gamefonts_pc") && (x.xy[0][0] - 960.0).abs() < 200.0)
            .collect();
        assert!(!centre.is_empty());
    }

    #[test]
    fn the_compass_turns_with_the_view_under_its_mask() {
        let mut data = hud();
        let ring = |name: &str, od: i32, bg: &str| Item {
            name: name.into(),
            owner_draw: od,
            rect: [-109.0, -118.0, 126.0, 126.0],
            align: [10, 10],
            color: [1.0; 4],
            background: bg.into(),
            ..Default::default()
        };
        data.items.push(ring("Pass1", 0, "compass_alpha"));
        data.items.push(ring("Pass2", OD_COMPASS_RING, "compass_letters"));
        let mut hud = Mw2Hud::new(data);
        let rings = |hud: &mut Mw2Hud, yaw: f32| {
            let player = Mw2Player::new([0.0; 3], yaw);
            let q = hud.draw(&player, &Weapon::default(), view(), &[]);
            assert_eq!(images(hud, &q, "compass_alpha"), 0, "the mask is not drawn");
            q.into_iter().filter(|x| hud.image_name(x.image) == Some("compass_letters")).collect::<Vec<_>>()
        };
        // Facing north (+Y, IW4 yaw 90): the letters upright, masked.
        let north = rings(&mut hud, NORTH_YAW);
        assert_eq!(north.len(), 1);
        assert!(north[0].mask.is_some_and(|m| hud.image_name(m) == Some("compass_alpha")));
        assert!((north[0].st[0][0]).abs() < 1e-4 && (north[0].st[0][1]).abs() < 1e-4, "{:?}", north[0].st);
        // Facing east (yaw 0): the ring a quarter turn round.
        let east = rings(&mut hud, 0.0);
        assert!((east[0].st[0][0] - 1.0).abs() < 1e-4 && east[0].st[0][1].abs() < 1e-4, "{:?}", east[0].st);
    }

    #[test]
    fn hits_damage_and_grenades_show_around_the_centre() {
        let mut hud = Mw2Hud::new(hud());
        let player = Mw2Player::new([0.0; 3], 0.0);
        let w = Weapon::default();
        hud.hits(1, 0);
        let q = hud.draw(&player, &w, view(), &[]);
        let marker: Vec<&Quad> = q.iter().filter(|x| hud.image_name(x.image) == Some("damage_feedback")).collect();
        assert_eq!(marker.len(), 1);
        assert!((marker[0].xy[0][0] - (960.0 - 12.0 * 2.25)).abs() < 0.5 && marker[0].color[3] > 0.99);
        hud.advance(1500);
        let q = hud.draw(&player, &w, view(), &[]);
        assert_eq!(images(&hud, &q, "damage_feedback"), 0, "faded");
        // Shot from ahead (the bullet travels back towards the player: yaw 180): the arrow is above the centre.
        hud.damaged(180.0, false);
        hud.advance(16);
        let q = hud.draw(&player, &w, view(), &[]);
        let arrow: Vec<&Quad> = q.iter().filter(|x| hud.image_name(x.image) == Some("hit_direction")).collect();
        assert_eq!(arrow.len(), 1);
        let cy = arrow[0].xy.iter().map(|p| p[1]).sum::<f32>() / 4.0;
        assert!(cy < 540.0 - 100.0, "{cy}");
        // A grenade to the left, close by: its icon left of the centre.
        let q = hud.draw(&player, &w, view(), &[[0.0, 100.0, 0.0]]);
        let icon: Vec<&Quad> = q.iter().filter(|x| hud.image_name(x.image) == Some("grenadeicon_white")).collect();
        assert_eq!(icon.len(), 1);
        assert!(icon[0].xy[1][0] < 960.0);
        let q = hud.draw(&player, &w, view(), &[[0.0, 1000.0, 0.0]]);
        assert_eq!(images(&hud, &q, "grenadeicon_white"), 0, "far away");
    }

    #[test]
    fn a_killstreak_slides_in_and_out_and_waits_in_the_action_slot() {
        let mut data = hud();
        data.strings.extend([
            ("MP_KILLSTREAK_N".to_owned(), "&&1 Kill Streak!".to_owned()),
            ("MP_EARNED_UAV".to_owned(), "Press [{+actionslot 4}] for UAV.".to_owned()),
        ]);
        data.images.extend([
            ("specialty_uav".to_owned(), "specialty_uav".to_owned()),
            ("dpad_killstreak_uav".to_owned(), "dpad_ks_uav".to_owned()),
        ]);
        data.killstreaks.push(hudfile::Killstreak {
            name: "uav".into(),
            kills: 3,
            earned: "MP_EARNED_UAV".into(),
            splash_title: "MP_KILLSTREAK_N".into(),
            splash_ms: 2500,
            glow: [0.25, 0.75, 0.25, 1.0],
            icon: "specialty_uav".into(),
            dpad_icon: "dpad_killstreak_uav".into(),
            ..Default::default()
        });
        let text = |name: &str, y: f32, font: i32, scale: f32| Item {
            name: name.into(),
            rect: [0.0, y, 0.0, 0.0],
            align: [2, 2],
            color: [1.0; 4],
            text_scale: scale,
            font,
            text_align: 1,
            text_style: 3,
            ..Default::default()
        };
        data.splash_items = vec![
            text("killstreak_title", -110.0, 9, 0.5833),
            text("killstreak_description", -90.0, 10, 0.375),
            Item {
                name: "killstreak_icon".into(),
                rect: [-24.0, -190.0, 48.0, 48.0],
                align: [2, 2],
                color: [1.0; 4],
                ..Default::default()
            },
        ];
        data.dpad_items = vec![
            Item {
                owner_draw: OD_ACTION_SLOT_4,
                rect: [-61.0, -93.0, 32.0, 32.0],
                align: [10, 10],
                color: [1.0; 4],
                ..Default::default()
            },
            Item {
                rect: [-61.0, -93.0, 32.0, 32.0],
                align: [10, 10],
                color: [1.0, 1.0, 0.0, 1.0],
                text_scale: 0.25,
                font: 10,
                text_align: 1,
                text_align_xy: [0.0, 35.0],
                ..Default::default()
            },
        ];
        let mut hud = Mw2Hud::new(data);
        let player = Mw2Player::new([0.0; 3], 0.0);
        let w = Weapon::default();
        hud.killstreak_earned("uav", 3);
        hud.killstreak_earned("nonesuch", 4);
        let icon_x = |hud: &mut Mw2Hud| {
            let q = hud.draw(&player, &w, view(), &[]);
            q.iter().find(|x| hud.image_name(x.image) == Some("specialty_uav")).map(|x| (x.xy[0][0], x.color[3]))
        };
        // Opening: off to the left and transparent, then centred (48 units wide around the middle).
        let (x0, a0) = icon_x(&mut hud).unwrap();
        assert!(x0 < 0.0 && a0 < 0.05, "{x0} {a0}");
        hud.advance(1000);
        let (x1, a1) = icon_x(&mut hud).unwrap();
        assert!((x1 - (960.0 - 24.0 * 2.25)).abs() < 1.0 && a1 == 1.0, "{x1}");
        // The title with its glow: glyph quads in the glow's green.
        let q = hud.draw(&player, &w, view(), &[]);
        assert!(q.iter().any(|x| x.color[0] == 0.25 && x.color[1] == 0.75));
        hud.advance(1000);
        hud.advance(580);
        let (x2, _) = icon_x(&mut hud).unwrap();
        assert!(x2 > x1, "closing slides right");
        hud.advance(200);
        assert!(icon_x(&mut hud).is_none(), "gone; the unknown streak never shows");
        // Held: the dpad icon and the key under it.
        hud.held = Some("uav".into());
        let q = hud.draw(&player, &w, view(), &[]);
        assert_eq!(images(&hud, &q, "dpad_ks_uav"), 1);
        assert!(q.iter().any(|x| x.color == [1.0, 1.0, 0.0, 1.0] && hud.image_name(x.image) == Some("gamefonts_pc")));
        assert_eq!(atlas_frame([4, 8], 0), [0.0, 0.0, 0.125, 0.25]);
        assert_eq!(atlas_frame([4, 8], 9 * 50), [0.125, 0.25, 0.25, 0.5]);
        assert_eq!(splash_motion(-5, 2500), Some((-640.0, 0.0)));
        assert_eq!(splash_motion(75, 2500), Some((-320.0, 0.5)));
        assert_eq!(splash_motion(2650, 2500), None);
        // The host's own splash (a Juggernaut): the title in its glow, and in a cinematic the splash alone.
        assert!(hud.custom_splash("JUGGERNAUT", "Enemy Juggernaut inbound", [1.0, 0.5, 0.1, 1.0]));
        let _ = hud.draw(&player, &w, View { cinematic: true, ..view() }, &[]); // it starts showing
        hud.advance(1000);
        let q = hud.draw(&player, &w, View { cinematic: true, ..view() }, &[]);
        assert!(q.iter().any(|x| x.color[0] == 1.0 && x.color[1] == 0.5), "the title in its glow");
        assert_eq!(images(&hud, &q, "dpad_ks_uav"), 0, "a cinematic: the splash only");
        let q = hud.draw(&player, &w, view(), &[]);
        assert_eq!(images(&hud, &q, "dpad_ks_uav"), 1, "the HUD as usual outside it");
        for _ in 0..4 {
            hud.advance(1000); // (a step is at most 1 s)
        }
        let q = hud.draw(&player, &w, View { cinematic: true, ..view() }, &[]);
        assert!(q.is_empty(), "gone after its 3 s");
    }
}

#[cfg(test)]
mod preview {
    use super::*;

    /// Dumps a frame of MW2's HUD from the player's import (CODV_MW2_OUT;
    /// CODV_HUD_DUMP = output file) for a visual check; skipped otherwise.
    #[test]
    fn dump_a_frame() {
        let (Ok(dir), Ok(dump)) = (std::env::var("CODV_MW2_OUT"), std::env::var("CODV_HUD_DUMP")) else { return };
        let dir = std::path::Path::new(&dir);
        let data = hudfile::from_json(&std::fs::read_to_string(dir.join("hud/hud.json")).unwrap()).unwrap();
        let table =
            codv_mw2data::table::from_json(&std::fs::read_to_string(dir.join("weapons.json")).unwrap()).unwrap();
        let row = table.find("m4_mp").unwrap().clone();
        let frag = table.find("frag_grenade_mp").map(|r| r.s("hud_icon_image").to_owned()).unwrap_or_default();
        let mut hud = Mw2Hud::new(data);
        let player = Mw2Player::new([0.0; 3], 120.0);
        let frame = GunFrame { weapon: 7, clip: 7, stock: 180, frags: 2, ..Default::default() };
        let w = Weapon { frame: Some(&frame), row: Some(&row), frag_icon: frag };
        hud.hits(1, 0);
        hud.damaged(135.0, false);
        if let Ok(kind) = std::env::var("CODV_HUD_VIEW") {
            hud.streak_view = StreakView {
                kind: match kind.as_str() {
                    "predator" => ViewKind::Predator,
                    "ac130" => ViewKind::Ac130,
                    "chopper" => ViewKind::Chopper,
                    _ => ViewKind::TargetMap,
                },
                weapon: 1,
                reloading: true,
                seconds_left: 31.0,
                nuke_seconds: Some(7.0),
                hint: "MP_UAV_PICKUP".into(),
                hint_progress: Some(0.4),
                targets: vec![
                    [700.0, 700.0, 700.0, 640.0],
                    [1200.0, 500.0, 1205.0, 470.0],
                    [975.0, 590.0, 975.0, 530.0],
                ],
                heading: 37.0,
                ..Default::default()
            };
            hud.vehicle_reticle = table
                .find(codv_mw2data::KILLSTREAK_TURRETS[0])
                .map(|r| (r.s("reticle_center_image").to_owned(), r.f("reticle_center_size")))
                .filter(|(i, _)| !i.is_empty());
            // CODV_HUD_LOCK: hold the target near the crosshair until the Chopper Gunner's lock completes.
            if std::env::var("CODV_HUD_LOCK").is_ok() {
                let _ = hud.draw(&player, &w, View { width: 1920.0, height: 1080.0, ..Default::default() }, &[]);
                hud.advance(600);
            }
        }
        let javelin = std::env::var("CODV_HUD_JAVELIN").ok().map(|l| Clu {
            lock: match l.as_str() {
                "locked" => Lock::Locked,
                "locking" => Lock::Locking,
                _ => Lock::None,
            },
            at: [1150.0, 470.0],
            has_rocket: true,
            too_close: std::env::var("CODV_HUD_TOO_CLOSE").is_ok(),
        });
        if std::env::var("CODV_HUD_STREAK").is_ok() {
            hud.killstreak_earned("uav", 3);
            hud.held = Some("uav".into());
            let _ = hud.draw(&player, &w, View { width: 1920.0, height: 1080.0, ..Default::default() }, &[]);
            hud.advance(1000);
        }
        if let Ok(spec) = std::env::var("CODV_HUD_SCORE") {
            // "total,life,deaths,next[,died]": the kill counter (and "Round Over" after a death).
            let v: Vec<i64> = spec.split(',').filter_map(|x| x.parse().ok()).collect();
            hud.score.kills(v[0] as u32);
            if v.len() > 4 && v[4] != 0 {
                hud.score.died();
                hud.score.kills(0);
            }
            hud.score.life = v[1] as u32;
            hud.score.deaths = v[2] as u32;
            hud.score.next = (v[3] > 0).then_some(v[3] as i32);
        }
        if let Ok(spec) = std::env::var("CODV_HUD_KILLS") {
            // "kills,headshots,longshots[,health]": the kill feedback (and the blood at that health).
            let v: Vec<f32> = spec.split(',').filter_map(|x| x.parse().ok()).collect();
            hud.kill_feedback(v[0] as u32, v[1] as u32, v[2] as u32);
            if let Some(h) = v.get(3) {
                hud.health = *h;
            }
            let _ = hud.draw(&player, &w, View { width: 1920.0, height: 1080.0, ..Default::default() }, &[]);
            hud.advance(450);
        }
        hud.advance(200);
        let mut q = hud.draw(
            &player,
            &w,
            View { width: 1920.0, height: 1080.0, javelin, ..Default::default() },
            &[[60.0, -80.0, -40.0]],
        );
        if let Ok(left) = std::env::var("CODV_HUD_STAMINA") {
            let sp = h::ScreenPlacement::setup_fullscreen(1920.0, 1080.0, 1.0);
            hud.stamina(&mut q, &sp, left.parse().unwrap_or(0.5));
        }
        let mut out = String::from("[\n");
        for (i, x) in q.iter().enumerate() {
            out += &format!(
                "{{\"image\":\"{}\",\"mask\":\"{}\",\"xy\":{:?},\"st\":{:?},\"color\":{:?}}}{}\n",
                hud.image_name(x.image).unwrap(),
                x.mask.and_then(|m| hud.image_name(m)).unwrap_or(""),
                x.xy,
                x.st,
                x.color,
                if i + 1 < q.len() { "," } else { "" }
            );
        }
        out += "]\n";
        std::fs::write(dump, out).unwrap();
    }
}
