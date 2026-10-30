#![allow(clippy::missing_safety_doc)]
#![allow(non_snake_case)]
//! CodVRuntime.dll: the CODV core behind a versioned C ABI.
//! Layout mirrors host/include/codv_runtime.h; both sides assert sizes
//! (`abi_layout_matches_header` here, static_asserts there).
//!
//! A frame of weapon logic or of MW2 movement costs microseconds, so
//! `cv_step` and `cv_mw2_step` run synchronously on GTA's script thread. One
//! worker thread reads GTA's collision and builds the MW2 meshes around the
//! player (`collision.rs`). Every export catches panics and fails closed
//! (returns 0 / null).

mod arms;
mod audio;
mod body;
mod collision;
mod hud;
mod killstreaks;
mod movers;
mod mw2;
mod mw2hud;
mod viewmodel;

pub use movers::{CV_MOVER_OBJECT, CV_MOVER_PED, CV_MOVER_VEHICLE, CvMover};

pub use mw2::{
    CV_MOVE_JUMPING, CV_MOVE_MANTLING, CV_MOVE_ON_GROUND, CV_MOVE_SPRINTING, CV_MOVE_WAITING, CvMoveInput,
    CvMoveOutput, CvMoveSpawn, CvTraceFn, CvTraceQuery, CvTraceResult,
};

use codv_core::player::{self, FrameIn, Hit, Player};
use codv_core::{State, Table};
use codv_engine::World;
use codv_engine::units::METRES_PER_UNIT;
use codv_gtaworld::live::Level;
use std::ffi::{CStr, c_char, c_void};
use std::fs::OpenOptions;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const CODV_ABI_VERSION: u32 = 10;
pub const CV_MAX_SHOTS: usize = 24;

// CvInput::flags
pub const CV_IN_ON_FOOT: u32 = 1 << 0;
pub const CV_IN_AIRBORNE: u32 = 1 << 1;
pub const CV_IN_SPRINTING: u32 = 1 << 2;

// CvOutput::flags
pub const CV_OUT_BLOCK_SPRINT: u32 = 1 << 0;
pub const CV_OUT_RELOAD: u32 = 1 << 1;
pub const CV_OUT_MELEE: u32 = 1 << 2;
pub const CV_OUT_EQUIP: u32 = 1 << 3;
pub const CV_OUT_JUMP: u32 = 1 << 4;
pub const CV_OUT_CROUCH_CHANGED: u32 = 1 << 5;
pub const CV_OUT_CROUCHED: u32 = 1 << 6;
pub const CV_OUT_ACTIVE: u32 = 1 << 7;

// CvHud::flags
pub const CV_HUD_KILL: u32 = 1 << 0;
pub const CV_HUD_HEADSHOT: u32 = 1 << 1;

// CvHit::flags
pub const CV_HIT_HEAD: u32 = 1 << 0;
pub const CV_HIT_PED: u32 = 1 << 1;

// cv_set_option ids
pub const CV_OPT_ENABLED: u32 = 1;
pub const CV_OPT_QUICK_JUMP: u32 = 2;
pub const CV_OPT_REGEN: u32 = 3;
pub const CV_OPT_RECOIL_SCALE: u32 = 4;
pub const CV_OPT_PRIMARY: u32 = 5;
pub const CV_OPT_SECONDARY: u32 = 6;
pub const CV_OPT_VERBOSE: u32 = 7;
/// MW2 (ABI 7): spare rounds times this; the menu's gun index for the primary
/// / secondary slot (cv_mw2_gun_info); frags carried.
pub const CV_OPT_AMMO_SCALE: u32 = 8;
pub const CV_OPT_MW2_PRIMARY: u32 = 9;
pub const CV_OPT_MW2_SECONDARY: u32 = 10;
pub const CV_OPT_FRAGS: u32 = 11;
/// A random MW2 primary and secondary now (the host sends it when the mode is switched on).
pub const CV_OPT_MW2_RANDOMIZE: u32 = 12;
/// MW2's jump height, IW4 units (MW2: 39); the host's JumpHeight.
pub const CV_OPT_JUMP_HEIGHT: u32 = 13;
/// MW2's sprint length before the weapon's scale, seconds (MW2: 4; 1..16); the host's SprintTime.
pub const CV_OPT_SPRINT_TIME: u32 = 14;
/// F5c (ABI 8): killstreaks on (1) / off (0); the three picks (CV_KS_* ids, 0 none; only
/// implemented rewards are taken); the EMP's kill count (testing; 0 = MW2's 15).
pub const CV_OPT_KILLSTREAKS: u32 = 15;
pub const CV_OPT_KS_SLOT1: u32 = 16;
pub const CV_OPT_KS_SLOT2: u32 = 17;
pub const CV_OPT_KS_SLOT3: u32 = 18;
pub const CV_OPT_KS_EMP_KILLS: u32 = 19;
/// MW2's Scavenger: each hostile kill (cv_killstreak_step's hostile_kills) gives a magazine
/// more for every carried gun and a frag (1 on, the default; 0 off).
pub const CV_OPT_SCAVENGER: u32 = 20;
/// Fall damage: 0 none, 1 light (from 6 m, deadly from 20 m: CODV's default), 2 MW2's (from 3.3 m, deadly from 7.6 m).
pub const CV_OPT_FALL_DAMAGE: u32 = 21;
/// CODV's slide: crouch while sprinting (1 on, the default).
pub const CV_OPT_SLIDE: u32 = 22;
/// Sprint speed over the run speed (MW2's 1.5; CODV's default 1.65), 1.0..2.5.
pub const CV_OPT_SPRINT_SPEED: u32 = 23;
/// CODV's sprint meter under the crosshair (1 on, the default).
pub const CV_OPT_STAMINA_BAR: u32 = 24;
/// MW2's jump slowdown on landing (0 off: CODV's default, the landing keeps the speed; 1 MW2's).
pub const CV_OPT_JUMP_SLOWDOWN: u32 = 25;
/// CODV's parkour (1 on, the default): sprinting into a low ledge vaults it without the jump key, and
/// every mantle keeps the speed it came in with.
pub const CV_OPT_AUTO_VAULT: u32 = 26;
/// Mantle pace over MW2's, 0.5 .. 3 (CODV's default 1.8).
pub const CV_OPT_MANTLE_SPEED: u32 = 27;
/// CODV's climb onto ledges over MW2's mantle reach, 1.5 to 2.4 m (1 on, the default).
pub const CV_OPT_HIGH_CLIMB: u32 = 28;
/// CODV's dive: holding crouch through a slide (1 on, the default).
pub const CV_OPT_DIVE: u32 = 29;
/// CODV's snag guard (1 on, the default): a run caught on an edge (a car's corner, a fence) keeps its
/// speed when the way ahead is free.
pub const CV_OPT_SNAG_GUARD: u32 = 30;
/// The slide's speed over the sprint at its start, 1.0 .. 2.0 (CODV's default 1.3).
pub const CV_OPT_SLIDE_BOOST: u32 = 31;
/// The slide's length, seconds, 0.2 .. 3 (CODV's default 0.75).
pub const CV_OPT_SLIDE_TIME: u32 = 32;
/// Auto vault only over obstacles this tall, metres, 0 .. 1.5 (0: every ledge MW2 can mantle).
pub const CV_OPT_VAULT_MIN_HEIGHT: u32 = 33;
/// The high climb's top, metres, 1.7 .. 3.0 (CODV's default 2.4).
pub const CV_OPT_CLIMB_MAX_HEIGHT: u32 = 34;
/// MW2's movement sounds: footsteps by surface, landings, mantles, the gear rattle (1 on, the default).
pub const CV_OPT_MOVEMENT_SOUNDS: u32 = 35;
/// CODV's lean (CoD4's): 0 off, else the sideways reach in metres, 0.1 .. 0.6 (CODV's default 0.4).
pub const CV_OPT_LEAN: u32 = 36;
/// The ground under the player for MW2's footsteps: an index into codv_mw2data::MOVEMENT_SURFACES
/// (the host reads GTA's material; 0 concrete).
pub const CV_OPT_SURFACE: u32 = 37;
/// CODV's kill counter in MW2's score bar art (1 on, the default).
pub const CV_OPT_SCOREBAR: u32 = 38;
/// MW2's blood on the screen as health drops (1 on, the default).
pub const CV_OPT_BLOOD: u32 = 39;
/// MW2's kill feedback: the "+100" score popup and the bonus splashes (1 on, the default).
pub const CV_OPT_KILL_FEEDBACK: u32 = 40;
/// Killstreaks not used yet survive a death (MW2's rule: 1); 0, CODV's default: a death loses them.
pub const CV_OPT_KS_KEEP_ON_DEATH: u32 = 41;
/// How fast the sprint meter refills over MW2's (1.0..4.0; CODV's default 1.3).
pub const CV_OPT_SPRINT_RECHARGE: u32 = 42;
/// Every killstreak in play at its kill count (1), not just the three picks (0, MW2's rule).
pub const CV_OPT_KS_ALL: u32 = 43;
/// CODV's spaced kill counts (1: UAV 3 .. Tactical Nuke 35), else MW2's table (0).
pub const CV_OPT_KS_SPACED: u32 = 44;
/// CODV's weapon sets (1): up to three random guns behind each number key, the key again for the
/// group's next; 0: one gun per key (MW2's two plus the sniper and the launcher).
pub const CV_OPT_WEAPON_SETS: u32 = 45;
/// CODV's reload while sprinting (1, the default): a sprint no longer waits for the reload, which goes
/// on at a slightly slower sprint; 0: the sprint starts once the reload ends (CODV 0.1.0).
pub const CV_OPT_RELOAD_SPRINT: u32 = 46;
/// CODV's sprint refill (30% faster than MW2's: the meter was rarely full again).
const SPRINT_RECHARGE: f32 = 1.3;
/// CODV's default sprint speed scale: a little over MW2's 1.5.
const SPRINT_SPEED: f32 = 1.65;
/// CODV's light fall damage heights (units): 6 m and 20 m.
const LIGHT_FALL_HEIGHTS: (f32, f32) = (236.0, 787.0);

