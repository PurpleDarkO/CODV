//! Modern Warfare 2's third-person character animation for the player:
//! the player animation script picks the legs and torso animations from the
//! move type, stance, weapon and events, the multiplayer animation tree
//! blends them, and the soldier skeleton is posed with MW2's player
//! controller (torso pitch from the view, prone). `retarget` puts the pose on
//! the GTA ped.
//!
//! Ported from iw4L (Apache-2.0, NOTICE) without its ECS and network layers:
//!
//! * `sim::player_anim_script` (`Script`: the conditions from the player
//!   state, item matching, `play_anim` with timers and restart toggles) and
//!   the calls `sim::step` / `sim::combat` make after pmove and weapon events;
//! * `render_anim::anim::remote_animation::advance_remote_tree` (`Body`: the
//!   tree rebound to the chosen leaves, goal weights and rates through
//!   `xmodel_runtime::apply_player_anim_goals` / `apply_player_anim_rates`);
//! * `render_anim::anim::remote_body::pose_remote_dobj` (the pose with
//!   `xmodel_runtime::apply_player_controller`).
use crate::player::Mw2Player;
use crate::viewmodel::{clip, pose_src};
use anim_iw4::{
    ANIM_COND_AKIMBO, ANIM_COND_FASTMANTLE, ANIM_COND_FIRING, ANIM_COND_MOUNTED, ANIM_COND_MOVETYPE, ANIM_COND_PERK,
    ANIM_COND_PLAYERANIMTYPE, ANIM_COND_PLAYERANIMTYPEPRIMARY, ANIM_COND_RIOTSHIELDNEXT, ANIM_COND_STRAFING,
    ANIM_COND_WEAPON_POSITION, ANIM_COND_WEAPONCLASS, ANIM_ET_DEATH, PLAYER_ANIM_INDEX_MASK, PlayerAnimValue,
    anim_weapon_position_from_pm_flags, random,
};
use codv_mw2data::playeranim::{self, CompiledAnimTreeDefinition, ParsedAnimItem, ParsedPlayerAnimScript};
use codv_mw2data::vm;
use glam::Mat4;
use playerstate_iw4::PlayerState;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use xmodel_runtime::{
    AnimClip, ClientAnimSample, DObj, DObjPoseRequest, PlayerBodyBranches, PlayerControllerInput, XAnimNodeDefinition,
    XAnimNodeId, XAnimNodeKind, XAnimTreeDefinition, XAnimTreeRuntime,
};

pub mod retarget;

/// Player animation script events (anim_iw4 ANIM_ET_*).
pub mod event {
    pub const FIREWEAPON: u8 = anim_iw4::ANIM_ET_FIREWEAPON;
    pub const LAND: u8 = 5;
    pub const DROPWEAPON: u8 = anim_iw4::ANIM_ET_DROPWEAPON;
    pub const RAISEWEAPON: u8 = anim_iw4::ANIM_ET_RAISEWEAPON;
    pub const RELOAD: u8 = anim_iw4::ANIM_ET_RELOAD;
    pub const MELEEATTACK: u8 = anim_iw4::ANIM_ET_MELEEATTACK;
    pub const KNIFE_MELEE: u8 = anim_iw4::ANIM_ET_KNIFE_MELEE;
}

/// The held weapon as the script sees it (WeaponCompleteDef
/// playerAnimType and weapClass, from the weapon table).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GunAnim {
    pub player_anim_type: i32,
    pub weap_class: i32,
}

// ---- sources ---------------------------------------------------------------

/// The imported character animation (`<data>/mw2/body/`): tree, script,
/// soldier skeleton and the tree's animations.
pub struct Sources {
    pub tree: Arc<CompiledAnimTreeDefinition>,
    pub script: Arc<ParsedPlayerAnimScript>,
    pub skeleton: vm::Model,
    pub dobj: DObj,
    branches: PlayerBodyBranches,
    anims: HashMap<u16, vm::Anim>,
    /// Command durations by tree index (iw4L session::command_duration_ms).
    durations: HashMap<u16, i32>,
}

