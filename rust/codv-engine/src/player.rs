//! The player as Modern Warfare 2 moves it: a retail `playerState_s` stepped
//! by iw4L's `movement_iw4::pmove` with one retail `usercmd_s` per frame.
//!
//! The spawn state and the pmove context values are the ones iw4L's own
//! simulation uses (`sim/src/world.rs` `spawn_player_state` and
//! `sim/src/step.rs` `pmove_context`, at the pinned commit), so standing
//! height, speeds, sprint time, jump height and stance bounds are MW2's.
use crate::world::{Backend, World};
use movement_iw4::{
    AdsFracContext, AdsIntentContext, AirMoveContext, CmdScaleWalkContext, FlatMantleAnimLength, JumpLaunchContext,
    MantleRootDelta, MeleeChargeWeaponDelays, MoveBounds, PmoveSingleContext, SprintContext, ViewAngleClamp,
    WalkMoveContext, get_max_sprint_time,
};
use playerstate_iw4::{PlayerState, UserCmd, pm_flags};

/// iw4L's ENTITYNUM_NONE for `ground_entity_num`: the player is in the air.
const NO_GROUND: i32 = 0x7ff;
/// Retail player hull and trace mask (sim/src/step.rs).
pub const PLAYER_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
pub const PLAYER_MAXS: [f32; 3] = [15.0, 15.0, 70.0];
pub const PLAYER_TRACE_MASK: u32 = 0x0281_0011;
/// usercmd_s carries at most 200 ms of movement per command.
const MAX_MSEC: u32 = 200;
/// Command time at spawn. Retail timers compare against server time (the jump
/// gate refuses a jump within 500 ms of `jump_time`, which starts at 0), and a
/// live server's clock is never near zero when a player spawns.
const SPAWN_TIME: i32 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    Stand,
    Crouch,
    Prone,
}

/// One frame of player intent.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveInput {
    pub msec: u32,
    /// -127 .. 127: forward/back and right/left (retail usercmd_s).
    pub forward: i8,
    pub right: i8,
    /// Retail button bits (`playerstate_iw4::buttons`).
    pub buttons: u32,
    /// View change this frame, IW4 degrees: yaw grows to the left, pitch grows down.
    pub yaw_delta: f32,
    pub pitch_delta: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveOutput {
    /// Feet position and velocity, IW4 units (and units per second).
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// pitch (down positive), yaw, roll; IW4 degrees.
    pub view_angles: [f32; 3],
    /// Eye height above the feet, IW4 units.
    pub view_height: f32,
    pub on_ground: bool,
    pub stance: Stance,
    pub sprinting: bool,
    pub mantling: bool,
    pub jumping: bool,
    /// CODV's slide is under way.
    pub sliding: bool,
    /// CODV's lean: how far the eye is moved sideways (IW4 units, world axes) and the view's
    /// roll (degrees, right positive), both 0 upright.
    pub lean_offset: [f32; 3],
    pub lean_roll: f32,
    /// Fall damage pmove reported this frame (retail health points).
    pub fall_damage: i32,
}

/// The weapon side of the pmove context (iw4L sim `pmove_context`): the MW2
/// weapon in hand (`arms::Gunner::arm`) sets it every frame; unarmed, the
/// defaults.
#[derive(Clone, Copy, Debug)]
pub struct WeaponMoveFacts {
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
    pub sprint_duration_scale: f32,
    /// ADS allowed right now (weapon_iw4::is_ads_allowed).
    pub ads_allowed: bool,
    pub aim_down_sight: bool,
    pub ads_in_rate: f32,
    pub ads_out_rate: f32,
    pub ads_reload_trans_time_ms: i32,
    pub rechamber_while_ads: bool,
    pub ads_fire_only: bool,
    /// A scoped weapon (overlay reticle).
    pub scope: bool,
    pub can_hold_breath: bool,
    pub segmented_reload: bool,
    pub melee_delay_ms: i32,
    pub melee_charge_delay_ms: i32,
    /// The weapon is reloading: no sprint until it finishes. MW2 lets a sprint
    /// cancel a reload (weapon_iw4 drops the reload for the sprint-in); CODV
    /// keeps the reload and sprints after it, the sprint button still held.
    pub hold_sprint: bool,
    /// The weapon is reloading and may sprint meanwhile (CODV's reload while sprinting): a held
    /// sprint button sprints at once, the reload goes on (Gunner::reload_sprint).
    pub reload_sprint: bool,
}

impl Default for WeaponMoveFacts {
    fn default() -> Self {
        WeaponMoveFacts {
            move_speed_scale: 1.0,
            ads_move_speed_scale: 1.0,
            sprint_duration_scale: 1.0,
            ads_allowed: false,
            aim_down_sight: false,
            // iw4L sim's unarmed rates (1 / 200 ms).
            ads_in_rate: 1.0 / 200.0,
            ads_out_rate: 1.0 / 200.0,
            ads_reload_trans_time_ms: 0,
            rechamber_while_ads: true,
            ads_fire_only: false,
            scope: false,
            can_hold_breath: false,
            segmented_reload: false,
            melee_delay_ms: 0,
            melee_charge_delay_ms: 0,
            hold_sprint: false,
            reload_sprint: false,
        }
    }
}

/// What the last pmove told the character animation (iw4L sim/src/step.rs
/// reads the same after each pmove): the move type, the stance, jump and
/// landing events, and the usercmd's move and buttons.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimStep {
    pub movetype: Option<u8>,
    pub stance_event: Option<u8>,
    pub reset_torso: bool,
    /// ANIM_ET_JUMP (3) / JUMPBK (4), forced or not.
    pub jumps: [Option<(u8, bool)>; 4],
    pub landing: bool,
    /// A mantle chose the move type: the script must take it now.
    pub force_movement: bool,
    pub forward: i8,
    pub right: i8,
    pub buttons: u32,
    pub msec: i32,
}

/// Sprint over aim: how long after the sprint press an aim held through it waits for the sprint.
const SPRINT_OVER_ADS_GRACE_MS: i32 = 250;

pub struct Mw2Player {
    pub ps: PlayerState,
    pub weapon: WeaponMoveFacts,
    /// The character animation inputs of the last step.
    pub anim: AnimStep,
    cmd_angles: [f32; 3],
    old_buttons: u32,
    /// CODV's sprint and aim, the last pressed wins while both are held (MW2 walked with neither):
    /// a sprint pressed while aiming sprints, and the aim is back when the sprint ends (the aim still
    /// held); an aim pressed while the sprint is held aims, and the sprint is back when the aim is let go.
    /// The weapon tick takes the aim's half too.
    pub sprint_over_ads: bool,
    sprint_over_ads_at: i32,
    ads_over_sprint: bool,
    /// The input's buttons last step (the press edges).
    raw_old_buttons: u32,
    time: i32,
    /// How many times the player was freed from overlapping the world (debug line).
    pub unstuck: u32,
    /// Where the hull last stood free (after a step), to go back to when stuck.
    last_free: Option<[f32; 3]>,
    /// The last step was mid-mantle.
    was_mantling: bool,
    /// Jump height, units (MW2's jump_height dvar: 39). The host may raise it.
    pub jump_height: f32,
    /// Sprint length before the weapon's scale, seconds (MW2's player_sprintTime: 4). The host may raise it.
    pub sprint_seconds: f32,
    /// How fast the sprint meter refills, over MW2's (1.0: a second's sprint back in a second).
    pub sprint_recharge: f32,
    sprint_recharge_carry: f32,
    /// Fall damage from the first height to death at the second (units); None: no fall damage.
    /// MW2's bg_fallDamageMinHeight / MaxHeight (128 / 300) unless the host sets others.
    pub fall_heights: Option<(f32, f32)>,
    /// The highest the feet were since the player left the ground (None: on it).
    air_peak: Option<f32>,
    /// CODV's slide (not MW2's): crouch pressed while sprinting on the ground; off: never starts.
    pub slide_enabled: bool,
    /// Sprint speed over the run speed (MW2's player_sprintSpeedScale: 1.5). The host may raise it.
    pub sprint_speed_scale: f32,
    /// MW2's jump slowdown (jump_slowdownEnable): a landing cuts the speed to 0.67 and the ground
    /// drags for 1.2 s. Off (CODV's default), a landing that does not hurt keeps the speed it had.
    pub jump_slowdown: bool,
    /// CODV's parkour: sprinting into a ledge MW2 can mantle (up to 60 units) mantles without the jump
    /// key (MW2's automantle perk), at `mantle_speed` times MW2's pace (1: MW2's 400 / 200 ms).
    pub auto_vault: bool,
    pub mantle_speed: f32,
    /// The run speed the current mantle started from (CODV keeps it through the mantle, as a vault).
    mantle_entry: Option<f32>,
    /// Keep the speed through a mantle (CODV's vault) instead of MW2's dead stop.
    pub mantle_momentum: bool,
    /// CODV's snag guard: a run that loses most of its speed in one step with nothing ahead (an edge
    /// of a car, a fence's corner, a mesh seam) keeps it.
    pub snag_guard: bool,
    /// CODV: the sprint key is held, not pressed: whatever ended a sprint (a weapon switch, firing,
    /// aiming, a landing, a mantle), it starts again as soon as it can while the key stays down
    /// (MW2 waits for the key to be released and pressed again). Off: MW2's.
    pub hold_to_sprint: bool,
    /// Steps the guard has restored (tests, the debug line).
    pub snags: u32,
    /// The view's step smoothing (units, added to the eye): a step up or down a stair moves the feet at
    /// once and the eye after them (cg's step smoothing), so stairs do not shake the view.
    step_smooth: f32,
    /// The snag guard's last restore (where, when) and a pause after restores that made no headway
    /// (the guard and a car's side taking turns every frame: a shake in place).
    snag_last: Option<([f32; 3], i32)>,
    snag_pause_until: i32,
    /// The slide's tuning: speed over the sprint at its start, and how long it lasts (ms).
    pub slide_boost: f32,
    pub slide_ms: i32,
    /// Auto vault only over obstacles at least this tall (units; under MW2's 20-unit mantle: all).
    pub vault_min_height: f32,
    /// The high climb's top (units over the feet; at most 120).
    pub climb_max: f32,
    /// CODV's lean (CoD4's, which MW2's multiplayer dropped): holding lean left / right moves the
    /// eye `lean_dist` units sideways with a roll; off: the buttons do nothing.
    pub lean_enabled: bool,
    pub lean_dist: f32,
    /// Movement sounds since the last `take_foley`.
    foley: Vec<Foley>,
    event_seen: i32,
    /// CODV's climb for ledges above MW2's mantle (60 to 96 units: vans, walls): jump at one.
    pub high_climb: bool,
    climb: Option<Climb>,
    /// CODV's dive: holding crouch through a slide throws the player forward to land prone.
    pub dive_enabled: bool,
    diving: bool,
    slide: Option<Slide>,
    slide_ended: i32,
    crouch_held: bool,
}