#[repr(C)]
pub struct CvCreateInfo {
    pub size: u32,
    pub abi_version: u32,
    /// Installed data folder (<GTA>/CODV): weapons.json.
    pub data_root_utf8: *const c_char,
    /// Runtime log; null disables.
    pub log_path_utf8: *const c_char,
    pub flags: u32,
    pub seed: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvInput {
    pub size: u32,
    pub dt_seconds: f32,
    /// codv_core::player button bits (FIRE, ADS, SPRINT, ...).
    pub held: u32,
    pub flags: u32,
    pub speed: f32,
    pub gta_weapon: u32,
    pub gta_ammo: u32,
    pub health: f32,
    pub health_lost: f32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvShot {
    pub weapon: u32,
    /// Degrees off the view direction.
    pub yaw: f32,
    pub pitch: f32,
    pub gta_hash: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvHud {
    pub mag: u32,
    pub reserve: u32,
    /// Weapon index, -1 none.
    pub weapon: i32,
    pub spread: f32,
    pub ads: f32,
    pub sprint: f32,
    pub reload: f32,
    pub hitmarker: f32,
    pub flags: u32,
    pub damage_flash: f32,
    /// NUL-terminated weapon label.
    pub label: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CvOutput {
    pub size: u32,
    /// codv_core::State; 0 idle.
    pub state: u32,
    pub flags: u32,
    pub shot_count: u32,
    pub shots: [CvShot; CV_MAX_SHOTS],
    /// Camera kick this frame, degrees: pitch (up), yaw (right).
    pub kick: [f32; 2],
    pub move_rate: f32,
    pub sprint_multiplier: f32,
    pub equip_hash: u32,
    pub ammo_used: u32,
    pub jump_speed: f32,
    /// Fraction of max health to restore.
    pub heal: f32,
    pub hud: CvHud,
    /// The held weapon's magazine size (0: unknown; low-ammo warning).
    pub mag_size: u32,
    pub reserved: u32,
}

impl Default for CvOutput {
    fn default() -> Self {
        CvOutput {
            size: size_of::<CvOutput>() as u32,
            state: 0,
            flags: 0,
            shot_count: 0,
            shots: [CvShot::default(); CV_MAX_SHOTS],
            kick: [0.0; 2],
            move_rate: 1.0,
            sprint_multiplier: 1.0,
            equip_hash: 0,
            ammo_used: 0,
            jump_speed: 0.0,
            heal: 0.0,
            hud: CvHud::default(),
            mag_size: 0,
            reserved: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvHit {
    pub weapon: u32,
    pub distance: f32,
    pub flags: u32,
    /// MW2 hit location (weapon_iw4::HITLOC_NAMES index, 1..19); 0: unknown.
    pub hitloc: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CvWeaponInfo {
    pub size: u32,
    pub gta_hash: u32,
    pub mag: u32,
    pub reserve: u32,
    pub name: [u8; 32],
    pub label: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvRules {
    pub size: u32,
    pub melee_damage: f32,
    pub melee_range: f32,
    pub trace_range: f32,
    pub primary: u32,
    pub secondary: u32,
}

type Log = Box<dyn Fn(&str) + Send + Sync>;

struct Inner {
    player: Player,
    /// The Modern Warfare 2 player (iw4L pmove), once spawned.
    mw2: Option<mw2::Mw2State>,
    /// The debug panel: what the hull was last found inside of (and when it was looked at).
    freed_why: String,
    freed_at: Option<Instant>,
    /// Modern Warfare 2's weapons (the setup imported the player's MW2).
    arms: Option<arms::Arms>,
    /// The held MW2 weapon's first-person view model (imported viewmodels).
    vm: Option<viewmodel::Viewmodels>,
    /// Thrown grenades in flight (the MW2 player's frame) and the ones that
    /// went off since the host last asked (`cv_mw2_grenades`).
    grenades: Vec<codv_engine::grenade::Grenade>,
    /// MW2's crosshair and scope state; `scoped`: the view model hides behind the scope.
    hud: hud::Hud,
    scoped: bool,
    /// The player's health (0..1) from the last cv_step, for MW2's blood overlay.
    health: f32,
    /// The Javelin is fully aimed: MW2's CLU view covers the screen (hud `ads_javelin`).
    javelin_ads: bool,
    /// MW2's weapon bar, hit marker, damage arrows and grenade indicator (F5b), with the import's HUD.
    mw2hud: Option<mw2hud::Mw2Hud>,
    /// The weapon's hit counters at the last hit alert (MW2's "tic" on every hit).
    hit_alert_seen: (u32, u32),
    /// F5c: the killstreak count and the rewards earned.
    streaks: killstreaks::Streaks,
    /// MW2's Scavenger on hostile kills (CV_OPT_SCAVENGER).
    scavenger: bool,
    /// Unused killstreaks survive a death (CV_OPT_KS_KEEP_ON_DEATH; off: lost).
    ks_keep_on_death: bool,
    /// MW2's jump height (CV_OPT_JUMP_HEIGHT), units.
    jump_height: f32,
    /// MW2's sprint length (CV_OPT_SPRINT_TIME), seconds.
    sprint_seconds: f32,
    /// The sprint meter's refill over MW2's (CV_OPT_SPRINT_RECHARGE).
    sprint_recharge: f32,
    /// Fall damage heights (CV_OPT_FALL_DAMAGE), units; None: none.
    fall_heights: Option<(f32, f32)>,
    /// CODV's slide (CV_OPT_SLIDE).
    slide: bool,
    /// Sprint speed scale (CV_OPT_SPRINT_SPEED).
    sprint_speed: f32,
    /// MW2's jump slowdown (CV_OPT_JUMP_SLOWDOWN).
    jump_slowdown: bool,
    /// CODV's reload while sprinting (CV_OPT_RELOAD_SPRINT).
    reload_sprint: bool,
    /// CODV's parkour (CV_OPT_AUTO_VAULT, CV_OPT_MANTLE_SPEED, CV_OPT_HIGH_CLIMB, CV_OPT_DIVE).
    auto_vault: bool,
    mantle_speed: f32,
    high_climb: bool,
    dive: bool,
    snag_guard: bool,
    /// The movement tuning (CV_OPT_SLIDE_BOOST .. CV_OPT_CLIMB_MAX_HEIGHT), IW4 units / ms.
    slide_boost: f32,
    slide_ms: i32,
    vault_min_height: f32,
    climb_max: f32,
    /// MW2's movement sounds (CV_OPT_MOVEMENT_SOUNDS).
    movement_sounds: bool,
    /// CODV's lean reach, units (CV_OPT_LEAN; 0 off).
    lean_dist: f32,
    /// The ground's surface for the footsteps (CV_OPT_SURFACE).
    surface: usize,
    /// Footsteps played (the gear rattle comes with some of them).
    steps: u32,
    explosions: Vec<CvGrenade>,
    next_grenade: u32,
    /// MW2's third-person character on the GTA ped (imported animation).
    body: Option<body::Character>,
    /// Vehicles, props and peds near the player, as the host listed them.
    movers: movers::Movers,
    /// The host's world: trace callback + its user pointer.
    world: Option<(CvTraceFn, usize)>,
    verbose: bool,
    last_state: u32,
    status: String,
}

struct Runtime {
    inner: Mutex<Inner>,
    /// GTA's static map as MW2 meshes (its own lock and worker thread).
    collision: collision::Streamer,
    /// MW2's weapon sounds (F5a); its own lock: the host's audio feeder
    /// thread renders from it while the game thread steps.
    audio: Mutex<Option<audio::Audio>>,
    log: Arc<Log>,
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

unsafe fn runtime<'a>(rt: *const c_void) -> Option<&'a Runtime> {
    unsafe { (rt as *const Runtime).as_ref() }
}

unsafe fn path_arg(p: *const c_char) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_str().ok()?;
    (!s.is_empty()).then(|| PathBuf::from(s))
}

fn file_logger(path: Option<PathBuf>) -> Log {
    let file = path.and_then(|p| OpenOptions::new().create(true).append(true).open(p).ok());
    let file = Mutex::new(file);
    Box::new(move |line: &str| {
        if let Ok(mut f) = file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = writeln!(f, "{line}");
        }
    })
}

/// The installed weapons.json or the embedded copy, logging which.
pub fn load_table(root: Option<&Path>, log: &dyn Fn(&str)) -> Table {
    let table = match root.map(|r| r.join("weapons.json")) {
        Some(path) if path.exists() => {
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Table::parse(&s)) {
                Ok(t) => t,
                Err(e) => {
                    log(&format!("weapons.json unusable ({e}); using the embedded table"));
                    Table::embedded()
                }
            }
        }
        _ => Table::embedded(),
    };
    log(&format!(
        "{} weapons; loadout {} / {}",
        table.weapons.len(),
        table.weapons[table.primary].name,
        table.weapons[table.secondary].name
    ));
    table
}

fn copy_str(dst: &mut [u8], s: &str) {
    dst.fill(0);
    let n = s.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&s.as_bytes()[..n]);
}

fn encode(p: &Player, o: &player::FrameOut) -> CvOutput {
    let mut out = CvOutput {
        state: o.state.map_or(State::Idle as u32, |s| s as u32),
        kick: o.kick,
        move_rate: o.move_rate,
        sprint_multiplier: o.sprint_multiplier,
        equip_hash: o.equip.unwrap_or(0),
        ammo_used: o.ammo_used,
        jump_speed: o.jump.unwrap_or(0.0),
        heal: o.heal,
        ..Default::default()
    };
    let mut flag = |cond: bool, bit: u32| {
        if cond {
            out.flags |= bit;
        }
    };
    flag(o.block_sprint, CV_OUT_BLOCK_SPRINT);
    flag(o.reload_started, CV_OUT_RELOAD);
    flag(o.melee, CV_OUT_MELEE);
    flag(o.equip.is_some(), CV_OUT_EQUIP);
    flag(o.jump.is_some(), CV_OUT_JUMP);
    flag(o.crouch_changed.is_some(), CV_OUT_CROUCH_CHANGED);
    flag(p.crouched(), CV_OUT_CROUCHED);
    flag(o.state.is_some(), CV_OUT_ACTIVE);
    for (slot, s) in out.shots.iter_mut().zip(&o.shots) {
        *slot = CvShot {
            weapon: s.weapon as u32,
            yaw: s.yaw,
            pitch: s.pitch,
            gta_hash: p.table.weapons[s.weapon].gta_hash,
        };
    }
    out.shot_count = o.shots.len().min(CV_MAX_SHOTS) as u32;
    let h = &o.hud;
    out.hud = CvHud {
        mag: h.mag,
        reserve: h.reserve,
        weapon: h.weapon.map_or(-1, |w| w as i32),
        spread: h.spread,
        ads: h.ads,
        sprint: h.sprint,
        reload: h.reload,
        hitmarker: h.hitmarker,
        flags: (if h.kill { CV_HUD_KILL } else { 0 }) | (if h.headshot { CV_HUD_HEADSHOT } else { 0 }),
        damage_flash: h.damage_flash,
        label: [0; 32],
    };
    if let Some(w) = h.weapon {
        copy_str(&mut out.hud.label, &p.table.weapons[w].label);
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn cv_api_version() -> u32 {
    CODV_ABI_VERSION
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_create(info: *const CvCreateInfo) -> *mut c_void {
    guard(ptr::null_mut(), || {
        let Some(info) = (unsafe { info.as_ref() }) else {
            return ptr::null_mut();
        };
        if info.abi_version != CODV_ABI_VERSION || (info.size as usize) < size_of::<CvCreateInfo>() {
            return ptr::null_mut();
        }
        let root = unsafe { path_arg(info.data_root_utf8) };
        let log = Arc::new(file_logger(unsafe { path_arg(info.log_path_utf8) }));
        log(&format!("CodVRuntime {} ABI {CODV_ABI_VERSION}", env!("CARGO_PKG_VERSION")));
        let table = load_table(root.as_deref(), &**log);
        let arms = arms::Arms::load(root.as_deref(), &**log);
        if arms.is_none() {
            log("mw2: no imported MW2 weapons (run INSTALL.bat with -Mw2): interim gunplay");
        }
        let vm = arms.as_ref().and_then(|a| viewmodel::Viewmodels::load(root.as_deref(), &a.table, &**log));
        let character = body::Character::load(root.as_deref(), &**log);
        let sound = arms.as_ref().and_then(|_| audio::Audio::load(root.as_deref(), &**log));
        let mut mw2hud = arms.as_ref().and_then(|_| mw2hud::Mw2Hud::load(root.as_deref(), &**log));
        // The Chopper Gunner's turret reticle (its gun's centre reticle, from the import).
        if let (Some(h), Some(a)) = (mw2hud.as_mut(), arms.as_ref()) {
            h.vehicle_reticle = a
                .find_row(codv_mw2data::KILLSTREAK_TURRETS[0])
                .map(|r| (r.s("reticle_center_image").to_owned(), r.f("reticle_center_size")))
                .filter(|(img, size)| !img.is_empty() && *size > 0.0);
        }
        let streaks = killstreaks::Streaks::new(&mw2hud.as_ref().map(|h| h.killstreak_table()).unwrap_or_default());
        let arms_status =
            arms.as_ref().map_or_else(|| "ready (no MW2 weapons)".into(), |a| format!("ready; {}", a.summary));
        let rt = Box::new(Runtime {
            inner: Mutex::new(Inner {
                player: Player::new(table, info.seed as u64),
                mw2: None,
                freed_why: String::new(),
                freed_at: None,
                arms,
                vm,
                body: character,
                movers: movers::Movers::default(),
                grenades: Vec::new(),
                hud: hud::Hud::default(),
                scoped: false,
                health: 1.0,
                javelin_ads: false,
                mw2hud,
                hit_alert_seen: (0, 0),
                streaks,
                scavenger: true,
                ks_keep_on_death: false,
                jump_height: codv_engine::player::MW2_JUMP_HEIGHT,
                sprint_seconds: codv_engine::player::MW2_SPRINT_SECONDS,
                sprint_recharge: SPRINT_RECHARGE,
                fall_heights: Some(LIGHT_FALL_HEIGHTS),
                slide: true,
                sprint_speed: SPRINT_SPEED,
                jump_slowdown: false,
                reload_sprint: true,
                auto_vault: true,
                mantle_speed: 1.8,
                high_climb: true,
                dive: true,
                snag_guard: true,
                slide_boost: 1.3,
                slide_ms: codv_engine::player::SLIDE_MS,
                vault_min_height: 0.0,
                climb_max: 96.0,
                movement_sounds: true,
                lean_dist: codv_engine::player::LEAN_DIST,
                surface: 0,
                steps: 0,
                explosions: Vec::new(),
                next_grenade: 1,
                world: None,
                verbose: false,
                last_state: 0,
                status: arms_status,
            }),
            collision: collision::Streamer::new(log.clone()),
            audio: Mutex::new(sound),
            log,
        });
        if let Ok(inner) = rt.inner.lock() {
            prewarm(&inner, &rt.audio);
        }
        Box::into_raw(rt) as *mut c_void
    })
}

/// How loud each movement sound plays against its alias's volume: steps under the guns, a walk
/// softer than a run, a sprint the loudest step.
fn foley_gain(f: codv_engine::player::Foley) -> f32 {
    use codv_engine::player::{Foley, StepKind};
    match f {
        Foley::Step(StepKind::Walk) => 0.35,
        Foley::Step(StepKind::Prone) => 0.4,
        Foley::Step(StepKind::Run) => 0.5,
        Foley::Step(StepKind::Sprint) => 0.65,
        Foley::Land { .. } => 0.8,
        Foley::Mantle | Foley::Slide | Foley::Dive => 0.7,
    }
}

/// MW2's aliases for a movement sound on `surface` (codv_mw2data::movement_sounds).
fn foley_aliases(f: codv_engine::player::Foley, surface: &str) -> Vec<String> {
    use codv_engine::player::{Foley, StepKind};
    let on = |kind: &str| format!("{kind}_{surface}");
    match f {
        Foley::Step(StepKind::Walk) => vec![on("step_walk_plr")],
        Foley::Step(StepKind::Run) => vec![on("step_run_plr")],
        Foley::Step(StepKind::Sprint) => vec![on("step_sprint_plr")],
        Foley::Step(StepKind::Prone) => vec![on("step_prone_plr")],
        Foley::Land { hard: false } => vec![on("land_plr"), "gear_rattle_plr_land".into()],
        Foley::Land { hard: true } => vec!["land_plr_damage".into(), "gear_rattle_plr_land".into()],
        Foley::Mantle => vec!["gear_rattle_plr_mantle".into()],
        Foley::Slide => vec!["gear_rattle_plr_prone".into(), on("step_sprint_plr")],
        Foley::Dive => vec!["gear_rattle_plr_land".into()],
    }
}

/// Reads the guns the player can hold next (view models, sounds) on
/// background threads, so switching the mode on or drawing one of them does
/// not wait on the disk. Takes the audio lock after the inner one (cv_mw2_step's order).
fn prewarm(inner: &Inner, audio: &Mutex<Option<audio::Audio>>) {
    let Some(arms) = inner.arms.as_ref() else { return };
    let guns = arms.ready_set();
    if let Some(vm) = inner.vm.as_ref() {
        vm.prewarm(guns.iter().map(|g| g.name.clone()).collect());
    }
    if let Ok(mut a) = audio.lock()
        && let Some(a) = a.as_mut()
    {
        a.prewarm(&guns);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_destroy(rt: *mut c_void) {
    if rt.is_null() {
        return;
    }
    guard((), || {
        let rt = unsafe { Box::from_raw(rt as *mut Runtime) };
        (rt.log)("runtime released");
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_step(rt: *mut c_void, input: *const CvInput, out: *mut CvOutput) -> u32 {
    guard(0, || {
        let (Some(rt), Some(input), Some(out)) =
            (unsafe { runtime(rt) }, unsafe { input.as_ref() }, unsafe { out.as_mut() })
        else {
            return 0;
        };
        if (input.size as usize) < size_of::<CvInput>() || (out.size as usize) < size_of::<CvOutput>() {
            return 0;
        }
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let finite = |x: f32| if x.is_finite() { x } else { 0.0 };
        let frame = FrameIn {
            dt: finite(input.dt_seconds),
            held: input.held,
            on_foot: input.flags & CV_IN_ON_FOOT != 0,
            airborne: input.flags & CV_IN_AIRBORNE != 0,
            sprinting: input.flags & CV_IN_SPRINTING != 0,
            speed: finite(input.speed).max(0.0),
            gta_weapon: input.gta_weapon,
            gta_ammo: input.gta_ammo,
            health: finite(input.health).clamp(0.0, 1.0),
            health_lost: finite(input.health_lost).max(0.0),
        };
        inner.health = frame.health;
        // MW2's weapons while MW2 movement drives the player: the interim
        // core only keeps health (regeneration, damage flash) with no weapon.
        if inner.mw2.is_some()
            && let Some(arms) = inner.arms.as_mut()
            && arms.last.is_some()
        {
            // GTA's weapon_unarmed: the interim core holds nothing.
            let idle = FrameIn { held: 0, gta_weapon: 0xA271_9263, ..frame };
            let o = inner.player.step(&idle);
            let arms = inner.arms.as_mut().unwrap();
            let encoded = arms.encode(frame.dt, o.heal, o.hud.damage_flash);
            inner.status = format!(
                "MW2 {} {}/{} state {} spread {:.2} ads {:.2}",
                std::str::from_utf8(&encoded.hud.label).unwrap_or("").trim_end_matches('\0'),
                encoded.hud.mag,
                encoded.hud.reserve,
                encoded.state,
                encoded.hud.spread,
                encoded.hud.ads
            );
            *out = encoded;
            return 1;
        }
        let o = inner.player.step(&frame);
        let encoded = encode(&inner.player, &o);
        if inner.verbose && (encoded.state != inner.last_state || o.reload_started || o.equip.is_some()) {
            (rt.log)(&format!(
                "state {} -> {} reload {} equip {:?} mag {}",
                inner.last_state, encoded.state, o.reload_started, o.equip, o.hud.mag
            ));
        }
        inner.last_state = encoded.state;
        inner.status = match o.hud.weapon {
            Some(w) => format!(
                "{} {}/{} state {} spread {:.2} ads {:.2} sprint {:.2}",
                inner.player.table.weapons[w].name,
                o.hud.mag,
                o.hud.reserve,
                encoded.state,
                o.hud.spread,
                o.hud.ads,
                o.hud.sprint
            ),
            None => "no CODV weapon in hand".into(),
        };
        *out = encoded;
        1
    })
}

/// Damage for each hit (same order) into `damage_out`; returns the count written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_report_hits(rt: *mut c_void, hits: *const CvHit, count: u32, damage_out: *mut f32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if hits.is_null() || damage_out.is_null() || count == 0 {
            return 0;
        }
        let n = (count as usize).min(256);
        let hits = unsafe { std::slice::from_raw_parts(hits, n) };
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        if inner.mw2.is_some()
            && let Some(arms) = inner.arms.as_mut()
            && arms.gunner.is_some()
        {
            let out = unsafe { std::slice::from_raw_parts_mut(damage_out, n) };
            for (o, h) in out.iter_mut().zip(hits) {
                *o = arms.damage(h);
            }
            return n as u32;
        }
        let list: Vec<Hit> = hits
            .iter()
            .map(|h| Hit {
                weapon: h.weapon as usize,
                distance: if h.distance.is_finite() { h.distance.max(0.0) } else { 1e9 },
                head: h.flags & CV_HIT_HEAD != 0,
                ped: h.flags & CV_HIT_PED != 0,
            })
            .collect();
        let damage = inner.player.report_hits(&list);
        let out = unsafe { std::slice::from_raw_parts_mut(damage_out, n) };
        out.copy_from_slice(&damage);
        n as u32
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_report_kill(rt: *mut c_void) {
    guard((), || {
        if let Some(rt) = unsafe { runtime(rt) }
            && let Ok(mut inner) = rt.inner.lock()
        {
            inner.player.report_kill();
            if let Some(arms) = inner.arms.as_mut() {
                arms.killed();
            }
        }
    })
}

/// Forgets transient state (mode switched off): crouch, kick, ADS.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_reset(rt: *mut c_void) {
    guard((), || {
        if let Some(rt) = unsafe { runtime(rt) }
            && let Ok(mut inner) = rt.inner.lock()
        {
            inner.player.reset();
            if let Some(arms) = inner.arms.as_mut() {
                arms.reset();
            }
            if let Some(vm) = inner.vm.as_mut() {
                vm.reset();
            }
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_set_option(rt: *mut c_void, id: u32, value: f32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else {
            return 0;
        };
        if !value.is_finite() {
            return 0;
        }
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        let p = &mut inner.player;
        let n = p.table.weapons.len();
        match id {
            CV_OPT_ENABLED => p.options.enabled = value != 0.0,
            CV_OPT_QUICK_JUMP => p.options.quick_jump = value != 0.0,
            CV_OPT_REGEN => p.options.regen = value != 0.0,
            CV_OPT_RECOIL_SCALE => p.options.recoil_scale = value.clamp(0.25, 2.0),
            CV_OPT_PRIMARY | CV_OPT_SECONDARY => {
                let i = value as usize;
                if value < 0.0 || i >= n {
                    return 0;
                }
                if id == CV_OPT_PRIMARY {
                    p.primary = i;
                } else {
                    p.secondary = i;
                }
            }
            CV_OPT_VERBOSE => inner.verbose = value != 0.0,
            CV_OPT_KILLSTREAKS => inner.streaks.enabled = value != 0.0,
            CV_OPT_SCAVENGER => inner.scavenger = value != 0.0,
            CV_OPT_KS_KEEP_ON_DEATH => inner.ks_keep_on_death = value != 0.0,
            CV_OPT_KS_ALL => {
                let all = value != 0.0;
                if inner.streaks.all != all {
                    (rt.log)(if all { "killstreak: all ten in play" } else { "killstreak: the three picks only" });
                }
                inner.streaks.all = all;
            }
            CV_OPT_KS_SPACED => {
                let spaced = value != 0.0;
                if inner.streaks.spaced != spaced {
                    (rt.log)(if spaced {
                        "killstreak: spaced kill counts (uav 3 .. nuke 35)"
                    } else {
                        "killstreak: MW2's kill counts (uav 3 .. nuke 25)"
                    });
                }
                inner.streaks.spaced = spaced;
            }
            CV_OPT_SLIDE => inner.slide = value != 0.0,
            CV_OPT_JUMP_SLOWDOWN => inner.jump_slowdown = value != 0.0,
            CV_OPT_RELOAD_SPRINT => inner.reload_sprint = value != 0.0,
            CV_OPT_AUTO_VAULT => inner.auto_vault = value != 0.0,
            CV_OPT_HIGH_CLIMB => inner.high_climb = value != 0.0,
            CV_OPT_DIVE => inner.dive = value != 0.0,
            CV_OPT_SNAG_GUARD => inner.snag_guard = value != 0.0,
            CV_OPT_MOVEMENT_SOUNDS => inner.movement_sounds = value != 0.0,
            CV_OPT_SURFACE => {
                let i = value as usize;
                if !(value >= 0.0 && i < codv_mw2data::MOVEMENT_SURFACES.len()) {
                    return 0;
                }
                inner.surface = i;
            }
            CV_OPT_LEAN => {
                if !value.is_finite() || !(value == 0.0 || (0.1..=0.6).contains(&value)) {
                    return 0;
                }
                inner.lean_dist = value / codv_engine::units::METRES_PER_UNIT;
            }
            CV_OPT_SLIDE_BOOST | CV_OPT_SLIDE_TIME | CV_OPT_VAULT_MIN_HEIGHT | CV_OPT_CLIMB_MAX_HEIGHT => {
                let range = match id {
                    CV_OPT_SLIDE_BOOST => 1.0..=2.0,
                    CV_OPT_SLIDE_TIME => 0.2..=3.0,
                    CV_OPT_VAULT_MIN_HEIGHT => 0.0..=1.5,
                    _ => 1.7..=3.0,
                };
                if !value.is_finite() || !range.contains(&value) {
                    return 0;
                }
                let units = value / codv_engine::units::METRES_PER_UNIT;
                match id {
                    CV_OPT_SLIDE_BOOST => inner.slide_boost = value,
                    CV_OPT_SLIDE_TIME => inner.slide_ms = (value * 1000.0) as i32,
                    CV_OPT_VAULT_MIN_HEIGHT => inner.vault_min_height = units,
                    _ => inner.climb_max = units,
                }
            }
            CV_OPT_MANTLE_SPEED => {
                if !value.is_finite() || !(0.5..=3.0).contains(&value) {
                    return 0;
                }
                inner.mantle_speed = value;
            }
            CV_OPT_SPRINT_SPEED => {
                if !value.is_finite() || !(1.0..=2.5).contains(&value) {
                    return 0;
                }
                inner.sprint_speed = value;
            }
            CV_OPT_STAMINA_BAR => {
                if let Some(h) = inner.mw2hud.as_mut() {
                    h.stamina_bar = value != 0.0;
                }
            }
            CV_OPT_SCOREBAR => {
                if let Some(h) = inner.mw2hud.as_mut() {
                    h.scorebar_on = value != 0.0;
                }
            }
            CV_OPT_BLOOD => {
                if let Some(h) = inner.mw2hud.as_mut() {
                    h.blood_on = value != 0.0;
                }
            }
            CV_OPT_KILL_FEEDBACK => {
                if let Some(h) = inner.mw2hud.as_mut() {
                    h.feedback_on = value != 0.0;
                }
            }
            CV_OPT_FALL_DAMAGE => {
                inner.fall_heights = match value as i32 {
                    0 => None,
                    1 => Some(LIGHT_FALL_HEIGHTS),
                    2 => Some(codv_engine::player::MW2_FALL_HEIGHTS),
                    _ => return 0,
                }
            }
            CV_OPT_KS_SLOT1 | CV_OPT_KS_SLOT2 | CV_OPT_KS_SLOT3 => {
                let before = inner.streaks.slots;
                if value < 0.0 || !inner.streaks.set_slot((id - CV_OPT_KS_SLOT1) as usize, value as u32) {
                    return 0;
                }
                // The log names the picks when they change (a reward not picked never comes).
                if inner.streaks.slots != before {
                    let picks: Vec<String> = inner
                        .streaks
                        .slots
                        .iter()
                        .map(|&id| match (killstreaks::name_of(id), inner.streaks.kills_for(id)) {
                            (Some(n), Some(k)) => format!("{n} ({k})"),
                            _ => "off".into(),
                        })
                        .collect();
                    (rt.log)(&format!("killstreak: picks {}", picks.join(" / ")));
                }
            }
            CV_OPT_KS_EMP_KILLS => inner.streaks.emp_kills = (value >= 1.0).then_some(value as i32),
            CV_OPT_SPRINT_RECHARGE => {
                if !value.is_finite() || !(1.0..=4.0).contains(&value) {
                    return 0;
                }
                inner.sprint_recharge = value;
            }
            CV_OPT_SPRINT_TIME => {
                if !value.is_finite() || !(1.0..=16.0).contains(&value) {
                    return 0;
                }
                inner.sprint_seconds = value;
            }
            CV_OPT_JUMP_HEIGHT => {
                if !value.is_finite() || !(20.0..=80.0).contains(&value) {
                    return 0;
                }
                inner.jump_height = value;
            }
            CV_OPT_AMMO_SCALE | CV_OPT_FRAGS | CV_OPT_MW2_PRIMARY | CV_OPT_MW2_SECONDARY | CV_OPT_MW2_RANDOMIZE
            | CV_OPT_WEAPON_SETS => {
                let Some(arms) = inner.arms.as_mut() else { return 0 };
                match id {
                    CV_OPT_WEAPON_SETS => {
                        if let Some(names) = arms.set_sets(value != 0.0) {
                            (rt.log)(&format!(
                                "mw2: weapon sets {}; loadout {}",
                                if value != 0.0 { "on" } else { "off" },
                                names.join(" / ")
                            ));
                            if let Some(vm) = inner.vm.as_mut() {
                                vm.reset();
                            }
                            prewarm(&inner, &rt.audio);
                        }
                    }
                    CV_OPT_MW2_RANDOMIZE => {
                        let Some(names) = arms.randomize() else { return 0 };
                        (rt.log)(&format!("mw2: random loadout {}", names.join(" / ")));
                        if let Some(vm) = inner.vm.as_mut() {
                            vm.reset();
                        }
                        prewarm(&inner, &rt.audio);
                    }
                    CV_OPT_AMMO_SCALE => arms.set_supplies(Some(value), None),
                    CV_OPT_FRAGS => arms.set_supplies(None, Some(value as i32)),
                    _ => {
                        if value < 0.0 || !arms.set_slot(usize::from(id == CV_OPT_MW2_SECONDARY), value as usize) {
                            return 0;
                        }
                        if let Some(vm) = inner.vm.as_mut() {
                            vm.reset();
                        }
                        prewarm(&inner, &rt.audio);
                    }
                }
            }
            _ => return 0,
        }
        1
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_weapon_count(rt: *const c_void) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        rt.inner.lock().map(|i| i.player.table.weapons.len() as u32).unwrap_or(0)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_weapon_info(rt: *const c_void, index: u32, out: *mut CvWeaponInfo) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if (out.size as usize) < size_of::<CvWeaponInfo>() {
            return 0;
        }
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(w) = inner.player.table.weapons.get(index as usize) else { return 0 };
        out.gta_hash = w.gta_hash;
        out.mag = w.mag;
        out.reserve = w.reserve;
        copy_str(&mut out.name, &w.name);
        copy_str(&mut out.label, &w.label);
        1
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_rules(rt: *const c_void, out: *mut CvRules) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if (out.size as usize) < size_of::<CvRules>() {
            return 0;
        }
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let p = &inner.player;
        out.melee_damage = p.table.player.melee_damage;
        out.melee_range = p.table.player.melee_range;
        out.trace_range = p.table.player.trace_range;
        out.primary = p.primary as u32;
        out.secondary = p.secondary as u32;
        1
    })
}

/// Sets (or with a null `trace`, clears) the world the MW2 player collides
/// with. `user` is passed back to every call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_set_world(rt: *mut c_void, trace: Option<CvTraceFn>, user: *mut c_void) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        inner.world = trace.map(|t| (t, user as usize));
        1
    })
}

/// Puts the MW2 player in the world at rest (replacing any). Returns 1 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_spawn(rt: *mut c_void, spawn: *const CvMoveSpawn) -> u32 {
    guard(0, || {
        let (Some(rt), Some(s)) = (unsafe { runtime(rt) }, unsafe { spawn.as_ref() }) else { return 0 };
        if (s.size as usize) < size_of::<CvMoveSpawn>() {
            return 0;
        }
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        match mw2::spawn(s) {
            Some(p) => {
                inner.mw2 = Some(p);
                if let Some(b) = inner.body.as_mut() {
                    b.reset();
                }
                if let Ok(mut a) = rt.audio.lock()
                    && let Some(a) = a.as_mut()
                {
                    a.reset();
                }
                (rt.log)(&format!("mw2 player spawned at {:?} heading {:.1}", s.position, s.heading));
                1
            }
            None => 0,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_despawn(rt: *mut c_void) {
    guard((), || {
        if let Some(rt) = unsafe { runtime(rt) }
            && let Ok(mut inner) = rt.inner.lock()
            && inner.mw2.take().is_some()
        {
            if let Some(vm) = inner.vm.as_mut() {
                vm.reset();
            }
            // Grenades in flight go with the player (the host removes their models).
            inner.grenades.clear();
            inner.explosions.clear();
            if let Ok(mut a) = rt.audio.lock()
                && let Some(a) = a.as_mut()
            {
                a.active = false;
            }
            (rt.log)("mw2 player removed");
        }
    })
}

/// One frame of MW2 movement. Returns 1 with `out` filled; 0 without a
/// spawned player or any world, or on bad arguments (`out` untouched).
///
/// The world is GTA's static map around the player once the host reported
/// GTA's physics level (`cv_set_physics_level`), plus the host's trace
/// callback when one is set. While the map around the player is still being
/// read, the player holds still and `out.flags` carries `CV_MOVE_WAITING`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_step(rt: *mut c_void, input: *const CvMoveInput, out: *mut CvMoveOutput) -> u32 {
    guard(0, || {
        let (Some(rt), Some(input), Some(out)) =
            (unsafe { runtime(rt) }, unsafe { input.as_ref() }, unsafe { out.as_mut() })
        else {
            return 0;
        };
        if (input.size as usize) < size_of::<CvMoveInput>() || (out.size as usize) < size_of::<CvMoveOutput>() {
            return 0;
        }
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let Some(st) = inner.mw2.as_mut() else { return 0 };
        let at = st.position();
        rt.collision.focus([at[0], at[1]]);
        let area = rt.collision.area().filter(|a| a.covers([at[0], at[1]]));
        if area.is_none() && rt.collision.has_level() {
            *out = mw2::hold(st);
            return 1;
        }
        let host =
            inner.world.map(|(trace, user)| mw2::CallbackWorld { trace, user: user as *mut c_void, frame: st.frame });
        let map = area.as_ref().map(|a| collision::AreaView::new(a, st.frame));
        let movers = movers::MoverView { bodies: inner.movers.bodies(), frame: st.frame };
        if map.is_none() && host.is_none() {
            return 0;
        }
        let mut worlds: Vec<&dyn World> = vec![&movers];
        if let Some(m) = &map {
            worlds.push(m);
        }
        if let Some(h) = &host {
            worlds.push(h);
        }
        // MW2's weapon: its ADS and movement facts into pmove, then one tick
        // after pmove with the same buttons (iw4L's sim order).
        if let Some(arms) = inner.arms.as_mut() {
            if input.weapon_request != 0 && arms.gunner.as_ref().is_some_and(|g| g.reloading()) {
                (rt.log)("weapon switch asked mid-reload: the reload is cancelled");
            }
            arms.reload_sprint = inner.reload_sprint;
            arms.before_step(&mut st.player, input.weapon_request);
        }
        // The knife or the frag on the trigger (number keys): pmove and the weapon see MW2's buttons.
        let mapped = CvMoveInput {
            buttons: inner.arms.as_ref().map_or(input.buttons, |a| a.map_buttons(input.buttons)),
            ..*input
        };
        let input = &mapped;
        let worlds = mw2::Many(worlds);
        st.player.jump_height = inner.jump_height;
        st.player.sprint_seconds = inner.sprint_seconds;
        st.player.sprint_recharge = inner.sprint_recharge;
        st.player.fall_heights = inner.fall_heights;
        st.player.slide_enabled = inner.slide;
        st.player.sprint_speed_scale = inner.sprint_speed;
        st.player.jump_slowdown = inner.jump_slowdown;
        st.player.auto_vault = inner.auto_vault;
        st.player.mantle_momentum = inner.auto_vault;
        st.player.mantle_speed = inner.mantle_speed;
        st.player.high_climb = inner.high_climb;
        st.player.dive_enabled = inner.dive;
        st.player.snag_guard = inner.snag_guard;
        st.player.slide_boost = inner.slide_boost;
        st.player.slide_ms = inner.slide_ms;
        st.player.vault_min_height = inner.vault_min_height;
        st.player.climb_max = inner.climb_max;
        st.player.lean_enabled = inner.lean_dist > 0.0;
        st.player.lean_dist = inner.lean_dist;
        // Grenades in flight move first, in the frame the worlds were made for.
        let frame_before = st.frame;
        let mut gone = Vec::new();
        for g in &mut inner.grenades {
            if let Some(e) = g.step(input.msec as i32, &worlds) {
                gone.push(e);
            }
        }
        let (before, hull) = (st.player.peek().origin, st.player.hull_maxs());
        *out = mw2::step(st, input, &worlds);
        drop(worlds);
        // Debug: the hull was found inside something; name it (at most a few times a second).
        if out.flags & mw2::CV_MOVE_FREED != 0
            && inner.freed_at.is_none_or(|t| t.elapsed() >= Duration::from_millis(400))
        {
            inner.freed_at = Some(Instant::now());
            use codv_engine::player::{PLAYER_MINS, PLAYER_TRACE_MASK};
            let movers = movers::MoverView { bodies: inner.movers.bodies(), frame: frame_before };
            let (t, entity) = movers.trace_entity(before, before, PLAYER_MINS, hull, PLAYER_TRACE_MASK);
            let mut why = Vec::new();
            if entity != 0 && t.startsolid != 0 {
                let kind = inner.movers.bodies().iter().find(|b| b.entity == entity).map_or(9, |b| b.kind);
                let kind = ["vehicle", "ped", "object"].get(kind as usize).copied().unwrap_or("mover");
                why.push(format!("{kind} entity {entity}"));
            }
            if let Some(a) = &area {
                why.extend(collision::AreaView::new(a, frame_before).overlaps(
                    before,
                    PLAYER_MINS,
                    hull,
                    PLAYER_TRACE_MASK,
                ));
            }
            if why.is_empty() {
                why.push("nothing found again (a sweep and the overlap test disagree)".into());
            }
            let m = METRES_PER_UNIT as f64;
            let at =
                [frame_before[0] + before[0] as f64 * m, frame_before[1] + before[1] as f64 * m, before[2] as f64 * m];
            inner.freed_why = format!("inside {}", why.join("; "));
            (rt.log)(&format!("freed at {:.2} {:.2} {:.2}: {}", at[0], at[1], at[2], inner.freed_why));
        }
        let shift = collision::offset(frame_before, st.frame);
        if shift != [0.0; 3] {
            for g in &mut inner.grenades {
                g.shift(shift);
            }
            for e in &mut gone {
                for (o, d) in e.origin.iter_mut().zip(shift) {
                    *o += d;
                }
            }
        }
        for e in gone {
            inner.grenades.retain(|g| g.id != e.id);
            inner.explosions.push(grenade_report(st.frame, &e, CV_GRENADE_EXPLODED));
        }
        if let Some(arms) = inner.arms.as_mut() {
            // A sprint over aim: the weapon is not aiming either.
            let buttons = if st.player.sprint_over_ads {
                input.buttons & !codv_engine::player::button::ADS
            } else {
                input.buttons
            };
            out.kick = arms.after_step(&mut st.player, input.msec, buttons, (input.forward, input.right));
            let mut notes = Vec::new();
            if let (Some(vm), Some(frame)) = (inner.vm.as_mut(), arms.last.as_ref()) {
                // An offhand throw shows the grenade's viewmodel (pin, cook, throw).
                let shown = frame.offhand_view.unwrap_or(frame.weapon);
                let name = arms.arsenal.any(shown).map_or("", |g| g.name.as_str());
                vm.tick(shown, name, &st.player, frame, input.msec as f32 / 1000.0, &**rt.log);
                notes = vm.take_notes();
            }
            if let Some(h) = inner.mw2hud.as_mut() {
                h.advance(input.msec);
            }
            // The scope overlay hides the view model (iw4L viewweapon_drawgun).
            let row = arms.row(arms.shown);
            inner.scoped = inner.hud.frame(&st.player, row, 65.0, false).scoped;
            // The Javelin's CLU (overlayInterface 1) at full ADS hides it too (hud `ads_javelin`).
            inner.javelin_ads = row.is_some_and(|r| r.f("overlay_interface") == 1.0)
                && arms.latest.ads >= 0.999
                && arms.shown == arms.latest.weapon;
            inner.scoped |= inner.javelin_ads;
            // Grenades: one leaves the hand, or a cooked one goes off in it.
            if let Some(frame) = arms.last.as_ref() {
                let ps = &st.player.ps;
                let eye = [ps.origin[0], ps.origin[1], ps.origin[2] + ps.view_height_current];
                if let Some((weapon, fuse)) = frame.thrown
                    && let Some((_, t)) = arms.arsenal.offhand(weapon)
                {
                    let id = inner.next_grenade;
                    inner.next_grenade = inner.next_grenade.wrapping_add(1).max(1);
                    let g =
                        codv_engine::grenade::Grenade::thrown(id, weapon, *t, eye, ps.viewangles, ps.velocity, fuse);
                    inner.grenades.push(g);
                }
                if let Some(weapon) = frame.cooked_off
                    && let Some((_, t)) = arms.arsenal.offhand(weapon)
                {
                    let id = inner.next_grenade;
                    inner.next_grenade = inner.next_grenade.wrapping_add(1).max(1);
                    let g =
                        codv_engine::grenade::Grenade::thrown(id, weapon, *t, eye, ps.viewangles, [0.0; 3], Some(1));
                    inner.explosions.push(grenade_report(st.frame, &g.explosion(), CV_GRENADE_EXPLODED));
                }
            }
            let foley = st.player.take_foley();
            // F5a: the weapon's sounds for this step (its events and the view
            // model's notetracks).
            if let Ok(mut a) = rt.audio.lock()
                && let Some(a) = a.as_mut()
            {
                a.active = true;
                a.listener = out.flags & mw2::CV_MOVE_THIRD_PERSON == 0;
                a.position = out.eye;
                a.velocity = out.velocity;
                if let Some(frame) = arms.last.as_ref()
                    && let Some(gun) = arms.arsenal.gun(frame.weapon)
                {
                    // Notetracks belong to the viewmodel shown (the grenade's during a throw).
                    let shown = frame.offhand_view.and_then(|id| arms.arsenal.any(id)).unwrap_or(gun);
                    a.gun_frame(gun, frame, &[], &**rt.log);
                    a.notes(shown, &notes);
                    if let Some(g) = frame.thrown.and_then(|(id, _)| arms.arsenal.any(id)) {
                        a.play_field(g, "fire_sound_player");
                    }
                }
                // The player's movement sounds (MW2's footsteps, landings, gear rattle).
                for f in foley.iter().filter(|_| inner.movement_sounds) {
                    let surface = codv_mw2data::MOVEMENT_SURFACES[inner.surface];
                    let gain = foley_gain(*f);
                    for (alias, fallback) in foley_aliases(*f, surface).into_iter().zip(foley_aliases(*f, "concrete")) {
                        // A surface the player's MW2 has no step for (metal, sand, snow live in map zones): concrete.
                        if a.play_foley(&alias, gain) == 0 && alias != fallback {
                            a.play_foley(&fallback, gain);
                        }
                    }
                    // The gear rattles along with a run's steps (every third) and a sprint's (every other).
                    if let codv_engine::player::Foley::Step(kind) = f {
                        inner.steps = inner.steps.wrapping_add(1);
                        let rattle = match kind {
                            codv_engine::player::StepKind::Sprint if inner.steps % 2 == 0 => "gear_rattle_plr_sprint",
                            codv_engine::player::StepKind::Run if inner.steps % 3 == 0 => "gear_rattle_plr_run",
                            _ => "",
                        };
                        if !rattle.is_empty() {
                            a.play_foley(rattle, 0.5);
                        }
                    }
                }
                // _damagefeedback.gsc: MP_hit_alert with the hit marker.
                if (arms.hits, arms.kills) != inner.hit_alert_seen {
                    inner.hit_alert_seen = (arms.hits, arms.kills);
                    a.play(HIT_ALERT_ALIAS);
                }
            }
        }
        // F4b: MW2's character animation for this step (after the weapon, as
        // iw4L's sim fires its weapon events).
        if let Some(character) = inner.body.as_mut() {
            let frame = inner.arms.as_ref().and_then(|a| a.last.as_ref());
            let gun =
                frame.and_then(|f| inner.arms.as_ref()?.arsenal.gun(f.weapon)).map(|g| codv_engine::body::GunAnim {
                    player_anim_type: g.facts.player_anim_type,
                    weap_class: g.facts.weap_class,
                });
            character.tick(&mut st.player, gun, frame, input.msec as f32 / 1000.0, &**rt.log);
        }
        1
    })
}

// ---- first-person view model (ABI 4) ------------------------------------------

pub const CV_VM_VISIBLE: u32 = 1;
/// The scope overlay covers the view: keep the lens (ads, zoom), draw no view model.
pub const CV_VM_SCOPED: u32 = 2;

/// This frame's view model: which mesh, and `bone_count` bone matrices
/// written to the caller's buffer.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CvVmFrame {
    pub size: u32,
    pub flags: u32,
    pub mesh_id: u32,
    pub bone_count: u32,
    /// ADS fraction 0..1 and the weapon's ADS zoom FOV (MW2 horizontal degrees at 4:3).
    pub ads: f32,
    pub ads_zoom_fov: f32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CvVmMeshInfo {
    pub size: u32,
    pub mesh_id: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub draw_count: u32,
    pub reserved: [u32; 3],
}

/// A view model vertex: bind position and normal (inches, the model's own
/// space), tangent with the binormal's sign in w, UV, four bone influences.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CvVmVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub bones: [u8; 4],
    pub weights: [f32; 4],
}

pub const CV_VM_DRAW_ALPHA_TEST: u32 = 1;
pub const CV_VM_DRAW_BLEND: u32 = 2;
pub const CV_VM_DRAW_TWO_SIDED: u32 = 4;

/// One surface: an index range and its material's images (file stems under
/// `<data>/mw2/viewmodels/images/`, `.dds`; empty: none).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CvVmDraw {
    pub first_index: u32,
    pub index_count: u32,
    pub flags: u32,
    pub reserved: u32,
    pub color_map: [u8; 64],
    pub normal_map: [u8; 64],
}

impl Default for CvVmDraw {
    fn default() -> Self {
        CvVmDraw { first_index: 0, index_count: 0, flags: 0, reserved: 0, color_map: [0; 64], normal_map: [0; 64] }
    }
}

/// The view model of the last `cv_mw2_step`. Writes up to `capacity` bone
/// matrices (12 floats each: rows x, y, z of a 3x4 matrix from bind model
/// space to view space; D3D axes x right, y up, z forward; inches). Returns 1
/// with `out.flags & CV_VM_VISIBLE` when there is one to draw, else 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_vm_frame(rt: *const c_void, out: *mut CvVmFrame, bones: *mut f32, capacity: u32) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if (out.size as usize) < size_of::<CvVmFrame>() {
            return 0;
        }
        let size = out.size;
        *out = CvVmFrame { size, ..Default::default() };
        let Ok(inner) = rt.inner.lock() else { return 0 };
        if inner.mw2.is_none() {
            return 0;
        }
        let Some((mesh_id, list, ads, fov)) = inner.vm.as_ref().and_then(|v| v.frame()) else { return 0 };
        if bones.is_null() || (capacity as usize) < list.len() {
            return 0;
        }
        let dst = unsafe { std::slice::from_raw_parts_mut(bones, list.len() * 12) };
        for (d, m) in dst.chunks_exact_mut(12).zip(list) {
            d.copy_from_slice(m);
        }
        *out = CvVmFrame {
            size,
            flags: CV_VM_VISIBLE | if inner.scoped { CV_VM_SCOPED } else { 0 },
            mesh_id,
            bone_count: list.len() as u32,
            ads,
            ads_zoom_fov: fov,
            reserved: [0; 2],
        };
        1
    })
}

/// Counts of a view model mesh (`mesh_id` from `cv_vm_frame`; the last few
/// stay available). Returns 1, or 0 for an unknown id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_vm_mesh_info(rt: *const c_void, mesh_id: u32, out: *mut CvVmMeshInfo) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if (out.size as usize) < size_of::<CvVmMeshInfo>() {
            return 0;
        }
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(mesh) = inner.vm.as_ref().and_then(|v| v.mesh(mesh_id)) else { return 0 };
        *out = CvVmMeshInfo {
            size: out.size,
            mesh_id,
            vertex_count: mesh.vertices.len() as u32,
            index_count: mesh.indices.len() as u32,
            draw_count: mesh.draws.len() as u32,
            reserved: [0; 3],
        };
        1
    })
}

// ---- third-person character (ABI 5) -------------------------------------------

/// The GTA ped's skeleton for the retarget: JSON (bones with name, parent,
/// local t / r / s, as host probe::SkeletonJson writes) and the ground's
/// height in the ped's object space. Returns the bone count, 0 when refused.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_body_set_skeleton(rt: *mut c_void, json: *const c_char, feet_z: f32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if json.is_null() || !feet_z.is_finite() {
            return 0;
        }
        let text = unsafe { CStr::from_ptr(json) }.to_string_lossy().into_owned();
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        let Some(character) = inner.body.as_mut() else { return 0 };
        match character.set_skeleton(&text, feet_z) {
            Ok(n) => {
                (rt.log)(&format!("mw2: character retargeted onto the GTA ped ({n} bones, ground {feet_z:.2} m)"));
                n as u32
            }
            Err(e) => {
                (rt.log)(&format!("mw2: GTA ped skeleton refused: {e}"));
                0
            }
        }
    })
}