impl Sources {
    /// Reads `dir` (multiplayer.atr, playeranim.script, skeleton.cvm, anims/).
    pub fn load(dir: &Path) -> Result<Sources, String> {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
        let skeleton = vm::read_model(&read("skeleton.cvm")?).map_err(|e| format!("skeleton.cvm: {e}"))?;
        let atr = read("multiplayer.atr")?;
        let script = read("playeranim.script")?;
        let tree =
            playeranim::compile_multiplayer(Some(&atr), Some(&script)).map_err(|e| format!("multiplayer.atr: {e}"))?;
        let mut anims = HashMap::new();
        for (i, node) in tree.nodes().iter().enumerate() {
            if node.child_count != 0 {
                continue;
            }
            let path = dir.join("anims").join(format!("{}.cva", vm::file_stem(&node.name)));
            if let Ok(bytes) = std::fs::read(&path) {
                anims.insert(i as u16, vm::read_anim(&bytes).map_err(|e| format!("{}: {e}", path.display()))?);
            }
        }
        Sources::new(skeleton, &atr, &script, anims)
    }

    /// From the parts (tests feed them from memory). `anims` by tree index.
    pub fn new(
        skeleton: vm::Model,
        atr: &[u8],
        script: &[u8],
        anims: HashMap<u16, vm::Anim>,
    ) -> Result<Sources, String> {
        let (tree, script) = playeranim::compile(atr, script)?;
        let src = pose_src(&skeleton);
        let dobj = DObj::build(&[(&src, None)]).map_err(|e| format!("{}: {e:?}", skeleton.name))?;
        let branch =
            |name: &str| tree.index_of(name).map(XAnimNodeId).ok_or_else(|| format!("tree has no {name} branch"));
        let branches = PlayerBodyBranches { legs: branch("legs")?, torso: branch("torso")? };
        let mut durations = HashMap::new();
        for items in script.slots.iter().map(|s| &s.2).chain(script.events.iter().map(|e| &e.1)) {
            for c in items.iter().flat_map(|i| &i.commands) {
                let ms = match c.duration_ms {
                    Some(ms) if ms > 0 => ms,
                    Some(_) => 500,
                    None => anims
                        .get(&c.anim_index)
                        .filter(|a| a.framerate > 0.0)
                        .map(|a| (f32::from(a.numframes) / a.framerate * 1000.0) as i32)
                        .filter(|&ms| ms > 0)
                        .unwrap_or(500),
                };
                durations.insert(c.anim_index, ms);
            }
        }
        Ok(Sources { tree, script, skeleton, dobj, branches, anims, durations })
    }

    pub fn anim_count(&self) -> usize {
        self.anims.len()
    }
}

// ---- the script (sim::player_anim_script) ------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Conditions {
    written: u32,
    value: [u64; 18],
}

impl Conditions {
    fn set_value(&mut self, cond: u8, v: u32) {
        if (cond as usize) < 18 {
            self.written |= 1 << cond;
            self.value[cond as usize] = u64::from(v);
        }
    }
    fn set_bit(&mut self, cond: u8, bit: u8) {
        if (cond as usize) < 18 && bit < 64 {
            self.written |= 1 << cond;
            self.value[cond as usize] |= 1u64 << bit;
        }
    }
}