/// A movement sound (the runtime plays MW2's aliases for them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Foley {
    /// A footstep: MW2's walk, run, sprint or prone step.
    Step(StepKind),
    /// A landing; `hard` when it hurt.
    Land { hard: bool },
    /// A mantle, vault or climb starting.
    Mantle,
    /// A slide starting.
    Slide,
    /// A dive leaving the ground.
    Dive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Walk,
    Run,
    Sprint,
    Prone,
}

/// A climb under way: from the start up `rise` units, then forward onto the ledge.
#[derive(Clone, Copy, Debug)]
struct Climb {
    from: [f32; 3],
    to: [f32; 3],
    ms: i32,
}

/// The corner slip's sideways run, units per second.
const SLIP_SPEED: f32 = 150.0;

/// The snag guard steps in when a step keeps less than this share of the speed.
const SNAG_KEEP: f32 = 0.85;

/// CODV's lean buttons (bits MW2's usercmd leaves free; CoD4's BUTTON_LEANLEFT / RIGHT).
pub const LEAN_LEFT: u32 = 0x40;
pub const LEAN_RIGHT: u32 = 0x80;
/// The lean: sideways reach of the head (units) and the time to lean fully (ms).
pub const LEAN_DIST: f32 = 16.0;
/// The hips the upper body turns about, below the eye (units), and how much of that tilt the view
/// rolls (the head keeps itself a little more upright than the body).
const LEAN_PIVOT: f32 = 44.0;
const LEAN_ROLL_SHARE: f32 = 0.6;
const LEAN_MS: f32 = 220.0;
/// Cover a lean may start beside: this far past the hull (units).
const LEAN_COVER_REACH: f32 = 24.0;

/// Auto vault from this speed up (units per second: a run is 190, a sprint 285).
const AUTO_VAULT_SPEED: f32 = 150.0;
/// The snag guard rests this long after restores that made no headway.
const SNAG_PAUSE_MS: i32 = 500;
/// The auto vault: the run's direction and the obstacle's face within about 25 degrees of the view, the obstacle at least
/// this tall (units; lower ones are stepped or jumped over, not vaulted).
const AUTO_VAULT_HEAD_ON: f32 = 0.9;
const AUTO_VAULT_MIN_HEIGHT: f32 = 24.0;

/// CODV's climb: ledges this high above the feet (units: over MW2's 60, about 1.5 to 2.4 m), in this time.
const CLIMB_MIN: f32 = 62.0;
const CLIMB_MAX: f32 = 96.0;
const CLIMB_MS: i32 = 650;
/// How far ahead the ledge may be (units, from the hull's front).
const CLIMB_REACH: f32 = 20.0;
/// The dive: forward speed (at least) and the lift, units per second.
const DIVE_SPEED: f32 = 330.0;
const DIVE_LIFT: f32 = 170.0;

/// A slide under way.
#[derive(Clone, Copy, Debug)]
struct Slide {
    ms: i32,
    /// Horizontal direction (unit) and the speed it started at, units per second.
    dir: [f32; 2],
    start: f32,
    /// The speed it was given last step (a wall that stops it ends it).
    given: f32,
}

/// The slide (modern Call of Duty's, tuned to MW2's speeds): from a sprint of
/// at least `SLIDE_MIN_SPEED`, 1.3x the speed (at most `SLIDE_MAX_SPEED`)
/// easing to `SLIDE_END_SPEED` over `SLIDE_MS`, crouched, steering slowly
/// towards the view; jumping or standing ends it.
pub const SLIDE_MS: i32 = 750;
const SLIDE_MIN_SPEED: f32 = 200.0;
const SLIDE_BOOST: f32 = 1.3;
const SLIDE_MAX_SPEED: f32 = 400.0;
const SLIDE_END_SPEED: f32 = 110.0;
const SLIDE_COOLDOWN_MS: i32 = 400;
/// Degrees per second the slide turns towards the view.
const SLIDE_STEER: f32 = 50.0;
/// pmove's ground friction (PM_Friction: 5.5 x speed per second).
const GROUND_FRICTION: f32 = 5.5;

/// MW2's bg_fallDamageMinHeight and bg_fallDamageMaxHeight (units).
pub const MW2_FALL_HEIGHTS: (f32, f32) = (128.0, 300.0);

/// Fall damage for a drop of `height` units with heights `(min, max)`: none up
/// to `min`, 100 (death) from `max`, linear between (MW2's BG_FallDamage).
pub fn fall_damage(height: f32, (min, max): (f32, f32)) -> i32 {
    if !height.is_finite() || height <= min {
        return 0;
    }
    if height >= max || max <= min {
        return 100;
    }
    ((height - min) / (max - min) * 100.0) as i32
}

/// MW2's jump_height.
pub const MW2_JUMP_HEIGHT: f32 = 39.0;
/// MW2's player_sprintSpeedScale.
pub const MW2_SPRINT_SPEED_SCALE: f32 = 1.5;
/// MW2's player_sprintTime, seconds.
pub const MW2_SPRINT_SECONDS: f32 = 4.0;

/// The mantle's root motion (MW2 reads it from the mp_mantle_* animations'
/// root track; iw4L's `CreateAnimsMantleRootDelta` moves in a straight line
/// to the same end): up the wall first, then onto the ledge, so the view
/// rises over the edge instead of cutting through it. The end deltas are
/// iw4L's `create_anims_end_delta`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClimbRootDelta;

impl MantleRootDelta for ClimbRootDelta {
    fn abs_delta(&self, _fast: bool, anim_index: i32, frac: f32) -> [f32; 3] {
        let end = movement_iw4::mantle::create_anims_end_delta(anim_index);
        let f = frac.clamp(0.0, 1.0);
        let smooth = |t: f32| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let (along, up) = if (1..=7).contains(&anim_index) {
            // Up: rise over the first 55 %, forward from 25 % (a vault keeps moving).
            (smooth((f - 0.25) / 0.75), smooth(f / 0.55))
        } else {
            // Over: forward all the way, the drop at the end.
            (f, f * f)
        };
        [end[0] * along, end[1] * along, end[2] * up]
    }
}

/// The hull's height for the stance pmove will use.
fn hull_maxs_z(ps: &PlayerState) -> f32 {
    if ps.pm_flags & pm_flags::PRONE != 0 {
        movement_iw4::PRONE_MAXS_Z
    } else if ps.pm_flags & pm_flags::CROUCH != 0 {
        movement_iw4::CROUCH_MAXS_Z
    } else {
        movement_iw4::STAND_MAXS_Z
    }
}

/// Retail ANGLE2SHORT, as usercmd_s carries view angles.
fn angle_to_short(a: f32) -> i32 {
    ((a * (65536.0 / 360.0)) as i32) & 0xffff
}