/// The GTA ped's bone object matrices for the last `cv_mw2_step` (16 floats
/// per bone: X axis, Y axis, Z axis, translation; the ped's object space,
/// metres) into `out` (capacity in bones). Returns the bone count, 0 when
/// there is no pose (no skeleton set, no character animation imported).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_body_pose(rt: *const c_void, out: *mut f32, capacity: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(pose) = inner.body.as_ref().map(|c| c.pose()) else { return 0 };
        if out.is_null() || pose.is_empty() || (capacity as usize) < pose.len() {
            return 0;
        }
        let dst = unsafe { std::slice::from_raw_parts_mut(out, pose.len() * 16) };
        for (d, m) in dst.chunks_exact_mut(16).zip(pose) {
            d.copy_from_slice(m);
        }
        pose.len() as u32
    })
}

/// Copies a view model mesh into buffers sized from `cv_vm_mesh_info`.
/// Returns 1, or 0 when the id is unknown or a buffer is too small.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn cv_vm_mesh_data(
    rt: *const c_void,
    mesh_id: u32,
    vertices: *mut CvVmVertex,
    vertex_capacity: u32,
    indices: *mut u32,
    index_capacity: u32,
    draws: *mut CvVmDraw,
    draw_capacity: u32,
) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(mesh) = inner.vm.as_ref().and_then(|v| v.mesh(mesh_id)) else { return 0 };
        if vertices.is_null()
            || indices.is_null()
            || draws.is_null()
            || (vertex_capacity as usize) < mesh.vertices.len()
            || (index_capacity as usize) < mesh.indices.len()
            || (draw_capacity as usize) < mesh.draws.len()
        {
            return 0;
        }
        let vs = unsafe { std::slice::from_raw_parts_mut(vertices, mesh.vertices.len()) };
        for (d, v) in vs.iter_mut().zip(&mesh.vertices) {
            *d = CvVmVertex {
                position: v.position,
                normal: v.normal,
                tangent: v.tangent,
                uv: v.uv,
                bones: v.bones,
                weights: v.weights,
            };
        }
        unsafe { std::slice::from_raw_parts_mut(indices, mesh.indices.len()) }.copy_from_slice(&mesh.indices);
        let ds = unsafe { std::slice::from_raw_parts_mut(draws, mesh.draws.len()) };
        for (d, s) in ds.iter_mut().zip(&mesh.draws) {
            let m = &s.material;
            *d = CvVmDraw {
                first_index: s.first_index,
                index_count: s.index_count,
                flags: (if m.alpha_test() != 0 { CV_VM_DRAW_ALPHA_TEST } else { 0 })
                    | (if m.blended() { CV_VM_DRAW_BLEND } else { 0 })
                    | (if m.cull() != 2 { CV_VM_DRAW_TWO_SIDED } else { 0 }),
                ..Default::default()
            };
            copy_str(&mut d.color_map, &codv_mw2data::vm::file_stem(&m.color_map.to_ascii_lowercase()));
            if !m.normal_map.starts_with('$') {
                copy_str(&mut d.normal_map, &codv_mw2data::vm::file_stem(&m.normal_map.to_ascii_lowercase()));
            }
        }
        1
    })
}

/// This frame's vehicles, props and peds near the player (`count` entries;
/// the nearest first, at most 96 are used). Returns how many became bodies.
/// Send it every frame the MW2 player moves: a list older than half a
/// second counts as empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_set_movers(rt: *mut c_void, list: *const CvMover, count: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let list = if list.is_null() || count == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(list, count as usize) }
        };
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        inner.movers.update(list, &rt.collision);
        inner.movers.bodies().len() as u32
    })
}

/// Where GTA's physics level is (the host's reader found it; `table` 0: lost),
/// and GTA's main image, which every live instance's vtable points into.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_set_physics_level(rt: *mut c_void, table: usize, image_lo: usize, image_hi: usize) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let level = (table != 0 && image_hi > image_lo).then_some(Level { base: table, image: (image_lo, image_hi) });
        rt.collision.set_level(level);
        (rt.log)(&match level {
            Some(_) => format!("collision: physics level at {table:#x} (image {image_lo:#x}..{image_hi:#x})"),
            None => "collision: physics level withdrawn".into(),
        });
        1
    })
}