fn conditions(ps: &PlayerState, gun: Option<GunAnim>, movetype: Option<u8>, strafing: u8, buttons: u32) -> Conditions {
    let mut c = Conditions::default();
    c.set_value(ANIM_COND_MOUNTED, u32::from(ps.e_flags & 0xc00 != 0));
    c.set_value(ANIM_COND_FIRING, u32::from(buttons & playerstate_iw4::buttons::ATTACK != 0));
    c.set_value(ANIM_COND_FASTMANTLE, u32::from(ps.mantle_flags & playerstate_iw4::mantle_flags::FAST_MANTLE != 0));
    if let Some(g) = gun {
        let bit = |v: i32| u8::try_from(v).unwrap_or(u8::MAX);
        c.set_bit(ANIM_COND_PLAYERANIMTYPE, bit(g.player_anim_type));
        c.set_bit(ANIM_COND_WEAPONCLASS, bit(g.weap_class));
        c.set_bit(ANIM_COND_PLAYERANIMTYPEPRIMARY, bit(g.player_anim_type));
    }
    c.set_value(ANIM_COND_AKIMBO, u32::from(ps.last_weapon_hand == 1));
    if let Some(m) = movetype {
        c.set_bit(ANIM_COND_MOVETYPE, m);
    }
    c.set_value(ANIM_COND_STRAFING, u32::from(strafing));
    c.set_value(ANIM_COND_PERK, 0);
    c.set_value(ANIM_COND_RIOTSHIELDNEXT, 0);
    c.set_value(ANIM_COND_WEAPON_POSITION, anim_weapon_position_from_pm_flags(ps.pm_flags));
    c
}

fn strafing(ps: &PlayerState, forward: i8, right: i8) -> u8 {
    let (forward, right) = (f32::from(forward), f32::from(right));
    let len = (forward * forward + right * right).sqrt();
    let speed_sq = ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1];
    if len == 0.0 || speed_sq <= 100.0 || forward.abs() > len * 0.5 {
        0
    } else if right > 0.0 {
        2
    } else {
        1
    }
}

fn item_matches(item: &ParsedAnimItem, c: &Conditions) -> bool {
    item.conditions.iter().all(|cond| {
        let idx = cond.index as usize;
        if idx >= 18 {
            return false;
        }
        if cond.bitflags {
            cond.bits & c.value[idx] != 0
        } else {
            c.written & (1 << cond.index) != 0 && c.value[idx] as i32 == cond.value
        }
    })
}

fn matching<'a>(items: &'a [ParsedAnimItem], c: &Conditions) -> Option<&'a ParsedAnimItem> {
    items.iter().find(|i| !i.skip && item_matches(i, c))
}

fn play_anim(
    ps: &mut PlayerState,
    anim: u16,
    legs: bool,
    duration: i32,
    set_timer: bool,
    is_continue: bool,
    force: bool,
) {
    let (timer, raw) = if legs { (ps.legs_timer, ps.legs_anim as u16) } else { (ps.torso_timer, ps.torso_anim as u16) };
    if timer >= 50 && !force {
        return;
    }
    let same = (raw & PLAYER_ANIM_INDEX_MASK) == (anim & PLAYER_ANIM_INDEX_MASK);
    if is_continue && same {
        if set_timer {
            if legs {
                ps.legs_timer = duration;
            } else {
                ps.torso_timer = duration;
            }
        }
        return;
    }
    let written = PlayerAnimValue::restarting(anim, raw).raw() as i32;
    if legs {
        ps.legs_anim = written;
        if set_timer {
            ps.legs_timer = duration;
        }
    } else {
        ps.torso_anim = written;
        if set_timer {
            ps.torso_timer = duration;
        }
    }
}

fn execute(
    ps: &mut PlayerState,
    body_part: u8,
    anim: u16,
    duration_ms: i32,
    set_timer: bool,
    is_continue: bool,
    force: bool,
) {
    let duration = duration_ms.saturating_add(50);
    if body_part == 1 || body_part == 3 {
        play_anim(ps, anim, true, duration, set_timer, is_continue, force);
    }
    if body_part == 2 {
        play_anim(ps, anim, false, duration, set_timer, is_continue, force);
    } else if body_part == 3 {
        play_anim(ps, 0, false, duration, set_timer, is_continue, force);
    }
}

// ---- the body ---------------------------------------------------------------

/// The player's character animation state.
pub struct Body {
    src: Arc<Sources>,
    runtime: Option<XAnimTreeRuntime>,
    clips: HashMap<u16, Arc<AnimClip>>,
    legs: u16,
    torso: u16,
    legs_restart: bool,
    torso_restart: bool,
    legs_rate: ClientAnimSample,
    torso_rate: ClientAnimSample,
    last_movetype: Option<u8>,
    last_strafing: u8,
    seed: u32,
    time_ms: i32,
    /// The first failure, kept for the log (the body then holds its last pose).
    pub error: Option<String>,
}

