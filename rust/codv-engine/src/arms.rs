//! Modern Warfare 2's weapons, run by iw4L's retail weapon state machine
//! (`weapon_iw4::weapon_hands`) on the definitions read from the player's own
//! MW2 (`codv_mw2data`'s table).
//!
//! The host glue follows iw4L's simulation (`sim/src/combat.rs`): the aim
//! spread scale is adjusted every frame from the player's motion
//! (`adjust_aim_spread_scale`), the weapon ticks with a retail command built
//! from the MW2 player's state, and each accepted shot grows the spread
//! (`add_aim_spread_fire`), takes the cone `get_spread_for_weapon` /
//! `fire_weapon_spread_degrees` give, and seeds the view kick
//! (`weapon_fire_recoil`, a critically damped impulse per axis as iw4L
//! renders it). The weapon's ADS rates, move and sprint scales go into the
//! player's pmove context, so aiming and sprinting follow the gun in hand.
//!
//! Damage is MW2's: `damage` falling to `min_damage` between the two ranges,
//! times the hit location's multiplier (MW2's global table times the weapon's).
use crate::player::{Mw2Player, WeaponMoveFacts};
use codv_mw2data::table::{HITLOCS, Row, Table};
use playerstate_iw4::pm_flags;
use weapon_iw4 as w;
use weapon_iw4::{GunKickRange, ViewKickRange, WeaponCombatFacts, WeaponHandState};

/// CODV's reload while sprinting: the sprint's speed while the reload goes on.
pub const RELOAD_SPRINT_SPEED: f32 = 0.88;

/// One MW2 weapon, ready for the state machine.
#[derive(Clone, Debug)]
pub struct Gun {
    pub name: String,
    pub display_name: String,
    pub facts: WeaponCombatFacts,
    pub moves: WeaponMoveFacts,
    pub kick: Kick,
    pub strings: Vec<(String, String)>,
}

impl Gun {
    pub fn string(&self, key: &str) -> &str {
        self.strings.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
    }

    /// MW2's damage at `distance` IW4 units (iw4L `bullet_damage_at_distance`).
    pub fn damage_at(&self, distance: f32) -> i32 {
        let f = &self.facts;
        let base = f.damage;
        let min = if f.min_damage > 0 { f.min_damage } else { base };
        let range = f.min_damage_range - f.max_damage_range;
        let mut damage = base;
        if base != min && range != 0.0 && f.max_damage_range <= distance {
            damage = if f.min_damage_range <= distance {
                min
            } else {
                let t = ((distance - f.max_damage_range) / range).clamp(0.0, 1.0);
                ((base as f32) * (1.0 - t) + (min as f32) * t) as i32
            };
        }
        damage.max(0)
    }

    /// The hit location's damage multiplier (weapon_iw4::HITLOC_NAMES order).
    pub fn location_scale(&self, hitloc: usize) -> f32 {
        w::location_damage_scale(&self.facts.location_damage, hitloc.min(HITLOCS - 1) as u8)
    }
}

/// The weapon's recoil ranges (WeaponKickCapture).
#[derive(Clone, Copy, Debug, Default)]
pub struct Kick {
    pub hip_view: ViewKickRange,
    pub ads_view: ViewKickRange,
    pub hip_gun: GunKickRange,
    pub ads_gun: GunKickRange,
    pub hip_reduced_percent: f32,
    pub ads_reduced_percent: f32,
}

/// The table's weapons; a weapon's id is its index + 1 (0: none).
#[derive(Clone, Debug, Default)]
pub struct Arsenal {
    guns: Vec<Gun>,
    /// Thrown offhand grenades (frag class): their weapon facts and how they fly.
    /// Ids follow the guns' (guns.len() + 1 ...).
    offhand: Vec<(Gun, Throw)>,
}

/// How a thrown grenade flies and explodes (the weapon row's projectile and
/// explosion fields; iw4L `EquipmentRuntimeFacts`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throw {
    /// MW2 offhandClass (1 frag).
    pub offhand_class: i32,
    pub hold_fire_time_ms: i32,
    pub fuse_time_ms: i32,
    /// Cooking: the fuse runs while the grenade is held.
    pub cook_off_hold: bool,
    /// Launch speed along the view and extra up / forward speed, units per second.
    pub speed: f32,
    pub speed_up: f32,
    pub speed_forward: f32,
    /// Explosion radius (units) and damage at the centre and at the edge.
    pub radius: f32,
    pub inner_damage: f32,
    pub outer_damage: f32,
}