impl Mw2Player {
    /// A player standing at `origin` (feet, IW4 units) facing `yaw`, with the
    /// state iw4L's simulation spawns players in.
    pub fn new(origin: [f32; 3], yaw: f32) -> Mw2Player {
        let mut ps = PlayerState::ZERO;
        ps.origin = origin;
        ps.viewangles = [0.0, yaw, 0.0];
        // usercmd angles start at zero; delta_angles carries the spawn facing.
        ps.delta_angles = [0.0, yaw, 0.0];
        ps.speed = 190;
        ps.health = 100;
        ps.max_health = 100;
        ps.move_speed_scale_multiplier = 1.0;
        ps.view_height_target = 60;
        ps.view_height_current = 60.0;
        ps.gravity = 800;
        ps.ground_entity_num = 0x7fe;
        ps.other_flags |= playerstate_iw4::other_flags::PLAYER;
        ps.corpse_index = -1;
        ps.command_time = SPAWN_TIME;
        Mw2Player {
            ps,
            weapon: WeaponMoveFacts::default(),
            anim: AnimStep::default(),
            cmd_angles: [0.0; 3],
            old_buttons: 0,
            sprint_over_ads: false,
            sprint_over_ads_at: 0,
            ads_over_sprint: false,
            raw_old_buttons: 0,
            time: SPAWN_TIME,
            unstuck: 0,
            last_free: None,
            was_mantling: false,
            jump_height: MW2_JUMP_HEIGHT,
            sprint_seconds: MW2_SPRINT_SECONDS,
            sprint_recharge: 1.0,
            sprint_recharge_carry: 0.0,
            fall_heights: Some(MW2_FALL_HEIGHTS),
            air_peak: None,
            slide_enabled: true,
            sprint_speed_scale: MW2_SPRINT_SPEED_SCALE,
            jump_slowdown: true,
            auto_vault: false,
            mantle_speed: 1.0,
            mantle_entry: None,
            mantle_momentum: false,
            snag_guard: false,
            hold_to_sprint: true,
            snags: 0,
            step_smooth: 0.0,
            snag_last: None,
            snag_pause_until: 0,
            slide_boost: SLIDE_BOOST,
            slide_ms: SLIDE_MS,
            vault_min_height: 0.0,
            climb_max: CLIMB_MAX,
            foley: Vec::new(),
            event_seen: 0,
            lean_enabled: false,
            lean_dist: LEAN_DIST,
            high_climb: false,
            climb: None,
            dive_enabled: false,
            diving: false,
            slide: None,
            slide_ended: SPAWN_TIME - SLIDE_COOLDOWN_MS,
            crouch_held: false,
        }
    }

    /// Moves the player somewhere else (teleport, hand-back from GTA), at rest.
    pub fn place(&mut self, origin: [f32; 3], yaw: f32) {
        self.ps.origin = origin;
        self.ps.velocity = [0.0; 3];
        self.ps.delta_angles = [self.ps.delta_angles[0], yaw - self.cmd_angles[1], 0.0];
        self.ps.viewangles = [self.ps.viewangles[0], yaw, 0.0];
        self.ps.ground_entity_num = NO_GROUND;
        self.last_free = None;
        self.air_peak = None;
        self.slide = None;
        self.climb = None;
        self.diving = false;
    }

    /// Feet position, IW4 units.
    pub fn origin(&self) -> [f32; 3] {
        self.ps.origin
    }

    /// Moves the player by `d` (IW4 units) without touching its motion: the
    /// host re-centring the frame it keeps the player in. `origin` is the
    /// only position a retail playerState_s carries (mantles keep a yaw and
    /// a timer, ladders a direction), so nothing else needs moving.
    pub fn shift(&mut self, d: [f32; 3]) {
        for (o, d) in self.ps.origin.iter_mut().zip(d) {
            *o += d;
        }
        self.ps.jump_origin_z += d[2];
        if let Some(b) = self.last_free.as_mut() {
            for (o, d) in b.iter_mut().zip(d) {
                *o += d;
            }
        }
    }

    fn context(&self) -> PmoveSingleContext {
        // sim/src/step.rs pmove_context, unarmed until F3.
        let air = AirMoveContext {
            player_spectate_speed_scale: 1.0,
            shellshock_gravity_scale: 1.0,
            shellshock_gravity_bias: 0.0,
        };
        PmoveSingleContext {
            walk: WalkMoveContext {
                cmd_scale: CmdScaleWalkContext {
                    player_back_speed_scale: 0.7,
                    player_strafe_speed_scale: 0.8,
                    player_sprint_speed_scale: if self.sprint_speed_scale.is_finite() {
                        self.sprint_speed_scale.clamp(1.0, 2.5)
                    } else {
                        MW2_SPRINT_SPEED_SCALE
                    },
                    player_last_stand_crawl_speed_scale: 0.15,
                    weapon_move_speed_scale: self.weapon.move_speed_scale,
                    weapon_ads_move_speed_scale: self.weapon.ads_move_speed_scale,
                    shellshock_affects_movement: false,
                },
                weapon_move_scale: 1.0,
                old_buttons: self.old_buttons,
                jump: JumpLaunchContext {
                    jump_height: if self.jump_height.is_finite() {
                        self.jump_height.clamp(MW2_JUMP_HEIGHT * 0.5, MW2_JUMP_HEIGHT * 2.0)
                    } else {
                        MW2_JUMP_HEIGHT
                    },
                    dive: false,
                    crouch_jump_scale: 1.0,
                    jump_ladder_push_vel: 128.0,
                },
                air,
            },
            air,
            bounds: MoveBounds { mins: PLAYER_MINS, maxs: PLAYER_MAXS, tracemask: PLAYER_TRACE_MASK },
            view_angles: ViewAngleClamp { pitch_up: 85.0, pitch_down: 85.0, unclamped_pitch_bit: false },
            sprint: SprintContext {
                weapon_max_sprint_time: get_max_sprint_time(
                    self.weapon.sprint_duration_scale,
                    if self.sprint_seconds.is_finite() {
                        self.sprint_seconds.clamp(1.0, 16.0)
                    } else {
                        MW2_SPRINT_SECONDS
                    },
                ),
                sprint_forever: false,
                min_sprint_time_seconds: 1.0,
                sprint_delay_seconds: 0.0,
                sprint_forward_minimum: 105,
                stand_up_clear: true,
                sprint_recharge_pause_seconds: 0.0,
            },
            ads_intent: AdsIntentContext {
                ads_allowed: self.weapon.ads_allowed,
                weapon_def_scope: self.weapon.scope,
                sprint_hold_ads: false,
            },
            ads_frac: AdsFracContext {
                aim_down_sight: self.weapon.aim_down_sight,
                ads_in_rate: self.weapon.ads_in_rate,
                ads_out_rate: self.weapon.ads_out_rate,
                ads_reload_trans_time_ms: self.weapon.ads_reload_trans_time_ms,
                segmented_reload: self.weapon.segmented_reload,
                rechamber_while_ads: self.weapon.rechamber_while_ads,
                ads_fire_only: self.weapon.ads_fire_only,
            },
            melee_charge: MeleeChargeWeaponDelays {
                melee_delay_ms: self.weapon.melee_delay_ms,
                melee_charge_delay_ms: self.weapon.melee_charge_delay_ms,
            },
            player_melee_range: movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
            old_buttons: self.old_buttons,
            weapon_blocks_prone: false,
            can_hold_breath: self.weapon.can_hold_breath,
        }
    }