/// One box sweep against the MW2 meshes of GTA's map, GTA metres. Returns 1
/// with `result` filled, 0 when no built area covers `query.start` yet (it
/// also asks for one there).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_world_trace(
    rt: *mut c_void,
    query: *const CvTraceQuery,
    result: *mut CvTraceResult,
) -> u32 {
    guard(0, || {
        let (Some(rt), Some(q), Some(r)) =
            (unsafe { runtime(rt) }, unsafe { query.as_ref() }, unsafe { result.as_mut() })
        else {
            return 0;
        };
        if !q.start.iter().chain(&q.end).chain(&q.mins).chain(&q.maxs).all(|c| c.is_finite()) {
            return 0;
        }
        let from = [q.start[0] as f64, q.start[1] as f64];
        rt.collision.focus(from);
        let Some(area) = rt.collision.area().filter(|a| a.covers(from)) else { return 0 };
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let map = collision::AreaView::new(&area, from);
        let movers = movers::MoverView { bodies: inner.movers.bodies(), frame: from };
        let k = 1.0 / METRES_PER_UNIT;
        let units = |p: [f32; 3]| [(p[0] - q.start[0]) * k, (p[1] - q.start[1]) * k, p[2] * k];
        let (s, e, lo, hi) = (units(q.start), units(q.end), to_units3(q.mins), to_units3(q.maxs));
        let t_map = map.trace(s, e, lo, hi, q.mask);
        let (t_mov, entity) = movers.trace_entity(s, e, lo, hi, q.mask);
        let (t, entity) = if t_mov.fraction < t_map.fraction || (t_mov.startsolid != 0 && t_map.startsolid == 0) {
            (t_mov, entity)
        } else {
            (t_map, 0)
        };
        *r = CvTraceResult {
            fraction: t.fraction,
            normal: t.normal,
            start_solid: t.startsolid as u32,
            all_solid: t.allsolid as u32,
            entity,
            surface: t.surface_flags,
        };
        1
    })
}

fn to_units3(p: [f32; 3]) -> [f32; 3] {
    p.map(|c| c / METRES_PER_UNIT)
}

/// Writes a NUL-terminated status line; returns its length without the NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_status_text(rt: *const c_void, buf: *mut c_char, cap: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else {
            return 0;
        };
        if buf.is_null() || cap == 0 {
            return 0;
        }
        let (gun, movers, character) = rt
            .inner
            .lock()
            .map(|s| {
                let character = s.body.as_ref().map(|c| {
                    let (legs, torso) = c.playing();
                    format!(" | body: {legs}{}{torso}", if torso.is_empty() { "" } else { " + " })
                });
                let unstuck = s.mw2.as_ref().map_or(0, |m| m.player.unstuck);
                let character = character.unwrap_or_default()
                    + &if unstuck > 0 {
                        format!(" | freed from geometry {unstuck}x, last {}", s.freed_why)
                    } else {
                        String::new()
                    };
                (s.status.clone(), s.movers.summary(), character)
            })
            .unwrap_or_default();
        let sound = rt
            .audio
            .lock()
            .ok()
            .and_then(|a| a.as_ref().map(|a| format!(" | sound: {} voices", a.mixer.voices())))
            .unwrap_or_default();
        let s = format!("{gun} | world: {}; {movers}{character}{sound}", rt.collision.status());
        let n = s.len().min(cap as usize - 1);
        unsafe {
            ptr::copy_nonoverlapping(s.as_ptr(), buf as *mut u8, n);
            *buf.add(n) = 0;
        }
        n as u32
    })
}

// ---- MW2 weapon sounds (ABI 6) --------------------------------------------------
// Contract of SK8V's audio block (skatev_runtime.h, `sv_audio_*`), so the
// host's GTA audio bridge is SK8V's: one emitter (the player's weapon), MONO
// float32 at 48,000 Hz, rendered when the host pulls (`cv_audio_pull` from
// one feeder thread). See host/include/codv_runtime.h.

pub const CV_AUDIO_SAMPLE_RATE: u32 = audio::SAMPLE_RATE;
pub const CV_AUDIO_EMITTER_WEAPON: u32 = 0;
pub const CV_AUDIO_EMITTER_COUNT: u32 = 1;
pub const CV_AUDIO_EMITTER_ACTIVE: u32 = 1 << 0;
pub const CV_AUDIO_EMITTER_SOUNDING: u32 = 1 << 1;
pub const CV_AUDIO_EMITTER_LISTENER: u32 = 1 << 2;
pub const CV_AUDIO_OFF: u32 = 0;
pub const CV_AUDIO_READY: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvAudioConfig {
    pub size: u32,
    /// 0 or 48000.
    pub sample_rate: u32,
    /// Linear gain on every voice; <= 0 means 1.
    pub master_gain: f32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvAudioEmitter {
    pub size: u32,
    pub id: u32,
    pub flags: u32,
    pub channels: u32,
    pub sample_rate: u32,
    /// Always 0: the runtime renders on demand.
    pub queued_frames: u32,
    /// GTA world metres (the player's eye) and m/s.
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    /// Linear volume to apply (the PCM already carries the master gain: 1).
    pub gain: f32,
    pub voices: u32,
    /// Frames rendered since the runtime started.
    pub frame: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CvAudioStatus {
    pub size: u32,
    pub state: u32,
    pub voices: u32,
    pub underruns: u32,
    pub frames: u64,
    pub message: [u8; 96],
}

impl Default for CvAudioStatus {
    fn default() -> Self {
        CvAudioStatus { size: 0, state: 0, voices: 0, underruns: 0, frames: 0, message: [0; 96] }
    }
}

fn emitter_info(a: &audio::Audio) -> CvAudioEmitter {
    let sounding = a.mixer.voices() > 0;
    CvAudioEmitter {
        size: size_of::<CvAudioEmitter>() as u32,
        id: CV_AUDIO_EMITTER_WEAPON,
        flags: (if a.active { CV_AUDIO_EMITTER_ACTIVE } else { 0 })
            | (if sounding { CV_AUDIO_EMITTER_SOUNDING } else { 0 })
            | (if a.listener { CV_AUDIO_EMITTER_LISTENER } else { 0 }),
        channels: 1,
        sample_rate: audio::SAMPLE_RATE,
        queued_frames: 0,
        position: a.position,
        velocity: a.velocity,
        gain: 1.0,
        voices: a.mixer.voices() as u32,
        frame: a.mixer.frames,
    }
}

/// Returns 1 when MW2 weapon sounds are available (the setup imported them).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_audio_configure(rt: *mut c_void, config: *const CvAudioConfig) -> u32 {
    guard(0, || {
        let (Some(rt), Some(c)) = (unsafe { runtime(rt) }, unsafe { config.as_ref() }) else { return 0 };
        if (c.size as usize) < size_of::<CvAudioConfig>()
            || !(c.sample_rate == 0 || c.sample_rate == audio::SAMPLE_RATE)
        {
            return 0;
        }
        let Ok(mut a) = rt.audio.lock() else { return 0 };
        let Some(a) = a.as_mut() else { return 0 };
        a.mixer.gain = if c.master_gain > 0.0 && c.master_gain.is_finite() { c.master_gain.min(8.0) } else { 1.0 };
        (rt.log)(&format!("audio: MW2 weapon sounds on, master gain {:.2}", a.mixer.gain));
        1
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_audio_set_paused(rt: *mut c_void, paused: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let Ok(mut a) = rt.audio.lock() else { return 0 };
        let Some(a) = a.as_mut() else { return 0 };
        a.paused = paused != 0;
        1
    })
}

/// The emitter count (CV_AUDIO_EMITTER_COUNT, 0 without sounds) and up to
/// `capacity` emitters into `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_audio_emitters(rt: *const c_void, out: *mut CvAudioEmitter, capacity: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let Ok(a) = rt.audio.lock() else { return 0 };
        let Some(a) = a.as_ref() else { return 0 };
        if !out.is_null() && capacity > 0 {
            unsafe { *out = emitter_info(a) };
        }
        CV_AUDIO_EMITTER_COUNT
    })
}

/// Renders `frames` mono frames of `emitter` into `out`; returns how many
/// (all of them, silence included; 0 on bad arguments). `info` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_audio_pull(
    rt: *mut c_void,
    emitter: u32,
    out: *mut f32,
    frames: u32,
    info: *mut CvAudioEmitter,
) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if out.is_null() || emitter != CV_AUDIO_EMITTER_WEAPON {
            return 0;
        }
        let buf = unsafe { std::slice::from_raw_parts_mut(out, frames as usize) };
        let Ok(mut a) = rt.audio.lock() else {
            buf.fill(0.0);
            return 0;
        };
        let Some(a) = a.as_mut() else {
            buf.fill(0.0);
            return 0;
        };
        a.pull(buf);
        if let Some(info) = unsafe { info.as_mut() } {
            *info = emitter_info(a);
        }
        frames
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_audio_get_status(rt: *const c_void, out: *mut CvAudioStatus) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        let Ok(a) = rt.audio.lock() else { return 0 };
        let mut st = CvAudioStatus { size: size_of::<CvAudioStatus>() as u32, ..Default::default() };
        let msg = match a.as_ref() {
            Some(a) => {
                st.state = CV_AUDIO_READY;
                st.voices = a.mixer.voices() as u32;
                st.frames = a.mixer.frames;
                format!("{} aliases, {} sounds read", a.aliases.len(), a.files.loaded())
            }
            None => "no imported MW2 weapon sounds".to_owned(),
        };
        copy_str(&mut st.message, &msg);
        *out = st;
        1
    })
}

// ---- MW2 menu guns and grenades (ABI 7) ---------------------------------------

pub const CV_WEAPON_REQUEST_KNIFE: u32 = arms::REQUEST_KNIFE;
pub const CV_WEAPON_REQUEST_GRENADE: u32 = arms::REQUEST_GRENADE;
pub const CV_GRENADE_FLYING: u32 = 1;
pub const CV_GRENADE_EXPLODED: u32 = 2;

/// A thrown grenade: in flight (move the host's model there) or gone off
/// (once: the host makes the explosion; GTA deals its damage).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CvGrenade {
    pub size: u32,
    pub id: u32,
    pub state: u32,
    pub weapon: u32,
    /// GTA world metres.
    pub position: [f32; 3],
    /// Degrees (pitch, roll, yaw) for the model.
    pub rotation: [f32; 3],
    /// Explosion radius, metres.
    pub radius: f32,
    pub reserved: u32,
}

fn grenade_report(frame: [f64; 2], e: &codv_engine::grenade::Explosion, state: u32) -> CvGrenade {
    let m = METRES_PER_UNIT as f64;
    CvGrenade {
        size: size_of::<CvGrenade>() as u32,
        id: e.id,
        state,
        weapon: e.weapon,
        position: [
            (frame[0] + e.origin[0] as f64 * m) as f32,
            (frame[1] + e.origin[1] as f64 * m) as f32,
            (e.origin[2] as f64 * m) as f32,
        ],
        rotation: [0.0; 3],
        radius: e.radius * METRES_PER_UNIT,
        reserved: 0,
    }
}

/// How many guns the menu offers (MW2's base guns the player's MW2 has; 0
/// without the MW2 import).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_gun_count(rt: *const c_void) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        rt.inner.lock().ok().and_then(|i| i.arms.as_ref().map(|a| a.menu.len() as u32)).unwrap_or(0)
    })
}

/// The menu's gun `index`: MW2 name, label, magazine and spare rounds (with
/// the ammo scale), the GTA weapon it borrows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_gun_info(rt: *const c_void, index: u32, out: *mut CvWeaponInfo) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if (out.size as usize) < size_of::<CvWeaponInfo>() {
            return 0;
        }
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(arms) = inner.arms.as_ref() else { return 0 };
        let Some(&id) = arms.menu.get(index as usize) else { return 0 };
        let Some(g) = arms.arsenal.gun(id) else { return 0 };
        let total = g.facts.start_ammo.max(0);
        let mag = g.facts.clip_size.max(0);
        out.gta_hash = arms.gta_hash(id);
        out.mag = mag as u32;
        out.reserve = (((total - mag.min(total)) as f32) * arms.ammo_scale).round() as u32;
        copy_str(&mut out.name, &g.name);
        copy_str(&mut out.label, &arms::label(&g.display_name, &g.name));
        1
    })
}

/// The grenades in flight and those that went off since the last call
/// (each explosion reported once). Returns how many were written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_grenades(rt: *mut c_void, out: *mut CvGrenade, capacity: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if out.is_null() || capacity == 0 {
            return 0;
        }
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let frame = inner.mw2.as_ref().map_or([0.0; 2], |st| st.frame);
        let out = unsafe { std::slice::from_raw_parts_mut(out, capacity as usize) };
        let mut n = 0;
        let explosions = std::mem::take(&mut inner.explosions);
        for e in explosions.into_iter().chain(inner.grenades.iter().map(|g| {
            let mut r = grenade_report(frame, &g.explosion(), CV_GRENADE_FLYING);
            r.rotation = g.angles;
            r
        })) {
            if n == out.len() {
                break;
            }
            out[n] = e;
            n += 1;
        }
        n as u32
    })
}

// ---- MW2 crosshair and scope (ABI 7) ---------------------------------------------

pub const CV_HUD_CROSSHAIR: u32 = 1;
pub const CV_HUD_OVERLAY: u32 = 2;
pub const CV_HUD_OVERLAY_FOUR_QUADS: u32 = 4;

/// MW2's hip crosshair and scope overlay for this frame. Sizes in MW2's
/// virtual screen units (480 = the screen's height), from the centre.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CvMw2Hud {
    pub size: u32,
    pub flags: u32,
    pub crosshair_alpha: f32,
    /// Side images' offsets (x, y) from the centre.
    pub gap: [f32; 2],
    pub side_size: f32,
    pub center_size: f32,
    pub overlay_alpha: f32,
    /// One overlay quad's size (four mirrored quads around the centre with CV_HUD_OVERLAY_FOUR_QUADS).
    pub overlay_size: [f32; 2],
    /// Image file stems under <data>/mw2/viewmodels/images/ (.dds); "" none.
    pub center_image: [u8; 48],
    pub side_image: [u8; 48],
    pub overlay_image: [u8; 48],
}

impl Default for CvMw2Hud {
    fn default() -> Self {
        CvMw2Hud {
            size: 0,
            flags: 0,
            crosshair_alpha: 0.0,
            gap: [0.0; 2],
            side_size: 0.0,
            center_size: 0.0,
            overlay_alpha: 0.0,
            overlay_size: [0.0; 2],
            center_image: [0; 48],
            side_image: [0; 48],
            overlay_image: [0; 48],
        }
    }
}

/// The crosshair and scope for the camera's vertical FOV `fov_y` (degrees);
/// `third_person` hides the crosshair. Returns 1 with `out` filled (flags 0:
/// nothing to draw), 0 without the MW2 player or weapons.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_hud(rt: *mut c_void, fov_y: f32, third_person: u32, out: *mut CvMw2Hud) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if (out.size as usize) < size_of::<CvMw2Hud>() {
            return 0;
        }
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let (Some(st), Some(arms)) = (inner.mw2.as_ref(), inner.arms.as_ref()) else { return 0 };
        let f = inner.hud.frame(&st.player, arms.row(arms.shown), fov_y, third_person != 0);
        let mut h = CvMw2Hud { size: size_of::<CvMw2Hud>() as u32, ..Default::default() };
        if f.crosshair {
            h.flags |= CV_HUD_CROSSHAIR;
            h.crosshair_alpha = f.alpha;
            h.gap = f.gap;
            h.side_size = f.side_size;
            h.center_size = f.center_size;
        }
        if f.overlay_alpha > 0.0 {
            h.flags |= CV_HUD_OVERLAY | if f.overlay_four_quads { CV_HUD_OVERLAY_FOUR_QUADS } else { 0 };
            h.overlay_alpha = f.overlay_alpha;
            h.overlay_size = f.overlay_size;
        }
        let stem = |s: &str| codv_mw2data::vm::file_stem(&s.to_ascii_lowercase());
        copy_str(&mut h.center_image, &stem(&f.center_image));
        copy_str(&mut h.side_image, &stem(&f.side_image));
        copy_str(&mut h.overlay_image, &stem(&f.overlay_image));
        *out = h;
        1
    })
}

// ---- MW2's HUD (ABI 7, F5b) -------------------------------------------------------

/// One textured quad of MW2's HUD, backbuffer pixels: corners top-left,
/// top-right, bottom-right, bottom-left; `image` names a DDS file under
/// <data>/mw2/viewmodels/images/ through `cv_mw2_hud_image`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvHudQuad {
    pub image: u32,
    pub flags: u32,
    pub xy: [f32; 8],
    pub st: [f32; 8],
    pub color: [f32; 4],
}

pub const CV_HUD_THIRD_PERSON: u32 = 1;
/// ABI 10: GTA has the player in a vehicle: MW2's HUD without the weapon bar, sprint meter and compass.
pub const CV_HUD_VEHICLE: u32 = 2;
/// A host cinematic has the view (a Juggernaut's entrance): only the splashes are drawn.
pub const CV_HUD_CINEMATIC: u32 = 4;
/// The hit marker's sound alias (_damagefeedback.gsc playlocalsound "MP_hit_alert").
pub const HIT_ALERT_ALIAS: &str = "mp_hit_alert";
/// CvHudQuad flags: the quad's alpha is masked by the image in bits 8.. (its
/// rectangle's unrotated corners), as MW2's compass letters fade.
pub const CV_HUD_QUAD_MASKED: u32 = 1;
/// ABI 9: a plain fill (no image: the host uses white) / added to the frame instead of blended.
pub const CV_HUD_QUAD_SOLID: u32 = 2;
pub const CV_HUD_QUAD_ADDITIVE: u32 = 4;
/// ABI 10: MW2's splatter_alt (the blood overlay): the image's alpha spread by the mask image
/// (bits 8..) against the quad's alpha (the intensity); low mask values show first.
pub const CV_HUD_QUAD_SPLATTER: u32 = 8;
pub const CV_HUD_MAX_GRENADES: usize = 8;

/// The frame MW2's HUD is drawn for: the backbuffer size, the camera, and
/// GTA's live grenades near the player (world metres).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvHudDrawInput {
    pub size: u32,
    pub flags: u32,
    pub width: f32,
    pub height: f32,
    pub grenade_count: u32,
    /// ABI 10: the launcher's lock-on (CV_HUD_LOCK_*), for the Javelin's CLU.
    pub lock: u32,
    pub grenades: [[f32; 3]; CV_HUD_MAX_GRENADES],
    /// ABI 10: the locked (or locking) target's screen position, backbuffer pixels.
    pub lock_xy: [f32; 2],
}

pub const CV_HUD_LOCK_NONE: u32 = 0;
pub const CV_HUD_LOCK_LOCKING: u32 = 1;
pub const CV_HUD_LOCK_LOCKED: u32 = 2;
/// ABI 10, or-ed into `lock`: the target is too close to lock (or fire) at (the Javelin's CLU warns).
pub const CV_HUD_LOCK_TOO_CLOSE: u32 = 4;

/// MW2's HUD for this frame into `out` (at most `capacity` quads); returns
/// how many. 0 without the import's HUD, the MW2 player or its weapons.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_hud_draw(
    rt: *mut c_void,
    input: *const CvHudDrawInput,
    out: *mut CvHudQuad,
    capacity: u32,
) -> u32 {
    guard(0, || {
        let (Some(rt), Some(input)) = (unsafe { runtime(rt) }, unsafe { input.as_ref() }) else { return 0 };
        if out.is_null() || capacity == 0 || (input.size as usize) < size_of::<CvHudDrawInput>() {
            return 0;
        }
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let (Some(st), Some(arms), Some(h)) = (inner.mw2.as_ref(), inner.arms.as_ref(), inner.mw2hud.as_mut()) else {
            return 0;
        };
        h.hits(arms.hits, arms.kills);
        let frame = &arms.latest;
        let frag_icon = arms
            .arsenal
            .offhand_id(arms::FRAG)
            .and_then(|id| arms.row(id))
            .map_or(String::new(), |r| r.s("hud_icon_image").to_owned());
        let weapon = mw2hud::Weapon { frame: Some(frame), row: arms.row(frame.weapon), frag_icon };
        let javelin = inner.javelin_ads.then_some(mw2hud::Clu {
            too_close: input.lock & CV_HUD_LOCK_TOO_CLOSE != 0,
            lock: match input.lock & 3 {
                CV_HUD_LOCK_LOCKING => mw2hud::Lock::Locking,
                CV_HUD_LOCK_LOCKED => mw2hud::Lock::Locked,
                _ => mw2hud::Lock::None,
            },
            at: input.lock_xy,
            has_rocket: frame.clip > 0,
        });
        let vehicle = input.flags & CV_HUD_VEHICLE != 0;
        let view = mw2hud::View {
            width: input.width,
            height: input.height,
            scoped: inner.scoped && !vehicle,
            javelin: javelin.filter(|_| !vehicle),
            vehicle,
            cinematic: input.flags & CV_HUD_CINEMATIC != 0,
        };
        // Live grenades relative to the eye, IW4 units: CODV's own, and GTA's the host found.
        let ps = &st.player.ps;
        let eye = [ps.origin[0], ps.origin[1], ps.origin[2] + ps.view_height_current];
        let mut near: Vec<[f32; 3]> = inner.grenades.iter().map(|g| [0, 1, 2].map(|i| g.origin[i] - eye[i])).collect();
        let feet = st.position();
        for g in input.grenades.iter().take((input.grenade_count as usize).min(CV_HUD_MAX_GRENADES)) {
            let k = 1.0 / f64::from(METRES_PER_UNIT);
            let rel = [0, 1, 2].map(|i| ((f64::from(g[i]) - feet[i]) * k) as f32);
            near.push([rel[0], rel[1], rel[2] - ps.view_height_current]);
        }
        h.health = inner.health;
        let quads = h.draw(&st.player, &weapon, view, &near);
        let n = quads.len().min(capacity as usize);
        let out = unsafe { std::slice::from_raw_parts_mut(out, n) };
        for (o, q) in out.iter_mut().zip(&quads) {
            *o = CvHudQuad {
                image: q.image,
                flags: q.mask.map_or(0, |m| CV_HUD_QUAD_MASKED | (m << 8)) | q.flags,
                xy: [q.xy[0][0], q.xy[0][1], q.xy[1][0], q.xy[1][1], q.xy[2][0], q.xy[2][1], q.xy[3][0], q.xy[3][1]],
                st: [q.st[0][0], q.st[0][1], q.st[1][0], q.st[1][1], q.st[2][0], q.st[2][1], q.st[3][0], q.st[3][1]],
                color: q.color,
            };
        }
        n as u32
    })
}