fn kick(r: &Row) -> Kick {
    let k = |s: &str| r.f(&format!("kick.{s}"));
    Kick {
        hip_view: ViewKickRange {
            pitch_min: k("hip_view_kick_pitch_min"),
            pitch_max: k("hip_view_kick_pitch_max"),
            yaw_min: k("hip_view_kick_yaw_min"),
            yaw_max: k("hip_view_kick_yaw_max"),
        },
        ads_view: ViewKickRange {
            pitch_min: k("ads_view_kick_pitch_min"),
            pitch_max: k("ads_view_kick_pitch_max"),
            yaw_min: k("ads_view_kick_yaw_min"),
            yaw_max: k("ads_view_kick_yaw_max"),
        },
        hip_gun: GunKickRange {
            pitch_min: k("hip_gun_kick_pitch_min"),
            pitch_max: k("hip_gun_kick_pitch_max"),
            yaw_min: k("hip_gun_kick_yaw_min"),
            yaw_max: k("hip_gun_kick_yaw_max"),
        },
        ads_gun: GunKickRange {
            pitch_min: k("ads_gun_kick_pitch_min"),
            pitch_max: k("ads_gun_kick_pitch_max"),
            yaw_min: k("ads_gun_kick_yaw_min"),
            yaw_max: k("ads_gun_kick_yaw_max"),
        },
        hip_reduced_percent: k("hip_gun_kick_reduced_kick_percent"),
        ads_reduced_percent: k("ads_gun_kick_reduced_kick_percent"),
    }
}

/// A table row as iw4L's `CapturedCombatInput` (asset_game's
/// `capture_iw4` + `combat::captured_input`), compiled with MW2's global
/// hit-location table.
fn combat_facts(r: &Row, global: [f32; HITLOCS]) -> Result<WeaponCombatFacts, w::MissingCombatFacts> {
    let segmented_reload = r.b("segmented_reload") && r.i("reload_start_time_ms") > 0;
    let reload_ammo_add = match r.i("reload_ammo_add") {
        n if n > 0 => n,
        _ if segmented_reload => 1,
        _ => 0,
    };
    let fire_type = r.i("fire_type");
    let burst = w::FireType::from_i32(fire_type).is_ok_and(|t| t.is_burst());
    let input = w::CapturedCombatInput {
        dual_wield: false,
        fire_melees: false,
        fire_time_ms: r.i("fire_time_ms"),
        fire_delay_ms: r.i("fire_delay_ms"),
        raise_time_ms: r.i("raise_time_ms"),
        drop_time_ms: r.i("drop_time_ms"),
        alternate_weapon: 0,
        alternate_raise_time_ms: r.i("alternate_raise_time_ms"),
        alternate_drop_time_ms: r.i("alternate_drop_time_ms"),
        first_raise_time_ms: r.i("first_raise_time_ms"),
        reload_time_ms: r.i("reload_time_ms"),
        reload_empty_time_ms: r.i("reload_empty_time_ms"),
        // A separate empty reload exists when its time is authored.
        empty_reload: if r.i("reload_empty_time_ms") > 0 {
            w::EmptyReloadPolicy::Authored
        } else {
            w::EmptyReloadPolicy::Ordinary
        },
        clip_size: r.i("clip_size"),
        start_ammo: r.i("start_ammo"),
        max_ammo: r.i("max_ammo"),
        ammo_index: r.i("ammo_index"),
        clip_index: r.i("clip_index"),
        fire_type,
        weap_type: r.i("weap_type"),
        weap_class: r.i("weap_class"),
        player_anim_type: r.i("player_anim_type"),
        inventory_type: r.i("inventory_type"),
        impact_type: r.i("impact_type"),
        shots_per_fire: r.i("shots_per_fire"),
        burst_cooldown_ms: if burst { w::BURST_COOLDOWN_DEFAULT_MS } else { 0 },
        bolt_action: r.b("bolt_action"),
        rechamber_time_ms: r.i("rechamber_time_ms"),
        rechamber_bolt_time_ms: r.i("rechamber_bolt_time_ms"),
        rechamber_bolt_delay_ms: r.i("rechamber_bolt_delay_ms"),
        segmented_reload,
        reload_start_time_ms: r.i("reload_start_time_ms"),
        reload_end_time_ms: r.i("reload_end_time_ms"),
        reload_ammo_add,
        reload_add_time_ms: r.i("reload_add_time_ms"),
        reload_empty_add_time_ms: 0,
        reload_start_add_time_ms: r.i("reload_start_add_time_ms"),
        reload_start_add: r.i("reload_start_add"),
        no_partial_reload: r.b("no_partial_reload"),
        sprint_raise_time_ms: r.i("sprint_raise_time_ms"),
        sprint_drop_time_ms: r.i("sprint_drop_time_ms"),
        stunned_start_time_ms: r.i("stunned_start_time_ms"),
        stunned_end_time_ms: r.i("stunned_end_time_ms"),
        damage: r.i("damage"),
        flame_damage_interval_ms: 0,
        min_damage: r.i("min_damage"),
        max_damage_range: r.f("max_damage_range"),
        min_damage_range: r.f("min_damage_range"),
        hip_spread_stand_min: r.f("hip_spread_stand_min"),
        hip_spread_ducked_min: r.f("hip_spread_ducked_min"),
        hip_spread_prone_min: r.f("hip_spread_prone_min"),
        hip_spread_stand_max: r.f("hip_spread_stand_max"),
        hip_spread_ducked_max: r.f("hip_spread_ducked_max"),
        hip_spread_prone_max: r.f("hip_spread_prone_max"),
        hip_spread_decay_rate: r.f("hip_spread_decay_rate"),
        hip_spread_fire_add: r.f("hip_spread_fire_add"),
        hip_spread_turn_add: r.f("hip_spread_turn_add"),
        hip_spread_move_add: r.f("hip_spread_move_add"),
        hip_spread_ducked_decay: r.f("hip_spread_ducked_decay"),
        hip_spread_prone_decay: r.f("hip_spread_prone_decay"),
        ads_spread: r.f("ads_spread"),
        aim_down_sight: r.b("aim_down_sight"),
        no_ads_when_mag_empty: r.b("no_ads_when_mag_empty"),
        inherits_perks: r.b("inherits_perks"),
        ads_reload_trans_time_ms: r.i("ads_reload_trans_time_ms"),
        ads_in_rate: r.f("ads_in_rate"),
        ads_out_rate: r.f("ads_out_rate"),
        rechamber_while_ads: r.b("rechamber_while_ads"),
        ads_fire_only: r.b("ads_fire_only"),
        melee_damage: r.i("melee_damage"),
        can_hold_breath: r.i("overlay_reticle") != 0 && r.i("weap_class") != 11,
        scope_zoom: w::ScopeZoom::NONE,
        overlay_reticle: r.i("overlay_reticle"),
        melee_time_ms: r.i("melee_time_ms"),
        melee_delay_ms: r.i("melee_delay_ms"),
        melee_charge_time_ms: r.i("melee_charge_time_ms"),
        melee_charge_delay_ms: r.i("melee_charge_delay_ms"),
        melee_charge_anim: false,
        // MW2's guns melee with the knife (their melee clips are viewmodel_knife_melee_*, knifeModel
        // viewmodel_knife): the melee ends with the gun's quick raise (MELEE_END), not a cut to idle.
        knife_model: u32::from(r.s("xanim.7").contains("knife") || r.s("xanim.8").contains("knife")),
        quick_raise_time_ms: r.i("quick_raise_time_ms"),
        quick_drop_time_ms: r.i("quick_drop_time_ms"),
        select_requires_ammo: r.opt_b("select_requires_ammo"),
        offhand_hold_is_cancelable: r.opt_b("offhand_hold_is_cancelable"),
        ads_gun_kick_reduced_kick_bullets: r.i("kick.ads_gun_kick_reduced_kick_bullets"),
        hip_gun_kick_reduced_kick_bullets: r.i("kick.hip_gun_kick_reduced_kick_bullets"),
        location_damage: w::bake_location_damage(r.i("weap_type"), r.i("weap_class"), global, r.location),
        dual_mag: None,
    };
    WeaponCombatFacts::try_from_captured(input)
}