    /// One frame: builds the usercmd and runs iw4L's pmove against `world`.
    pub fn step<W: World + ?Sized>(&mut self, input: &MoveInput, world: &W) -> MoveOutput {
        let msec = input.msec.min(MAX_MSEC) as i32;
        self.time = self.time.wrapping_add(msec);
        let finite = |x: f32| if x.is_finite() { x } else { 0.0 };
        self.cmd_angles[0] = finite(self.cmd_angles[0] + finite(input.pitch_delta)) % 360.0;
        self.cmd_angles[1] = finite(self.cmd_angles[1] + finite(input.yaw_delta)) % 360.0;
        let mut cmd = UserCmd {
            server_time: self.time,
            buttons: input.buttons,
            angles: [angle_to_short(self.cmd_angles[0]), angle_to_short(self.cmd_angles[1]), 0],
            forwardmove: input.forward,
            rightmove: input.right,
            ..UserCmd::default()
        };
        // Sprint and aim: the last pressed wins while both are held.
        let (ads, sprint) = (input.buttons & button::ADS != 0, input.buttons & button::SPRINT != 0);
        let sprint_press = sprint && self.raw_old_buttons & button::SPRINT == 0;
        let ads_press = ads && self.raw_old_buttons & button::ADS == 0;
        let sprint_release = !sprint && self.raw_old_buttons & button::SPRINT != 0;
        self.raw_old_buttons = input.buttons;
        if !ads {
            self.sprint_over_ads = false;
            self.ads_over_sprint = false;
        } else if sprint_press && input.forward > 0 {
            self.sprint_over_ads = true;
            self.ads_over_sprint = false;
            self.sprint_over_ads_at = self.time;
        } else if ads_press {
            // Aim pressed: it ends the sprint (MW2); a held sprint (CODV's) waits for the aim to go.
            self.sprint_over_ads = false;
            self.ads_over_sprint = sprint && self.hold_to_sprint;
        }
        if !sprint {
            self.ads_over_sprint = false;
        }
        // The held sprint let go over a held aim: the sprint ends and the aim is back (CODV's held sprint;
        // MW2's pressed sprint runs on until the aim is pressed again).
        if self.sprint_over_ads && sprint_release && self.hold_to_sprint {
            movement_iw4::end_sprint(&mut self.ps, &cmd);
            self.sprint_over_ads = false;
        }
        if self.sprint_over_ads {
            cmd.buttons &= !button::ADS;
        }
        if self.ads_over_sprint {
            movement_iw4::end_sprint(&mut self.ps, &cmd);
            cmd.buttons &= !button::SPRINT;
            // Let go of the aim with the sprint held: sprinting again at once.
            self.ps.sprint_button_up_required = 0;
        }
        if self.weapon.hold_sprint {
            cmd.buttons &= !button::SPRINT;
            movement_iw4::end_sprint(&mut self.ps, &cmd);
            // Held through the reload: sprint again as soon as it ends.
            self.ps.sprint_button_up_required = 0;
        }
        if self.weapon.reload_sprint {
            // The reload press ends a sprint (MW2): the held button sprints again straight away.
            self.ps.sprint_button_up_required = 0;
        }
        if self.hold_to_sprint && cmd.buttons & button::SPRINT != 0 {
            self.ps.sprint_button_up_required = 0;
        }
        // A hull overlapping the world cannot move at all (every sweep starts
        // solid): free it first. Not mid-mantle, whose path crosses the ledge.
        if self.climb.is_none() && self.ps.pm_flags & pm_flags::MANTLE == 0 {
            self.free_hull(world);
        }
        self.climb_before(input, &mut cmd, world);
        self.slide_before(input, &mut cmd, msec, world);
        self.lean_before(&mut cmd, msec, world);
        // CODV's auto vault: MW2's automantle perk while running or sprinting forward (a ledge in
        // the way is mantled without the jump key), at the quicker pace.
        const PERK_AUTOMANTLE: u32 = 0x2;
        let running = self.ps.velocity[0].hypot(self.ps.velocity[1]) > AUTO_VAULT_SPEED;
        // Not onto a vehicle: running beside or past a car facing it mantled it (or half did): a car is
        // climbed with the jump key, as in MW2.
        // Only sprinting head-on into it (the run along the view): brushing a wall, a fence or a curb at
        // an angle, or jogging past one, vaulted it and broke the run.
        let speed = self.ps.velocity[0].hypot(self.ps.velocity[1]);
        let yaw = self.ps.viewangles[1].to_radians();
        let head_on = speed > 1.0
            && (self.ps.velocity[0] * yaw.cos() + self.ps.velocity[1] * yaw.sin()) / speed > AUTO_VAULT_HEAD_ON;
        let vault = self.auto_vault
            && running
            && self.ps.pm_flags & pm_flags::SPRINTING != 0
            && head_on
            && cmd.forwardmove > 0
            && cmd.rightmove.abs() < 64
            && self.tall_enough_to_vault(world)
            && !self.vehicle_ahead(world);
        if vault {
            self.ps.perks[1] |= PERK_AUTOMANTLE;
        } else {
            self.ps.perks[1] &= !PERK_AUTOMANTLE;
        }
        let pace = if self.mantle_speed.is_finite() { self.mantle_speed.clamp(0.5, 3.0) } else { 1.0 };
        let lengths = FlatMantleAnimLength { up_msec: (400.0 / pace) as i32, over_msec: (200.0 / pace) as i32 };
        let airborne = self.ps.ground_entity_num == NO_GROUND;
        let air_velocity = self.ps.velocity;
        let before_step = self.ps.origin;
        let ground_before = self.ps.ground_entity_num;
        let sent_buttons = cmd.buttons;
        let cmd_forward = cmd.forwardmove;
        let context = self.context();
        let result = movement_iw4::pmove(&mut self.ps, &mut cmd, context, &Backend(world), &lengths, &ClimbRootDelta);
        self.old_buttons = sent_buttons;
        // No sprint came of it (or it is over): aiming again, a moment after the press.
        if self.sprint_over_ads
            && self.ps.pm_flags & pm_flags::SPRINTING == 0
            && self.time.wrapping_sub(self.sprint_over_ads_at) > SPRINT_OVER_ADS_GRACE_MS
        {
            self.sprint_over_ads = false;
        }
        self.collect_events();
        let mantling = self.ps.pm_flags & pm_flags::MANTLE != 0;
        if mantling && !self.was_mantling {
            // The spot before the ledge is no place to return to after the climb.
            self.last_free = None;
            self.mantle_entry = Some(air_velocity[0].hypot(air_velocity[1]));
        }
        if !mantling && self.was_mantling {
            // Over the ledge: on at the speed it came in with, along the mantle (CODV's vault).
            if let Some(speed) = self.mantle_entry.take()
                && self.mantle_momentum
                && speed > 1.0
            {
                let yaw = self.ps.mantle_yaw.to_radians();
                let now = self.ps.velocity[0].hypot(self.ps.velocity[1]);
                if now < speed {
                    self.ps.velocity[0] = yaw.cos() * speed;
                    self.ps.velocity[1] = yaw.sin() * speed;
                }
            }
        }
        self.was_mantling = mantling;
        self.slide_after(msec);
        self.climb_after(msec);
        if !airborne && !mantling && sent_buttons & button::JUMP == 0 {
            self.no_step_onto_movers(before_step, world);
            self.stay_on_ground(ground_before, world);
        }
        let landed = airborne && self.ps.ground_entity_num != NO_GROUND;
        self.smooth_step(before_step, airborne, mantling, msec);
        self.recharge_sprint(msec);
        if landed {
            self.diving = false;
        }
        let fall = self.fall(result.pml.fall_damage);
        if landed && result.pml.fall_damage > 0 && fall == 0 && self.fall_heights != Some(MW2_FALL_HEIGHTS) {
            // pmove's hard landing (MW2's heights) for a drop CODV's heights call harmless: none.
            self.ps.pm_flags &= !pm_flags::TIME_HARDLANDING;
            self.ps.pm_time = 0;
        }
        if self.snag_guard
            && !mantling
            && self.climb.is_none()
            && self.slide.is_none()
            && cmd_forward > 0
            && !(landed && (self.jump_slowdown || fall > 0))
            && self.ps.ground_entity_num != NO_GROUND
            && self.time.wrapping_sub(self.snag_pause_until) >= 0
        {
            let before = self.snags;
            self.unsnag(air_velocity, world);
            self.slip_corner(world);
            if self.snags != before {
                // Restored again within a tenth of a second and less than a unit on: no headway, rest.
                let o = self.ps.origin;
                if let Some((at, t)) = self.snag_last
                    && self.time.wrapping_sub(t) <= 100
                    && (o[0] - at[0]).hypot(o[1] - at[1]) < 1.0
                {
                    self.snag_pause_until = self.time.wrapping_add(SNAG_PAUSE_MS);
                }
                self.snag_last = Some((o, self.time));
            }
        }
        if !self.jump_slowdown && landed && fall == 0 {
            // Landed: keep the air speed (pmove's hard-landing cut) and drop the jump's ground drag.
            let (was, now) = (air_velocity[0].hypot(air_velocity[1]), self.ps.velocity[0].hypot(self.ps.velocity[1]));
            if now < was && now > 1.0 {
                let k = was / now;
                self.ps.velocity[0] *= k;
                self.ps.velocity[1] *= k;
            }
            if self.ps.pm_flags & pm_flags::JUMPING != 0 {
                movement_iw4::jump::clear_state(&mut self.ps);
                self.ps.pm_time = 0;
            }
        }
        if !mantling {
            let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)];
            let at = self.ps.origin;
            if crate::world::is_free(world, at, PLAYER_MINS, maxs, PLAYER_TRACE_MASK) {
                self.last_free = Some(at);
            }
        }
        let pml = &result.pml;
        let mut jumps = [None; 4];
        for (j, a) in jumps.iter_mut().zip(pml.jump_animations) {
            *j = a.map(|(animation, force)| {
                let event = match animation {
                    movement_iw4::JumpAnimation::Forward => 3,
                    movement_iw4::JumpAnimation::Backward => 4,
                };
                (event, force)
            });
        }
        self.anim = AnimStep {
            movetype: pml.mantle_movetype.or_else(|| {
                movement_iw4::footsteps_anim_move_type(
                    &self.ps,
                    cmd.forwardmove,
                    cmd.rightmove,
                    pml.almost_ground_plane != 0,
                )
            }),
            stance_event: result.stance_event,
            reset_torso: result.reset_torso,
            jumps,
            landing: pml.landing_animation,
            force_movement: pml.mantle_movetype.is_some(),
            forward: cmd.forwardmove,
            right: cmd.rightmove,
            buttons: cmd.buttons,
            msec,
        };
        self.output(fall)
    }

    /// Starts, steers or ends the slide before pmove: while sliding pmove gets
    /// no move input (no acceleration) and the slide's speed, friction's share
    /// given back.
    fn slide_before<W: World + ?Sized>(&mut self, input: &MoveInput, cmd: &mut UserCmd, msec: i32, world: &W) {
        let crouch = input.buttons & button::CROUCH != 0;
        let pressed = crouch && !self.crouch_held;
        self.crouch_held = crouch;
        let v = self.ps.velocity;
        let speed = v[0].hypot(v[1]);
        if self.slide.is_none()
            && self.slide_enabled
            && pressed
            && self.ps.ground_entity_num != NO_GROUND
            && self.ps.pm_flags & pm_flags::SPRINTING != 0
            && self.ps.pm_flags & pm_flags::MANTLE == 0
            && speed >= SLIDE_MIN_SPEED
            && self.time.wrapping_sub(self.slide_ended) >= SLIDE_COOLDOWN_MS
        {
            let boost = if self.slide_boost.is_finite() { self.slide_boost.clamp(1.0, 2.0) } else { SLIDE_BOOST };
            let start = (speed * boost).min(SLIDE_MAX_SPEED * boost / SLIDE_BOOST);
            self.push_foley(Foley::Slide);
            self.slide = Some(Slide { ms: 0, dir: [v[0] / speed, v[1] / speed], start, given: start });
        }
        let Some(sl) = self.slide.as_mut() else { return };
        if self.dive_enabled && input.buttons & button::PRONE != 0 && self.ps.ground_entity_num != NO_GROUND {
            // The dive: off the ground forward, crouched in the air; the held prone lays the player
            // down on landing (pmove's prone needs the ground), the speed kept.
            let dir = sl.dir;
            let speed = self.ps.velocity[0].hypot(self.ps.velocity[1]).max(DIVE_SPEED);
            self.end_slide();
            self.ps.velocity = [dir[0] * speed, dir[1] * speed, DIVE_LIFT];
            self.ps.ground_entity_num = NO_GROUND;
            self.diving = true;
            self.push_foley(Foley::Dive);
            return;
        }
        if !crouch || cmd.buttons & button::JUMP != 0 {
            self.end_slide();
            // A jump out of the slide: up at once (pmove refuses a crouched jump), room overhead allowing.
            let stand = [PLAYER_MAXS[0], PLAYER_MAXS[1], movement_iw4::STAND_MAXS_Z];
            if cmd.buttons & button::JUMP != 0
                && cmd.buttons & button::CROUCH == 0
                && crate::world::is_free(world, self.ps.origin, PLAYER_MINS, stand, PLAYER_TRACE_MASK)
            {
                self.ps.pm_flags &= !pm_flags::CROUCH;
                self.ps.view_height_target = 60; // standing: pmove's jump gate reads the stance from it
            }
            return;
        }
        // Steer towards the view, slowly.
        let dt = msec as f32 / 1000.0;
        let yaw = self.ps.viewangles[1].to_radians();
        let (want, have) = (yaw, sl.dir[1].atan2(sl.dir[0]));
        let mut d = want - have;
        while d > std::f32::consts::PI {
            d -= std::f32::consts::TAU;
        }
        while d < -std::f32::consts::PI {
            d += std::f32::consts::TAU;
        }
        let turn = d.clamp(-SLIDE_STEER.to_radians() * dt, SLIDE_STEER.to_radians() * dt);
        let a = have + turn;
        sl.dir = [a.cos(), a.sin()];
        let t = (sl.ms as f32 / self.slide_ms.clamp(200, 3000) as f32).clamp(0.0, 1.0);
        let speed = SLIDE_END_SPEED + (sl.start - SLIDE_END_SPEED) * (1.0 - t) * (1.0 - t);
        sl.given = speed;
        let v = speed / (1.0 - GROUND_FRICTION * dt).max(0.5);
        self.ps.velocity[0] = sl.dir[0] * v;
        self.ps.velocity[1] = sl.dir[1] * v;
        cmd.forwardmove = 0;
        cmd.rightmove = 0;
        cmd.buttons &= !button::SPRINT;
    }

    /// The lean before pmove: towards the held side at CoD4's pace, upright when sprinting, in
    /// the air, prone, mantling, climbing or sliding; never through a wall (the leaned eye is
    /// traced). pmove slows a leaning player to a walk and keeps it from sprinting (ps.leanf).
    fn lean_before<W: World + ?Sized>(&mut self, cmd: &mut UserCmd, msec: i32, world: &W) {
        let held = cmd.buttons & (LEAN_LEFT | LEAN_RIGHT);
        cmd.buttons &= !(LEAN_LEFT | LEAN_RIGHT);
        let free = self.ps.ground_entity_num != NO_GROUND
            && self.ps.pm_flags & (pm_flags::PRONE | pm_flags::MANTLE | pm_flags::LADDER) == 0
            && self.climb.is_none()
            && self.slide.is_none();
        let mut target = match held {
            LEAN_LEFT => -1.0,
            LEAN_RIGHT => 1.0,
            _ => 0.0,
        };
        if !self.lean_enabled
            || !free
            || cmd.buttons & button::SPRINT != 0 && self.ps.pm_flags & pm_flags::SPRINTING != 0
        {
            target = 0.0;
        }
        // A lean starts only beside cover (a wall, a corner, a crate within reach): in the open the
        // eye swinging out looks wrong. Once leaning it holds while the key does.
        if target != 0.0 && self.ps.leanf == 0.0 && !self.cover_near(world) {
            target = 0.0;
        }
        let step = msec as f32 / LEAN_MS;
        let mut lean = self.ps.leanf;
        lean = if lean < target { (lean + step).min(target) } else { (lean - step).max(target) };
        // No further than the wall beside the eye.
        if lean != 0.0 {
            let eye = [self.ps.origin[0], self.ps.origin[1], self.ps.origin[2] + self.ps.view_height_current];
            let want = self.lean_eye(eye, lean);
            let t = world.trace(eye, want, [-6.0; 3], [6.0; 3], PLAYER_TRACE_MASK);
            if t.startsolid == 0 && t.fraction < 1.0 {
                lean *= (t.fraction - 0.05).max(0.0);
            }
        }
        self.ps.leanf = if lean.abs() < 1e-3 { 0.0 } else { lean };
    }

    /// Something solid within LEAN_COVER_REACH of the hull around the player, at eye or chest height.
    fn cover_near<W: World + ?Sized>(&self, world: &W) -> bool {
        let o = self.ps.origin;
        let eye = o[2] + self.ps.view_height_current;
        let reach = PLAYER_MAXS[0] + LEAN_COVER_REACH;
        for z in [eye - 4.0, eye - 24.0] {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let from = [o[0], o[1], z];
                let to = [o[0] + a.cos() * reach, o[1] + a.sin() * reach, z];
                let t = world.trace(from, to, [-2.0; 3], [2.0; 3], PLAYER_TRACE_MASK);
                if t.startsolid == 0 && t.fraction < 1.0 {
                    return true;
                }
            }
        }
        false
    }

    /// The body's tilt for lean `frac`: the angle (radians, right positive) the upper body turns
    /// about the hips, LEAN_PIVOT below the eye, for the head to reach (2 - |f|) f of `lean_dist`
    /// sideways (CoD4's curve).
    fn lean_angle(&self, frac: f32) -> f32 {
        let dist = if self.lean_dist.is_finite() { self.lean_dist.clamp(4.0, 24.0) } else { LEAN_DIST };
        let side = (2.0 - frac.abs()) * frac * dist;
        (side / LEAN_PIVOT).clamp(-0.9, 0.9).asin()
    }

    /// The eye moved by lean `frac`: the head swings on the upper body about the hips (sideways along
    /// the view's right, a little down), so it leads and the hips stay where they are.
    fn lean_eye(&self, eye: [f32; 3], frac: f32) -> [f32; 3] {
        let a = self.lean_angle(frac);
        let (side, drop) = (LEAN_PIVOT * a.sin(), LEAN_PIVOT * (1.0 - a.cos()));
        let yaw = self.ps.viewangles[1].to_radians();
        // IW4's right vector for this yaw (y left): (sin yaw, -cos yaw).
        [eye[0] + yaw.sin() * side, eye[1] - yaw.cos() * side, eye[2] - drop]
    }

    /// Something at least `vault_min_height` tall just ahead (no minimum: always).
    /// A vehicle (a mover) within a vault's reach ahead, along the view or the run.
    fn vehicle_ahead<W: World + ?Sized>(&self, world: &W) -> bool {
        let o = self.ps.origin;
        let from = [o[0], o[1], o[2] + 20.0];
        let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], 30.0];
        let yaw = self.ps.viewangles[1].to_radians();
        let speed = self.ps.velocity[0].hypot(self.ps.velocity[1]).max(1.0);
        let dirs = [[yaw.cos(), yaw.sin()], [self.ps.velocity[0] / speed, self.ps.velocity[1] / speed]];
        dirs.iter().any(|d| {
            let to = [from[0] + d[0] * 40.0, from[1] + d[1] * 40.0, from[2]];
            world.mover_hit(from, to, PLAYER_MINS, maxs, PLAYER_TRACE_MASK)
        })
    }

    fn tall_enough_to_vault<W: World + ?Sized>(&self, world: &W) -> bool {
        let min = if self.vault_min_height.is_finite() {
            self.vault_min_height.max(AUTO_VAULT_MIN_HEIGHT)
        } else {
            AUTO_VAULT_MIN_HEIGHT
        };
        let yaw = self.ps.viewangles[1].to_radians();
        let o = self.ps.origin;
        let z = o[2] + min.min(60.0) - 2.0;
        let from = [o[0], o[1], z];
        let to = [o[0] + yaw.cos() * 32.0, o[1] + yaw.sin() * 32.0, z];
        let t = world.trace(from, to, [-4.0; 3], [4.0; 3], PLAYER_TRACE_MASK);
        // And faced squarely: a wall or a car's side met at a slant is run along, not vaulted.
        let square = t.fraction < 1.0 && -(t.normal[0] * yaw.cos() + t.normal[1] * yaw.sin()) > AUTO_VAULT_HEAD_ON;
        // One face from the shin up: a low step or a bin's base nearer than what the knee met (a wall
        // behind) is the ledge the mantle would take, too low to vault.
        let shin =
            world.trace([o[0], o[1], o[2] + 8.0], [to[0], to[1], o[2] + 8.0], [-4.0; 3], [4.0; 3], PLAYER_TRACE_MASK);
        square && (shin.fraction >= 1.0 || shin.fraction * 32.0 >= t.fraction * 32.0 - 4.0)
    }

    /// No step up onto a vehicle or a prop: pmove's step (18 units) took the player up a car's sill, a
    /// wheel or a bumper while running past it, with nothing to stand on there: up, then off and down
    /// again (a hop, a landing and a jolt). On the ground before and after, risen more than a little,
    /// standing on a mover now: the move is taken flat instead, sliding along what stopped it. A car is
    /// still climbed with the jump key (the mantle).
    fn no_step_onto_movers<W: World + ?Sized>(&mut self, before: [f32; 3], world: &W) {
        let o = self.ps.origin;
        if self.ps.ground_entity_num == NO_GROUND
            || self.climb.is_some()
            || self.ps.pm_flags & (pm_flags::JUMPING | pm_flags::MANTLE) != 0
            || o[2] - before[2] <= 4.0
        {
            return;
        }
        let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)];
        if !world.mover_hit(o, [o[0], o[1], o[2] - 4.0], PLAYER_MINS, maxs, PLAYER_TRACE_MASK) {
            return;
        }
        let flat = world.trace(before, [o[0], o[1], before[2]], PLAYER_MINS, maxs, PLAYER_TRACE_MASK);
        if flat.startsolid != 0 || flat.allsolid != 0 {
            return;
        }
        self.ps.origin = flat.endpos;
        if flat.fraction < 1.0 {
            let n = flat.normal;
            let h = n[0].hypot(n[1]);
            if h > 1e-4 {
                let (nx, ny) = (n[0] / h, n[1] / h);
                let into = self.ps.velocity[0] * nx + self.ps.velocity[1] * ny;
                if into < 0.0 {
                    self.ps.velocity[0] -= nx * into;
                    self.ps.velocity[1] -= ny * into;
                }
            }
        }
        self.ps.velocity[2] = 0.0;
        self.step_smooth = 0.0;
    }

    /// Down a step without leaving the ground: on the ground before the move and not jumping, with
    /// walkable ground at most a step (18 units) under the feet after it, the player is put on it
    /// instead of falling. pmove alone left the ground at every stair going down, every curb and
    /// every crest of GTA's uneven ground (a hop, a landing, its sound and a bounce of the view each
    /// time); the eye takes the step down smoothly (`smooth_step`). A real drop still falls.
    fn stay_on_ground<W: World + ?Sized>(&mut self, ground: i32, world: &W) {
        const STEP: f32 = 18.0;
        if self.ps.ground_entity_num != NO_GROUND
            || ground == NO_GROUND
            || self.climb.is_some()
            || self.ps.pm_flags & (pm_flags::JUMPING | pm_flags::MANTLE) != 0
            || self.ps.velocity[2] > 60.0
        {
            return;
        }
        let o = self.ps.origin;
        let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)];
        let t = world.trace(o, [o[0], o[1], o[2] - STEP], PLAYER_MINS, maxs, PLAYER_TRACE_MASK);
        if t.startsolid != 0 || t.allsolid != 0 || t.fraction >= 1.0 || t.normal[2] < 0.7 {
            return;
        }
        self.ps.origin = t.endpos;
        self.ps.ground_entity_num = ground;
        self.ps.velocity[2] = 0.0;
    }

    /// A faster sprint refill than MW2's: pmove counts the meter back from when the last sprint ended (one
    /// millisecond back per millisecond rested, the end counting twice), so moving that end earlier by
    /// (rate - 1) / 2 of each rested millisecond refills it `sprint_recharge` times as fast; the delay before
    /// the next sprint shortens with it.
    fn recharge_sprint(&mut self, msec: i32) {
        let rate = if self.sprint_recharge.is_finite() { self.sprint_recharge.clamp(1.0, 4.0) } else { 1.0 };
        let ps = &mut self.ps;
        let resting = ps.pm_flags & pm_flags::SPRINTING == 0
            && ps.last_sprint_start != 0
            && ps.last_sprint_end.wrapping_sub(ps.last_sprint_start) > 0;
        if rate <= 1.0 || !resting || self.sprint_left() >= 1.0 {
            self.sprint_recharge_carry = 0.0;
            return;
        }
        self.sprint_recharge_carry += msec.max(0) as f32 * (rate - 1.0) * 0.5;
        let shift = (self.sprint_recharge_carry as i32)
            .min(self.ps.last_sprint_end.wrapping_sub(self.ps.last_sprint_start) - 1);
        if shift > 0 {
            self.ps.last_sprint_end = self.ps.last_sprint_end.wrapping_sub(shift);
            self.sprint_recharge_carry -= shift as f32;
        }
    }

    /// cg's step smoothing: on the ground before and after, a rise or a drop steeper than any walkable
    /// slope (a stair's step, a curb) is taken up by the eye over ~60 ms; slopes, jumps, falls and
    /// climbs move the eye with the feet.
    fn smooth_step(&mut self, before: [f32; 3], airborne: bool, mantling: bool, msec: i32) {
        const TIME_MS: f32 = 60.0;
        const MAX: f32 = 24.0;
        let grounded = !airborne && self.ps.ground_entity_num != NO_GROUND;
        let dz = self.ps.origin[2] - before[2];
        let run = (self.ps.origin[0] - before[0]).hypot(self.ps.origin[1] - before[1]);
        if grounded
            && !mantling
            && self.climb.is_none()
            && dz.abs() > 0.5
            && dz.abs() <= MAX
            && dz.abs() > run * 1.05 + 0.5
        {
            self.step_smooth = (self.step_smooth - dz).clamp(-MAX, MAX);
        } else if !grounded || mantling || self.climb.is_some() {
            self.step_smooth = 0.0;
        }
        self.step_smooth *= (-(msec.max(0) as f32) / TIME_MS).exp();
        if self.step_smooth.abs() < 0.05 {
            self.step_smooth = 0.0;
        }
    }

    fn push_foley(&mut self, f: Foley) {
        if self.foley.len() < 16 {
            self.foley.push(f);
        }
    }

    /// The movement sounds since the last call.
    pub fn take_foley(&mut self) -> Vec<Foley> {
        std::mem::take(&mut self.foley)
    }

    /// pmove's footstep, landing and mantle events of the last step as movement sounds.
    fn collect_events(&mut self) {
        const EV_FOOTSTEP_SPRINT: i32 = 0x6b;
        const EV_FOOTSTEP_RUN: i32 = 0x6c;
        const EV_FOOTSTEP_WALK: i32 = 0x6d;
        const EV_FOOTSTEP_PRONE: i32 = 0x6e;
        const EV_LANDING_FIRST: i32 = 0x70;
        const EV_LANDING_PAIN_FIRST: i32 = 0x8f;
        const EV_MANTLE: i32 = 0xae;
        let seq = self.ps.event_sequence;
        let new = (seq.wrapping_sub(self.event_seen) & 0x7ff).min(4);
        self.event_seen = seq;
        for k in (0..new).rev() {
            let slot = seq.wrapping_sub(1 + k) & 3;
            let e = [self.ps.events_0, self.ps.events_1, self.ps.events_2, self.ps.events_3][slot as usize] & 0xff;
            let f = match e {
                EV_FOOTSTEP_SPRINT => Some(Foley::Step(StepKind::Sprint)),
                EV_FOOTSTEP_RUN => Some(Foley::Step(StepKind::Run)),
                EV_FOOTSTEP_WALK => Some(Foley::Step(StepKind::Walk)),
                EV_FOOTSTEP_PRONE => Some(Foley::Step(StepKind::Prone)),
                EV_MANTLE => Some(Foley::Mantle),
                e if (EV_LANDING_PAIN_FIRST..EV_LANDING_PAIN_FIRST + 31).contains(&e) => {
                    Some(Foley::Land { hard: true })
                }
                e if (EV_LANDING_FIRST..EV_LANDING_FIRST + 31).contains(&e) => Some(Foley::Land { hard: false }),
                _ => None,
            };
            if let Some(f) = f {
                self.push_foley(f);
            }
        }
    }

    /// The snag guard: from a walk or more, the step lost a good share of the horizontal speed, yet
    /// the hull could go on that way (an edge it caught, not a wall): the speed it had comes back.
    fn unsnag<W: World + ?Sized>(&mut self, before: [f32; 3], world: &W) {
        const MIN: f32 = 100.0;
        let was = before[0].hypot(before[1]);
        let now = self.ps.velocity[0].hypot(self.ps.velocity[1]);
        if was < MIN || now >= was * SNAG_KEEP {
            return;
        }
        let dir = [before[0] / was, before[1] / was];
        let o = self.ps.origin;
        let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)];
        // Lifted a step's height: a curb or a seam underfoot is no wall.
        let from = [o[0], o[1], o[2] + 18.0];
        let to = [from[0] + dir[0] * 12.0, from[1] + dir[1] * 12.0, from[2]];
        let t = world.trace(from, to, PLAYER_MINS, maxs, PLAYER_TRACE_MASK);
        if t.startsolid == 0 && t.allsolid == 0 && t.fraction >= 1.0 {
            self.ps.velocity[0] = before[0];
            self.ps.velocity[1] = before[1];
            self.snags = self.snags.wrapping_add(1);
        }
    }

    /// The corner slip: pushing forward on the ground into something that stopped the player dead,
    /// when a small step to one side clears it (a car's corner, a post, a fence's end: not a wall),
    /// the run turns that way around it instead of standing still.
    fn slip_corner<W: World + ?Sized>(&mut self, world: &W) {
        if self.ps.ground_entity_num == NO_GROUND || self.ps.velocity[0].hypot(self.ps.velocity[1]) > 20.0 {
            return;
        }
        let yaw = self.ps.viewangles[1].to_radians();
        let f = [yaw.cos(), yaw.sin()];
        let r = [yaw.sin(), -yaw.cos()];
        let o = self.ps.origin;
        let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)];
        let lift = [o[0], o[1], o[2] + 18.0];
        // Blocked straight ahead (else nothing to slip around).
        let ahead = world.trace(
            lift,
            [lift[0] + f[0] * 4.0, lift[1] + f[1] * 4.0, lift[2]],
            PLAYER_MINS,
            maxs,
            PLAYER_TRACE_MASK,
        );
        if ahead.fraction >= 1.0 && ahead.startsolid == 0 {
            return;
        }
        for offset in [6.0f32, 12.0, 20.0] {
            for side in [1.0f32, -1.0] {
                let at = [lift[0] + r[0] * side * offset, lift[1] + r[1] * side * offset, lift[2]];
                let step = world.trace(lift, at, PLAYER_MINS, maxs, PLAYER_TRACE_MASK);
                if step.fraction < 1.0 || step.startsolid != 0 {
                    continue;
                }
                let past = world.trace(
                    at,
                    [at[0] + f[0] * 16.0, at[1] + f[1] * 16.0, at[2]],
                    PLAYER_MINS,
                    maxs,
                    PLAYER_TRACE_MASK,
                );
                if past.fraction >= 1.0 && past.startsolid == 0 {
                    let d = [f[0] + r[0] * side, f[1] + r[1] * side];
                    let len = d[0].hypot(d[1]);
                    let speed = SLIP_SPEED;
                    self.ps.velocity[0] = d[0] / len * speed;
                    self.ps.velocity[1] = d[1] / len * speed;
                    self.snags = self.snags.wrapping_add(1);
                    return;
                }
            }
        }
    }

    /// CODV's climb: the jump at a ledge above MW2's mantle starts it; while it runs pmove gets no
    /// input and the climb places the player (up first, then onto the ledge).
    fn climb_before<W: World + ?Sized>(&mut self, input: &MoveInput, cmd: &mut UserCmd, world: &W) {
        if let Some(c) = self.climb {
            let _ = c;
            cmd.forwardmove = 0;
            cmd.rightmove = 0;
            cmd.buttons &= !(button::JUMP | button::SPRINT | button::CROUCH | button::PRONE);
            self.ps.velocity = [0.0; 3];
            return;
        }
        let jump = input.buttons & button::JUMP != 0 && self.old_buttons & button::JUMP == 0;
        if !self.high_climb
            || !jump
            || input.forward <= 0
            || self.ps.pm_flags & (pm_flags::MANTLE | pm_flags::PRONE) != 0
        {
            return;
        }
        if let Some(to) = self.find_high_ledge(world) {
            self.climb = Some(Climb { from: self.ps.origin, to, ms: 0 });
            self.push_foley(Foley::Mantle);
            self.mantle_entry = Some(self.ps.velocity[0].hypot(self.ps.velocity[1]));
            self.end_slide();
            cmd.forwardmove = 0;
            cmd.rightmove = 0;
            cmd.buttons &= !(button::JUMP | button::SPRINT);
            self.ps.velocity = [0.0; 3];
        }
    }

    /// A ledge 62 to 96 units above the feet just ahead, with room to stand on it and to rise to it.
    fn find_high_ledge<W: World + ?Sized>(&self, world: &W) -> Option<[f32; 3]> {
        use crate::world::is_free;
        let yaw = self.ps.viewangles[1].to_radians();
        let f = [yaw.cos(), yaw.sin()];
        let o = self.ps.origin;
        let stand = [PLAYER_MAXS[0], PLAYER_MAXS[1], movement_iw4::STAND_MAXS_Z];
        // Something solid in front at chest height (a wall, not open air).
        let ahead = [o[0] + f[0] * (CLIMB_REACH + 2.0), o[1] + f[1] * (CLIMB_REACH + 2.0), o[2] + 40.0];
        let wall =
            world.trace([o[0], o[1], o[2] + 40.0], ahead, [-4.0, -4.0, -4.0], [4.0, 4.0, 4.0], PLAYER_TRACE_MASK);
        if wall.fraction >= 1.0 || wall.startsolid != 0 {
            return None;
        }
        // The ledge top: down from above MW2's reach, a hull's depth past the wall face.
        let past = wall.fraction * (CLIMB_REACH + 2.0) + 2.0 * PLAYER_MAXS[0] + 2.0;
        let top_xy = [o[0] + f[0] * past, o[1] + f[1] * past];
        let top = if self.climb_max.is_finite() { self.climb_max.clamp(CLIMB_MIN + 4.0, 120.0) } else { CLIMB_MAX };
        let high = o[2] + top + 1.0;
        let down = world.trace(
            [top_xy[0], top_xy[1], high],
            [top_xy[0], top_xy[1], o[2] + CLIMB_MIN - 1.0],
            PLAYER_MINS,
            stand,
            PLAYER_TRACE_MASK,
        );
        if down.startsolid != 0 || down.fraction >= 1.0 || down.normal[2] < 0.7 {
            return None;
        }
        let ledge_z = high + (o[2] + CLIMB_MIN - 1.0 - high) * down.fraction;
        if !(o[2] + CLIMB_MIN..=o[2] + top).contains(&ledge_z) {
            return None;
        }
        let to = [top_xy[0], top_xy[1], ledge_z + 0.25];
        // Room to rise straight up, then to move over onto the ledge.
        let up = [o[0], o[1], ledge_z + 0.25];
        let rise = world.trace(o, up, PLAYER_MINS, stand, PLAYER_TRACE_MASK);
        let over = world.trace(up, to, PLAYER_MINS, stand, PLAYER_TRACE_MASK);
        (rise.fraction >= 1.0 && over.fraction >= 1.0 && is_free(world, to, PLAYER_MINS, stand, PLAYER_TRACE_MASK))
            .then_some(to)
    }

    /// The climb's path after pmove: up over the first 60 %, onto the ledge from 45 %.
    fn climb_after(&mut self, msec: i32) {
        let Some(c) = self.climb.as_mut() else { return };
        c.ms += msec;
        let t = (c.ms as f32 / CLIMB_MS as f32).clamp(0.0, 1.0);
        let smooth = |x: f32| {
            let x = x.clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let (up, along) = (smooth(t / 0.6), smooth((t - 0.45) / 0.55));
        self.ps.origin = [
            c.from[0] + (c.to[0] - c.from[0]) * along,
            c.from[1] + (c.to[1] - c.from[1]) * along,
            c.from[2] + (c.to[2] - c.from[2]) * up,
        ];
        self.ps.velocity = [0.0; 3];
        if t >= 1.0 {
            let c = *c;
            self.climb = None;
            self.last_free = None;
            self.air_peak = None;
            // On at the speed it came in with, along the climb.
            if let Some(speed) = self.mantle_entry.take()
                && self.mantle_momentum
            {
                let d = [c.to[0] - c.from[0], c.to[1] - c.from[1]];
                let len = d[0].hypot(d[1]);
                if len > 0.0 && speed > 1.0 {
                    self.ps.velocity = [d[0] / len * speed, d[1] / len * speed, 0.0];
                }
            }
        }
    }

    /// CODV's climb is under way.
    pub fn climbing(&self) -> bool {
        self.climb.is_some()
    }

    /// After pmove: the slide runs out, leaves the ground (it flies on with
    /// its speed) or meets a wall.
    fn slide_after(&mut self, msec: i32) {
        let Some(sl) = self.slide.as_mut() else { return };
        sl.ms += msec;
        let speed = self.ps.velocity[0].hypot(self.ps.velocity[1]);
        if sl.ms >= self.slide_ms.clamp(200, 3000) || self.ps.ground_entity_num == NO_GROUND || speed < sl.given * 0.5 {
            self.end_slide();
        }
    }

    fn end_slide(&mut self) {
        if self.slide.take().is_some() {
            self.slide_ended = self.time;
        }
    }

    /// The sprint left, 0..1 (1: full; it recharges after a sprint as MW2's does).
    pub fn sprint_left(&self) -> f32 {
        let sprint = self.context().sprint;
        let max = sprint.weapon_max_sprint_time.max(1);
        (movement_iw4::sprint_time_remaining(&self.ps, self.time, sprint) as f32 / max as f32).clamp(0.0, 1.0)
    }

    /// A slide is under way.
    pub fn sliding(&self) -> bool {
        self.slide.is_some()
    }

    /// The landing's fall damage: pmove's (MW2's heights, its no-damage and
    /// soft surfaces) or, with the host's heights, from the drop since the
    /// highest point in the air.
    fn fall(&mut self, pmove_damage: i32) -> i32 {
        let z = self.ps.origin[2];
        if self.ps.ground_entity_num == NO_GROUND {
            self.air_peak = Some(self.air_peak.map_or(z, |p| p.max(z)));
            return 0;
        }
        let Some(peak) = self.air_peak.take() else { return 0 };
        match self.fall_heights {
            None => 0,
            Some(h) if h == MW2_FALL_HEIGHTS => pmove_damage,
            // A drop pmove found harmless although MW2 would hurt (a no-damage surface) stays harmless.
            Some(_) if pmove_damage == 0 && peak - z > MW2_FALL_HEIGHTS.0 + 24.0 => 0,
            Some(h) => fall_damage(peak - z, h),
        }
    }

    /// Frees a hull that overlaps the world: crouched when only the standing
    /// hull is blocked (a low ceiling, the end of a climb under something),
    /// else back where it last stood free (something moved into it), else the
    /// nearest free spot with room beyond it.
    fn free_hull<W: World + ?Sized>(&mut self, world: &W) {
        use crate::world::{UNSTICK_REACH, free_spot, is_free};
        let maxs = [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)];
        let at = self.ps.origin;
        if is_free(world, at, PLAYER_MINS, maxs, PLAYER_TRACE_MASK) {
            return;
        }
        let crouched = [PLAYER_MAXS[0], PLAYER_MAXS[1], movement_iw4::CROUCH_MAXS_Z];
        if self.ps.pm_flags & (pm_flags::CROUCH | pm_flags::PRONE) == 0
            && is_free(world, at, PLAYER_MINS, crouched, PLAYER_TRACE_MASK)
        {
            // pmove stands back up by itself once there is room.
            self.ps.pm_flags |= pm_flags::CROUCH;
            return;
        }
        // A hair's overlap first (a car or a prop that settled or was nudged into the hull by
        // millimetres, a step's edge): the smallest sideways push that frees it. Jumping back to the
        // last free spot (up to 48 units) or up onto it every frame grabbed the run and shook the view.
        let yaw = self.ps.viewangles[1].to_radians();
        for d in [0.5f32, 1.0, 2.0, 4.0] {
            for k in 0..8 {
                let a = yaw + std::f32::consts::PI + k as f32 * std::f32::consts::FRAC_PI_4;
                let p = [at[0] + a.cos() * d, at[1] + a.sin() * d, at[2]];
                if is_free(world, p, PLAYER_MINS, maxs, PLAYER_TRACE_MASK) {
                    self.ps.origin = p;
                    self.unstuck = self.unstuck.wrapping_add(1);
                    return;
                }
            }
        }
        let back = self.last_free.filter(|b| {
            let d = [b[0] - at[0], b[1] - at[1], b[2] - at[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= 48.0 * 48.0
                && is_free(world, *b, PLAYER_MINS, maxs, PLAYER_TRACE_MASK)
        });
        if let Some(free) = back.or_else(|| free_spot(world, at, PLAYER_MINS, maxs, PLAYER_TRACE_MASK, UNSTICK_REACH)) {
            self.ps.origin = free;
            self.unstuck = self.unstuck.wrapping_add(1);
        }
    }

    /// MW2's third-person camera (cg_thirdPerson: iw4L playerstate_iw4
    /// `offset_third_person_view`, pulled in front of walls by `world`):
    /// (position, angles), IW4 units and degrees. `range` units behind,
    /// `angle` degrees around (cg_thirdPersonRange / cg_thirdPersonAngle).
    pub fn third_person_view<W: World + ?Sized>(&self, range: f32, angle: f32, world: &W) -> ([f32; 3], [f32; 3]) {
        use playerstate_iw4::{CG_CAMERA_PULLBACK_BOX_HALF, CG_CAMERA_PULLBACK_CLIPMASK, OffsetThirdPersonViewInputs};
        let h = CG_CAMERA_PULLBACK_BOX_HALF;
        let v = playerstate_iw4::offset_third_person_view(
            OffsetThirdPersonViewInputs {
                origin: self.ps.origin,
                view_height_current: self.ps.view_height_current,
                viewangles: self.ps.viewangles,
                pm_type: self.ps.pm_type,
                look_at_killer_yaw: self.ps.viewangles[1],
                corpse_j_mainroot: None,
                other_flags: self.ps.other_flags & !playerstate_iw4::other_flags::DEAD_KILLCAM_TPV,
                delta_time: 16,
                cg_third_person_angle: angle,
                cg_third_person_range: range,
            },
            |start, end| world.trace(start, end, [-h; 3], [h; 3], CG_CAMERA_PULLBACK_CLIPMASK).fraction,
        );
        (v.origin, v.angles)
    }

    /// The player as it stands, without stepping (no fall damage).
    /// The hull's maxs for the stance it is in (stand, crouch, prone).
    pub fn hull_maxs(&self) -> [f32; 3] {
        [PLAYER_MAXS[0], PLAYER_MAXS[1], hull_maxs_z(&self.ps)]
    }

    pub fn peek(&self) -> MoveOutput {
        self.output(0)
    }

    fn output(&self, fall_damage: i32) -> MoveOutput {
        let flags = self.ps.pm_flags;
        MoveOutput {
            origin: self.ps.origin,
            velocity: self.ps.velocity,
            view_angles: self.ps.viewangles,
            view_height: self.ps.view_height_current + self.step_smooth,
            on_ground: self.ps.ground_entity_num != NO_GROUND,
            stance: if flags & pm_flags::PRONE != 0 {
                Stance::Prone
            } else if flags & pm_flags::CROUCH != 0 {
                Stance::Crouch
            } else {
                Stance::Stand
            },
            sprinting: flags & pm_flags::SPRINTING != 0,
            mantling: flags & pm_flags::MANTLE != 0 || self.climb.is_some(),
            jumping: flags & pm_flags::JUMPING != 0,
            sliding: self.slide.is_some(),
            lean_offset: {
                let eye = [0.0; 3];
                self.lean_eye(eye, self.ps.leanf)
            },
            lean_roll: self.lean_angle(self.ps.leanf).to_degrees() * LEAN_ROLL_SHARE,
            fall_damage,
        }
    }
}

/// Retail buttons CODV maps its inputs to.
pub mod button {
    pub use playerstate_iw4::buttons::{ADS, ATTACK, CROUCH, FRAG, JUMP, MELEE_CHARGE, PRONE, RELOAD, SPRINT};
}

#[cfg(test)]
mod tests;