/// The image file stem of a HUD quad's `image` into `name` (NUL-terminated);
/// returns its length, 0 when unknown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_hud_image(rt: *const c_void, index: u32, name: *mut c_char, capacity: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if name.is_null() || capacity == 0 {
            return 0;
        }
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(s) = inner.mw2hud.as_ref().and_then(|h| h.image_name(index)) else { return 0 };
        let n = s.len().min(capacity as usize - 1);
        unsafe {
            ptr::copy_nonoverlapping(s.as_ptr(), name as *mut u8, n);
            *name.add(n) = 0;
        }
        n as u32
    })
}

/// ABI 10: the image file stems of the view models the player can hold next
/// (the loadout and the frag: "c:stem" colour maps, "n:stem" normal maps) and of
/// MW2's HUD ("h:stem"), '\n'-separated and NUL-terminated in `out`, so
/// the host can read them before the mode is on. Returns the length written (0:
/// none, or `capacity` too small). Decodes the models it has not yet: call it
/// from a worker thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_vm_ready_images(rt: *const c_void, out: *mut c_char, capacity: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if out.is_null() || capacity == 0 {
            return 0;
        }
        // The decoding happens after the lock is released (the game keeps stepping).
        let (shared, names, hud) = {
            let Ok(inner) = rt.inner.lock() else { return 0 };
            let (Some(vm), Some(arms)) = (inner.vm.as_ref(), inner.arms.as_ref()) else { return 0 };
            let hud = inner.mw2hud.as_ref().map(|h| h.known_images()).unwrap_or_default();
            let icons: Vec<String> = arms
                .ready_set()
                .iter()
                .filter_map(|g| arms.find_row(&g.name))
                .map(|r| r.s("hud_icon_image").to_owned())
                .collect();
            (vm.shared(), arms.ready_set().iter().map(|g| g.name.clone()).collect::<Vec<_>>(), (hud, icons))
        };
        let (hud, icons) = hud;
        let mut images = shared.images(&names);
        images.extend(hud.into_iter().map(|h| format!("h:{h}")));
        // The weapon bar's icons of those guns (shown over the name on a switch).
        for icon in icons {
            let entry = format!("h:{}", codv_mw2data::vm::file_stem(&icon.to_ascii_lowercase()));
            if !icon.is_empty() && !images.contains(&entry) {
                images.push(entry);
            }
        }
        let list = images.join("\n");
        if list.is_empty() || list.len() + 1 > capacity as usize {
            return 0;
        }
        unsafe {
            ptr::copy_nonoverlapping(list.as_ptr(), out as *mut u8, list.len());
            *out.add(list.len()) = 0;
        }
        list.len() as u32
    })
}

/// ABI 10: how the held MW2 weapon locks on (MW2's launchers): CV_LOCK_NONE (the RPG-7, AT4 and
/// Thumper are aimed, every other gun), CV_LOCK_AIR (the Stinger: aircraft only) or CV_LOCK_GROUND
/// (the Javelin: ground vehicles, its missile attacking from above).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_lock_kind(rt: *const c_void) -> u32 {
    guard(CV_LOCK_NONE, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return CV_LOCK_NONE };
        let Ok(inner) = rt.inner.lock() else { return CV_LOCK_NONE };
        let Some(arms) = inner.arms.as_ref() else { return CV_LOCK_NONE };
        arms.arsenal.any(arms.shown).map_or(CV_LOCK_NONE, |g| lock_kind(&g.name))
    })
}

pub const CV_LOCK_NONE: u32 = 0;
pub const CV_LOCK_AIR: u32 = 1;
pub const CV_LOCK_GROUND: u32 = 2;

/// MW2's lock-on launchers by weapon name: the Stinger (air) and the Javelin (ground, top attack).
pub fn lock_kind(name: &str) -> u32 {
    let base = name.to_ascii_lowercase();
    if base.starts_with("stinger") {
        CV_LOCK_AIR
    } else if base.starts_with("javelin") {
        CV_LOCK_GROUND
    } else {
        CV_LOCK_NONE
    }
}

/// ABI 10: the GTA weapons standing in for the loadout's guns (no repeats) into `out`; returns
/// how many (at most `capacity`). The host gives them and keeps their models loaded, so a
/// switch does not wait for GTA to stream the next one in.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_ready_hashes(rt: *const c_void, out: *mut u32, capacity: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if out.is_null() || capacity == 0 {
            return 0;
        }
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(arms) = inner.arms.as_ref() else { return 0 };
        let hashes = arms.ready_gta_hashes();
        let n = hashes.len().min(capacity as usize);
        unsafe { ptr::copy_nonoverlapping(hashes.as_ptr(), out, n) };
        n as u32
    })
}

// ---- killstreaks (ABI 8, F5c) ------------------------------------------------------

/// CvStreakInput flags: killstreaks may act now (MW2 drives the player, no story
/// mission); the player died; action slot 4 was pressed; the mode was switched off.
pub const CV_STREAK_ALLOWED: u32 = 1;
pub const CV_STREAK_DIED: u32 = 2;
pub const CV_STREAK_ACTIVATE: u32 = 4;
pub const CV_STREAK_RESET: u32 = 8;
/// The Tactical Nuke is over, its blast's kills counted: the streak starts again from zero and the
/// rewards still held are cleared (MW2 ends the match on it).
pub const CV_STREAK_NUKE_OVER: u32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvStreakInput {
    pub size: u32,
    pub flags: u32,
    /// Hostile kills since the last call (police, military, anyone fighting the player).
    pub hostile_kills: u32,
    /// ABI 9: a reward handed over (a care package's content, CV_KS_* id; 0 none): on top of the
    /// stack with MW2's pickup splash.
    pub give: u32,
    /// The key that uses a reward, for the HUD ("4"; NUL-terminated).
    pub key_label: [u8; 16],
    /// ABI 10: of `hostile_kills`, those by a head shot and those from afar (MW2's Headshot and
    /// Longshot bonuses on the kill feedback).
    pub headshots: u32,
    pub longshots: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvStreakOutput {
    pub size: u32,
    pub flags: u32,
    /// Kills in a row now, rewards held, the one action slot 4 would use (CV_KS_* id, 0 none).
    pub streak: u32,
    pub held: u32,
    pub top: u32,
    /// The reward called in by this call's CV_STREAK_ACTIVATE (0 none) and the last earned now.
    pub activated: u32,
    pub earned: u32,
    pub reserved: u32,
}

/// One frame of the killstreaks: counts the hostile kills (earning rewards:
/// their splash, jingle and announcer), resets the streak on death or mode
/// off, and calls in the top reward on CV_STREAK_ACTIVATE (the host carries
/// out its effect). Returns 1 with `out` filled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_killstreak_step(
    rt: *mut c_void,
    input: *const CvStreakInput,
    out: *mut CvStreakOutput,
) -> u32 {
    guard(0, || {
        let (Some(rt), Some(input), Some(out)) =
            (unsafe { runtime(rt) }, unsafe { input.as_ref() }, unsafe { out.as_mut() })
        else {
            return 0;
        };
        if (input.size as usize) < size_of::<CvStreakInput>() || (out.size as usize) < size_of::<CvStreakOutput>() {
            return 0;
        }
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let mut o = CvStreakOutput { size: size_of::<CvStreakOutput>() as u32, ..Default::default() };
        if input.flags & (CV_STREAK_DIED | CV_STREAK_RESET) != 0 {
            // The log says how far the streak got (the next reward picked needs that many in one life).
            if inner.streaks.streak > 0 {
                let why = if input.flags & CV_STREAK_DIED != 0 { "death" } else { "mode off" };
                let next = inner.streaks.next_kills().map_or(String::new(), |k| format!(", next reward at {k}"));
                (rt.log)(&format!("killstreak: streak ended at {} kills ({why}){next}", inner.streaks.streak));
            }
            inner.streaks.reset_streak();
            // A death loses the rewards not used yet (unless kept, as MW2 does).
            if input.flags & CV_STREAK_DIED != 0 && !inner.ks_keep_on_death {
                let lost = inner.streaks.drop_held();
                if lost > 0 {
                    (rt.log)(&format!("killstreak: {lost} unused lost with the death"));
                }
            }
        }
        // CODV's kill counter: the session (since the mode came on), this life, the deaths.
        if let Some(h) = inner.mw2hud.as_mut() {
            if input.flags & CV_STREAK_RESET != 0 {
                h.score.reset();
                h.feedback.reset();
            }
            if input.flags & CV_STREAK_DIED != 0 {
                h.score.died();
            }
            h.score.kills(input.hostile_kills.min(64));
            // MW2's kill feedback: "+100" and the bonus splashes.
            h.kill_feedback(input.hostile_kills.min(64), input.headshots, input.longshots);
        }
        let mut sounds: Vec<String> = Vec::new();
        // A reward handed over (a care package's content).
        let given = (input.give != 0 && inner.streaks.give(input.give)).then_some(input.give);
        if let Some(id) = given
            && let Some(name) = killstreaks::name_of(id)
        {
            o.earned = id;
            (rt.log)(&format!("killstreak: {name} from a care package"));
            if let Some(h) = inner.mw2hud.as_mut() {
                h.killstreak_earned(name, 0);
                if let Some((jingle, earn, _)) = h.killstreak_sounds(name) {
                    sounds.extend([jingle, earn]);
                }
            }
        }
        // MW2's Scavenger: each hostile kill tops up the guns and frags.
        if inner.scavenger && input.hostile_kills > 0 && input.flags & CV_STREAK_ALLOWED != 0 {
            let mut topped = false;
            for _ in 0..input.hostile_kills.min(8) {
                topped |= inner.arms.as_mut().is_some_and(|a| a.scavenge());
            }
            if topped {
                if let Some(h) = inner.mw2hud.as_mut() {
                    h.scavenged();
                }
                sounds.push("scavenger_pack_pickup".into());
            }
        }
        // Kills count whenever the host reports them (in a vehicle, in a killstreak's view too);
        // calling a reward in needs CV_STREAK_ALLOWED.
        if inner.streaks.enabled {
            for _ in 0..input.hostile_kills.min(64) {
                for id in inner.streaks.kill() {
                    let Some(name) = killstreaks::name_of(id) else { continue };
                    (rt.log)(&format!("killstreak: {name} earned at {} kills", inner.streaks.streak));
                    o.earned = id;
                    if let Some(h) = inner.mw2hud.as_mut() {
                        h.killstreak_earned(name, inner.streaks.streak);
                        if let Some((jingle, earn, _)) = h.killstreak_sounds(name) {
                            sounds.extend([jingle, earn]);
                        }
                    }
                }
            }
            if input.flags & CV_STREAK_ACTIVATE != 0
                && input.flags & CV_STREAK_ALLOWED != 0
                && let Some(id) = inner.streaks.activate()
            {
                let name = killstreaks::name_of(id).unwrap_or("");
                (rt.log)(&format!("killstreak: {name} called in"));
                o.activated = id;
                if let Some((_, _, used)) = inner.mw2hud.as_ref().and_then(|h| h.killstreak_sounds(name)) {
                    sounds.push(used);
                }
                // _emp.gsc: playLocalSound( "emp_activate" ) for everyone.
                if id == killstreaks::EMP {
                    sounds.push("emp_activate".into());
                }
            }
        }
        // The Tactical Nuke over (the host says so once its blast's kills are in): the run ends, as MW2
        // ends the match on it; the count starts again from zero and the rewards still held are gone.
        if input.flags & CV_STREAK_NUKE_OVER != 0 {
            let (streak, lost) = (inner.streaks.streak, inner.streaks.start_over());
            let round = inner.mw2hud.as_mut().map(|h| {
                h.score.start_over();
                h.score.round()
            });
            let round = round.map_or(String::new(), |r| format!("; round {r}"));
            (rt.log)(&format!(
                "killstreak: the nuke is over: the streak starts again (was {streak} kills, {lost} unused rewards cleared{round})"
            ));
        }
        let next = inner.streaks.next_kills().filter(|_| inner.streaks.enabled);
        if let Some(h) = inner.mw2hud.as_mut() {
            h.score.next = next;
        }
        let top = inner.streaks.top().filter(|_| inner.streaks.enabled);
        if let Some(h) = inner.mw2hud.as_mut() {
            h.held = top.and_then(killstreaks::name_of).map(str::to_owned);
            let label = CStr::from_bytes_until_nul(&input.key_label).ok().and_then(|c| c.to_str().ok()).unwrap_or("");
            if !label.is_empty() {
                h.key_label = label.to_owned();
            }
        }
        o.streak = inner.streaks.streak.max(0) as u32;
        o.held = inner.streaks.held() as u32;
        o.top = top.unwrap_or(0);
        drop(lock);
        if !sounds.is_empty()
            && let Ok(mut a) = rt.audio.lock()
            && let Some(a) = a.as_mut()
        {
            for s in sounds.iter().filter(|s| !s.is_empty()) {
                a.play(s);
            }
        }
        *out = o;
        1
    })
}

// ---- killstreak views, sounds and weapons (ABI 9, F5c stages 2-4) ------------------------

pub const CV_VIEW_NONE: u32 = 0;
pub const CV_VIEW_PREDATOR: u32 = 1;
pub const CV_VIEW_AC130: u32 = 2;
pub const CV_VIEW_CHOPPER: u32 = 3;
pub const CV_VIEW_TARGET_MAP: u32 = 4;
/// CvStreakView flags: the AC-130's gun reloads; the Predator's static after the hit; a nuke is inbound.
pub const CV_VIEW_RELOADING: u32 = 1;
pub const CV_VIEW_STATIC: u32 = 2;
pub const CV_VIEW_NUKE: u32 = 4;
pub const CV_VIEW_HINT_PROGRESS: u32 = 8;
pub const CV_VIEW_MAX_TARGETS: usize = 32;

/// The killstreak view of this frame (the host's camera and its state) for MW2's HUD.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CvStreakView {
    pub size: u32,
    pub kind: u32,
    pub flags: u32,
    /// The AC-130's gun: 0 105 mm, 1 40 mm, 2 25 mm.
    pub weapon: u32,
    pub seconds_left: f32,
    pub nuke_seconds: f32,
    pub hint_progress: f32,
    pub target_count: u32,
    /// A localize key shown at the bottom centre ("" none; NUL-terminated).
    pub hint: [u8; 48],
    /// Hostiles in view: base x, base y, head x, head y (backbuffer pixels).
    pub targets: [[f32; 4]; CV_VIEW_MAX_TARGETS],
    /// ABI 10: the camera's heading (GTA degrees), for the Chopper Gunner's heading tape.
    pub heading: f32,
    pub reserved: u32,
}

impl Default for CvStreakView {
    fn default() -> Self {
        CvStreakView {
            size: size_of::<CvStreakView>() as u32,
            kind: 0,
            flags: 0,
            weapon: 0,
            seconds_left: 0.0,
            nuke_seconds: 0.0,
            hint_progress: 0.0,
            target_count: 0,
            hint: [0; 48],
            targets: [[0.0; 4]; CV_VIEW_MAX_TARGETS],
            heading: 0.0,
            reserved: 0,
        }
    }
}

/// The killstreak view MW2's HUD draws from now on (until the next call). Returns 1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_killstreak_view(rt: *mut c_void, view: *const CvStreakView) -> u32 {
    guard(0, || {
        let (Some(rt), Some(v)) = (unsafe { runtime(rt) }, unsafe { view.as_ref() }) else { return 0 };
        if (v.size as usize) < size_of::<CvStreakView>() {
            return 0;
        }
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        let Some(h) = inner.mw2hud.as_mut() else { return 0 };
        h.streak_view = mw2hud::StreakView {
            kind: match v.kind {
                CV_VIEW_PREDATOR => mw2hud::ViewKind::Predator,
                CV_VIEW_AC130 => mw2hud::ViewKind::Ac130,
                CV_VIEW_CHOPPER => mw2hud::ViewKind::Chopper,
                CV_VIEW_TARGET_MAP => mw2hud::ViewKind::TargetMap,
                _ => mw2hud::ViewKind::None,
            },
            weapon: v.weapon.min(2),
            reloading: v.flags & CV_VIEW_RELOADING != 0,
            static_noise: v.flags & CV_VIEW_STATIC != 0,
            seconds_left: v.seconds_left,
            nuke_seconds: (v.flags & CV_VIEW_NUKE != 0).then_some(v.nuke_seconds),
            hint: CStr::from_bytes_until_nul(&v.hint).ok().and_then(|c| c.to_str().ok()).unwrap_or("").to_owned(),
            hint_progress: (v.flags & CV_VIEW_HINT_PROGRESS != 0).then_some(v.hint_progress),
            targets: v.targets[..(v.target_count as usize).min(CV_VIEW_MAX_TARGETS)].to_vec(),
            heading: v.heading,
        };
        1
    })
}

/// Plays one of the imported MW2 sound aliases (a killstreak's gun, explosion, voice).
/// Returns the sounds started (0: not imported, or MW2's audio is off).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_play_alias(rt: *mut c_void, alias: *const c_char) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if alias.is_null() {
            return 0;
        }
        let name = unsafe { CStr::from_ptr(alias) }.to_string_lossy().to_ascii_lowercase();
        let Ok(mut a) = rt.audio.lock() else { return 0 };
        a.as_mut().map_or(0, |a| a.play(&name) as u32)
    })
}

/// The host's own splash in MW2's killstreak splash (a Juggernaut's entrance): `title` (UTF-8, shown
/// as given) with its glow `glow` (RGBA 0..1; null: MW2's orange) and `description` under it (may be
/// null). Returns 1 when queued, 0 without the import's HUD. An addition to ABI 10 (optional export).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_splash(
    rt: *mut c_void,
    title: *const c_char,
    description: *const c_char,
    glow: *const f32,
) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if title.is_null() {
            return 0;
        }
        let title = unsafe { CStr::from_ptr(title) }.to_string_lossy().into_owned();
        let description = if description.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(description) }.to_string_lossy().into_owned()
        };
        let glow = if glow.is_null() {
            [1.0, 0.55, 0.12, 1.0]
        } else {
            let g = unsafe { std::slice::from_raw_parts(glow, 4) };
            [g[0], g[1], g[2], g[3]].map(|c| if c.is_finite() { c.clamp(0.0, 1.0) } else { 1.0 })
        };
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        lock.mw2hud.as_mut().map_or(0, |h| h.custom_splash(&title, &description, glow) as u32)
    })
}

/// Numbers of an imported MW2 weapon the host needs for the killstreaks' guns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvWeaponNumbers {
    pub size: u32,
    pub fire_ms: u32,
    pub clip: u32,
    pub reload_ms: u32,
    pub damage: f32,
    /// IW4 units.
    pub explosion_radius: f32,
    pub explosion_inner: f32,
    pub explosion_outer: f32,
    /// Units per second.
    pub projectile_speed: f32,
    /// ADS zoom, degrees.
    pub zoom_fov: f32,
}

/// The numbers of MW2 weapon `name` (e.g. "ac130_105mm_mp"); 0 when the import has none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_weapon_numbers(
    rt: *const c_void,
    name: *const c_char,
    out: *mut CvWeaponNumbers,
) -> u32 {
    guard(0, || {
        let (Some(rt), Some(out)) = (unsafe { runtime(rt) }, unsafe { out.as_mut() }) else { return 0 };
        if name.is_null() || (out.size as usize) < size_of::<CvWeaponNumbers>() {
            return 0;
        }
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().to_ascii_lowercase();
        let Ok(inner) = rt.inner.lock() else { return 0 };
        let Some(row) = inner.arms.as_ref().and_then(|a| a.find_row(&name)) else { return 0 };
        *out = CvWeaponNumbers {
            size: size_of::<CvWeaponNumbers>() as u32,
            fire_ms: row.i("fire_time_ms").max(0) as u32,
            clip: row.i("clip_size").max(0) as u32,
            reload_ms: row.i("reload_time_ms").max(0) as u32,
            damage: row.f("damage"),
            explosion_radius: row.f("explosion_radius"),
            explosion_inner: row.f("explosion_inner_damage"),
            explosion_outer: row.f("explosion_outer_damage"),
            projectile_speed: row.f("projectile_speed"),
            zoom_fov: row.f("ads_zoom_fov"),
        };
        1
    })
}

/// Refills the MW2 player's guns and frags (a care package's ammo). Returns 1 when refilled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_refill(rt: *mut c_void) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        let Ok(mut inner) = rt.inner.lock() else { return 0 };
        inner.arms.as_mut().map_or(0, |a| u32::from(a.refill()))
    })
}

pub const CV_HUD_DAMAGE_STUN: u32 = 1;