fn throw_facts(r: &Row) -> Throw {
    Throw {
        offhand_class: r.i("offhand_class"),
        hold_fire_time_ms: r.i("hold_fire_time_ms"),
        fuse_time_ms: r.i("fuse_time_ms"),
        // MW2's frag cooks (cookOffHold); the row does not carry the flag.
        cook_off_hold: r.opt_b("cook_off_hold").unwrap_or(r.i("offhand_class") == 1),
        speed: r.f("projectile_speed"),
        speed_up: r.f("projectile_speed_up"),
        speed_forward: r.f("projectile_speed_forward"),
        radius: r.f("explosion_radius"),
        inner_damage: r.f("explosion_inner_damage"),
        outer_damage: r.f("explosion_outer_damage"),
    }
}

impl Arsenal {
    /// The table's weapons a player can hold: bullet guns (with a magazine and a
    /// fire time) and the launchers (codv_mw2data::is_holdable_gun); killstreak
    /// guns and alt modes are left out. Returns the weapons refused too.
    pub fn from_table(t: &Table) -> (Arsenal, Vec<(String, String)>) {
        let global = t.location.unwrap_or(w::LOCATION_DAMAGE_IDENTITY);
        let mut guns = Vec::new();
        let mut refused = Vec::new();
        let mut offhand = Vec::new();
        for r in &t.weapons {
            // Thrown frag-class grenades (offhand class 1): the frag, semtex.
            if r.i("weap_type") == w::WEAPTYPE_GRENADE && r.i("offhand_class") == 1 && r.i("fire_time_ms") > 0 {
                if let Ok(facts) = combat_facts(r, global) {
                    let gun = Gun {
                        name: r.name.clone(),
                        display_name: r.display_name.clone(),
                        facts,
                        moves: WeaponMoveFacts::default(),
                        kick: kick(r),
                        strings: r.strings.clone(),
                    };
                    offhand.push((gun, throw_facts(r)));
                }
                continue;
            }
            // Bullet guns, and MW2's launchers (CODV fires their rockets as GTA's: the host's stand-in).
            if !codv_mw2data::is_holdable_gun(r) {
                continue;
            }
            match combat_facts(r, global) {
                Ok(facts) => guns.push(Gun {
                    name: r.name.clone(),
                    display_name: r.display_name.clone(),
                    facts,
                    moves: WeaponMoveFacts {
                        move_speed_scale: r.f("move_speed_scale").max(0.1),
                        ads_move_speed_scale: r.f("ads_move_speed_scale").max(0.1),
                        sprint_duration_scale: r.f("sprint_duration_scale").max(0.1),
                        ads_allowed: false,
                        aim_down_sight: facts.aim_down_sight,
                        ads_in_rate: facts.ads_in_rate,
                        ads_out_rate: facts.ads_out_rate,
                        ads_reload_trans_time_ms: facts.ads_reload_trans_time_ms,
                        rechamber_while_ads: facts.rechamber_while_ads,
                        ads_fire_only: facts.ads_fire_only,
                        scope: facts.overlay_reticle != 0,
                        can_hold_breath: facts.can_hold_breath,
                        segmented_reload: facts.segmented_reload,
                        melee_delay_ms: facts.melee_delay_ms,
                        melee_charge_delay_ms: facts.melee_charge_delay_ms,
                        hold_sprint: false,
                        reload_sprint: false,
                    },
                    kick: kick(r),
                    strings: r.strings.clone(),
                }),
                Err(e) => refused.push((r.name.clone(), format!("{e:?}"))),
            }
        }
        (Arsenal { guns, offhand }, refused)
    }

