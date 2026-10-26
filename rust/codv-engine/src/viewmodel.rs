//! Modern Warfare 2's first-person view model: the arms and the held gun,
//! animated by the weapon's own XAnims and placed in the view by MW2's
//! weapon placement (bob, movement offsets, idle sway, look sway, gun kick).
//!
//! Ported from iw4L (Apache-2.0, NOTICE) without its Bevy layers:
//!
//! * `asset_anim::ClipScheduler` (`Scheduler` here: per-slot clip time, rate
//!   and goal weight; notetracks are left out until F5's audio);
//! * `render_anim::viewmodel_controller::ViewmodelController` and
//!   `render_anim::anim::fpv` (`Controller`: which szXAnims slot plays, from
//!   the hand's weapAnim edges and shot cues; the ADS up/down scrub);
//! * `asset_game::FpvAssembly` / `render_anim::fpv_rig` (`Rig`: the hands
//!   with the gun on `tag_weapon` as one `xmodel_runtime::DObj`, clip tracks
//!   bound by bone name, surfaces hidden by the weapon's hide tags, the pose
//!   made relative to `tag_view`);
//! * `render_anim::occupancy::fpv_present` / `view_sway` (the placement
//!   inputs to `weapon_iw4::weapon_placement_assemble`).
//!
//! The rig's output is one skinning matrix per bone, in view space (D3D
//! axes: x right, y up, z forward; inches), for the host's renderer.
use crate::arms::GunFrame;
use crate::player::Mw2Player;
use codv_mw2data::table::Row;
use codv_mw2data::vm;
use glam::{Mat4, Quat, Vec3, Vec4};
use std::sync::Arc;
use weapon_iw4 as w;
use xmodel_runtime::{AnimClip, AnimInstance, Attach, ClipNotify, DObj, ModelPoseSrc, RawDeltaTrans, RawXAnimParts};

/// szXAnims slots (asset_iw4 weap_anim) plus the two quick-reload slots.
pub const SLOTS: usize = w::WEAPON_ANIM_SLOTS;
mod slot {
    pub const IDLE: usize = 0x1;
    pub const EMPTY_IDLE: usize = 0x2;
    pub const FIRE: usize = 0x3;
    pub const HOLD_FIRE: usize = 0x4;
    pub const LASTSHOT: usize = 0x5;
    pub const RECHAMBER: usize = 0x6;
    pub const MELEE: usize = 0x7;
    pub const MELEE_CHARGE: usize = 0x8;
    pub const RELOAD: usize = 0x9;
    pub const RELOAD_EMPTY: usize = 0xA;
    pub const RELOAD_START: usize = 0xB;
    pub const RELOAD_END: usize = 0xC;
    pub const RAISE: usize = 0xD;
    pub const FIRST_RAISE: usize = 0xE;
    pub const DROP: usize = 0x10;
    pub const ALT_RAISE: usize = 0x11;
    pub const ALT_DROP: usize = 0x12;
    pub const QUICK_RAISE: usize = 0x13;
    pub const QUICK_DROP: usize = 0x14;
    pub const EMPTY_RAISE: usize = 0x15;
    pub const EMPTY_DROP: usize = 0x16;
    pub const SPRINT_IN: usize = 0x17;
    pub const SPRINT_LOOP: usize = 0x18;
    pub const SPRINT_OUT: usize = 0x19;
    pub const DETONATE: usize = 0x1D;
    pub const ADS_FIRE: usize = 0x20;
    pub const ADS_LASTSHOT: usize = 0x21;
    pub const ADS_RECHAMBER: usize = 0x22;
    pub const ADS_UP: usize = 0x23;
    pub const ADS_DOWN: usize = 0x24;
    pub const RELOAD_QUICK: usize = super::w::weap_anim_extra::RELOAD_QUICK;
    pub const RELOAD_QUICK_EMPTY: usize = super::w::weap_anim_extra::RELOAD_QUICK_EMPTY;
}

/// The bind skeleton iw4L's DObj composes from (asset_model `capture_pose_src`).
pub fn pose_src(m: &vm::Model) -> ModelPoseSrc {
    ModelPoseSrc {
        name: m.name.clone(),
        num_bones: m.bone_names.len(),
        num_root_bones: m.num_root_bones as usize,
        scale: m.scale,
        no_scale_part_bits: m.no_scale_part_bits,
        bone_names: m.bone_names.clone(),
        parent_list: m.parent_list.clone(),
        quats: m.quats.clone(),
        trans: m.trans.clone(),
        base_mat: m
            .base_mat
            .iter()
            .map(|(q, t)| (Quat::from_xyzw(q[0], q[1], q[2], q[3]), Vec3::from_array(*t)))
            .collect(),
        root_rest: None,
    }
}