/// The player took damage from `from` (GTA world metres: the attacker or the
/// explosion): MW2's damage direction arrow. Returns 1 when shown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_mw2_hud_damage(rt: *mut c_void, from: *const f32, flags: u32) -> u32 {
    guard(0, || {
        let Some(rt) = (unsafe { runtime(rt) }) else { return 0 };
        if from.is_null() {
            return 0;
        }
        let from = unsafe { std::slice::from_raw_parts(from, 3) };
        let Ok(mut lock) = rt.inner.lock() else { return 0 };
        let inner = &mut *lock;
        let (Some(st), Some(h)) = (inner.mw2.as_ref(), inner.mw2hud.as_mut()) else { return 0 };
        let p = st.position();
        let (dx, dy) = (p[0] - f64::from(from[0]), p[1] - f64::from(from[1]));
        if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) < 1e-3 {
            return 0;
        }
        // The direction the damage travels (iw4L's damage_yaw): from the attacker to the player.
        h.damaged(dy.atan2(dx).to_degrees() as f32, flags & CV_HUD_DAMAGE_STUN != 0);
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codv_core::player::{FIRE, RELOAD};
    use std::mem::offset_of;

    #[test]
    fn abi_layout_matches_header() {
        assert_eq!(size_of::<CvCreateInfo>(), 32);
        assert_eq!(offset_of!(CvCreateInfo, log_path_utf8), 16);
        assert_eq!(size_of::<CvInput>(), 40);
        assert_eq!(offset_of!(CvInput, gta_weapon), 20);
        assert_eq!(size_of::<CvShot>(), 16);
        assert_eq!(offset_of!(CvHud, label), 40);
        assert_eq!(size_of::<CvHud>(), 72);
        assert_eq!(offset_of!(CvOutput, shots), 16);
        assert_eq!(offset_of!(CvOutput, kick), 400);
        assert_eq!(offset_of!(CvOutput, heal), 428);
        assert_eq!(offset_of!(CvOutput, hud), 432);
        assert_eq!(offset_of!(CvOutput, mag_size), 504);
        assert_eq!(size_of::<CvOutput>(), 512);
        assert_eq!(size_of::<CvHit>(), 16);
        assert_eq!(offset_of!(CvWeaponInfo, name), 16);
        assert_eq!(size_of::<CvWeaponInfo>(), 80);
        assert_eq!(size_of::<CvRules>(), 24);
        assert_eq!(size_of::<CvTraceQuery>(), 56);
        assert_eq!(offset_of!(CvTraceQuery, mask), 48);
        assert_eq!(size_of::<CvTraceResult>(), 32);
        assert_eq!(size_of::<CvMoveSpawn>(), 24);
        assert_eq!(offset_of!(CvMoveInput, third_person_range), 32);
        assert_eq!(size_of::<CvMoveInput>(), 40);
        assert_eq!(offset_of!(CvMoveInput, weapon_request), 28);
        assert_eq!(offset_of!(CvHit, hitloc), 12);
        assert_eq!(offset_of!(CvMoveOutput, position), 16);
        assert_eq!(offset_of!(CvMoveOutput, heading), 52);
        assert_eq!(offset_of!(CvMoveOutput, kick), 64);
        assert_eq!(offset_of!(CvMoveOutput, camera), 72);
        assert_eq!(size_of::<CvMoveOutput>(), 96);
        assert_eq!(offset_of!(CvMover, instance), 16);
        assert_eq!(offset_of!(CvMover, position), 24);
        assert_eq!(offset_of!(CvMover, box_min), 72);
        assert_eq!(size_of::<CvMover>(), 96);
        assert_eq!(size_of::<CvAudioConfig>(), 16);
        assert_eq!(offset_of!(CvAudioEmitter, position), 24);
        assert_eq!(offset_of!(CvAudioEmitter, gain), 48);
        assert_eq!(offset_of!(CvAudioEmitter, frame), 56);
        assert_eq!(size_of::<CvAudioEmitter>(), 64);
        assert_eq!(offset_of!(CvAudioStatus, frames), 16);
        assert_eq!(offset_of!(CvAudioStatus, message), 24);
        assert_eq!(size_of::<CvAudioStatus>(), 120);
        assert_eq!(size_of::<CvGrenade>(), 48);
        assert_eq!(size_of::<CvMw2Hud>(), 184);
        assert_eq!(size_of::<CvHudQuad>(), 88);
        assert_eq!(size_of::<CvHudDrawInput>(), 128);
        assert_eq!(std::mem::offset_of!(CvHudDrawInput, lock_xy), 120);
        assert_eq!(size_of::<CvStreakInput>(), 40);
        assert_eq!((lock_kind("stinger_mp"), lock_kind("javelin_mp")), (CV_LOCK_AIR, CV_LOCK_GROUND));
        assert_eq!((lock_kind("rpg_mp"), lock_kind("at4_mp"), lock_kind("m4_mp")), (0, 0, 0));
        assert_eq!(offset_of!(CvStreakInput, key_label), 16);
        assert_eq!(size_of::<CvStreakOutput>(), 32);
        assert_eq!(offset_of!(CvStreakInput, give), 12);
        assert_eq!(size_of::<CvStreakView>(), 600);
        assert_eq!(offset_of!(CvStreakView, hint), 32);
        assert_eq!(offset_of!(CvStreakView, targets), 80);
        assert_eq!(size_of::<CvWeaponNumbers>(), 40);
        assert_eq!(offset_of!(CvStreakOutput, activated), 20);
        assert_eq!(offset_of!(CvHudDrawInput, grenades), 24);
        assert_eq!(offset_of!(CvMw2Hud, center_image), 40);
        assert_eq!(offset_of!(CvGrenade, position), 16);
        assert_eq!(offset_of!(CvGrenade, radius), 40);
        assert_eq!(size_of::<CvVmFrame>(), 32);
        assert_eq!(size_of::<CvVmMeshInfo>(), 32);
        assert_eq!(size_of::<CvVmVertex>(), 68);
        assert_eq!(offset_of!(CvVmVertex, bones), 48);
        assert_eq!(size_of::<CvVmDraw>(), 144);
    }

    /// A host world: flat ground whose top is at z = 10 m, everywhere.
    unsafe extern "C" fn flat_ground(user: *mut c_void, q: *const CvTraceQuery, r: *mut CvTraceResult) {
        let calls = unsafe { &mut *(user as *mut u32) };
        *calls += 1;
        let (q, r) = unsafe { (&*q, &mut *r) };
        let top = 10.0f32;
        let (s, e) = (q.start[2] + q.mins[2], q.end[2] + q.mins[2]);
        *r = CvTraceResult { fraction: 1.0, ..Default::default() };
        if s < top - 1e-4 {
            *r = CvTraceResult {
                fraction: 0.0,
                normal: [0.0, 0.0, 1.0],
                start_solid: 1,
                all_solid: u32::from(e < top),
                ..Default::default()
            };
        } else if e < top {
            r.fraction = ((s - top) / (s - e)).clamp(0.0, 1.0);
            r.normal = [0.0, 0.0, 1.0];
        }
    }

    #[test]
    fn mw2_player_walks_on_the_host_world() {
        let rt = create();
        let mut calls = 0u32;
        unsafe {
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [100.0, 200.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            let mut input = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut out = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            assert_eq!(cv_mw2_step(rt, &input, &mut out), 0, "nothing spawned");
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_step(rt, &input, &mut out), 0, "no world yet");
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            for _ in 0..30 {
                assert_eq!(cv_mw2_step(rt, &input, &mut out), 1);
            }
            assert!(calls > 0, "pmove asked the host world");
            assert!(out.flags & CV_MOVE_ON_GROUND != 0 && (out.position[2] - 10.0).abs() < 0.02, "{out:?}");
            assert!((out.eye[2] - out.position[2] - 60.0 * 0.0254).abs() < 1e-3, "MW2 eye height 60 units");
            // Heading 0 is +Y in GTA: running forward moves +Y at 190 u/s (4.83 m/s).
            input.forward = 127;
            for _ in 0..90 {
                cv_mw2_step(rt, &input, &mut out);
            }
            assert!(out.velocity[1] > 4.6 && out.velocity[0].abs() < 0.05, "{:?}", out.velocity);
            assert!(out.position[1] > 205.0 && (out.heading - 0.0).abs() < 0.01);
            // Look up 10 degrees (GTA pitch up positive).
            input.forward = 0;
            input.pitch_delta = 10.0;
            cv_mw2_step(rt, &input, &mut out);
            assert!((out.pitch - 10.0).abs() < 0.1, "{}", out.pitch);
            let mut small = CvMoveOutput { size: 8, ..Default::default() };
            assert_eq!(cv_mw2_step(rt, &input, &mut small), 0, "short output refused");
            cv_mw2_despawn(rt);
            assert_eq!(cv_mw2_step(rt, &input, &mut out), 0);
            cv_destroy(rt);
        }
    }

    /// GTA's map (a fake physics level in memory) all the way to pmove: the
    /// streamer reads it, builds MW2 meshes, and the player runs across
    /// several areas and frame re-centrings to a wall.
    #[test]
    fn mw2_player_runs_on_gta_collision_read_from_the_physics_level() {
        use codv_gtaworld::fake::FakeLevel;
        let (x0, y0, z) = (-1200.0f32, 2400.0f32, 30.0f32);
        let mut gta = FakeLevel::new();
        // 400 m x 400 m of street around the start, and a wall 150 m north.
        gta.add_floor([x0 - 200.0, y0 - 200.0], 40, 10.0, z);
        gta.add_block([x0 - 50.0, y0 + 150.0, z], [x0 + 50.0, y0 + 151.0, z + 4.0]);
        let level = gta.level();
        let rt = create();
        unsafe {
            assert_eq!(cv_set_physics_level(rt, level.base, level.image.0, level.image.1), 1);
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [x0, y0, z + 0.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            let mut input = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut out = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            // Waits (holding still) until the area around the player is built.
            let mut waited = 0;
            loop {
                assert_eq!(cv_mw2_step(rt, &input, &mut out), 1);
                if out.flags & CV_MOVE_WAITING == 0 {
                    break;
                }
                assert!(
                    (0..3).all(|k| (out.position[k] - spawn.position[k]).abs() < 1e-4),
                    "holds still while waiting: {out:?}"
                );
                waited += 1;
                assert!(waited < 10_000, "no area built");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            for _ in 0..30 {
                cv_mw2_step(rt, &input, &mut out);
            }
            assert!(out.flags & CV_MOVE_ON_GROUND != 0 && (out.position[2] - z).abs() < 0.01, "{out:?}");
            // The world trace answers from the same meshes.
            let q = CvTraceQuery {
                start: [x0, y0, z + 2.0],
                end: [x0, y0, z - 2.0],
                mins: [-0.3, -0.3, 0.0],
                maxs: [0.3, 0.3, 1.8],
                mask: 1,
                reserved: 0,
            };
            let mut r = CvTraceResult::default();
            assert_eq!(cv_world_trace(rt, &q, &mut r), 1);
            assert!((r.fraction - 0.5).abs() < 0.01 && r.normal[2] > 0.99, "{r:?}");
            // Run north (heading 0 = +Y) to the wall, crossing areas.
            input.forward = 127;
            let (mut lowest, mut highest, mut fastest) = (f32::MAX, f32::MIN, 0.0f32);
            for _ in 0..60_000 {
                cv_mw2_step(rt, &input, &mut out);
                if out.flags & CV_MOVE_WAITING != 0 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
                lowest = lowest.min(out.position[2]);
                highest = highest.max(out.position[2]);
                fastest = fastest.max(out.velocity[1]);
                if out.velocity[1].abs() < 0.01 && out.position[1] > y0 + 140.0 {
                    break;
                }
            }
            assert!(
                (lowest - z).abs() < 0.01 && (highest - z).abs() < 0.01,
                "stayed on the street: {lowest}..{highest}"
            );
            assert!((fastest - 190.0 * 0.0254).abs() < 0.05, "MW2 run speed: {fastest} m/s");
            let wall = y0 + 150.0 - 15.0 * 0.0254;
            assert!(
                (out.position[1] - wall).abs() < 0.03,
                "stops a hull radius short of the wall: {} vs {wall}",
                out.position[1]
            );
            assert!((out.position[0] - x0).abs() < 0.01, "ran straight: {out:?}");
            let mut status = [0 as c_char; 256];
            cv_status_text(rt, status.as_mut_ptr(), 256);
            let status = CStr::from_ptr(status.as_ptr()).to_string_lossy().into_owned();
            assert!(status.contains("MW2 tris around"), "{status}");
            assert_eq!(cv_set_physics_level(rt, 0, 0, 0), 1, "withdrawn: the last area stays usable");
            assert_eq!(cv_mw2_step(rt, &input, &mut out), 1);
            assert_eq!(out.flags & CV_MOVE_WAITING, 0);
            cv_destroy(rt);
        }
    }

    /// A car (GTA's bound read from its instance, turned 90 degrees) and a ped
    /// in the MW2 player's way: it stops at the car's side, stands on its
    /// roof, and stops two hull radii from the ped.
    #[test]
    fn movers_block_the_mw2_player() {
        use codv_gtaworld::fake::FakeLevel;
        let (x0, y0, z) = (300.0f32, -800.0f32, 20.0f32);
        let mut gta = FakeLevel::new();
        gta.add_floor([x0 - 100.0, y0 - 100.0], 20, 10.0, z);
        // The car's bound in model space: 2 m wide (x), 4.4 m long (y), 1.4 m high, origin at its centre bottom.
        let car_verts: Vec<[f32; 3]> =
            (0..8).map(|i| [[-1.0, 1.0][i & 1], [-2.2, 2.2][i >> 1 & 1], [0.0, 1.4][i >> 2]]).collect();
        let quads = [[1u16, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]];
        let car_tris: Vec<[u16; 3]> = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        let car_inst = gta.instance(&car_verts, &car_tris, 1, 0);
        let level = gta.level();
        // Parked 10 m north of the start, turned 90 degrees (its forward axis along -X): 4.4 m across the path.
        let car = CvMover {
            entity: 777,
            model: 0xC0FFEE,
            kind: CV_MOVER_VEHICLE,
            flags: 0,
            instance: car_inst as u64,
            position: [x0, y0 + 10.0, z],
            right: [0.0, 1.0, 0.0],
            forward: [-1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            box_min: [-1.1, -2.3, 0.0],
            box_max: [1.1, 2.3, 1.5],
        };
        let rt = create();
        unsafe {
            cv_set_physics_level(rt, level.base, level.image.0, level.image.1);
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [x0, y0, z + 0.05],
                heading: 0.0,
                flags: 0,
            };
            cv_mw2_spawn(rt, &spawn);
            let mut input = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut out = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            let mut exact = false;
            for _ in 0..10_000 {
                assert_eq!(cv_set_movers(rt, &car, 1), 1);
                cv_mw2_step(rt, &input, &mut out);
                let inner = (*(rt as *const Runtime)).inner.lock().unwrap();
                exact = inner.movers.stats.exact == 1;
                drop(inner);
                if exact && out.flags & CV_MOVE_WAITING == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(exact, "the car's own bound was read from its instance");
            // Run north into the car's side, which is 1 m from its centre: y0 + 9 (CODV's auto
            // vault off: it would take the player over the car).
            cv_set_option(rt, CV_OPT_AUTO_VAULT, 0.0);
            input.forward = 127;
            for _ in 0..400 {
                cv_set_movers(rt, &car, 1);
                cv_mw2_step(rt, &input, &mut out);
            }
            let side = y0 + 10.0 - 1.0 - 15.0 * 0.0254;
            assert!((out.position[1] - side).abs() < 0.03, "stops at the car's side: {} vs {side}", out.position[1]);
            assert!((out.position[2] - z).abs() < 0.01);
            // The world trace names the car; from above it lands on the roof at z + 1.4.
            let q = CvTraceQuery {
                start: [x0, y0 + 10.0, z + 3.0],
                end: [x0, y0 + 10.0, z - 1.0],
                mins: [-0.3, -0.3, 0.0],
                maxs: [0.3, 0.3, 1.8],
                mask: 1,
                reserved: 0,
            };
            let mut r = CvTraceResult::default();
            assert_eq!(cv_world_trace(rt, &q, &mut r), 1);
            let roof = q.start[2] + (q.end[2] - q.start[2]) * r.fraction;
            assert!(r.entity == 777 && (roof - (z + 1.4)).abs() < 0.02, "{r:?} roof {roof}");
            // A ped in the car's place instead: a MW2 hull, so 30 units centre to centre.
            let ped = CvMover {
                entity: 42,
                model: 0xBEEF,
                kind: CV_MOVER_PED,
                position: [x0, y0 + 10.0, z + 1.0],
                right: [1.0, 0.0, 0.0],
                forward: [0.0, 1.0, 0.0],
                up: [0.0, 0.0, 1.0],
                box_min: [-0.6, -0.3, -1.0],
                box_max: [0.6, 0.3, 0.9],
                ..Default::default()
            };
            for _ in 0..400 {
                cv_set_movers(rt, &ped, 1);
                cv_mw2_step(rt, &input, &mut out);
            }
            let hulls = y0 + 10.0 - 30.0 * 0.0254;
            assert!(
                (out.position[1] - hulls).abs() < 0.03,
                "stops a hull from the ped: {} vs {hulls}",
                out.position[1]
            );
            // Nobody listed for a while: the way is clear.
            cv_set_movers(rt, ptr::null(), 0);
            for _ in 0..200 {
                cv_mw2_step(rt, &input, &mut out);
            }
            assert!(out.position[1] > y0 + 15.0, "runs on once the list is empty: {out:?}");
            cv_destroy(rt);
        }
    }

    /// The whole MW2 weapon path through the ABI, from the player's own import
    /// (set CODV_MW2_TABLE to its weapons.json; skipped otherwise).
    #[test]
    fn mw2_weapons_fire_through_the_abi() {
        let Ok(table) = std::env::var("CODV_MW2_TABLE") else { return };
        let root = std::env::temp_dir().join(format!("codv-arms-{}", std::process::id()));
        std::fs::create_dir_all(root.join("mw2")).unwrap();
        std::fs::copy(&table, root.join("mw2").join("weapons.json")).unwrap();
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 3,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mut mv = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            let input =
                CvInput { size: size_of::<CvInput>() as u32, dt_seconds: 0.016, health: 1.0, ..Default::default() };
            let mut out = CvOutput::default();
            // Raise the M4 (first slot of data/mw2.json), then hold the trigger for a second.
            for _ in 0..60 {
                assert_eq!(cv_mw2_step(rt, &mv, &mut mo), 1);
                assert_eq!(cv_step(rt, &input, &mut out), 1);
            }
            assert!(out.flags & CV_OUT_ACTIVE != 0 && out.mag_size == 30, "flags {} mag {}", out.flags, out.mag_size);
            let label = CStr::from_bytes_until_nul(&out.hud.label).unwrap();
            assert!(label.to_string_lossy().contains("M4"), "{label:?}");
            // ABI 10: the loadout's GTA stand-ins, for the host to keep loaded (the M4's carbine first).
            let mut hashes = [0u32; 8];
            let n = cv_mw2_ready_hashes(rt, hashes.as_mut_ptr(), 8) as usize;
            assert!(n >= 2 && hashes[0] == 0x83BF_0278, "{:x?}", &hashes[..n]);
            assert!(hashes[..n].iter().all(|h| *h != 0));
            mv.buttons = 0x1;
            let (mut shots, mut kicked, mut m4) = (0, 0.0f32, 0);
            for _ in 0..62 {
                cv_mw2_step(rt, &mv, &mut mo);
                kicked = kicked.max(mo.kick[0].abs() + mo.kick[1].abs());
                cv_step(rt, &input, &mut out);
                shots += out.shot_count;
                if out.shot_count > 0 {
                    assert_eq!(out.shots[0].gta_hash, 0x83BF_0278, "M4 borrows GTA's carbine rifle");
                    m4 = out.shots[0].weapon;
                }
            }
            assert!((13..=16).contains(&shots), "~14 rounds a second: {shots}");
            assert!(kicked > 0.05, "the view kicks: {kicked}");
            assert_eq!(out.hud.mag, 30 - shots);
            // A body hit at 10 m: MW2's 30, and the head scales it.
            let hits = [
                CvHit { weapon: m4, distance: 10.0, flags: CV_HIT_PED, hitloc: 4 },
                CvHit { weapon: m4, distance: 10.0, flags: CV_HIT_PED, hitloc: 2 },
            ];
            let mut dmg = [0.0f32; 2];
            assert_eq!(cv_report_hits(rt, hits.as_ptr(), 2, dmg.as_mut_ptr()), 2);
            assert!((dmg[0] - 30.0).abs() < 0.01 && (dmg[1] - 42.0).abs() < 0.01, "{dmg:?}");
            // Next weapon: the USP.
            mv.buttons = 0;
            mv.weapon_request = 1;
            cv_mw2_step(rt, &mv, &mut mo);
            mv.weapon_request = 0;
            for _ in 0..120 {
                cv_mw2_step(rt, &mv, &mut mo);
                cv_step(rt, &input, &mut out);
            }
            let label = CStr::from_bytes_until_nul(&out.hud.label).unwrap();
            assert!(label.to_string_lossy().contains("USP"), "{label:?}");
            // Sprint held through a switch back to the M4: sprinting again once it is up, no new press.
            mv.buttons = 0x2; // CV_MW2_SPRINT
            mv.forward = 127;
            for _ in 0..30 {
                cv_mw2_step(rt, &mv, &mut mo);
                cv_step(rt, &input, &mut out);
            }
            assert!(mo.flags & CV_MOVE_SPRINTING != 0, "sprinting with the USP");
            mv.weapon_request = 1;
            cv_mw2_step(rt, &mv, &mut mo);
            mv.weapon_request = 0;
            let mut stopped = false;
            let mut again = false;
            for _ in 0..200 {
                cv_mw2_step(rt, &mv, &mut mo);
                cv_step(rt, &input, &mut out);
                let sprinting = mo.flags & CV_MOVE_SPRINTING != 0;
                stopped |= !sprinting;
                if stopped && sprinting {
                    again = true;
                    break;
                }
            }
            println!("switch while sprinting: stopped {stopped}, sprinting again {again}");
            assert!(!stopped || again, "the held sprint comes back after the switch");
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// The view model through the ABI, from the player's own import
    /// (CODV_MW2_OUT: weapons.json + viewmodels/); skipped otherwise.
    #[cfg(unix)]
    #[test]
    fn mw2_viewmodel_through_the_abi() {
        let Ok(src) = std::env::var("CODV_MW2_OUT") else { return };
        let root = std::env::temp_dir().join(format!("codv-vm-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let _ = std::os::unix::fs::symlink(&src, root.join("mw2"));
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 5,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            // ABI 10: the images of the loadout's view models, for the host to read ahead.
            let mut list = vec![0 as c_char; 16384];
            let n = cv_vm_ready_images(rt, list.as_mut_ptr(), list.len() as u32) as usize;
            assert!(n > 0);
            let list = CStr::from_ptr(list.as_ptr()).to_string_lossy().into_owned();
            assert_eq!(list.len(), n);
            let stems: Vec<&str> = list.split('\n').map(|l| l.split_once(':').map_or(l, |(_, s)| s)).collect();
            assert!(stems.len() > 4, "{stems:?}");
            assert!(list.starts_with("c:") && list.contains("\nn:") && list.contains("\nh:"), "{list}");
            assert!(
                stems
                    .iter()
                    .filter(|s| std::path::Path::new(&src).join("viewmodels/images").join(format!("{s}.dds")).is_file())
                    .count()
                    * 10
                    >= stems.len() * 9,
                "{stems:?}"
            );
            let mut tiny = [0 as c_char; 4];
            assert_eq!(cv_vm_ready_images(rt, tiny.as_mut_ptr(), 4), 0);
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mv = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            let mut frame = CvVmFrame { size: size_of::<CvVmFrame>() as u32, ..Default::default() };
            let mut bones = vec![0.0f32; 256 * 12];
            for _ in 0..30 {
                cv_mw2_step(rt, &mv, &mut mo);
            }
            assert_eq!(cv_vm_frame(rt, &mut frame, bones.as_mut_ptr(), 256), 1);
            assert!(frame.flags & CV_VM_VISIBLE != 0 && frame.bone_count > 60, "{frame:?}");
            assert!(bones[..frame.bone_count as usize * 12].iter().all(|x| x.is_finite()));
            let mut info = CvVmMeshInfo { size: size_of::<CvVmMeshInfo>() as u32, ..Default::default() };
            assert_eq!(cv_vm_mesh_info(rt, frame.mesh_id, &mut info), 1);
            let mut vs = vec![CvVmVertex::default(); info.vertex_count as usize];
            let mut is = vec![0u32; info.index_count as usize];
            let mut ds = vec![CvVmDraw::default(); info.draw_count as usize];
            assert_eq!(
                cv_vm_mesh_data(
                    rt,
                    frame.mesh_id,
                    vs.as_mut_ptr(),
                    info.vertex_count,
                    is.as_mut_ptr(),
                    info.index_count,
                    ds.as_mut_ptr(),
                    info.draw_count
                ),
                1
            );
            assert!(is.iter().all(|i| (*i as usize) < vs.len()));
            assert!(vs.iter().all(|v| (v.bones.iter().max().copied().unwrap_or(0) as u32) < frame.bone_count));
            let first = CStr::from_bytes_until_nul(&ds[0].color_map).unwrap().to_string_lossy().into_owned();
            assert!(
                std::path::Path::new(&src).join("viewmodels/images").join(format!("{first}.dds")).is_file(),
                "{first}"
            );
            // Too small a bone buffer is refused.
            assert_eq!(cv_vm_frame(rt, &mut frame, bones.as_mut_ptr(), 4), 0);
            // A frag throw shows the grenade's viewmodel, then the gun again.
            let gun_mesh = frame.mesh_id;
            let mut meshes = std::collections::BTreeSet::new();
            let mut mv = mv;
            for i in 0..150 {
                mv.buttons = if i < 30 { 0x4000 } else { 0 };
                cv_mw2_step(rt, &mv, &mut mo);
                if cv_vm_frame(rt, &mut frame, bones.as_mut_ptr(), 256) == 1 {
                    assert!(bones[..frame.bone_count as usize * 12].iter().all(|x| x.is_finite()));
                    meshes.insert(frame.mesh_id);
                }
            }
            assert!(meshes.len() >= 2, "the grenade's viewmodel showed: {meshes:?} (gun {gun_mesh})");
            assert_ne!(frame.mesh_id, gun_mesh, "the gun is rebuilt after the throw");
            // Weapon switches around the loadout: once each gun was held, a switch builds its rig
            // from memory and reuses its mesh (no step near a frame's time).
            for round in 0..6 {
                mv.weapon_request = 1;
                let mut worst = 0.0f64;
                let mut fetch = 0.0f64;
                let mut last_mesh = frame.mesh_id;
                for i in 0..120 {
                    let t = std::time::Instant::now();
                    cv_mw2_step(rt, &mv, &mut mo);
                    worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
                    mv.weapon_request = 0;
                    if cv_vm_frame(rt, &mut frame, bones.as_mut_ptr(), 256) == 1 && frame.mesh_id != last_mesh {
                        last_mesh = frame.mesh_id;
                        let t = std::time::Instant::now();
                        let mut info = CvVmMeshInfo { size: size_of::<CvVmMeshInfo>() as u32, ..Default::default() };
                        cv_vm_mesh_info(rt, frame.mesh_id, &mut info);
                        let mut vs = vec![CvVmVertex::default(); info.vertex_count as usize];
                        let mut is = vec![0u32; info.index_count as usize];
                        let mut ds = vec![CvVmDraw::default(); info.draw_count as usize];
                        cv_vm_mesh_data(
                            rt,
                            frame.mesh_id,
                            vs.as_mut_ptr(),
                            info.vertex_count,
                            is.as_mut_ptr(),
                            info.index_count,
                            ds.as_mut_ptr(),
                            info.draw_count,
                        );
                        fetch = t.elapsed().as_secs_f64() * 1000.0;
                        let _ = i;
                    }
                }
                println!(
                    "switch {round}: slowest step {worst:.2} ms, mesh fetch {fetch:.2} ms, mesh {}",
                    frame.mesh_id
                );
                if round >= 4 {
                    assert!(worst < 50.0, "a switch to a gun held before hitches: {worst:.1} ms");
                }
            }
            cv_mw2_despawn(rt);
            assert_eq!(cv_vm_frame(rt, &mut frame, bones.as_mut_ptr(), 256), 0);
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// A GTA-like ped skeleton (SK8V's synthetic rig: metres, left on +x,
    /// toes towards -y, ped origin at the pelvis) as probe::SkeletonJson writes it.
    fn gta_skeleton_json() -> String {
        let rig: [(&str, &str, [f32; 3]); 25] = [
            ("SKEL_ROOT", "", [0., 0., 0.]),
            ("SKEL_Pelvis", "SKEL_ROOT", [0., 0., 0.]),
            ("SKEL_Spine_Root", "SKEL_ROOT", [0., 0., 0.02]),
            ("SKEL_Spine0", "SKEL_Spine_Root", [0., 0., 0.1]),
            ("SKEL_Spine1", "SKEL_Spine0", [0., 0.01, 0.2]),
            ("SKEL_Spine2", "SKEL_Spine1", [0., 0.02, 0.3]),
            ("SKEL_Spine3", "SKEL_Spine2", [0., 0.02, 0.4]),
            ("SKEL_Neck_1", "SKEL_Spine3", [0., 0.03, 0.55]),
            ("SKEL_Head", "SKEL_Neck_1", [0., 0.04, 0.65]),
            ("SKEL_L_Clavicle", "SKEL_Spine3", [0.05, 0.02, 0.5]),
            ("SKEL_L_UpperArm", "SKEL_L_Clavicle", [0.2, 0., 0.5]),
            ("SKEL_L_Forearm", "SKEL_L_UpperArm", [0.25, 0.05, 0.25]),
            ("SKEL_L_Hand", "SKEL_L_Forearm", [0.3, 0.2, 0.1]),
            ("SKEL_R_Clavicle", "SKEL_Spine3", [-0.05, 0.02, 0.5]),
            ("SKEL_R_UpperArm", "SKEL_R_Clavicle", [-0.2, 0., 0.5]),
            ("SKEL_R_Forearm", "SKEL_R_UpperArm", [-0.25, 0.05, 0.25]),
            ("SKEL_R_Hand", "SKEL_R_Forearm", [-0.3, 0.2, 0.1]),
            ("SKEL_L_Thigh", "SKEL_Pelvis", [0.1, 0., -0.05]),
            ("SKEL_L_Calf", "SKEL_L_Thigh", [0.1, -0.05, -0.5]),
            ("SKEL_L_Foot", "SKEL_L_Calf", [0.1, 0., -0.9]),
            ("SKEL_L_Toe0", "SKEL_L_Foot", [0.1, -0.15, -0.98]),
            ("SKEL_R_Thigh", "SKEL_Pelvis", [-0.1, 0., -0.05]),
            ("SKEL_R_Calf", "SKEL_R_Thigh", [-0.1, -0.05, -0.5]),
            ("SKEL_R_Foot", "SKEL_R_Calf", [-0.1, 0., -0.9]),
            ("SKEL_R_Toe0", "SKEL_R_Foot", [-0.1, -0.15, -0.98]),
        ];
        let pos = |n: &str| rig.iter().find(|b| b.0 == n).map_or([0.0; 3], |b| b.2);
        let bones: Vec<String> = rig
            .iter()
            .map(|(name, parent, p)| {
                let q = pos(parent);
                let index = rig.iter().position(|b| b.0 == *parent).map_or(-1, |i| i as i64);
                format!(
                    "{{\"name\":\"{name}\",\"tag\":0,\"parent\":{index},\"t\":[{},{},{}],\"r\":[0,0,0,1],\"s\":[1,1,1]}}",
                    p[0] - q[0],
                    p[1] - q[1],
                    p[2] - q[2]
                )
            })
            .collect();
        format!("{{\"source\":\"live\",\"bones\":[{}]}}", bones.join(","))
    }

    /// F4b through the ABI, from the player's own import (CODV_MW2_OUT: the
    /// importer's output folder; skipped otherwise): the character's pose on
    /// the GTA skeleton, and MW2's third-person camera.
    #[test]
    fn mw2_character_poses_the_gta_skeleton_through_the_abi() {
        let Ok(out) = std::env::var("CODV_MW2_OUT") else { return };
        let root = std::env::temp_dir().join(format!("codv-body-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        // Only the character animation is needed: a copy of `body/` (portable, no symlink).
        let body = root.join("mw2").join("body");
        std::fs::create_dir_all(body.join("anims")).unwrap();
        let from = std::path::Path::new(&out).join("body");
        for name in ["multiplayer.atr", "playeranim.script", "skeleton.cvm"] {
            std::fs::copy(from.join(name), body.join(name)).unwrap();
        }
        for e in std::fs::read_dir(from.join("anims")).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), body.join("anims").join(e.file_name())).unwrap();
        }
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 5,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            let json = std::ffi::CString::new(gta_skeleton_json()).unwrap();
            assert_eq!(cv_body_set_skeleton(rt, json.as_ptr(), -0.98), 25);
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mv = CvMoveInput {
                size: size_of::<CvMoveInput>() as u32,
                msec: 16,
                forward: 127,
                third_person_range: 3.0,
                ..Default::default()
            };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            for _ in 0..60 {
                assert_eq!(cv_mw2_step(rt, &mv, &mut mo), 1);
            }
            let mut pose = vec![0.0f32; 64 * 16];
            assert_eq!(cv_body_pose(rt, pose.as_mut_ptr(), 64), 25);
            assert!(pose[..25 * 16].iter().all(|x| x.is_finite()));
            let z = |i: usize| pose[i * 16 + 14];
            assert!(z(8) - z(1) > 0.4, "head {} above pelvis {}", z(8), z(1));
            let lowest = z(20).min(z(24)).min(z(19)).min(z(23));
            assert!((lowest + 0.98).abs() < 0.15, "feet near the ground: {lowest}");
            // MW2's camera sits behind and above the eye.
            assert!(mo.flags & mw2::CV_MOVE_THIRD_PERSON != 0);
            let back = ((mo.camera[0] - mo.eye[0]).powi(2) + (mo.camera[1] - mo.eye[1]).powi(2)).sqrt();
            assert!((2.5..3.2).contains(&back) && mo.camera[2] >= mo.eye[2] - 0.05, "{mo:?}");
            let mut status = [0 as c_char; 512];
            cv_status_text(rt, status.as_mut_ptr(), 512);
            let status = CStr::from_ptr(status.as_ptr()).to_string_lossy().into_owned();
            assert!(status.contains("body: pb_"), "{status}");
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    fn create() -> *mut c_void {
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: ptr::null(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 11,
        };
        unsafe {
            assert!(cv_create(&CvCreateInfo { abi_version: 99, ..info }).is_null(), "wrong ABI is refused");
            cv_create(&info)
        }
    }

    #[test]
    fn the_nuke_over_counts_its_kills_then_starts_the_streak_over() {
        let rt = create();
        let step = |flags: u32, kills: u32| {
            let input = CvStreakInput {
                size: size_of::<CvStreakInput>() as u32,
                flags,
                hostile_kills: kills,
                ..Default::default()
            };
            let mut out = CvStreakOutput { size: size_of::<CvStreakOutput>() as u32, ..Default::default() };
            assert_eq!(unsafe { cv_killstreak_step(rt, &input, &mut out) }, 1);
            out
        };
        // Three kills: a UAV held. The nuke's blast kills count as the player's...
        let o = step(0, 3);
        assert_eq!((o.streak, o.held), (3, 1));
        let o = step(0, 4);
        assert_eq!(o.streak, 7, "the blast's kills are counted");
        // ...then, with the nuke over, the streak starts again and nothing is held.
        let o = step(CV_STREAK_NUKE_OVER, 0);
        assert_eq!((o.streak, o.held, o.top), (0, 0, 0));
        let o = step(0, 3);
        assert_eq!((o.streak, o.earned), (3, killstreaks::UAV), "back to the first reward");
        unsafe { cv_destroy(rt) };
    }

    #[test]
    fn killstreaks_through_the_c_abi() {
        let rt = create();
        let step = |flags: u32, kills: u32| {
            let input = CvStreakInput {
                size: size_of::<CvStreakInput>() as u32,
                flags,
                hostile_kills: kills,
                ..Default::default()
            };
            let mut out = CvStreakOutput { size: size_of::<CvStreakOutput>() as u32, ..Default::default() };
            assert_eq!(unsafe { cv_killstreak_step(rt, &input, &mut out) }, 1);
            out
        };
        unsafe {
            assert_eq!(cv_set_option(rt, CV_OPT_KS_EMP_KILLS, 4.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_KS_SLOT2, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_KS_SLOT3, 4.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_KS_SLOT3, 11.0), 0, "no such reward");
        }
        // Kills count without CV_STREAK_ALLOWED too (in a vehicle, in a killstreak's view; the host
        // drops a story mission's); calling a reward in needs it.
        let o = step(0, 3);
        assert_eq!((o.streak, o.earned, o.top, o.held), (3, killstreaks::UAV, killstreaks::UAV, 1));
        let o = step(CV_STREAK_ACTIVATE, 0);
        assert_eq!((o.activated, o.held), (0, 1), "not allowed: nothing is called in");
        let o = step(CV_STREAK_ALLOWED | CV_STREAK_DIED, 0);
        assert_eq!((o.streak, o.held), (0, 0), "death resets the streak and loses the unused UAV (CODV)");
        let o = step(CV_STREAK_ALLOWED, 4);
        // A new streak earns the UAV again on its way to the EMP.
        assert_eq!((o.earned, o.top, o.held), (killstreaks::EMP, killstreaks::EMP, 2));
        let o = step(CV_STREAK_ALLOWED | CV_STREAK_ACTIVATE, 0);
        assert_eq!((o.activated, o.top), (killstreaks::EMP, killstreaks::UAV));
        // MW2's rule on request: the unused UAV survives the next death.
        unsafe {
            assert_eq!(cv_set_option(rt, CV_OPT_KS_KEEP_ON_DEATH, 1.0), 1);
        }
        let o = step(CV_STREAK_ALLOWED | CV_STREAK_DIED, 0);
        assert_eq!((o.streak, o.top, o.held), (0, killstreaks::UAV, 1), "kept through the death");
        // A care package's reward goes on top, even outside a streak.
        let input =
            CvStreakInput { size: size_of::<CvStreakInput>() as u32, give: killstreaks::AC130, ..Default::default() };
        let mut out = CvStreakOutput { size: size_of::<CvStreakOutput>() as u32, ..Default::default() };
        assert_eq!(unsafe { cv_killstreak_step(rt, &input, &mut out) }, 1);
        assert_eq!(out.top, killstreaks::AC130);
        assert_eq!(step(CV_STREAK_ALLOWED | CV_STREAK_ACTIVATE, 0).activated, killstreaks::AC130);
        unsafe { cv_set_option(rt, CV_OPT_KILLSTREAKS, 0.0) };
        let o = step(CV_STREAK_ALLOWED | CV_STREAK_ACTIVATE, 5);
        assert_eq!((o.activated, o.top, o.streak), (0, 0, 0), "off: nothing counts or fires");
        unsafe { cv_destroy(rt) };
    }

    #[test]
    fn fire_hit_reload_through_the_c_abi() {
        let rt = create();
        assert!(!rt.is_null());
        unsafe {
            assert!(cv_weapon_count(rt) >= 8);
            let mut info = CvWeaponInfo {
                size: size_of::<CvWeaponInfo>() as u32,
                gta_hash: 0,
                mag: 0,
                reserve: 0,
                name: [0; 32],
                label: [0; 32],
            };
            let mut rules = CvRules { size: size_of::<CvRules>() as u32, ..Default::default() };
            assert_eq!(cv_rules(rt, &mut rules), 1);
            assert_eq!(cv_weapon_info(rt, rules.primary, &mut info), 1);
            assert_eq!(CStr::from_bytes_until_nul(&info.name).unwrap().to_str().unwrap(), "carbine");
            assert_eq!(cv_weapon_info(rt, 999, &mut info), 0);
            cv_weapon_info(rt, rules.primary, &mut info);
            let mut input = CvInput {
                size: size_of::<CvInput>() as u32,
                dt_seconds: 1.0 / 60.0,
                flags: CV_IN_ON_FOOT,
                gta_weapon: info.gta_hash,
                gta_ammo: 120,
                health: 1.0,
                ..Default::default()
            };
            let mut out = CvOutput::default();
            for _ in 0..60 {
                assert_eq!(cv_step(rt, &input, &mut out), 1); // raise
            }
            assert!(out.flags & CV_OUT_ACTIVE != 0);
            input.held = FIRE;
            assert_eq!(cv_step(rt, &input, &mut out), 1);
            assert_eq!(out.shot_count, 1);
            assert_eq!(out.shots[0].gta_hash, info.gta_hash);
            assert_eq!(out.ammo_used, 1);
            assert!(out.kick[0] > 0.0);
            assert_eq!(CStr::from_bytes_until_nul(&out.hud.label).unwrap().to_str().unwrap(), "Carbine");
            let hits = [
                CvHit { weapon: out.shots[0].weapon, distance: 10.0, flags: CV_HIT_PED, hitloc: 0 },
                CvHit { weapon: out.shots[0].weapon, distance: 10.0, flags: CV_HIT_PED | CV_HIT_HEAD, hitloc: 0 },
            ];
            let mut dmg = [0.0f32; 2];
            assert_eq!(cv_report_hits(rt, hits.as_ptr(), 2, dmg.as_mut_ptr()), 2);
            assert_eq!(dmg[0], 30.0);
            assert!(dmg[1] > dmg[0]);
            cv_report_kill(rt);
            input.held = 0;
            input.gta_ammo = 119;
            cv_step(rt, &input, &mut out);
            assert!(out.hud.hitmarker > 0.0 && out.hud.flags & CV_HUD_KILL != 0);
            input.held = RELOAD;
            cv_step(rt, &input, &mut out);
            assert!(out.flags & CV_OUT_RELOAD != 0);
            assert_eq!(cv_set_option(rt, CV_OPT_PRIMARY, 3.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_PRIMARY, 999.0), 0);
            assert_eq!(cv_set_option(rt, 999, 1.0), 0);
            assert_eq!(cv_set_option(rt, CV_OPT_FALL_DAMAGE, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_FALL_DAMAGE, 2.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_FALL_DAMAGE, 3.0), 0, "no such setting");
            assert_eq!(cv_set_option(rt, CV_OPT_SLIDE, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_SPRINT_SPEED, 1.8), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_SPRINT_SPEED, 3.0), 0, "out of range");
            assert_eq!(cv_set_option(rt, CV_OPT_STAMINA_BAR, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_SCOREBAR, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_BLOOD, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_KILL_FEEDBACK, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_JUMP_SLOWDOWN, 1.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_RELOAD_SPRINT, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_RELOAD_SPRINT, 1.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_MANTLE_SPEED, 2.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_MANTLE_SPEED, 9.0), 0, "out of range");
            assert_eq!(cv_set_option(rt, CV_OPT_SLIDE_BOOST, 1.5), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_SLIDE_TIME, 5.0), 0, "out of range");
            assert_eq!(cv_set_option(rt, CV_OPT_VAULT_MIN_HEIGHT, 0.6), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_CLIMB_MAX_HEIGHT, 2.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_LEAN, 0.3), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_SURFACE, 3.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_SURFACE, 99.0), 0, "no such surface");
            assert_eq!(cv_set_option(rt, CV_OPT_LEAN, 0.0), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_LEAN, 2.0), 0, "out of range");
            for id in [CV_OPT_AUTO_VAULT, CV_OPT_HIGH_CLIMB, CV_OPT_DIVE, CV_OPT_SNAG_GUARD, CV_OPT_MOVEMENT_SOUNDS] {
                assert_eq!(cv_set_option(rt, id, 0.0), 1);
            }
            let mut buf = [0 as c_char; 96];
            assert!(cv_status_text(rt, buf.as_mut_ptr(), 96) > 0);
            let mut small = CvOutput { size: 8, ..Default::default() };
            assert_eq!(cv_step(rt, &input, &mut small), 0, "short output struct refused");
            cv_reset(rt);
            cv_destroy(rt);
        }
    }

    #[test]
    fn installed_table_loads_and_bad_ones_fall_back() {
        let dir = std::env::temp_dir().join(format!("codv-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lines = Mutex::new(Vec::<String>::new());
        let log = |s: &str| lines.lock().unwrap().push(s.to_owned());
        std::fs::write(
            dir.join("weapons.json"),
            codv_core::defs::EMBEDDED_JSON.replacen("\"rpm\": 750", "\"rpm\": 800", 1),
        )
        .unwrap();
        assert_eq!(load_table(Some(&dir), &log).weapons[0].rpm, 800.0);
        std::fs::write(dir.join("weapons.json"), "{ nope").unwrap();
        assert_eq!(load_table(Some(&dir), &log).weapons[0].rpm, 750.0);
        assert!(lines.lock().unwrap().iter().any(|l| l.contains("unusable")));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// MW2's weapon sounds through the ABI, from the player's own import
    /// (CODV_MW2_OUT: weapons.json + sounds/); skipped otherwise.
    #[test]
    fn mw2_weapon_sounds_through_the_abi() {
        let Ok(src) = std::env::var("CODV_MW2_OUT") else { return };
        let from = std::path::Path::new(&src);
        let root = std::env::temp_dir().join(format!("codv-audio-{}", std::process::id()));
        let files = root.join("mw2").join("sounds").join("files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::copy(from.join("weapons.json"), root.join("mw2").join("weapons.json")).unwrap();
        std::fs::copy(from.join("sounds/aliases.json"), root.join("mw2/sounds/aliases.json")).unwrap();
        for e in std::fs::read_dir(from.join("sounds/files")).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), files.join(e.file_name())).unwrap();
        }
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 9,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            let cfg =
                CvAudioConfig { size: size_of::<CvAudioConfig>() as u32, sample_rate: 48_000, ..Default::default() };
            assert_eq!(cv_audio_configure(rt, &cfg), 1);
            let mut em = [CvAudioEmitter::default()];
            assert_eq!(cv_audio_emitters(rt, em.as_mut_ptr(), 1), CV_AUDIO_EMITTER_COUNT);
            assert_eq!(em[0].flags & CV_AUDIO_EMITTER_ACTIVE, 0, "idle before the mode starts");
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mut mv = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            let mut buf = vec![0.0f32; 768];
            let mut pull = |rt: *mut c_void| {
                let mut info = CvAudioEmitter::default();
                assert_eq!(cv_audio_pull(rt, CV_AUDIO_EMITTER_WEAPON, buf.as_mut_ptr(), 768, &mut info), 768);
                (buf.iter().fold(0.0f32, |m, s| m.max(s.abs())), info)
            };
            // The raise, then a second of fire; pulled at the real-time rate.
            let mut loudest = 0.0f32;
            for i in 0..120 {
                mv.buttons = if i >= 60 { 0x1 } else { 0 };
                assert_eq!(cv_mw2_step(rt, &mv, &mut mo), 1);
                let (peak, info) = pull(rt);
                assert!(info.flags & CV_AUDIO_EMITTER_ACTIVE != 0 && info.flags & CV_AUDIO_EMITTER_LISTENER != 0);
                if i >= 60 {
                    loudest = loudest.max(peak);
                }
            }
            assert!(loudest > 0.05, "the M4 is heard: {loudest}");
            let mut st = CvAudioStatus::default();
            assert_eq!(cv_audio_get_status(rt, &mut st), 1);
            assert_eq!(st.state, CV_AUDIO_READY);
            assert!(st.frames >= 120 * 768);
            assert_eq!(cv_audio_set_paused(rt, 1), 1);
            assert_eq!(pull(rt).0, 0.0, "silent while paused");
            cv_mw2_despawn(rt);
            assert_eq!(pull(rt).1.flags & CV_AUDIO_EMITTER_ACTIVE, 0);
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// MW2's crosshair and scope through the ABI, from the player's own import
    /// (CODV_MW2_TABLE, with the HUD images); skipped otherwise: the hip
    /// crosshair opens while running and the sniper's scope covers full ADS.
    #[test]
    fn mw2_crosshair_and_scope_through_the_abi() {
        let Ok(table) = std::env::var("CODV_MW2_TABLE") else { return };
        let root = std::env::temp_dir().join(format!("codv-hud-{}", std::process::id()));
        std::fs::create_dir_all(root.join("mw2")).unwrap();
        std::fs::copy(&table, root.join("mw2").join("weapons.json")).unwrap();
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 4,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            let n = cv_mw2_gun_count(rt);
            let mut sniper = None;
            for i in 0..n {
                let mut w = CvWeaponInfo {
                    size: size_of::<CvWeaponInfo>() as u32,
                    gta_hash: 0,
                    mag: 0,
                    reserve: 0,
                    name: [0; 32],
                    label: [0; 32],
                };
                cv_mw2_gun_info(rt, i, &mut w);
                if CStr::from_bytes_until_nul(&w.name).unwrap().to_str().unwrap() == "cheytac_mp" {
                    sniper = Some(i);
                }
            }
            let m4 = 0;
            assert_eq!(cv_set_option(rt, CV_OPT_MW2_PRIMARY, m4 as f32), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_MW2_SECONDARY, sniper.expect("cheytac") as f32), 1);
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mut mv = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            let hud = |rt| {
                let mut h = CvMw2Hud { size: size_of::<CvMw2Hud>() as u32, ..Default::default() };
                assert_eq!(cv_mw2_hud(rt, 65.0, 0, &mut h), 1);
                h
            };
            for _ in 0..120 {
                cv_mw2_step(rt, &mv, &mut mo);
            }
            let still = hud(rt);
            assert!(still.flags & CV_HUD_CROSSHAIR != 0, "{still:?}");
            assert_eq!(CStr::from_bytes_until_nul(&still.side_image).unwrap().to_str().unwrap(), "side_small");
            mv.forward = 127;
            for _ in 0..60 {
                cv_mw2_step(rt, &mv, &mut mo);
            }
            let running = hud(rt);
            assert!(running.gap[1] > still.gap[1] + 2.0, "running opens it: {:?} -> {:?}", still.gap, running.gap);
            mv.forward = 0;
            // The sniper, then full ADS: the scope, and no view model behind it.
            mv.weapon_request = 3; // slot 2: the secondary
            cv_mw2_step(rt, &mv, &mut mo);
            mv.weapon_request = 0;
            for _ in 0..120 {
                cv_mw2_step(rt, &mv, &mut mo);
            }
            mv.buttons = 0x800;
            for _ in 0..90 {
                cv_mw2_step(rt, &mv, &mut mo);
            }
            let scoped = hud(rt);
            assert!(scoped.flags & CV_HUD_OVERLAY != 0 && scoped.flags & CV_HUD_CROSSHAIR == 0, "{scoped:?}");
            assert!(scoped.overlay_alpha > 0.99);
            assert_eq!(
                CStr::from_bytes_until_nul(&scoped.overlay_image).unwrap().to_str().unwrap(),
                "scope_overlay_m40a3_1024_0"
            );
            assert!(rt_inner_scoped(rt));
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    unsafe fn rt_inner_scoped(rt: *mut c_void) -> bool {
        unsafe { runtime(rt) }.unwrap().inner.lock().unwrap().scoped
    }

    /// MW2's HUD through the ABI from the player's import (CODV_MW2_OUT with
    /// hud/hud.json); skipped otherwise: the weapon bar's quads and images,
    /// and a damage arrow.
    #[test]
    fn mw2_hud_through_the_abi() {
        let Ok(out) = std::env::var("CODV_MW2_OUT") else { return };
        let out = std::path::Path::new(&out);
        if !out.join("hud/hud.json").is_file() {
            return;
        }
        let root = std::env::temp_dir().join(format!("codv-mw2hud-{}", std::process::id()));
        std::fs::create_dir_all(root.join("mw2/hud")).unwrap();
        std::fs::copy(out.join("weapons.json"), root.join("mw2/weapons.json")).unwrap();
        std::fs::copy(out.join("hud/hud.json"), root.join("mw2/hud/hud.json")).unwrap();
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 4,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mv = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            for _ in 0..60 {
                cv_mw2_step(rt, &mv, &mut mo);
            }
            let mut input = CvHudDrawInput {
                size: size_of::<CvHudDrawInput>() as u32,
                width: 1920.0,
                height: 1080.0,
                ..Default::default()
            };
            let mut quads = vec![CvHudQuad::default(); 1024];
            let n = cv_mw2_hud_draw(rt, &input, quads.as_mut_ptr(), 1024) as usize;
            assert!(n > 20, "the weapon bar: {n} quads");
            let mut names = std::collections::BTreeSet::new();
            for q in &quads[..n] {
                let mut name = [0 as c_char; 64];
                assert!(cv_mw2_hud_image(rt, q.image, name.as_mut_ptr(), 64) > 0);
                names.insert(CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned());
            }
            assert!(names.contains("hud_weaponbar") && names.contains("gamefonts_pc"), "{names:?}");
            // Shot from 10 m ahead: an arrow; a GTA grenade 3 m to the side: the indicator.
            let from = [mo.position[0], mo.position[1] + 10.0, mo.position[2]];
            assert_eq!(cv_mw2_hud_damage(rt, from.as_ptr(), 0), 1);
            input.grenade_count = 1;
            input.grenades[0] = [mo.position[0] + 3.0, mo.position[1], mo.position[2]];
            cv_mw2_step(rt, &mv, &mut mo);
            let m = cv_mw2_hud_draw(rt, &input, quads.as_mut_ptr(), 1024) as usize;
            let mut more = std::collections::BTreeSet::new();
            for q in &quads[..m] {
                let mut name = [0 as c_char; 64];
                cv_mw2_hud_image(rt, q.image, name.as_mut_ptr(), 64);
                more.insert(CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned());
            }
            assert!(more.contains("hit_direction") && more.contains("grenadeicon_white"), "{more:?}");
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// The menu's guns, the ammo scale and a thrown frag through the ABI, from
    /// the player's own import (CODV_MW2_TABLE); skipped otherwise.
    #[test]
    fn mw2_menu_guns_ammo_and_frags_through_the_abi() {
        let Ok(table) = std::env::var("CODV_MW2_TABLE") else { return };
        let root = std::env::temp_dir().join(format!("codv-frag-{}", std::process::id()));
        std::fs::create_dir_all(root.join("mw2")).unwrap();
        std::fs::copy(&table, root.join("mw2").join("weapons.json")).unwrap();
        let root_c = std::ffi::CString::new(root.to_str().unwrap()).unwrap();
        let info = CvCreateInfo {
            size: size_of::<CvCreateInfo>() as u32,
            abi_version: CODV_ABI_VERSION,
            data_root_utf8: root_c.as_ptr(),
            log_path_utf8: ptr::null(),
            flags: 0,
            seed: 4,
        };
        let mut calls = 0u32;
        unsafe {
            let rt = cv_create(&info);
            assert!(!rt.is_null());
            let n = cv_mw2_gun_count(rt);
            assert!(n >= 30, "MW2's base guns: {n}");
            let mut names = Vec::new();
            for i in 0..n {
                let mut w = CvWeaponInfo {
                    size: size_of::<CvWeaponInfo>() as u32,
                    gta_hash: 0,
                    mag: 0,
                    reserve: 0,
                    name: [0; 32],
                    label: [0; 32],
                };
                assert_eq!(cv_mw2_gun_info(rt, i, &mut w), 1);
                names.push(CStr::from_bytes_until_nul(&w.name).unwrap().to_string_lossy().into_owned());
                if i == 0 {
                    assert_eq!(names[0], "m4_mp");
                    assert_eq!((w.mag, w.reserve), (30, 180), "M4: 30 + 60 spare, x3");
                }
            }
            let ak = names.iter().position(|n| n == "ak47_mp").unwrap();
            let spas = names.iter().position(|n| n == "spas12_mp").unwrap();
            assert_eq!(cv_set_option(rt, CV_OPT_MW2_PRIMARY, ak as f32), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_MW2_SECONDARY, spas as f32), 1);
            assert_eq!(cv_set_option(rt, CV_OPT_MW2_PRIMARY, 999.0), 0);
            let spawn = CvMoveSpawn {
                size: size_of::<CvMoveSpawn>() as u32,
                position: [0.0, 0.0, 10.05],
                heading: 0.0,
                flags: 0,
            };
            assert_eq!(cv_mw2_spawn(rt, &spawn), 1);
            assert_eq!(cv_mw2_set_world(rt, Some(flat_ground), &mut calls as *mut u32 as *mut c_void), 1);
            let mut mv = CvMoveInput { size: size_of::<CvMoveInput>() as u32, msec: 16, ..Default::default() };
            let mut mo = CvMoveOutput { size: size_of::<CvMoveOutput>() as u32, ..Default::default() };
            let input =
                CvInput { size: size_of::<CvInput>() as u32, dt_seconds: 0.016, health: 1.0, ..Default::default() };
            let mut out = CvOutput::default();
            for _ in 0..80 {
                cv_mw2_step(rt, &mv, &mut mo);
                cv_step(rt, &input, &mut out);
            }
            let label = CStr::from_bytes_until_nul(&out.hud.label).unwrap().to_string_lossy().into_owned();
            assert!(label.contains("AK"), "the AK-47 in hand: {label}");
            assert_eq!(out.hud.reserve, 180, "AK: 60 spare x3");
            // Frag: press and release.
            let mut grenades = [CvGrenade::default(); 16];
            let (mut flying, mut exploded, mut last) = (0, 0, [0.0f32; 3]);
            for i in 0..400 {
                mv.buttons = if i < 20 { 0x4000 } else { 0 };
                cv_mw2_step(rt, &mv, &mut mo);
                let k = cv_mw2_grenades(rt, grenades.as_mut_ptr(), 16) as usize;
                for g in &grenades[..k] {
                    match g.state {
                        CV_GRENADE_FLYING => {
                            flying += 1;
                            last = g.position;
                        }
                        CV_GRENADE_EXPLODED => {
                            exploded += 1;
                            assert!((g.radius - 256.0 * METRES_PER_UNIT).abs() < 0.01);
                        }
                        _ => panic!("state {}", g.state),
                    }
                }
            }
            assert!(flying > 50, "in flight for a while: {flying}");
            assert_eq!(exploded, 1, "one explosion");
            assert!(last[1] > 4.0 && last[2] < 10.2, "thrown ahead (heading 0 = +Y) onto the ground: {last:?}");
            // Number keys: 1 the knife (the trigger melees), 4 the frag (the trigger throws), 2 the gun.
            mv.weapon_request = CV_WEAPON_REQUEST_KNIFE;
            cv_mw2_step(rt, &mv, &mut mo);
            mv.weapon_request = 0;
            let mut meleed = false;
            for i in 0..60 {
                mv.buttons = if i < 5 { 0x1 } else { 0 };
                cv_mw2_step(rt, &mv, &mut mo);
                cv_step(rt, &input, &mut out);
                meleed |= out.flags & CV_OUT_MELEE != 0;
                assert_eq!(out.shot_count, 0, "the knife fires nothing");
            }
            assert!(meleed, "the trigger melees with the knife");
            assert_eq!(CStr::from_bytes_until_nul(&out.hud.label).unwrap().to_str().unwrap(), "KNIFE");
            mv.weapon_request = CV_WEAPON_REQUEST_GRENADE;
            cv_mw2_step(rt, &mv, &mut mo);
            mv.weapon_request = 0;
            cv_step(rt, &input, &mut out);
            assert_eq!(CStr::from_bytes_until_nul(&out.hud.label).unwrap().to_str().unwrap(), "FRAG x1");
            let mut thrown = 0;
            for i in 0..120 {
                mv.buttons = if i < 20 { 0x1 } else { 0 };
                cv_mw2_step(rt, &mv, &mut mo);
                cv_step(rt, &input, &mut out);
                let k = cv_mw2_grenades(rt, grenades.as_mut_ptr(), 16) as usize;
                thrown = thrown.max(grenades[..k].iter().filter(|g| g.state == CV_GRENADE_FLYING).count());
            }
            assert_eq!(thrown, 1, "the trigger threw the frag");
            let label = CStr::from_bytes_until_nul(&out.hud.label).unwrap().to_string_lossy().into_owned();
            assert!(label.contains("AK"), "the gun is back: {label}");
            // No frags left: 4 does nothing.
            mv.weapon_request = CV_WEAPON_REQUEST_GRENADE;
            cv_mw2_step(rt, &mv, &mut mo);
            cv_step(rt, &input, &mut out);
            assert!(CStr::from_bytes_until_nul(&out.hud.label).unwrap().to_string_lossy().contains("AK"));
            // Random loadouts (each switch-on): a new set every time; slot 0 a primary that is not a
            // sniper, slot 1 a secondary, slot 2 (key 3) always a sniper, slot 3 (key 5) a launcher.
            let mut pairs = std::collections::BTreeSet::new();
            for _ in 0..8 {
                assert_eq!(cv_set_option(rt, CV_OPT_MW2_RANDOMIZE, 1.0), 1);
                let inner = (*(rt as *const Runtime)).inner.lock().unwrap();
                let l = inner.arms.as_ref().unwrap().loadout_names();
                assert!(arms::MENU_GUNS[..arms::MENU_SNIPERS].iter().any(|b| l[0] == format!("{b}_mp")), "{l:?}");
                assert!(arms::MENU_GUNS[arms::MENU_PRIMARIES..].iter().any(|b| l[1] == format!("{b}_mp")), "{l:?}");
                assert!(
                    arms::MENU_GUNS[arms::MENU_SNIPERS..arms::MENU_PRIMARIES].iter().any(|b| l[2] == format!("{b}_mp")),
                    "{l:?}"
                );
                assert!(arms::MENU_LAUNCHERS.iter().any(|b| l[3] == format!("{b}_mp")), "{l:?}");
                pairs.insert(l);
            }
            assert!(pairs.len() >= 6, "varied: {pairs:?}");
            // Weapon sets: three different random guns behind each number key, from the key's family;
            // the key again takes the group's next gun, another key its group's last one.
            assert_eq!(cv_set_option(rt, CV_OPT_WEAPON_SETS, 1.0), 1);
            let groups: Vec<Vec<String>> = {
                let inner = (*(rt as *const Runtime)).inner.lock().unwrap();
                inner
                    .arms
                    .as_ref()
                    .unwrap()
                    .all_names()
                    .iter()
                    .map(|g| g.split(", ").map(String::from).collect())
                    .collect()
            };
            let families: [Vec<&str>; 4] = [
                arms::MENU_GUNS[..arms::MENU_SNIPERS].to_vec(),
                arms::MENU_GUNS[arms::MENU_PRIMARIES..].to_vec(),
                arms::MENU_GUNS[arms::MENU_SNIPERS..arms::MENU_PRIMARIES].to_vec(),
                arms::MENU_LAUNCHERS.to_vec(),
            ];
            assert_eq!(groups.len(), 4, "{groups:?}");
            for (g, family) in groups.iter().zip(&families) {
                assert_eq!(g.len(), 3, "{groups:?}");
                assert!(g[0] != g[1] && g[1] != g[2] && g[0] != g[2], "{groups:?}");
                assert!(g.iter().all(|n| family.iter().any(|b| *n == format!("{b}_mp"))), "{groups:?}");
            }
            let press = |slot: u32| {
                let mut mv = mv;
                let mut mo = mo;
                mv.weapon_request = 2 + slot;
                cv_mw2_step(rt, &mv, &mut mo);
                let inner = (*(rt as *const Runtime)).inner.lock().unwrap();
                inner.arms.as_ref().unwrap().selected_name()
            };
            assert_eq!(press(0), groups[0][1], "key 1 again: the group's next");
            assert_eq!(press(0), groups[0][2]);
            assert_eq!(press(0), groups[0][0], "round the group");
            assert_eq!(press(0), groups[0][1]);
            assert_eq!(press(2), groups[2][0], "key 3: the snipers' first");
            assert_eq!(press(2), groups[2][1]);
            assert_eq!(press(0), groups[0][1], "key 1: its group's last gun");
            assert_eq!(press(3), groups[3][0], "key 5: the launchers");
            // Each switch-on rolls new sets; off, one gun per key again.
            assert_eq!(cv_set_option(rt, CV_OPT_MW2_RANDOMIZE, 1.0), 1);
            {
                let inner = (*(rt as *const Runtime)).inner.lock().unwrap();
                assert_eq!(inner.arms.as_ref().unwrap().all_names().len(), 4);
            }
            assert_eq!(cv_set_option(rt, CV_OPT_WEAPON_SETS, 0.0), 1);
            {
                let inner = (*(rt as *const Runtime)).inner.lock().unwrap();
                assert_eq!(inner.arms.as_ref().unwrap().all_names().len(), 4, "four guns, one per key");
            }
            cv_destroy(rt);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