    pub fn len(&self) -> usize {
        self.guns.len()
    }
    pub fn is_empty(&self) -> bool {
        self.guns.is_empty()
    }
    pub fn id(&self, name: &str) -> Option<u32> {
        self.guns.iter().position(|g| g.name == name).map(|i| i as u32 + 1)
    }
    pub fn gun(&self, id: u32) -> Option<&Gun> {
        id.checked_sub(1).and_then(|i| self.guns.get(i as usize))
    }
    pub fn guns(&self) -> &[Gun] {
        &self.guns
    }
    /// A thrown grenade's id by name (ids after the guns').
    pub fn offhand_id(&self, name: &str) -> Option<u32> {
        let base = self.guns.len() as u32;
        self.offhand.iter().position(|(g, _)| g.name == name).map(|i| base + i as u32 + 1)
    }
    pub fn offhand(&self, id: u32) -> Option<(&Gun, &Throw)> {
        let i = id.checked_sub(self.guns.len() as u32 + 1)?;
        self.offhand.get(i as usize).map(|(g, t)| (g, t))
    }
    /// A gun or a thrown grenade (viewmodel and sounds take either).
    pub fn any(&self, id: u32) -> Option<&Gun> {
        self.gun(id).or_else(|| self.offhand(id).map(|(g, _)| g))
    }
}

/// One bullet (pellet) leaving the view: its direction off the view centre,
/// degrees (yaw right +, pitch up +).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub weapon: u32,
    pub yaw: f32,
    pub pitch: f32,
}

/// What a frame of the weapon did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GunFrame {
    pub weapon: u32,
    pub shots: Vec<Shot>,
    /// The view kick now (degrees, pitch up +, yaw right +): an offset on
    /// top of the player's view, returning to zero.
    pub kick: [f32; 2],
    pub fired: bool,
    pub empty_click: bool,
    pub reload_started: bool,
    pub reload_ended: bool,
    pub raise_started: bool,
    pub putaway_started: bool,
    pub melee_started: bool,
    pub melee_fired: bool,
    pub clip: i32,
    pub stock: i32,
    pub weaponstate: i32,
    /// Milliseconds left in the current weapon state (reload progress).
    pub weapon_time: i32,
    /// The current spread cone half-angle, degrees (the crosshair).
    pub spread: f32,
    /// ADS fraction 0..1.
    pub ads: f32,
    /// The hand's weapon animation (playerstate weapAnim: event id plus the
    /// 0x200 restart bit) after this tick; the viewmodel follows its edges.
    pub weap_anim: i32,
    /// On a shot: the gun kick peak (pitch, yaw degrees) for the viewmodel.
    pub gun_kick: Option<[f32; 2]>,
    /// Grenades left (frags).
    pub frags: i32,
    /// The grenade the view shows while an offhand throw runs (the hands
    /// pull the pin and throw with its viewmodel).
    pub offhand_view: Option<u32>,
    /// The pin was pulled this tick.
    pub offhand_prepare: bool,
    /// A grenade left the hand this tick: its weapon and the fuse left when
    /// it was cooked (None: the full fuse).
    pub thrown: Option<(u32, Option<i32>)>,
    /// A cooked grenade went off in the hand this tick.
    pub cooked_off: Option<u32>,
}

/// The player's guns: a hand ticking the held weapon, the ammo of each
/// carried weapon, the aim spread and the view kick.
#[derive(Clone, Debug)]
pub struct Gunner {
    hands: [WeaponHandState; 2],
    /// The frag grenade carried (id, count), the offhand weapon in use and
    /// the fuse left while it is held (the weapon command's offhand state).
    frag: Option<(u32, i32)>,
    off_hand_index: i32,
    grenade_time_left: i32,
    /// Spare ammunition times this (CODV's AmmoScale; MW2 is 1).
    reserve_scale: f32,
    /// This frame's usercmd forward / right move (the aim spread grows while moving).
    moves: (i8, i8),
    loadout: Vec<u32>,
    ammo: Vec<(u32, i32, i32)>,
    used: Vec<u32>,
    want: u32,
    old_buttons: u32,
    camera: [w::RecoilAxis; 3],
    kick_angles: [f32; 3],
    old_angles: [i32; 3],
    rng: u32,
    time: i32,
    /// CODV's reload while sprinting (off: MW2's sprint waits for the reload, CODV's 0.1.0 rule).
    pub reload_sprint: bool,
}