impl Body {
    pub fn new(src: Arc<Sources>) -> Body {
        Body {
            src,
            runtime: None,
            clips: HashMap::new(),
            legs: 0,
            torso: 0,
            legs_restart: false,
            torso_restart: false,
            legs_rate: ClientAnimSample::default(),
            torso_rate: ClientAnimSample::default(),
            last_movetype: None,
            last_strafing: 0,
            seed: 0x1234_5678,
            time_ms: 0,
            error: None,
        }
    }

    pub fn sources(&self) -> &Arc<Sources> {
        &self.src
    }

    /// The legs and torso animation names now (debug overlay).
    pub fn playing(&self) -> (&str, &str) {
        let name = |i: u16| if i == 0 { "" } else { self.src.tree.node(i).map_or("", |n| n.name.as_str()) };
        (name(self.legs), name(self.torso))
    }

    fn duration(&self, anim: u16) -> i32 {
        self.src.durations.get(&anim).copied().unwrap_or(500)
    }

    /// A player animation event (sim::combat::apply_player_anim_event).
    pub fn event(&mut self, player: &mut Mw2Player, gun: Option<GunAnim>, event: u8) {
        let force = !matches!(event, event::RAISEWEAPON | 11..=16);
        self.event_forced(player, gun, event, force);
    }

    fn event_forced(&mut self, player: &mut Mw2Player, gun: Option<GunAnim>, event: u8, force: bool) {
        let ps = &mut player.ps;
        if event != ANIM_ET_DEATH && ps.pm_type >= 8 {
            return;
        }
        let movetype = self.last_movetype.unwrap_or(anim_iw4::ANIM_MT_IDLE);
        let c = conditions(ps, gun, Some(movetype), self.last_strafing, player.anim.buttons);
        let Some(items) = self.src.script.event(event) else { return };
        let Some(item) = matching(items, &c) else { return };
        if item.commands.is_empty() {
            return;
        }
        let pick = (random(&mut self.seed) as usize) % item.commands.len();
        let cmd = &item.commands[pick];
        let duration = self.duration(cmd.anim_index);
        execute(ps, cmd.body_part, cmd.anim_index, duration, true, false, force);
    }

    /// After pmove (sim::step): the stance, jump and landing events, then the
    /// movement animation for the move type.
    pub fn after_pmove(&mut self, player: &mut Mw2Player, gun: Option<GunAnim>) {
        let step = player.anim;
        if step.reset_torso {
            play_anim(&mut player.ps, 0, false, 0, false, true, true);
            player.ps.torso_timer = 0;
        }
        if let Some(e) = step.stance_event {
            self.event(player, gun, e);
        }
        for (e, force) in step.jumps.into_iter().flatten() {
            self.event_forced(player, gun, e, force);
        }
        if step.landing {
            self.event(player, gun, event::LAND);
        }
        let Some(movetype) = step.movetype else { return };
        if player.ps.pm_type >= 8 {
            return;
        }
        let strafe = strafing(&player.ps, step.forward, step.right);
        let c = conditions(&player.ps, gun, self.last_movetype, strafe, step.buttons);
        let script = Arc::clone(&self.src.script);
        let pick = |m: u8| script.slot(0, m).and_then(|items| matching(items, &c)).map(|i| (m, i));
        let Some((selected, item)) = pick(movetype).or_else(|| (movetype != 1).then(|| pick(1)).flatten()) else {
            return;
        };
        if let Some(cmd) = item.commands.first() {
            let duration = self.duration(cmd.anim_index);
            execute(&mut player.ps, cmd.body_part, cmd.anim_index, duration, false, true, step.force_movement);
            self.last_movetype = Some(selected);
            self.last_strafing = strafe;
        }
    }