/// A retail animation decoded by `xmodel_runtime`.
pub fn clip(a: &vm::Anim) -> Result<AnimClip, String> {
    let parts = RawXAnimParts {
        name: a.name.clone(),
        data_byte: a.data_byte.clone(),
        data_short: a.data_short.clone(),
        data_int: a.data_int.clone(),
        random_data_byte: a.random_data_byte.clone(),
        random_data_short: a.random_data_short.clone(),
        random_data_int: a.random_data_int.clone(),
        numframes: a.numframes,
        flags: a.flags,
        bone_count: a.bone_count,
        framerate: a.framerate,
        names: a.names.clone(),
        notifies: a.notifies.iter().map(|n| ClipNotify { name: n.name.clone(), time: n.time }).collect(),
        indices: a.indices.clone(),
        delta_trans: a.delta_trans.as_ref().map(|d| RawDeltaTrans {
            size: d.size,
            small: d.small,
            mins: d.mins,
            step: d.step,
            indices: d.indices.clone(),
            packed: d.packed.clone(),
        }),
    };
    AnimClip::from_parts(&parts).map_err(|e| format!("{}: {e:?}", a.name))
}

// ---- scheduler (asset_anim::ClipScheduler, notetracks forward only) -----------

#[derive(Clone, Debug, Default)]
struct Node {
    clip: Option<Arc<AnimClip>>,
    time: f64,
    rate: f32,
    weight: f32,
    goal_weight: f32,
    goal_time_remaining: f32,
    /// Completed loops of a looping clip, and whether the clip has not
    /// advanced since it was set (a marker at 0 fires on its first step).
    cycle: i64,
    initial: bool,
}

#[derive(Clone, Debug)]
struct Scheduler {
    nodes: Vec<Node>,
}

impl Scheduler {
    fn new(n: usize) -> Self {
        Self { nodes: (0..n).map(|_| Node { rate: 1.0, ..Default::default() }).collect() }
    }
    fn set_clip(&mut self, node: usize, clip: Arc<AnimClip>) {
        let n = &mut self.nodes[node];
        n.clip = Some(clip);
        n.time = 0.0;
        n.cycle = 0;
        n.initial = true;
    }
    fn set_time(&mut self, node: usize, time: f32) {
        self.nodes[node].time = if time.is_finite() { f64::from(time.max(0.0)) } else { 0.0 };
        self.nodes[node].initial = false;
    }
    fn set_rate(&mut self, node: usize, rate: f32) {
        self.nodes[node].rate = if rate.is_finite() { rate } else { 0.0 };
    }
    fn set_goal_weight(&mut self, node: usize, goal: f32, time: f32) {
        let n = &mut self.nodes[node];
        n.goal_weight = goal.max(0.0);
        n.goal_time_remaining = time.max(0.0);
        if n.goal_time_remaining == 0.0 {
            n.weight = n.goal_weight;
        }
    }
    fn weight(&self, node: usize) -> f32 {
        self.nodes[node].weight
    }
    fn time(&self, node: usize) -> f32 {
        self.nodes[node].time as f32
    }
    /// Advances every weighted clip; returns the notetracks crossed (in clip
    /// order) by clips with a positive goal weight (ClipScheduler
    /// collect_crossings, forward playback only).
    fn advance(&mut self, dt: f32) -> Vec<String> {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let mut notes = Vec::new();
        for n in &mut self.nodes {
            if n.weight > 0.0
                && let Some(clip) = &n.clip
            {
                let duration = f64::from(clip.duration());
                if !duration.is_finite() || duration <= f64::from(f32::EPSILON) {
                    n.time = 0.0;
                } else {
                    let old = if clip.looping { n.cycle as f64 + n.time / duration } else { n.time / duration };
                    let next = old + f64::from(dt) * f64::from(n.rate) / duration;
                    let new = if clip.looping { next } else { next.clamp(0.0, 1.0) };
                    let initial = n.initial;
                    if new != old {
                        n.initial = false;
                    }
                    if clip.looping {
                        n.cycle = new.floor() as i64;
                        n.time = new.rem_euclid(1.0) * duration;
                    } else {
                        n.time = new * duration;
                    }
                    if n.goal_weight > 0.0 && new > old {
                        for note in &clip.notifies {
                            if note.name.is_empty() || note.name.eq_ignore_ascii_case("end") || !note.time.is_finite() {
                                continue;
                            }
                            let point = f64::from(note.time.clamp(0.0, 1.0));
                            let first = if initial && old == 0.0 && point == 0.0 {
                                0
                            } else {
                                (old - point).floor() as i64 + 1
                            };
                            let last = (new - point).floor() as i64;
                            let fires = if clip.looping {
                                (last - first + 1).clamp(0, 4)
                            } else {
                                i64::from(first <= 0 && last >= 0)
                            };
                            for _ in 0..fires {
                                notes.push(note.name.clone());
                            }
                        }
                    }
                }
            }
            if n.goal_time_remaining > 0.0 {
                let step = dt.min(n.goal_time_remaining);
                n.weight += (n.goal_weight - n.weight) * step / n.goal_time_remaining;
                n.goal_time_remaining -= step;
                if n.goal_time_remaining <= f32::EPSILON {
                    n.weight = n.goal_weight;
                    n.goal_time_remaining = 0.0;
                }
            }
        }
        notes
    }
    /// (slot, clip, time, weight) of every weighted node.
    fn active(&self) -> impl Iterator<Item = (usize, &AnimClip, f32, f32)> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.weight > 0.0)
            .filter_map(|(i, n)| n.clip.as_deref().map(|c| (i, c, n.time as f32, n.weight)))
    }
}

// ---- controller (render_anim ViewmodelController + fpv edges) ---------------