fn short(a: f32) -> i32 {
    ((a * (65536.0 / 360.0)) as i32) & 0xffff
}

impl Gunner {
    /// Carrying `loadout` (weapon ids, the first in hand), full ammo.
    pub fn new(arsenal: &Arsenal, loadout: &[u32]) -> Gunner {
        let loadout: Vec<u32> = loadout.iter().copied().filter(|&id| arsenal.gun(id).is_some()).collect();
        let ammo = loadout
            .iter()
            .map(|&id| {
                let f = &arsenal.gun(id).unwrap().facts;
                let (clip, _, stock) = w::spawn_clip_stock(f, 0);
                (id, clip, stock)
            })
            .collect();
        let first = loadout.first().copied().unwrap_or(0);
        let mut hands = [WeaponHandState::default(); 2];
        if let Some(g) = arsenal.gun(first) {
            hands[0] = w::spawn_weapon_hand(first, &g.facts, true);
        }
        Gunner {
            hands,
            frag: None,
            off_hand_index: 0,
            grenade_time_left: 0,
            reserve_scale: 1.0,
            moves: (0, 0),
            loadout,
            ammo,
            used: vec![first],
            want: first,
            old_buttons: 0,
            camera: Default::default(),
            kick_angles: [0.0; 3],
            old_angles: [0; 3],
            rng: 0x2545_F491,
            time: 1_000_000,
            reload_sprint: false,
        }
    }

    /// Spare ammunition times `scale` (refills every carried weapon).
    pub fn with_reserve_scale(mut self, arsenal: &Arsenal, scale: f32) -> Gunner {
        self.reserve_scale = scale.clamp(0.0, 100.0);
        for (id, _, stock) in &mut self.ammo {
            let (_, _, s) = w::spawn_clip_stock(&arsenal.gun(*id).unwrap().facts, 0);
            *stock = (s as f32 * self.reserve_scale).round() as i32;
        }
        self
    }

    /// Carrying `count` of the thrown grenade `name` (MW2's frag button).
    pub fn with_frags(mut self, arsenal: &Arsenal, name: &str, count: i32) -> Gunner {
        self.frag = arsenal.offhand_id(name).filter(|_| count > 0).map(|id| (id, count));
        self
    }

    /// The usercmd move of the coming tick (-127..127 forward, right).
    pub fn set_moves(&mut self, forward: i32, right: i32) {
        self.moves = (forward.clamp(-127, 127) as i8, right.clamp(-127, 127) as i8);
    }

    pub fn frags(&self) -> i32 {
        self.frag.map_or(0, |f| f.1)
    }

    pub fn loadout(&self) -> &[u32] {
        &self.loadout
    }
    pub fn held(&self) -> u32 {
        self.hands[0].weapon
    }
    /// Asks for loadout slot `slot` (the switch takes MW2's drop and raise times).
    pub fn select(&mut self, slot: usize) {
        if let Some(&id) = self.loadout.get(slot) {
            self.want = id;
        }
    }
    /// Asks for the next weapon in the loadout.
    pub fn cycle(&mut self) {
        if let Some(i) = self.loadout.iter().position(|&id| id == self.want) {
            self.want = self.loadout[(i + 1) % self.loadout.len()];
        }
    }
    /// MW2's Scavenger pickup (_weapons.gsc handleScavengerBagPickup): every carried gun's
    /// spare rounds grow by one magazine (up to its spare-round limit with the reserve scale)
    /// and the frags by one, up to `max_frags`.
    pub fn scavenge(&mut self, arsenal: &Arsenal, max_frags: i32) {
        for (id, _, stock) in &mut self.ammo {
            let f = &arsenal.gun(*id).unwrap().facts;
            let (_, _, spawn) = w::spawn_clip_stock(f, 0);
            let limit = ((f.max_ammo.max(spawn).max(f.clip_size)) as f32 * self.reserve_scale.max(1.0)).round() as i32;
            *stock = (*stock + f.clip_size.max(0)).min(limit.max(*stock));
        }
        let held = self.hands[0].weapon;
        if let Some(&(_, _, s)) = self.ammo.iter().find(|a| a.0 == held) {
            self.hands[0].stock = s;
        }
        if let Some((_, n)) = self.frag.as_mut() {
            *n = (*n + 1).min(max_frags.max(*n));
        }
    }

    /// Each carried weapon's (id, clip, stock) and the frags carried.
    pub fn ammo_state(&self) -> (Vec<(u32, i32, i32)>, i32) {
        (self.ammo.clone(), self.frags())
    }