    fn leaf(&mut self, index: u16) -> Result<Arc<AnimClip>, String> {
        if let Some(c) = self.clips.get(&index) {
            return Ok(Arc::clone(c));
        }
        let a = self.src.anims.get(&index).ok_or_else(|| {
            format!("animation {} not imported", self.src.tree.node(index).map_or("?", |n| n.name.as_str()))
        })?;
        let c = Arc::new(clip(a)?);
        self.clips.insert(index, Arc::clone(&c));
        Ok(c)
    }

    fn definition(&self, bound: &HashMap<u16, Arc<AnimClip>>) -> Result<Arc<XAnimTreeDefinition>, String> {
        let nodes = self
            .src
            .tree
            .nodes()
            .iter()
            .enumerate()
            .map(|(i, n)| XAnimNodeDefinition {
                parent: n.parent.map(XAnimNodeId),
                kind: if n.child_count == 0 {
                    match bound.get(&(i as u16)) {
                        Some(c) => XAnimNodeKind::Leaf { clip: Arc::clone(c), parts: None },
                        None => XAnimNodeKind::Blend,
                    }
                } else if n.flags & anim_iw4::ANIMFLAG_ADDITIVE != 0 {
                    XAnimNodeKind::Additive
                } else {
                    XAnimNodeKind::Blend
                },
            })
            .collect();
        XAnimTreeDefinition::new(nodes).map(Arc::new).map_err(|e| e.to_string())
    }

    /// Advances the tree to the player state's legs and torso animations
    /// (advance_remote_tree for the local player).
    pub fn advance(&mut self, player: &Mw2Player, dt: f32) -> Result<(), String> {
        let legs = PlayerAnimValue::from_raw((player.ps.legs_anim as u16) & anim_iw4::PLAYER_ANIM_RAW_MASK)
            .unwrap_or_default();
        let torso = PlayerAnimValue::from_raw((player.ps.torso_anim as u16) & anim_iw4::PLAYER_ANIM_RAW_MASK)
            .unwrap_or_default();
        let (legs_index, torso_index) = (legs.effective_index(), torso.effective_index());
        if legs_index == 0 {
            return Ok(()); // nothing chosen yet: the bind pose
        }
        let (legs_restart, torso_restart) = (legs.restart_toggle(), torso.restart_toggle());
        self.time_ms = self.time_ms.wrapping_add((dt * 1000.0) as i32);
        let unchanged = self.runtime.is_some()
            && self.legs == legs_index
            && self.torso == torso_index
            && self.legs_restart == legs_restart
            && self.torso_restart == torso_restart;
        if !unchanged {
            let restart_legs = self.runtime.is_some() && self.legs_restart != legs_restart;
            let restart_torso = self.runtime.is_some() && self.torso_restart != torso_restart;
            let clip = self.leaf(legs_index)?;
            let (legs_for_tree, torso_clip) = if torso_index == 0 {
                (Arc::clone(&clip), None)
            } else {
                let t = self.leaf(torso_index)?;
                (Arc::new(xmodel_runtime::overlay_legs_clip(&clip, &t)), Some(t))
            };
            let (old_legs, old_torso) = (self.legs, self.torso);
            let old_legs_moving = self.legs_rate.move_speed > 0.0;
            let old_torso_moving = self.torso_rate.move_speed > 0.0;
            let mut bound: HashMap<u16, Arc<AnimClip>> = HashMap::new();
            if let Some(rt) = &self.runtime {
                for (i, st) in rt.states().iter().enumerate() {
                    if (st.weight > 0.0 || st.goal_weight > 0.0)
                        && let Some(c) = rt.leaf_clip(XAnimNodeId(i as u16))
                    {
                        bound.insert(i as u16, c);
                    }
                }
            }
            bound.insert(legs_index, Arc::clone(&legs_for_tree));
            if let Some(t) = &torso_clip {
                bound.insert(torso_index, Arc::clone(t));
            }
            let definition = self.definition(&bound)?;
            let mut rt = match self.runtime.take() {
                Some(mut rt) if rt.states().len() == definition.nodes().len() => {
                    rt.rebind(definition).map_err(|e| e.to_string())?;
                    rt
                }
                _ => XAnimTreeRuntime::new(definition),
            };
            let script = &self.src.script;
            let (lp, tp) = (script.animation_properties(legs_index), script.animation_properties(torso_index));
            self.legs_rate.move_speed = if lp.stationary { 0.0 } else { clip.move_speed() };
            self.legs_rate.ladder = lp.ladder;
            self.torso_rate.move_speed =
                if tp.stationary { 0.0 } else { torso_clip.as_ref().map_or(0.0, |c| c.move_speed()) };
            self.torso_rate.ladder = tp.ladder;
            // Locomotion keeps its phase from one looping cycle to the next.
            let phase = (old_legs != legs_index
                && old_legs_moving
                && self.legs_rate.move_speed > 0.0
                && clip.looping
                && rt.leaf_clip(XAnimNodeId(old_legs)).is_some_and(|c| c.looping))
            .then(|| rt.states()[old_legs as usize].time);
            xmodel_runtime::apply_player_anim_goals(
                &mut rt,
                self.src.branches,
                old_legs,
                old_torso,
                legs_index,
                torso_index,
                restart_legs,
                restart_torso,
                old_legs_moving,
                old_torso_moving,
                self.legs_rate.move_speed > 0.0,
                self.torso_rate.move_speed > 0.0,
                [lp.blend_ms, tp.blend_ms],
            )?;
            if let Some(time) = phase {
                let mut st = rt.states()[legs_index as usize];
                st.time = time;
                st.old_time = time;
                rt.set_state(XAnimNodeId(legs_index), st).map_err(|e| e.to_string())?;
            }
            self.runtime = Some(rt);
            self.legs = legs_index;
            self.torso = torso_index;
            self.legs_restart = legs_restart;
            self.torso_restart = torso_restart;
        }
        let rt = self.runtime.as_mut().expect("runtime set above");
        xmodel_runtime::apply_player_anim_rates(
            rt,
            &mut self.legs_rate,
            &mut self.torso_rate,
            legs_index,
            torso_index,
            player.ps.origin,
            self.time_ms,
        )?;
        rt.update(dt).map_err(|e| e.to_string())
    }

