//! The player's Call of Duty-style handling, one script frame at a time.
//!
//! GTA keeps the body: walking, the first-person camera and view model, aim
//! zoom, reload animations. CODV takes the decisions that make the feel:
//! when a shot leaves the barrel (fire rate, burst, semi-auto), where it goes
//! (hip / ADS spread with movement, air, crouch and bloom), how the view kicks,
//! magazines and reload timing, quick weapon swaps with raise and drop times,
//! a limited sprint with sprint-out delay, an ADS move slowdown, crouch, the
//! quick melee, quick jump and regenerating health.
//!
//! Pure logic: the host turns `FrameOut` into natives and reports hits back.
use crate::defs::{FireMode, Table, WeaponDef};

pub const FIRE: u32 = 1 << 0;
pub const ADS: u32 = 1 << 1;
pub const SPRINT: u32 = 1 << 2;
pub const RELOAD: u32 = 1 << 3;
pub const SWAP: u32 = 1 << 4;
pub const MELEE: u32 = 1 << 5;
pub const CROUCH: u32 = 1 << 6;
pub const JUMP: u32 = 1 << 7;

/// A semi-auto press this close before the gun is ready still fires.
const SEMI_BUFFER: f32 = 0.12;
/// Kicks reach the camera over a few frames rather than in one jump.
const KICK_RATE: f32 = 28.0;
/// Firing pauses this long before the recoverable part of the kick returns.
const RECOVER_PAUSE: f32 = 0.12;
/// Timers within this of zero count as expired (float drift over frames).
const EPS: f32 = 1e-4;

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameIn {
    pub dt: f32,
    pub held: u32,
    /// On foot, not in a vehicle, not ragdolled, not in a cutscene or menu.
    pub on_foot: bool,
    pub airborne: bool,
    /// GTA's sprint is running (IS_PED_SPRINTING).
    pub sprinting: bool,
    /// Horizontal speed, m/s.
    pub speed: f32,
    /// The weapon GTA has in the player's hands (joaat).
    pub gta_weapon: u32,
    /// Every round the player carries for that weapon (GET_AMMO_IN_PED_WEAPON).
    pub gta_ammo: u32,
    /// Health as a fraction of max, and how much was lost since last frame.
    pub health: f32,
    pub health_lost: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub weapon: usize,
    /// Offsets from the view direction, degrees (spread).
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum State {
    /// No CODV weapon in hand, or not on foot.
    Idle = 0,
    Ready = 1,
    Firing = 2,
    Reloading = 3,
    Raising = 4,
    Dropping = 5,
    Melee = 6,
    Sprinting = 7,
}

#[derive(Clone, Debug, Default)]
pub struct Hud {
    pub mag: u32,
    pub reserve: u32,
    pub weapon: Option<usize>,
    /// Current cone half-angle, degrees.
    pub spread: f32,
    pub ads: f32,
    pub sprint: f32,
    pub reload: f32,
    /// 0..1, fades after a hit; `kill` when the last hit killed.
    pub hitmarker: f32,
    pub kill: bool,
    pub headshot: bool,
    pub damage_flash: f32,
    pub crouched: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FrameOut {
    pub state: Option<State>,
    pub shots: Vec<Shot>,
    /// Degrees to add to the camera this frame (pitch up, yaw right).
    pub kick: [f32; 2],
    /// SET_PED_MOVE_RATE_OVERRIDE for this frame.
    pub move_rate: f32,
    /// SET_RUN_SPRINT_MULTIPLIER_FOR_PLAYER.
    pub sprint_multiplier: f32,
    pub block_sprint: bool,
    /// GTA weapon to put in the player's hands now (swap).
    pub equip: Option<u32>,
    /// A reload starts: play GTA's reload animation.
    pub reload_started: bool,
    /// Rounds spent this frame: take them off GTA's ammo count.
    pub ammo_used: u32,
    /// A quick melee starts.
    pub melee: bool,
    /// Quick jump: set this upward speed (m/s).
    pub jump: Option<f32>,
    pub crouch_changed: Option<bool>,
    /// Fraction of max health to restore this frame.
    pub heal: f32,
    pub hud: Hud,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Ready,
    Reloading { t: f32, empty: bool },
    Raising { t: f32 },
    Dropping { t: f32, to: usize },
    Melee { t: f32 },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub enabled: bool,
    pub quick_jump: bool,
    pub regen: bool,
    /// Scales every kick (0.5 .. 2).
    pub recoil_scale: f32,
}

/// One hit the host found for a shot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub weapon: usize,
    pub distance: f32,
    pub head: bool,
    /// A ped (not a car or wall): shows a hitmarker.
    pub ped: bool,
}

pub struct Player {
    pub table: Table,
    pub options: Options,
    /// Slot weapons (indices into table.weapons).
    pub primary: usize,
    pub secondary: usize,
    rng: u64,
    mags: Vec<Option<u32>>,
    current: Option<usize>,
    expected: Option<usize>,
    action: Action,
    cooldown: f32,
    burst_left: u32,
    semi_queued: f32,
    ads: f32,
    bloom: f32,
    kick_pending: [f32; 2],
    kick_recoverable: [f32; 2],
    since_shot: f32,
    sprint_meter: f32,
    sprint_locked: bool,
    sprint_rest: f32,
    sprint_out: f32,
    was_sprinting: bool,
    crouched: bool,
    regen_wait: f32,
    damage_flash: f32,
    hitmarker: f32,
    kill: bool,
    headshot: bool,
    prev_held: u32,
}

impl Player {
    pub fn new(table: Table, seed: u64) -> Player {
        let n = table.weapons.len();
        let (primary, secondary) = (table.primary, table.secondary);
        let sprint = table.player.sprint_seconds;
        Player {
            table,
            options: Options { enabled: true, quick_jump: false, regen: true, recoil_scale: 1.0 },
            primary,
            secondary,
            rng: seed | 1,
            mags: vec![None; n],
            current: None,
            expected: None,
            action: Action::Ready,
            cooldown: 0.0,
            burst_left: 0,
            semi_queued: 0.0,
            ads: 0.0,
            bloom: 0.0,
            kick_pending: [0.0; 2],
            kick_recoverable: [0.0; 2],
            since_shot: 10.0,
            sprint_meter: sprint,
            sprint_locked: false,
            sprint_rest: 0.0,
            sprint_out: 0.0,
            was_sprinting: false,
            crouched: false,
            regen_wait: 0.0,
            damage_flash: 0.0,
            hitmarker: 0.0,
            kill: false,
            headshot: false,
            prev_held: 0,
        }
    }

    fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, r: [f32; 2]) -> f32 {
        r[0] + (r[1] - r[0]) * self.rand()
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn crouched(&self) -> bool {
        self.crouched
    }

    /// Hands everything back (mode off): no crouch, no pending kick.
    pub fn reset(&mut self) {
        self.action = Action::Ready;
        self.crouched = false;
        self.kick_pending = [0.0; 2];
        self.kick_recoverable = [0.0; 2];
        self.burst_left = 0;
        self.ads = 0.0;
        self.bloom = 0.0;
        self.current = None;
        self.expected = None;
        self.prev_held = 0;
    }

    /// Damage for the hits the host found this frame (same order), and the hitmarker.
    pub fn report_hits(&mut self, hits: &[Hit]) -> Vec<f32> {
        let mut out = Vec::with_capacity(hits.len());
        for h in hits {
            let d = self.table.weapons.get(h.weapon).map_or(0.0, |w| w.damage_at(h.distance, h.head));
            out.push(d);
            if h.ped {
                self.hitmarker = 1.0;
                self.kill = false;
                self.headshot = h.head;
            }
        }
        out
    }

    /// The last ped hit died: the hitmarker turns into a kill marker.
    pub fn report_kill(&mut self) {
        self.hitmarker = 1.0;
        self.kill = true;
    }

    pub fn melee_damage(&self) -> f32 {
        self.table.player.melee_damage
    }

    fn spread(&self, w: &WeaponDef, input: &FrameIn) -> f32 {
        let p = &self.table.player;
        let move_frac = (input.speed / p.walk_speed).clamp(0.0, 1.0);
        let mut hip = w.hip_spread[0] + (w.hip_spread[1] - w.hip_spread[0]) * move_frac + w.move_spread * move_frac;
        if self.crouched {
            hip *= p.crouch_spread_scale;
        }
        hip = (hip + self.bloom).min(w.hip_spread[1] + w.move_spread);
        if input.airborne {
            hip += p.air_spread_add;
        }
        let ads = w.ads_spread + self.bloom * 0.15 + if input.airborne { p.air_spread_add * 0.5 } else { 0.0 };
        hip + (ads - hip) * self.ads
    }

    fn fire(&mut self, w_idx: usize, input: &FrameIn, out: &mut FrameOut) {
        let w = self.table.weapons[w_idx].clone();
        let cone = self.spread(&w, input);
        for _ in 0..w.pellets {
            // Uniform over the cone's disc.
            let r = cone * self.rand().sqrt();
            let a = self.rand() * std::f32::consts::TAU;
            out.shots.push(Shot { weapon: w_idx, yaw: r * a.cos(), pitch: r * a.sin() });
        }
        if let Some(m) = self.mags[w_idx].as_mut() {
            *m = m.saturating_sub(1);
        }
        out.ammo_used += 1;
        let mut scale = (1.0 + (w.ads_kick - 1.0) * self.ads) * self.options.recoil_scale;
        if self.crouched {
            scale *= self.table.player.crouch_kick_scale;
        }
        let kick = [self.range(w.kick_pitch) * scale, self.range(w.kick_yaw) * scale];
        for (i, k) in kick.into_iter().enumerate() {
            self.kick_pending[i] += k;
            self.kick_recoverable[i] += k * w.kick_recover;
        }
        self.bloom = (self.bloom + w.bloom).min(w.hip_spread[1]);
        let interval = w.shot_interval();
        // Keep the cadence exact across frames: carry the overshoot.
        self.cooldown = if self.cooldown > -interval { self.cooldown + interval } else { interval };
        self.since_shot = 0.0;
    }

    fn start_reload(&mut self, w_idx: usize, ammo: u32, out: &mut FrameOut) -> bool {
        let w = &self.table.weapons[w_idx];
        let mag = self.mags[w_idx].unwrap_or(0);
        if mag >= w.mag || ammo <= mag {
            return false;
        }
        let empty = mag == 0;
        let t = if w.reload_round > 0.0 {
            w.reload_round
        } else if empty {
            w.reload_empty
        } else {
            w.reload
        };
        self.action = Action::Reloading { t, empty };
        self.burst_left = 0;
        out.reload_started = true;
        true
    }

    pub fn step(&mut self, input: &FrameIn) -> FrameOut {
        let dt = if input.dt.is_finite() { input.dt.clamp(0.0, 0.25) } else { 0.0 };
        let pressed = input.held & !self.prev_held;
        self.prev_held = input.held;
        let mut out = FrameOut { move_rate: 1.0, sprint_multiplier: 1.0, ..Default::default() };
        let p = self.table.player.clone();

        self.hitmarker = (self.hitmarker - dt * 2.5).max(0.0);
        self.damage_flash = (self.damage_flash - dt * 1.2).max(0.0);
        if !self.options.enabled {
            return out;
        }

        // Regenerating health, on foot or not.
        if input.health_lost > 0.0 {
            self.regen_wait = p.regen_delay;
            self.damage_flash = (self.damage_flash + input.health_lost * 4.0).min(1.0);
        } else {
            self.regen_wait = (self.regen_wait - dt).max(0.0);
        }
        if self.options.regen && self.regen_wait <= 0.0 && input.health > 0.0 && input.health < 1.0 {
            out.heal = (dt / p.regen_seconds).min(1.0 - input.health);
        }

        if !input.on_foot {
            if self.crouched {
                self.crouched = false;
                out.crouch_changed = Some(false);
            }
            self.action = Action::Ready;
            self.burst_left = 0;
            self.ads = 0.0;
            self.current = None;
            self.was_sprinting = false;
            out.hud = self.hud(None, input);
            return out;
        }

        // Crouch toggles; sprinting stands you up.
        if pressed & CROUCH != 0 {
            self.crouched = !self.crouched;
            out.crouch_changed = Some(self.crouched);
        }
        if self.crouched && pressed & SPRINT != 0 {
            self.crouched = false;
            out.crouch_changed = Some(false);
        }

        // Which weapon is in hand: a change we did not ask for raises it.
        let now = self.table.by_hash(input.gta_weapon);
        if now != self.current {
            self.current = now;
            if now.is_some() && now != self.expected {
                self.action = Action::Raising { t: now.map_or(0.0, |i| self.table.weapons[i].raise) };
            }
            self.expected = None;
            self.burst_left = 0;
        }
        let Some(wi) = self.current else {
            // Not a CODV weapon: GTA handles it; sprint rules still apply.
            self.sprint(input, false, dt, &p, &mut out);
            out.hud = self.hud(None, input);
            return out;
        };
        let w = self.table.weapons[wi].clone();
        let ammo = input.gta_ammo;
        let mag = self.mags[wi].get_or_insert(w.mag.min(ammo));
        *mag = (*mag).min(ammo);

        self.cooldown = (self.cooldown - dt).max(-1.0);
        self.semi_queued = (self.semi_queued - dt).max(0.0);
        self.sprint_out = (self.sprint_out - dt).max(0.0);
        self.since_shot += dt;
        self.bloom = (self.bloom - w.bloom_decay * dt).max(0.0);

        let wants_fire = input.held & FIRE != 0 || self.burst_left > 0;
        let sprinting = self.sprint(input, wants_fire && self.action == Action::Ready, dt, &p, &mut out);
        if self.was_sprinting && !sprinting {
            self.sprint_out = w.sprint_out;
        }
        self.was_sprinting = sprinting;

        // Weapon actions.
        match self.action {
            Action::Dropping { t, to } => {
                let t = t - dt;
                if t <= 0.0 {
                    out.equip = Some(self.table.weapons[to].gta_hash);
                    self.expected = Some(to);
                    self.action = Action::Raising { t: self.table.weapons[to].raise };
                } else {
                    self.action = Action::Dropping { t, to };
                }
            }
            Action::Raising { t } => {
                self.action = if t - dt <= 0.0 { Action::Ready } else { Action::Raising { t: t - dt } };
            }
            Action::Melee { t } => {
                self.action = if t - dt <= 0.0 { Action::Ready } else { Action::Melee { t: t - dt } };
            }
            Action::Reloading { t, empty } => {
                let mag = self.mags[wi].unwrap_or(0);
                let per_round = w.reload_round > 0.0;
                if sprinting {
                    // Sprinting cancels a reload (rounds already loaded stay).
                    self.action = Action::Ready;
                } else if per_round && pressed & FIRE != 0 && mag > 0 {
                    self.action = Action::Ready;
                } else {
                    let t = t - dt;
                    if t > 0.0 {
                        self.action = Action::Reloading { t, empty };
                    } else if per_round {
                        let m = (mag + 1).min(w.mag).min(ammo);
                        self.mags[wi] = Some(m);
                        self.action = if m < w.mag && ammo > m {
                            Action::Reloading { t: t + w.reload_round, empty: false }
                        } else {
                            Action::Ready
                        };
                    } else {
                        self.mags[wi] = Some(w.mag.min(ammo));
                        self.action = Action::Ready;
                    }
                }
            }
            Action::Ready => {}
        }

        if self.action == Action::Ready && !sprinting {
            let other = if wi == self.primary { self.secondary } else { self.primary };
            let mag = self.mags[wi].unwrap_or(0);
            if pressed & SWAP != 0 && other != wi {
                self.action = Action::Dropping { t: w.drop, to: other };
                self.burst_left = 0;
            } else if pressed & MELEE != 0 {
                self.action = Action::Melee { t: p.melee_seconds };
                self.burst_left = 0;
                out.melee = true;
            } else if pressed & RELOAD != 0 {
                self.start_reload(wi, ammo, &mut out);
            } else {
                if w.fire == FireMode::Semi && pressed & FIRE != 0 {
                    self.semi_queued = SEMI_BUFFER;
                }
                let ready = self.cooldown <= EPS && self.sprint_out <= EPS;
                if ready && mag > 0 {
                    let fire = match w.fire {
                        FireMode::Auto => input.held & FIRE != 0,
                        FireMode::Semi => self.semi_queued > 0.0,
                        FireMode::Burst => {
                            if self.burst_left == 0 && pressed & FIRE != 0 {
                                self.burst_left = w.burst;
                            }
                            self.burst_left > 0
                        }
                    };
                    if fire {
                        self.semi_queued = 0.0;
                        self.fire(wi, input, &mut out);
                        if w.fire == FireMode::Burst {
                            self.burst_left -= 1;
                            if self.burst_left == 0 {
                                self.cooldown += w.burst_delay;
                            }
                        }
                    }
                } else if ready && mag == 0 && (input.held & FIRE != 0 || pressed & FIRE != 0) {
                    // Dry trigger: reload automatically.
                    self.start_reload(wi, ammo, &mut out);
                }
                if self.mags[wi] == Some(0) && self.cooldown <= 0.0 && self.action == Action::Ready {
                    self.burst_left = 0;
                    self.start_reload(wi, ammo.saturating_sub(out.ammo_used), &mut out);
                }
            }
        }

        // ADS: not while sprinting, reloading, swapping or meleeing.
        let can_ads = !sprinting && matches!(self.action, Action::Ready | Action::Raising { .. });
        let target = if input.held & ADS != 0 && can_ads { 1.0 } else { 0.0 };
        let step = dt / w.ads_time;
        self.ads = if target >= 1.0 { (self.ads + step).min(1.0) } else { (self.ads - step * 1.5).max(0.0) };

        // Recoil: pending kick reaches the camera over a few frames; the
        // recoverable part returns once firing pauses.
        if self.since_shot > RECOVER_PAUSE && self.burst_left == 0 {
            for i in 0..2 {
                self.kick_pending[i] -= self.kick_recoverable[i];
                self.kick_recoverable[i] = 0.0;
            }
        }
        let k = 1.0 - (-dt * KICK_RATE).exp();
        for i in 0..2 {
            let apply = self.kick_pending[i] * k;
            out.kick[i] = apply;
            self.kick_pending[i] -= apply;
        }

        out.move_rate = w.move_scale * (1.0 + (p.ads_move_scale * w.ads_move - 1.0) * self.ads);
        out.sprint_multiplier = (p.sprint_multiplier * w.move_scale).clamp(1.0, 1.49);
        if self.options.quick_jump && pressed & JUMP != 0 && !input.airborne && !self.crouched {
            out.jump = Some(p.quick_jump_speed);
        }
        out.state = Some(if sprinting {
            State::Sprinting
        } else {
            match self.action {
                Action::Ready if self.since_shot < 0.15 => State::Firing,
                Action::Ready => State::Ready,
                Action::Reloading { .. } => State::Reloading,
                Action::Raising { .. } => State::Raising,
                Action::Dropping { .. } => State::Dropping,
                Action::Melee { .. } => State::Melee,
            }
        });
        out.hud = self.hud(Some(wi), input);
        out.hud.reserve = ammo.saturating_sub(out.ammo_used).saturating_sub(out.hud.mag);
        out
    }

    /// Sprint meter. Returns whether the player is sprinting this frame.
    fn sprint(
        &mut self,
        input: &FrameIn,
        firing: bool,
        dt: f32,
        p: &crate::defs::PlayerRules,
        out: &mut FrameOut,
    ) -> bool {
        let sprinting = input.sprinting && !self.sprint_locked && !firing && !self.crouched;
        if sprinting {
            self.sprint_meter -= dt;
            self.sprint_rest = 0.0;
            if self.sprint_meter <= 0.0 {
                self.sprint_meter = 0.0;
                self.sprint_locked = true;
            }
        } else {
            self.sprint_rest += dt;
            if self.sprint_rest >= p.sprint_regen_delay {
                self.sprint_meter = (self.sprint_meter + p.sprint_regen_rate * dt).min(p.sprint_seconds);
            }
            if self.sprint_locked && self.sprint_meter >= p.sprint_min.min(p.sprint_seconds) {
                self.sprint_locked = false;
            }
        }
        out.block_sprint = self.sprint_locked || firing || self.crouched;
        sprinting
    }

    fn hud(&self, wi: Option<usize>, input: &FrameIn) -> Hud {
        let spread = wi.map_or(0.0, |i| self.spread(&self.table.weapons[i], input));
        let reload = match (self.action, wi) {
            (Action::Reloading { t, empty }, Some(i)) => {
                let w = &self.table.weapons[i];
                let total = if w.reload_round > 0.0 {
                    w.reload_round
                } else if empty {
                    w.reload_empty
                } else {
                    w.reload
                };
                (1.0 - t / total).clamp(0.0, 1.0)
            }
            _ => 0.0,
        };
        Hud {
            mag: wi.and_then(|i| self.mags[i]).unwrap_or(0),
            reserve: 0,
            weapon: wi,
            spread,
            ads: self.ads,
            sprint: self.sprint_meter / self.table.player.sprint_seconds,
            reload,
            hitmarker: self.hitmarker,
            kill: self.kill,
            headshot: self.headshot,
            damage_flash: self.damage_flash,
            crouched: self.crouched,
        }
    }
}

#[cfg(test)]
mod tests;