    /// Puts back what `ammo_state` saved (the weapons it shares with this loadout).
    pub fn restore_ammo(&mut self, ammo: &[(u32, i32, i32)], frags: i32) {
        for (id, clip, stock) in &mut self.ammo {
            if let Some(&(_, c, s)) = ammo.iter().find(|a| a.0 == *id) {
                *clip = c;
                *stock = s;
            }
        }
        let held = self.hands[0].weapon;
        if let Some(&(_, c, s)) = self.ammo.iter().find(|a| a.0 == held) {
            self.hands[0].clip = c;
            self.hands[0].stock = s;
        }
        if let Some((_, n)) = self.frag.as_mut() {
            *n = frags.max(0);
        }
    }

    /// Refills every carried weapon (a new life, an ammo pickup).
    pub fn refill(&mut self, arsenal: &Arsenal) {
        for (id, clip, stock) in &mut self.ammo {
            let (c, _, s) = w::spawn_clip_stock(&arsenal.gun(*id).unwrap().facts, 0);
            *clip = (*clip).max(c);
            *stock = ((s as f32 * self.reserve_scale).round() as i32).max(*stock);
        }
        let held = self.hands[0].weapon;
        if let Some(&(_, c, s)) = self.ammo.iter().find(|a| a.0 == held) {
            self.hands[0].clip = c;
            self.hands[0].stock = s;
        }
    }

    fn ammo_of(&self, id: u32) -> (i32, i32) {
        self.ammo.iter().find(|a| a.0 == id).map_or((0, 0), |a| (a.1, a.2))
    }
    fn set_ammo(&mut self, id: u32, clip: i32, stock: i32) {
        if let Some(a) = self.ammo.iter_mut().find(|a| a.0 == id) {
            a.1 = clip;
            a.2 = stock;
        }
    }