/// The weapon timers the controller scales clips to (WeaponDef, ms).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timers {
    pub fire: i32,
    pub detonate: i32,
    pub melee: i32,
    pub melee_charge: i32,
    pub raise: i32,
    /// The first raise of a weapon (WeaponCompleteDef firstRaiseTime): retail
    /// scales the first-time pullout to it (ANIM_RATE_TABLE slot 0xE).
    pub first_raise: i32,
    pub alt_raise: i32,
    pub alt_drop: i32,
    pub drop: i32,
    pub quick_drop: i32,
    pub quick_raise: i32,
    pub sprint_raise: i32,
    pub sprint_loop: i32,
    pub sprint_drop: i32,
    pub reload: i32,
    pub reload_empty: i32,
    pub reload_start: i32,
    pub reload_end: i32,
}

impl Timers {
    pub fn from_row(r: &Row) -> Timers {
        Timers {
            fire: r.i("fire_time_ms"),
            detonate: 0,
            melee: r.i("melee_time_ms"),
            melee_charge: r.i("melee_charge_time_ms"),
            raise: r.i("raise_time_ms"),
            first_raise: r.i("first_raise_time_ms"),
            alt_raise: r.i("alternate_raise_time_ms"),
            alt_drop: r.i("alternate_drop_time_ms"),
            drop: r.i("drop_time_ms"),
            quick_drop: r.i("quick_drop_time_ms"),
            quick_raise: r.i("quick_raise_time_ms"),
            sprint_raise: r.i("sprint_raise_time_ms"),
            sprint_loop: r.i("sprint_loop_time_ms"),
            sprint_drop: r.i("sprint_drop_time_ms"),
            reload: r.i("reload_time_ms"),
            reload_empty: r.i("reload_empty_time_ms"),
            reload_start: r.i("reload_start_time_ms"),
            reload_end: r.i("reload_end_time_ms"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmState {
    Ready,
    Raising,
    Firing,
    Reloading,
    ReloadStarting,
    ReloadEnding,
    Rechambering,
    Dropping,
    SprintIn,
    SprintLoop,
    SprintOut,
}

fn positive(ms: i32) -> Option<i32> {
    (ms > 0).then_some(ms)
}

/// The slots iw4L dispatches (everything but root and the ADS overlays).
fn dispatch_slots() -> impl Iterator<Item = usize> {
    (1..=0x22).chain(asset_count()..SLOTS)
}
const fn asset_count() -> usize {
    w::WEAPON_ANIM_SLOTS - 2
}

#[derive(Clone, Debug)]
struct Controller {
    clips: Vec<Option<Arc<AnimClip>>>,
    timers: Timers,
    tree: Scheduler,
    state: VmState,
    action: Option<usize>,
    action_remaining: Option<f32>,
    last_ads_frac: f32,
    last_weap_anim: Option<i32>,
}

impl Controller {
    fn new(clips: Vec<Option<Arc<AnimClip>>>, timers: Timers) -> Self {
        let mut tree = Scheduler::new(SLOTS);
        for (i, c) in clips.iter().enumerate() {
            if let Some(c) = c {
                tree.set_clip(i, Arc::clone(c));
            }
        }
        let mut this = Self {
            clips,
            timers,
            tree,
            state: VmState::Ready,
            action: None,
            action_remaining: None,
            last_ads_frac: 0.0,
            last_weap_anim: None,
        };
        this.set_weight(slot::IDLE, w::ACTIVE_GOAL_WEIGHT, w::ACTION_GOAL_TIME_SECS);
        // Bones only the ADS clips move still need their lowered pose at the hip.
        if let Some(c) = this.clip(slot::ADS_DOWN).cloned() {
            this.set_weight(slot::ADS_DOWN, w::ACTIVE_GOAL_WEIGHT, 0.0);
            this.tree.set_time(slot::ADS_DOWN, c.duration());
            this.tree.set_rate(slot::ADS_DOWN, 0.0);
        }
        this
    }

    fn clip(&self, slot: usize) -> Option<&Arc<AnimClip>> {
        self.clips.get(slot).and_then(Option::as_ref)
    }

    fn set_weight(&mut self, slot: usize, weight: f32, goal_time: f32) {
        if self.clip(slot).is_some() {
            self.tree.set_goal_weight(slot, weight, goal_time);
        }
    }

    fn fire(&mut self) {
        if !matches!(self.state, VmState::Ready | VmState::Firing) {
            return;
        }
        let slot =
            if self.last_ads_frac > 0.0 && self.clip(slot::ADS_FIRE).is_some() { slot::ADS_FIRE } else { slot::FIRE };
        self.start_action(slot, VmState::Firing, positive(self.timers.fire));
    }

    fn dispatch(&mut self, s: usize) {
        if s == slot::ADS_UP || s == slot::ADS_DOWN || s == 0 || s >= SLOTS {
            return;
        }
        if s == slot::IDLE || s == slot::EMPTY_IDLE {
            self.start_idle(false, false);
            return;
        }
        let t = self.timers;
        let (state, timer) = match s {
            slot::FIRE | slot::ADS_FIRE | slot::LASTSHOT | slot::ADS_LASTSHOT | slot::HOLD_FIRE => {
                (VmState::Firing, positive(t.fire))
            }
            slot::DETONATE => (VmState::Firing, positive(t.detonate)),
            slot::FIRST_RAISE => (VmState::Raising, positive(t.first_raise)),
            slot::EMPTY_RAISE => (VmState::Raising, None),
            slot::QUICK_RAISE => (VmState::Raising, positive(t.quick_raise)),
            slot::ALT_RAISE => (VmState::Raising, positive(t.alt_raise)),
            slot::RAISE => (VmState::Raising, Some(t.raise)),
            slot::RELOAD => (VmState::Reloading, positive(t.reload)),
            slot::RELOAD_EMPTY => (VmState::Reloading, positive(t.reload_empty)),
            slot::RELOAD_QUICK | slot::RELOAD_QUICK_EMPTY => (VmState::Reloading, None),
            slot::RELOAD_START => (VmState::ReloadStarting, positive(t.reload_start)),
            slot::RELOAD_END => (VmState::ReloadEnding, positive(t.reload_end)),
            slot::RECHAMBER | slot::ADS_RECHAMBER => (VmState::Rechambering, None),
            slot::DROP => (VmState::Dropping, positive(t.drop)),
            slot::QUICK_DROP => (VmState::Dropping, positive(t.quick_drop)),
            slot::ALT_DROP => (VmState::Dropping, positive(t.alt_drop)),
            slot::EMPTY_DROP => (VmState::Dropping, None),
            slot::SPRINT_IN => (VmState::SprintIn, positive(t.sprint_raise)),
            slot::SPRINT_LOOP => (VmState::SprintLoop, positive(t.sprint_loop)),
            slot::SPRINT_OUT => (VmState::SprintOut, positive(t.sprint_drop)),
            slot::MELEE => (VmState::Ready, positive(t.melee)),
            slot::MELEE_CHARGE => (VmState::Ready, positive(t.melee_charge)),
            _ => (VmState::Ready, None),
        };
        self.start_action(s, state, timer);
    }

    fn start_action(&mut self, s: usize, state: VmState, timer_ms: Option<i32>) {
        self.zero_dispatch(w::ACTION_GOAL_TIME_SECS);
        let Some(clip) = self.clip(s).cloned() else {
            self.action = None;
            self.action_remaining = None;
            self.state = state;
            return;
        };
        self.tree.set_clip(s, Arc::clone(&clip));
        let rate = if w::slot_uses_native_rate(s) {
            1.0
        } else {
            timer_ms
                .filter(|t| *t > 0)
                .map(|t| w::playback_rate((clip.duration() * 1000.0).round() as i32, t))
                .unwrap_or(1.0)
        };
        self.tree.set_rate(s, rate);
        self.tree.set_goal_weight(s, w::ACTIVE_GOAL_WEIGHT, w::ACTION_GOAL_TIME_SECS);
        self.action = Some(s);
        self.action_remaining =
            if clip.looping { None } else { timer_ms.filter(|t| *t > 0).map(|t| t as f32 / 1000.0) };
        self.state = state;
    }

    fn start_idle(&mut self, empty: bool, interrupt: bool) {
        let s = if empty && self.clip(slot::EMPTY_IDLE).is_some() { slot::EMPTY_IDLE } else { slot::IDLE };
        let goal = if interrupt { w::IDLE_INTERRUPT_GOAL_TIME_SECS } else { w::ACTION_GOAL_TIME_SECS };
        self.action = None;
        self.action_remaining = None;
        self.state = VmState::Ready;
        self.zero_dispatch(goal);
        self.set_weight(s, w::ACTIVE_GOAL_WEIGHT, goal);
    }

    fn zero_dispatch(&mut self, goal_time: f32) {
        for n in dispatch_slots() {
            self.tree.set_goal_weight(n, w::INACTIVE_GOAL_WEIGHT, goal_time);
            self.tree.set_rate(n, 1.0);
        }
    }

    fn dispatch_unfinished(&self) -> bool {
        dispatch_slots().any(|n| {
            self.tree.weight(n) > 0.0 && self.clip(n).is_some_and(|c| c.looping || self.tree.time(n) < c.duration())
        })
    }

    /// One weapAnim value (render_anim fpv `observe_weap_anim_edge_on`).
    fn weap_anim(&mut self, raw: i32, empty_mag: bool) {
        let last = self.last_weap_anim.replace(raw);
        if last == Some(raw) {
            return;
        }
        match raw as u32 & w::WEAP_ANIM_EVENT_MASK {
            0 => {
                if self.dispatch_unfinished() {
                    self.last_weap_anim = None; // try again next frame
                } else {
                    self.start_idle(empty_mag, false);
                }
            }
            1 => {
                let interrupt = self.dispatch_unfinished();
                self.start_idle(empty_mag, interrupt);
            }
            masked => match w::slot_for_weap_anim_event(masked) {
                Some(s) => self.dispatch(s),
                None => self.start_idle(empty_mag, false),
            },
        }
    }

    fn advance(&mut self, dt: f32) -> Vec<String> {
        let notes = self.tree.advance(dt);
        if let Some(r) = self.action_remaining.as_mut() {
            *r = (*r - dt).max(0.0);
        }
        if self.state == VmState::Firing
            && let Some(s) = self.action
        {
            let done = match self.action_remaining {
                Some(r) => r <= 0.0,
                None => self.clip(s).is_none_or(|c| !c.looping && self.tree.time(s) >= c.duration()),
            };
            if done {
                self.action = None;
                self.action_remaining = None;
                self.state = VmState::Ready;
            }
        }
        notes
    }

    /// The ADS overlays, scrubbed by the ADS fraction (WeightIsFrac, IW4's convention).
    fn ads_overlay(&mut self, frac: f32) {
        let scrub = w::ads_overlay_scrub(frac);
        self.last_ads_frac = scrub.ads_up_weight;
        if matches!(self.action, Some(slot::MELEE | slot::MELEE_CHARGE)) {
            self.set_weight(slot::ADS_UP, w::INACTIVE_GOAL_WEIGHT, 0.0);
            self.set_weight(slot::ADS_DOWN, w::INACTIVE_GOAL_WEIGHT, 0.0);
            return;
        }
        if let Some(c) = self.clip(slot::ADS_UP).cloned() {
            self.tree.set_rate(slot::ADS_UP, 0.0);
            self.tree.set_time(slot::ADS_UP, c.duration() * scrub.ads_up_time_norm);
            self.tree.set_goal_weight(slot::ADS_UP, scrub.ads_up_weight, w::ACTION_GOAL_TIME_SECS);
        }
        if let Some(c) = self.clip(slot::ADS_DOWN).cloned() {
            self.tree.set_rate(slot::ADS_DOWN, 0.0);
            self.tree.set_time(slot::ADS_DOWN, c.duration() * scrub.ads_down_time_norm);
            self.tree.set_goal_weight(slot::ADS_DOWN, w::ACTIVE_GOAL_WEIGHT, w::ACTION_GOAL_TIME_SECS);
        }
    }
}

// ---- placement facts --------------------------------------------------------

/// The weapon's viewmodel placement facts (imported under the WeaponDef names).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlacementFacts {
    pub stance: w::WeaponStanceStaticOfsInputs,
    pub movement: w::WeaponMovementOfsInputs,
    pub idle: w::WeaponIdleInputs,
    pub bob: w::WeaponBobInputs,
    pub sway_hip: w::WeaponSwayParams,
    pub sway_ads: w::WeaponSwayParams,
    pub aim_down_sight: bool,
    pub overlay_reticle: i32,
    pub gun_max: [f32; 2],
    pub ads_zoom_fov: f32,
}

fn v3(r: &Row, name: &str) -> [f32; 3] {
    [r.f(&format!("{name}.0")), r.f(&format!("{name}.1")), r.f(&format!("{name}.2"))]
}

impl PlacementFacts {
    pub fn from_row(r: &Row) -> PlacementFacts {
        PlacementFacts {
            stance: w::WeaponStanceStaticOfsInputs {
                ducked_ofs: v3(r, "ducked_ofs"),
                prone_ofs: v3(r, "prone_ofs"),
                ads_aim_pitch: r.f("ads_aim_pitch"),
                night_vision_wear_time: r.i("night_vision_wear_time_ms"),
            },
            movement: w::WeaponMovementOfsInputs {
                stand_move: v3(r, "movement.stand_move"),
                stand_rot: v3(r, "movement.stand_rot"),
                strafe_move: v3(r, "movement.strafe_move"),
                strafe_rot: v3(r, "movement.strafe_rot"),
                ducked_move: v3(r, "movement.ducked_move"),
                ducked_rot: v3(r, "movement.ducked_rot"),
                prone_move: v3(r, "movement.prone_move"),
                prone_rot: v3(r, "movement.prone_rot"),
                pos_move_rate: r.f("movement.pos_move_rate"),
                pos_prone_move_rate: r.f("movement.pos_prone_move_rate"),
                stand_move_min_speed: r.f("movement.stand_move_min_speed"),
                ducked_move_min_speed: r.f("movement.ducked_move_min_speed"),
                prone_move_min_speed: r.f("movement.prone_move_min_speed"),
                pos_rot_rate: r.f("movement.pos_rot_rate"),
                pos_prone_rot_rate: r.f("movement.pos_prone_rot_rate"),
            },
            idle: w::WeaponIdleInputs {
                ads_idle_amount: r.f("idle.ads_idle_amount"),
                hip_idle_amount: r.f("idle.hip_idle_amount"),
                ads_idle_speed: r.f("idle.ads_idle_speed"),
                hip_idle_speed: r.f("idle.hip_idle_speed"),
                idle_crouch_factor: r.f("idle.idle_crouch_factor"),
                idle_prone_factor: r.f("idle.idle_prone_factor"),
            },
            bob: w::WeaponBobInputs { ads_bob_factor: r.f("ads_bob_factor") },
            sway_hip: w::WeaponSwayParams {
                max_angle: r.f("sway.sway_max_angle"),
                pitch_scale: r.f("sway.sway_pitch_scale"),
                yaw_scale: r.f("sway.sway_yaw_scale"),
                horiz_scale: r.f("sway.sway_horiz_scale"),
                vert_scale: r.f("sway.sway_vert_scale"),
            },
            sway_ads: w::WeaponSwayParams {
                max_angle: r.f("sway.ads_sway_max_angle"),
                pitch_scale: r.f("sway.ads_sway_pitch_scale"),
                yaw_scale: r.f("sway.ads_sway_yaw_scale"),
                horiz_scale: r.f("sway.ads_sway_horiz_scale"),
                vert_scale: r.f("sway.ads_sway_vert_scale"),
            },
            aim_down_sight: r.b("aim_down_sight"),
            overlay_reticle: r.i("overlay_reticle"),
            gun_max: [r.f("kick.gun_max_pitch"), r.f("kick.gun_max_yaw")],
            ads_zoom_fov: r.f("ads_zoom_fov"),
        }
    }
}

// ---- the mesh the host draws -------------------------------------------------

/// A vertex in the combined hands + gun skeleton (bones index the rig's palette).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Tangent xyz, binormal sign w.
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub bones: [u8; 4],
    pub weights: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshDraw {
    pub first_index: u32,
    pub index_count: u32,
    pub material: vm::Material,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<MeshDraw>,
}

/// Bones of `m` named in `hide` or below one that is (asset_game
/// `bone_has_hidden_ancestor`), as part-bit words.
fn hidden_words(m: &vm::Model, hide: &[String]) -> [u32; 6] {
    let mut words = [0u32; 6];
    if hide.is_empty() {
        return words;
    }
    let roots = m.num_root_bones as usize;
    let parent = |b: usize| -> Option<usize> {
        if b < roots {
            return None;
        }
        let step = *m.parent_list.get(b - roots)? as usize;
        b.checked_sub(step).filter(|_| step > 0)
    };
    for b in 0..m.bone_names.len() {
        let mut cur = Some(b);
        while let Some(c) = cur {
            if hide.iter().any(|t| t.eq_ignore_ascii_case(&m.bone_names[c])) {
                anim_iw4::set_hide_part_bit(&mut words, b);
                break;
            }
            cur = parent(c);
        }
    }
    words
}

/// A model merged onto another's bones (a rocketModel): its vertices carried by `carry[bone]` (its
/// bind pose into the other's) and skinned to the extra bones from `bone_base`.
fn append_merged(mesh: &mut Mesh, m: &vm::Model, bone_base: usize, carry: &[Mat4]) -> Result<(), String> {
    let first = mesh.vertices.len();
    append_model(mesh, m, bone_base, &[0; 6])?;
    for v in &mut mesh.vertices[first..] {
        // Rigid parts: the first bone's carry (the bone index is already offset by bone_base).
        let local = usize::from(v.bones[0]).saturating_sub(bone_base);
        let Some(c) = carry.get(local) else { continue };
        v.position = c.transform_point3(Vec3::from_array(v.position)).to_array();
        v.normal = c.transform_vector3(Vec3::from_array(v.normal)).normalize_or_zero().to_array();
        let t = c.transform_vector3(Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2])).normalize_or_zero();
        v.tangent = [t.x, t.y, t.z, v.tangent[3]];
    }
    Ok(())
}

fn append_model(mesh: &mut Mesh, m: &vm::Model, bone_base: usize, hide: &[u32; 6]) -> Result<(), String> {
    for s in &m.surfaces {
        if anim_iw4::surface_hidden(&s.part_bits, hide, 0) {
            continue;
        }
        let name = s.material.name.to_ascii_lowercase();
        // Scope lenses and reticles need the scope's render path (later).
        if name.contains("lens") || name.contains("reticle") {
            continue;
        }
        let base = mesh.vertices.len() as u32;
        for v in &m.vertices[s.first_vertex as usize..(s.first_vertex + s.vertex_count) as usize] {
            let mut bones = [0u8; 4];
            for (o, b) in bones.iter_mut().zip(v.bones) {
                *o = u8::try_from(bone_base + b as usize).map_err(|_| format!("{}: more than 256 bones", m.name))?;
            }
            mesh.vertices.push(MeshVertex {
                position: v.position,
                normal: v.normal,
                tangent: [v.tangent[0], v.tangent[1], v.tangent[2], v.binormal_sign],
                uv: v.uv,
                bones,
                weights: v.weights,
            });
        }
        let first_index = mesh.indices.len() as u32;
        for &i in &m.indices[s.first_index as usize..(s.first_index + s.index_count) as usize] {
            mesh.indices.push(base + i - s.first_vertex);
        }
        mesh.draws.push(MeshDraw { first_index, index_count: s.index_count, material: s.material.clone() });
    }
    Ok(())
}

// ---- view sway (render_anim view_sway) --------------------------------------

#[derive(Clone, Debug, Default)]
struct Sway {
    look: w::LookSway,
    springs: w::SwaySpringState,
}

impl Sway {
    fn advance(&mut self, f: &PlacementFacts, angles: [f32; 3], frac: f32, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let config = w::sway_config();
        let sample = [f64::from(angles[0]), f64::from(angles[1])];
        if f.overlay_reticle != 0 && frac > 0.0 {
            if self.look.observe_only(sample, f64::from(dt), config) {
                self.springs = w::SwaySpringState::default();
            }
            return;
        }
        if !self.look.sample(sample, f64::from(dt), config) {
            return;
        }
        let params = if f.aim_down_sight { w::lerp_sway_params(f.sway_hip, f.sway_ads, frac) } else { f.sway_hip };
        self.springs = w::sway_from_drive(self.look.drive(config), params, 1.0);
    }
}

// ---- the rig ------------------------------------------------------------------

/// One held weapon's first-person rig.
pub struct Rig {
    pub weapon: u32,
    dobj: DObj,
    view_bone: usize,
    tracks: Vec<Option<Vec<Option<usize>>>>,
    controller: Controller,
    facts: PlacementFacts,
    placement: w::WeaponPlacementState,
    sway: Sway,
    mesh: Arc<Mesh>,
    origin: [f32; 3],
    angles: [f32; 3],
    ads: f32,
    /// Notetracks crossed since the last `take_notes` (sound aliases, through
    /// the weapon's notetrack sound map).
    notes: Vec<String>,
    /// The loaded rocket (its rocketModel, merged onto the gun's bones of the same names: the
    /// RPG-7's tag_clip, which the reload moves): per rocket bone, the rig bone it follows. Its
    /// vertices use extra bones after the rig's, copies of those, zeroed while the clip is empty.
    rocket_bones: Option<Vec<usize>>,
    rocket_shown: bool,
}

/// IW view axes (x forward, y left, z up) to D3D view axes (x right, y up, z forward).
fn iw_to_d3d() -> Mat4 {
    Mat4::from_cols(
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(-1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::W,
    )
}

impl Rig {
    /// The hands with `gun` on `tag_weapon`. `clips[slot]` are the weapon's
    /// szXAnims (None where the weapon has none).
    pub fn build(
        weapon: u32,
        hands: &vm::Model,
        gun: &vm::Model,
        hide_tags: &[String],
        clips: Vec<Option<Arc<AnimClip>>>,
        timers: Timers,
        facts: PlacementFacts,
    ) -> Result<Rig, String> {
        Self::build_with_rocket(weapon, hands, gun, None, hide_tags, clips, timers, facts)
    }

    /// As `build`, with the weapon's rocketModel merged onto the gun by bone name (MW2 attaches it
    /// without a tag: the RPG-7's rocket follows the gun's tag_clip), drawn while the clip is
    /// loaded; a rocket naming a bone the gun lacks is left out.
    #[allow(clippy::too_many_arguments)]
    pub fn build_with_rocket(
        weapon: u32,
        hands: &vm::Model,
        gun: &vm::Model,
        rocket: Option<&vm::Model>,
        hide_tags: &[String],
        clips: Vec<Option<Arc<AnimClip>>>,
        timers: Timers,
        facts: PlacementFacts,
    ) -> Result<Rig, String> {
        let (hp, gp) = (pose_src(hands), pose_src(gun));
        let dobj = DObj::build(&[(&hp, None), (&gp, Some(Attach { parent_model: 0, tag: "tag_weapon".into() }))])
            .map_err(|e| format!("{} + {}: {e}", hands.name, gun.name))?;
        let view_bone = dobj.find("tag_view").ok_or_else(|| format!("{} has no tag_view", hands.name))?;
        let mut mesh = Mesh::default();
        append_model(&mut mesh, hands, 0, &[0; 6])?;
        append_model(&mut mesh, gun, hands.bone_names.len(), &hidden_words(gun, hide_tags))?;
        let mut rocket_bones = None;
        if let Some(r) = rocket {
            // Each rocket bone onto the gun's bone of that name, its vertices carried from the rocket's
            // bind pose into the gun's (they match when authored together).
            let follow: Option<Vec<usize>> =
                r.bone_names.iter().map(|n| gun.bone_names.iter().position(|g| g.eq_ignore_ascii_case(n))).collect();
            if let Some(follow) = follow.filter(|f| !f.is_empty() && f.len() == r.base_mat.len()) {
                let bind = |(q, t): &([f32; 4], [f32; 3])| {
                    Mat4::from_rotation_translation(
                        Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize(),
                        Vec3::from_array(*t),
                    )
                };
                let carry: Vec<Mat4> = follow
                    .iter()
                    .zip(&r.base_mat)
                    .map(|(&g, rb)| bind(&gun.base_mat[g]) * bind(rb).inverse())
                    .collect();
                append_merged(&mut mesh, r, dobj.bone_count(), &carry)?;
                let base = hands.bone_names.len();
                rocket_bones = Some(follow.iter().map(|g| base + g).collect());
            }
        }
        let mut clips = clips;
        clips.resize(SLOTS, None);
        let tracks = clips.iter().map(|c| c.as_ref().map(|c| dobj.tracks_for(c))).collect();
        Ok(Rig {
            weapon,
            dobj,
            view_bone,
            tracks,
            controller: Controller::new(clips, timers),
            facts,
            placement: w::WeaponPlacementState::default(),
            sway: Sway::default(),
            mesh: Arc::new(mesh),
            origin: [0.0; 3],
            angles: [0.0; 3],
            ads: 0.0,
            notes: Vec::new(),
            rocket_bones,
            rocket_shown: true,
        })
    }

    /// Whether the rig carries a rocketModel (tests and the log).
    pub fn has_rocket(&self) -> bool {
        self.rocket_bones.is_some()
    }

    /// The notetracks crossed since the last call.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    pub fn mesh(&self) -> &Arc<Mesh> {
        &self.mesh
    }

    pub fn bone_count(&self) -> usize {
        self.dobj.bone_count()
    }

    pub fn state(&self) -> VmState {
        self.controller.state
    }

    /// The ADS zoom field of view (degrees, MW2's horizontal 4:3) and the ADS fraction.
    pub fn zoom(&self) -> (f32, f32) {
        (self.facts.ads_zoom_fov, self.ads)
    }

    /// One frame: the weapon's events into the animation, then the placement.
    pub fn tick(&mut self, player: &Mw2Player, frame: &GunFrame, dt: f32) {
        let ps = &player.ps;
        let dt = dt.clamp(0.0, 0.2);
        if frame.fired {
            self.controller.fire();
        }
        self.controller.weap_anim(frame.weap_anim, frame.clip == 0);
        // MW2 draws the rocket while the clip holds one: gone when fired, back mid-reload (when the
        // reload adds the round, reloadAddTime).
        self.rocket_shown = frame.clip > 0;
        let notes = self.controller.advance(dt);
        self.notes.extend(notes);
        let frac = ps.f_weapon_pos_frac.clamp(0.0, 1.0);
        self.ads = frac;
        self.controller.ads_overlay(frac);

        // Placement (render_anim fpv_present `apply_fpv_placement`).
        let f = &self.facts;
        if let Some(peak) = frame.gun_kick {
            for (axis, amplitude) in self.placement.gun_recoil.axes.iter_mut().zip(peak) {
                axis.add_peak(f64::from(amplitude), w::MODEL_PEAK_SECONDS);
            }
        }
        w::calculate_weapon_position_gun_recoil(
            &mut self.placement.gun_recoil,
            dt,
            frac,
            true,
            w::GunRecoilResponse::default(),
            w::GunRecoilResponse::default(),
            f.gun_max,
        );
        self.sway.advance(f, ps.viewangles, frac, dt);
        self.placement.sway_springs = self.sway.springs;
        let xyspeed = (ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1]).sqrt();
        let kinematics = w::WeaponMovementKinematics {
            xyspeed,
            speed: ps.speed as f32,
            velocity: ps.velocity,
            viewangles: ps.viewangles,
            weaponstate: ps.weaponstate_primary,
            weaponstate_secondary: 0,
            pm_flags: ps.pm_flags,
            frametime: dt,
        };
        let waveform = w::calculate_weapon_movement_bob_waveform(w::WeaponBobWaveformInputs {
            bob_cycle: (ps.bob_cycle as u32 & 0xff) as u8,
            xyspeed,
            view_height_target: ps.view_height_target,
            pm_flags: ps.pm_flags,
            weapon_pos_frac: frac,
        });
        let ps_in = w::WeaponPlacementPsInputs {
            e_flags: ps.e_flags,
            weapon_pos_frac: frac,
            weapon_time: ps.weapon_time,
            aim_down_sight: f.aim_down_sight,
            overlay_reticle: f.overlay_reticle,
            weapon_transition_active: false,
            lean_fraction: 0.0,
        };
        let mut steps = [w::WeaponPlacementAssembleStep::Sway; w::PLACEMENT_ASSEMBLE_STEP_COUNT];
        self.placement.damage_kick_time = 0;
        // Gun recoil was advanced above; the assembly only reads it (dt 0, as iw4L).
        let contrib = w::weapon_placement_assemble(
            &mut self.placement,
            ps_in,
            f.stance,
            w::StanceTransitionFadeGlobals::default(),
            f.movement,
            kinematics,
            f.bob,
            f.idle,
            Some(waveform),
            w::GunRecoilResponse::default(),
            w::GunRecoilResponse::default(),
            f.gun_max[0],
            f.gun_max[1],
            0.0,
            &mut steps,
        );
        self.origin = contrib.origin;
        self.angles = contrib.angles;
    }