    /// One frame after pmove and the weapon: the events, the movement
    /// animation, the tree. Failures are kept in `error` (logged once).
    pub fn tick(&mut self, player: &mut Mw2Player, gun: Option<GunAnim>, events: &[u8], dt: f32) {
        self.after_pmove(player, gun);
        for &e in events {
            self.event(player, gun, e);
        }
        if let Err(e) = self.advance(player, dt)
            && self.error.is_none()
        {
            self.error = Some(e);
        }
    }

    /// The soldier skeleton posed (model space: IW4 units, feet at the
    /// origin, facing +X), with MW2's player controller.
    pub fn pose(&self, player: &Mw2Player) -> Vec<Mat4> {
        let dobj = &self.src.dobj;
        let request = self.runtime.clone().map(DObjPoseRequest::with_tree).unwrap_or_else(DObjPoseRequest::bind_pose);
        let flags = player.ps.pm_flags;
        let input = PlayerControllerInput {
            view_pitch_deg: player.ps.viewangles[0],
            prone: flags & playerstate_iw4::pm_flags::PRONE != 0,
            crouch: flags & playerstate_iw4::pm_flags::CROUCH != 0,
            lean_frac: 0.0,
        };
        xmodel_runtime::pose_dobj_with_controller(dobj, &request, Mat4::IDENTITY, |d, _, locals| {
            xmodel_runtime::apply_player_controller(d, locals, input);
        })
        .unwrap_or_else(|_| dobj.compose(&dobj.bind_locals(), Mat4::IDENTITY))
    }

    /// The skeleton's bind pose in the same space.
    pub fn bind_pose(&self) -> Vec<Mat4> {
        let dobj = &self.src.dobj;
        dobj.compose(&dobj.bind_locals(), Mat4::IDENTITY)
    }
}

#[cfg(test)]
mod tests;