    fn unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = if x == 0 { 1 } else { x };
        x as f32 / u32::MAX as f32
    }

    /// Before the player's pmove: the held weapon's ADS and movement facts
    /// into its context (iw4L sim `pmove_context`).
    pub fn arm(&self, player: &mut Mw2Player, arsenal: &Arsenal) {
        let Some(g) = arsenal.gun(self.hands[0].weapon) else {
            player.weapon = WeaponMoveFacts::default();
            return;
        };
        let mut moves = g.moves;
        let mut allow = g.facts.ads_allow_facts();
        allow.clip_index = w::clip_table_key(allow.clip_index, self.hands[0].weapon);
        moves.ads_allowed = w::is_ads_allowed(&player.ps, &allow, &player.ps.ammoclip, 0);
        let reloading = self.reloading();
        moves.hold_sprint = reloading && !self.reload_sprint;
        moves.reload_sprint = reloading && self.reload_sprint;
        if moves.reload_sprint && player.ps.pm_flags & pm_flags::SPRINTING != 0 {
            moves.move_speed_scale *= RELOAD_SPRINT_SPEED;
        }
        player.weapon = moves;
    }

    /// The held weapon is in its reload (start, loop, end).
    pub fn reloading(&self) -> bool {
        w::WeaponState::from_i32(self.hands[0].weaponstate).is_ok_and(|s| s.is_reload_family())
    }

    /// After the player's pmove: one tick of the held weapon with the
    /// player's buttons (retail bits) and state. `msec` as pmove's.
    pub fn step(&mut self, player: &mut Mw2Player, arsenal: &Arsenal, msec: u32, buttons: u32) -> GunFrame {
        let msec = msec.min(200) as i32;
        self.time = self.time.wrapping_add(msec);
        let dt = msec as f32 / 1000.0;
        let held = self.hands[0].weapon;
        let mut out = GunFrame { weapon: held, ..GunFrame::default() };
        let Some(gun) = arsenal.gun(held).cloned() else {
            self.advance_kick(dt);
            return out;
        };
        let facts = gun.facts;
        let ps = &mut player.ps;
        // Aim spread from this frame's motion (sim/combat.rs).
        let angles = [short(ps.viewangles[0]), short(ps.viewangles[1]), 0];
        let mut state = w::AimSpreadState {
            aim_spread_scale: ps.aim_spread_scale,
            spread_override: ps.spread_override,
            spread_override_state: ps.spread_override_state,
        };
        let motion = w::AimSpreadMotion {
            frametime: dt,
            cmd_angles: angles,
            old_angles: self.old_angles,
            forwardmove: self.moves.0,
            rightmove: self.moves.1,
            velocity_xy: [ps.velocity[0], ps.velocity[1]],
            speed: ps.speed,
            move_speed_threshold: w::AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT,
        };
        w::adjust_aim_spread_scale(
            &mut state,
            &facts.spread_facts(),
            &facts.aim_spread_decay_facts(),
            ps.ground_entity_num,
            ps.pm_type,
            ps.e_flags,
            ps.f_weapon_pos_frac,
            &motion,
        );
        ps.aim_spread_scale = state.aim_spread_scale;
        ps.spread_override_state = state.spread_override_state;
        self.old_angles = angles;

        // The weapon tick (sim/combat.rs's WeaponCmd). Reloading while sprinting: the weapon is told
        // the player is not sprinting (weapon_iw4 would drop the reload for the sprint-in); the sprint
        // comes back to the playerstate after the tick, so the gun goes to its sprint once the reload ends.
        let sprint_hidden = self.reload_sprint && self.reloading() && ps.pm_flags & pm_flags::SPRINTING != 0;
        let (clip, stock) = self.ammo_of(held);
        self.hands[0].clip = clip;
        self.hands[0].stock = stock;
        let target = if self.want != 0 { self.want } else { held };
        let target_facts = arsenal.gun(target).map_or(facts, |g| g.facts);
        let mut cmd = w::WeaponCmd {
            msec,
            server_time: self.time,
            buttons,
            old_buttons: self.old_buttons,
            cmd_weapon: target as u16,
            pm_flags: if sprint_hidden { ps.pm_flags & !pm_flags::SPRINTING } else { ps.pm_flags },
            weap_flags: ps.weap_flags,
            pm_type: ps.pm_type,
            e_flags: ps.e_flags,
            last_weapon_hand: 0,
            f_weapon_pos_frac: ps.f_weapon_pos_frac,
            is_in_air: ps.ground_entity_num == playerstate_iw4::ENTITYNUM_NONE,
            cmd_weapon_owned: self.loadout.contains(&target),
            switch_raise_time_ms: target_facts.raise_time_ms,
            switch_quick_raise_time_ms: target_facts.quick_raise_time_ms,
            switch_first_raise_time_ms: if self.used.contains(&target) {
                0
            } else {
                target_facts.first_raise_time_ms.max(0)
            },
            ..w::WeaponCmd::default()
        };
        // The offhand inventory: the frag on MW2's frag button (sim/combat.rs's OffhandCmd).
        cmd.offhand.held_quick_drop_time_ms = facts.quick_drop_time_ms;
        cmd.offhand.off_hand_index = self.off_hand_index;
        cmd.offhand.grenade_time_left = self.grenade_time_left;
        if let Some((id, count)) = self.frag
            && let Some((g, t)) = arsenal.offhand(id)
        {
            cmd.offhand.offhand_primary = t.offhand_class;
            cmd.offhand.inventory[0] = w::OffhandInvRow {
                weapon: id,
                offhand_class: t.offhand_class,
                ammo: count,
                hold_fire_time_ms: t.hold_fire_time_ms,
                fire_time_ms: g.facts.fire_time_ms,
                fire_delay_ms: g.facts.fire_delay_ms,
                fuse_time_ms: t.fuse_time_ms,
                cook_off_hold: t.cook_off_hold,
                offhand_hold_is_cancelable: Some(g.facts.offhand_hold_is_cancelable.unwrap_or(false)),
                weap_type: g.facts.weap_type,
                has_detonator: false,
                detonate_delay_ms: 0,
                detonate_time_ms: 0,
            };
        }
        let events = w::weapon_hands(&mut self.hands, &facts, &mut cmd, 0);
        self.off_hand_index = cmd.offhand.off_hand_index;
        self.grenade_time_left = cmd.offhand.grenade_time_left;
        // The playerstate keeps them too (the HUD's cooking reticle reads them).
        ps.off_hand_index = self.off_hand_index;
        ps.grenade_time_left = self.grenade_time_left;
        if let Some((id, count)) = self.frag.as_mut() {
            *count = cmd.offhand.inventory.iter().find(|r| r.weapon == *id).map_or(*count, |r| r.ammo);
        }
        out.offhand_view = (cmd.weap_flags & playerstate_iw4::weap_flags::OFFHAND_VIEW != 0 && self.off_hand_index > 0)
            .then_some(self.off_hand_index as u32);
        self.old_buttons = buttons;
        let hand = self.hands[0];
        ps.weapon = hand.weapon;
        ps.weaponstate_primary = hand.weaponstate;
        ps.weapon_time = hand.weapon_time;
        ps.weapon_delay = hand.weapon_delay;
        ps.weap_anim = hand.weap_anim;
        ps.weapon_restrict_kick_time = hand.weapon_restrict_kick_time;
        ps.pm_flags = cmd.pm_flags | if sprint_hidden { pm_flags::SPRINTING } else { 0 };
        ps.weap_flags = cmd.weap_flags;
        // Ammo: the hand's rounds belong to the weapon it held when the tick began.
        self.set_ammo(held, hand.clip, hand.stock);
        if hand.weapon != held {
            if !self.used.contains(&hand.weapon) {
                self.used.push(hand.weapon);
            }
            let (c, s) = self.ammo_of(hand.weapon);
            self.hands[0].clip = c;
            self.hands[0].stock = s;
        }
        let clip_key = w::clip_table_key(facts.clip_index, held);
        let ammo_key = w::ammo_table_key(facts.ammo_index, held);
        let _ = w::set_clip_for_hand(&mut ps.ammoclip, clip_key, 0, hand.clip);
        let _ = w::set_ammo_not_in_clip(&mut ps.ammo, ammo_key, hand.stock);

        let ads = ps.f_weapon_pos_frac.clamp(0.0, 1.0);
        let view_height = ps.view_height_current;
        for (h, event) in events.iter().flatten() {
            if *h != 0 {
                continue;
            }
            match *event {
                w::WeaponTickEvent::ShotAccepted { .. } => {
                    out.fired = true;
                    w::add_aim_spread_fire(&mut ps.aim_spread_scale, ps.f_weapon_pos_frac, facts.hip_spread_fire_add);
                    let cone = w::get_spread_for_weapon(
                        view_height,
                        ps.spread_override,
                        w::SpreadOverrideState::from_i32(ps.spread_override_state),
                        &facts.spread_facts(),
                        1.0,
                    );
                    let spread = w::fire_weapon_spread_degrees(cone, facts.ads_spread, ads, ps.aim_spread_scale);
                    let reduce = hand.weapon_restrict_kick_time > 0;
                    out.gun_kick = Some(self.seed_kick(&gun.kick, ads, ps.weap_flags, reduce));
                    for _ in 0..facts.shots_per_fire.max(1) {
                        out.shots.push(self.spread_shot(held, spread));
                    }
                }
                w::WeaponTickEvent::EmptyClick => out.empty_click = true,
                w::WeaponTickEvent::ReloadStarted => out.reload_started = true,
                w::WeaponTickEvent::ReloadEnded | w::WeaponTickEvent::ReloadAmmoAdded { .. } => out.reload_ended = true,
                w::WeaponTickEvent::RaiseStarted => out.raise_started = true,
                w::WeaponTickEvent::PutawayStarted => out.putaway_started = true,
                w::WeaponTickEvent::MeleeFired => out.melee_fired = true,
                w::WeaponTickEvent::OffhandPrepare { .. } => out.offhand_prepare = true,
                w::WeaponTickEvent::OffhandUsed { weapon, remaining_fuse_ms } => {
                    out.thrown = Some((weapon, remaining_fuse_ms));
                }
                w::WeaponTickEvent::OffhandCookedOff { weapon } => out.cooked_off = Some(weapon),
                _ => {}
            }
        }
        out.melee_started = cmd.melee_started.is_some();
        self.advance_kick(dt);
        let ps = &player.ps;
        let held_now = self.hands[0].weapon;
        let facts_now = arsenal.gun(held_now).map_or(facts, |g| g.facts);
        let cone = w::get_spread_for_weapon(
            ps.view_height_current,
            ps.spread_override,
            w::SpreadOverrideState::from_i32(ps.spread_override_state),
            &facts_now.spread_facts(),
            1.0,
        );
        out.weapon = held_now;
        out.spread = w::fire_weapon_spread_degrees(cone, facts_now.ads_spread, ads, ps.aim_spread_scale);
        out.ads = ads;
        out.clip = self.hands[0].clip;
        out.stock = self.hands[0].stock;
        out.weaponstate = self.hands[0].weaponstate;
        out.weapon_time = self.hands[0].weapon_time;
        out.kick = [-self.kick_angles[0], -self.kick_angles[1]];
        out.weap_anim = self.hands[0].weap_anim;
        out.frags = self.frags();
        out
    }

    /// A pellet within `spread` degrees of the view centre, uniformly in
    /// radius and angle as iw4L's `spread_direction_on_plane`.
    fn spread_shot(&mut self, weapon: u32, spread: f32) -> Shot {
        let radius = self.unit();
        let theta = self.unit() * core::f32::consts::TAU;
        let lateral = spread.max(0.0).to_radians().tan() * radius;
        let (right, up) = (lateral * theta.cos(), lateral * theta.sin());
        // The kicked view is where the player sees the crosshair.
        let yaw = right.atan().to_degrees() - self.kick_angles[1];
        let pitch = up.atan2((1.0 + right * right).sqrt()).to_degrees() - self.kick_angles[0];
        Shot { weapon, yaw, pitch }
    }

    /// Seeds the camera kick; returns the gun (viewmodel) kick peak, degrees.
    fn seed_kick(&mut self, k: &Kick, ads: f32, weap_flags: u32, reduce: bool) -> [f32; 2] {
        let view = w::fire_recoil_view_range(ads, k.hip_view, k.ads_view);
        let gun = w::fire_recoil_gun_range(ads, k.hip_gun, k.ads_gun);
        let t = w::recoil_ads_weight(ads);
        let scales = w::FireRecoilPsScales {
            reduce_window_active: reduce,
            reduced_percent: k.hip_reduced_percent * (1.0 - t) + k.ads_reduced_percent * t,
            weap_flags,
            recoil_scale: 100,
        };
        let samples = [self.unit(), self.unit(), self.unit(), self.unit()];
        let impulse = w::weapon_fire_recoil(view, gun, scales, samples);
        for (axis, amplitude) in self.camera.iter_mut().zip(impulse.camera_peak) {
            axis.add_peak(f64::from(amplitude), w::CAMERA_PEAK_SECONDS);
        }
        impulse.model_peak
    }

    fn advance_kick(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        for (axis, out) in self.camera.iter_mut().zip(&mut self.kick_angles) {
            axis.advance(f64::from(dt), w::CAMERA_PEAK_SECONDS);
            *out = axis.bounded(w::CAMERA_LIMIT_DEGREES) as f32;
        }
    }
}

#[cfg(test)]
mod tests;