    /// Skinning matrices (bind model space to D3D view space, inches), one per
    /// bone, as 3x4 row-major (rows x, y, z: [m0 m1 m2 tx]).
    pub fn skin(&self) -> Vec<[f32; 12]> {
        let instances: Vec<AnimInstance<'_>> = self
            .controller
            .tree
            .active()
            .filter_map(|(s, clip, time, weight)| {
                Some(AnimInstance { clip, tracks: self.tracks.get(s)?.as_deref()?, time, weight, parts: None })
            })
            .collect();
        let world = self.dobj.pose(&instances, &self.dobj.all_parts(), Mat4::IDENTITY);
        let skin = self.dobj.skin_matrices(&world);
        let [fwd, left, up] = math_iw4::angles_to_axis(self.angles);
        let [o_fwd, o_right, o_up] = self.origin;
        let placement = Mat4::from_cols(
            Vec3::from_array(fwd).extend(0.0),
            Vec3::from_array(left).extend(0.0),
            Vec3::from_array(up).extend(0.0),
            Vec4::new(o_fwd, -o_right, o_up, 1.0),
        );
        let eye = iw_to_d3d() * placement * world[self.view_bone].inverse();
        // The rocket's extra bones: copies of the gun bones it follows, or (unloaded) None: collapsed
        // to a point, so nothing is drawn.
        let extra = self.rocket_bones.as_deref().unwrap_or(&[]);
        skin.iter()
            .map(|m| Some(*m))
            .chain(extra.iter().map(|&b| self.rocket_shown.then(|| skin[b])))
            .map(|m| {
                let Some(m) = m else { return [0.0; 12] };
                let m = eye * m;
                let c = m.to_cols_array_2d();
                [
                    c[0][0], c[1][0], c[2][0], c[3][0], c[0][1], c[1][1], c[2][1], c[3][1], c[0][2], c[1][2], c[2][2],
                    c[3][2],
                ]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
