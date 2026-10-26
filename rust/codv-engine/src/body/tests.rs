//! Against the player's own import (CODV_MW2_OUT = the folder holding
//! `body/`); skipped otherwise.
use super::*;
use crate::player::{MoveInput, button};
use crate::world::BoxWorld;

const MS: u32 = 16;
const RIFLE: GunAnim = GunAnim { player_anim_type: 4, weap_class: 0 };

fn sources() -> Option<Arc<Sources>> {
    let out = std::env::var("CODV_MW2_OUT").ok()?;
    Some(Arc::new(Sources::load(&Path::new(&out).join("body")).unwrap()))
}

struct Rig {
    world: BoxWorld,
    player: Mw2Player,
    body: Body,
}

impl Rig {
    fn new(src: Arc<Sources>) -> Rig {
        Rig { world: BoxWorld::floor(0.0, 20000.0), player: Mw2Player::new([0.0, 0.0, 1.0], 0.0), body: Body::new(src) }
    }
    fn frames(&mut self, input: MoveInput, n: usize) {
        for _ in 0..n {
            self.player.step(&input, &self.world);
            self.body.tick(&mut self.player, Some(RIFLE), &[], MS as f32 / 1000.0);
            assert!(self.body.error.is_none(), "{:?}", self.body.error);
        }
    }
    fn legs(&self) -> String {
        self.body.playing().0.to_owned()
    }
    /// The feet's lowest point and the head's height, IW4 units.
    fn heights(&self) -> (f32, f32) {
        let pose = self.body.pose(&self.player);
        let d = &self.body.sources().dobj;
        let z = |n: &str| pose[d.find(n).unwrap()].w_axis.z;
        (z("j_ball_le").min(z("j_ball_ri")).min(z("j_ankle_le")), z("j_head"))
    }
}

#[test]
fn walks_runs_crouches_and_goes_prone_with_mw2s_animations() {
    let Some(src) = sources() else { return };
    assert!(src.anim_count() > 400, "{} animations", src.anim_count());
    let mut rig = Rig::new(src);
    let idle = MoveInput { msec: MS, ..Default::default() };
    rig.frames(idle, 30);
    let standing = rig.legs();
    assert!(standing.contains("stand"), "standing idle: {standing}");
    let (feet, head) = rig.heights();
    assert!(feet.abs() < 8.0 && (55.0..75.0).contains(&head), "feet {feet} head {head}");

    rig.frames(MoveInput { msec: MS, forward: 127, ..Default::default() }, 60);
    let run = rig.legs();
    assert!(run != standing && (run.contains("run") || run.contains("walk")), "moving forward: {run}");

    rig.frames(MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() }, 60);
    let sprint = rig.legs();
    assert!(sprint.contains("sprint"), "sprinting: {sprint}");

    rig.frames(MoveInput { msec: MS, buttons: button::CROUCH, ..Default::default() }, 60);
    let crouch = rig.legs();
    assert!(crouch.contains("crouch"), "crouched: {crouch}");
    let (_, head_crouch) = rig.heights();
    assert!(head_crouch < head - 15.0, "crouched head {head_crouch} vs {head}");

    rig.frames(MoveInput { msec: MS, buttons: button::PRONE, ..Default::default() }, 90);
    let prone = rig.legs();
    assert!(prone.contains("prone"), "prone: {prone}");
    let (_, head_prone) = rig.heights();
    assert!(head_prone < 25.0, "prone head {head_prone}");
    println!("legs: stand {standing} | run {run} | sprint {sprint} | crouch {crouch} | prone {prone}");
}

#[test]
fn reload_and_fire_events_play_torso_animations() {
    let Some(src) = sources() else { return };
    let mut rig = Rig::new(src);
    let idle = MoveInput { msec: MS, ..Default::default() };
    rig.frames(idle, 30);
    rig.body.event(&mut rig.player, Some(RIFLE), event::RELOAD);
    rig.frames(idle, 2);
    let torso = rig.body.playing().1.to_owned();
    assert!(torso.contains("reload"), "reloading torso: {torso}");
    println!("reload torso: {torso}");
}

/// A GTA-like rig at the real soldier's joints (GTA axes, metres, 4% taller,
/// ped origin 1 m above the feet), as `retarget::GtaBone`s.
fn gta_rig(src: &Sources) -> Vec<retarget::GtaBone> {
    use glam::{Quat, Vec3};
    let bind = {
        let d = &src.dobj;
        d.compose(&d.bind_locals(), Mat4::IDENTITY)
    };
    let names = &src.skeleton.bone_names;
    let at = |n: &str| {
        let p = bind[names.iter().position(|x| x == n).unwrap()].w_axis.truncate();
        Vec3::new(-p.y, p.x, p.z) * 0.0254 * 1.04 - Vec3::new(0.0, 0.0, 1.0)
    };
    let mut rig: Vec<(String, String, Vec3)> = vec![("SKEL_ROOT".into(), String::new(), Vec3::ZERO)];
    let mut add = |g: &str, parent: &str, p: Vec3| rig.push((g.to_owned(), parent.to_owned(), p));
    add("SKEL_Pelvis", "SKEL_ROOT", at("j_mainroot"));
    // GTA's spine hangs off the root, not the pelvis (SKEL_ROOT -> SKEL_Spine_Root -> SKEL_Spine0).
    add("SKEL_Spine_Root", "SKEL_ROOT", at("j_mainroot") + Vec3::new(0.0, 0.0, 0.02));
    add("SKEL_Spine0", "SKEL_Spine_Root", at("j_spinelower"));
    add("SKEL_Spine1", "SKEL_Spine0", (at("j_spinelower") + at("j_spineupper")) * 0.5);
    add("SKEL_Spine2", "SKEL_Spine1", at("j_spineupper"));
    add("SKEL_Spine3", "SKEL_Spine2", at("j_spine4"));
    add("SKEL_Neck_1", "SKEL_Spine3", at("j_neck"));
    add("SKEL_Head", "SKEL_Neck_1", at("j_head"));
    for (g, s) in [("L", "le"), ("R", "ri")] {
        add(&format!("SKEL_{g}_Clavicle"), "SKEL_Spine3", at(&format!("j_clavicle_{s}")));
        add(&format!("SKEL_{g}_UpperArm"), &format!("SKEL_{g}_Clavicle"), at(&format!("j_shoulder_{s}")));
        add(&format!("SKEL_{g}_Forearm"), &format!("SKEL_{g}_UpperArm"), at(&format!("j_elbow_{s}")));
        add(&format!("SKEL_{g}_Hand"), &format!("SKEL_{g}_Forearm"), at(&format!("j_wrist_{s}")));
        add(&format!("SKEL_{g}_Thigh"), "SKEL_Pelvis", at(&format!("j_hip_{s}")));
        add(&format!("SKEL_{g}_Calf"), &format!("SKEL_{g}_Thigh"), at(&format!("j_knee_{s}")));
        add(&format!("SKEL_{g}_Foot"), &format!("SKEL_{g}_Calf"), at(&format!("j_ankle_{s}")));
        add(&format!("SKEL_{g}_Toe0"), &format!("SKEL_{g}_Foot"), at(&format!("j_ball_{s}")));
        for (f, finger) in ["thumb", "index", "mid", "ring", "pinky"].iter().enumerate() {
            for k in 0..3 {
                let parent = if k == 0 { format!("SKEL_{g}_Hand") } else { format!("SKEL_{g}_Finger{f}{}", k - 1) };
                add(&format!("SKEL_{g}_Finger{f}{k}"), &parent, at(&format!("j_{finger}_{s}_{}", k + 1)));
            }
        }
    }
    let pos = |n: &str, rig: &[(String, String, Vec3)]| rig.iter().find(|r| r.0 == n).map_or(Vec3::ZERO, |r| r.2);
    rig.iter()
        .map(|(n, p, w)| retarget::GtaBone {
            name: n.clone(),
            parent: rig.iter().position(|r| &r.0 == p),
            t: *w - pos(p, &rig),
            r: Quat::IDENTITY,
            s: Vec3::ONE,
        })
        .collect()
}

#[test]
fn mw2s_animations_retarget_onto_a_gta_rig() {
    let Some(src) = sources() else { return };
    let rig_bones = gta_rig(&src);
    let mut rig = Rig::new(Arc::clone(&src));
    let rt = retarget::Retarget::new(&rig_bones, &src.skeleton.bone_names, &rig.body.bind_pose(), -1.0).unwrap();
    let bone = |n: &str| rt.bone_names().iter().position(|x| x == n).unwrap();
    let check = |rig: &Rig, what: &str| {
        let pose = rig.body.pose(&rig.player);
        let out = rt.pose(&pose).unwrap_or_else(|| panic!("{what}: not finite"));
        // No GTA bone stretches: each keeps its bind distance from its parent.
        for (i, b) in rig_bones.iter().enumerate() {
            let Some(p) = b.parent else { continue };
            let got = out[i].w_axis.truncate().distance(out[p].w_axis.truncate());
            assert!((got - b.t.length()).abs() < 0.002, "{what}: {} stretched {got} vs {}", b.name, b.t.length());
        }
        let z = |n: &str| out[bone(n)].w_axis.z;
        let lowest = z("SKEL_L_Toe0").min(z("SKEL_R_Toe0")).min(z("SKEL_L_Foot")).min(z("SKEL_R_Foot"));
        assert!((lowest + 1.0).abs() < 0.12, "{what}: lowest foot point {lowest}");
        // Hands on MW2's wrists (the gun grip).
        for (g, m) in [("SKEL_L_Hand", "j_wrist_le"), ("SKEL_R_Hand", "j_wrist_ri")] {
            let w = pose[src.dobj.find(m).unwrap()].w_axis.truncate();
            let want = glam::Vec3::new(-w.y, w.x, w.z) * 0.0254 - glam::Vec3::new(0.0, 0.0, 1.0);
            let d = out[bone(g)].w_axis.truncate().distance(want);
            assert!(d < 0.06, "{what}: {g} {d} m from MW2's wrist");
        }
        (z("SKEL_Head"), z("SKEL_Pelvis"))
    };
    let idle = MoveInput { msec: MS, ..Default::default() };
    rig.frames(idle, 30);
    let (head, pelvis) = check(&rig, "standing");
    assert!(head - pelvis > 0.5, "standing: head {head} pelvis {pelvis}");
    rig.frames(MoveInput { msec: MS, forward: 127, ..Default::default() }, 45);
    check(&rig, "running");
    rig.frames(MoveInput { msec: MS, buttons: button::CROUCH, ..Default::default() }, 60);
    let (head_c, _) = check(&rig, "crouched");
    assert!(head_c < head - 0.3, "crouched head {head_c} vs {head}");
    rig.frames(MoveInput { msec: MS, buttons: button::PRONE, ..Default::default() }, 90);
    let (head_p, _) = check(&rig, "prone");
    assert!(head_p < -0.5, "prone head {head_p}");
}

/// Writes the retargeted rig's joints for a few poses (CODV_BODY_DUMP = a
/// JSON path) for a visual check; skipped otherwise.
#[test]
fn dump_retargeted_poses() {
    let (Some(src), Ok(path)) = (sources(), std::env::var("CODV_BODY_DUMP")) else { return };
    let bones = gta_rig(&src);
    let mut rig = Rig::new(Arc::clone(&src));
    let rt = retarget::Retarget::new(&bones, &src.skeleton.bone_names, &rig.body.bind_pose(), -1.0).unwrap();
    let mut frames = Vec::new();
    let mut shot = |rig: &Rig, name: &str| {
        let out = rt.pose(&rig.body.pose(&rig.player)).unwrap();
        let joints: Vec<String> = bones
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let p = b.parent?;
                let (a, c) = (out[i].w_axis, out[p].w_axis);
                Some(format!("[{},{},{},{},{},{}]", a.x, a.y, a.z, c.x, c.y, c.z))
            })
            .collect();
        let (legs, torso) = rig.body.playing();
        frames.push(format!(
            "{{\"name\":\"{name}\",\"legs\":\"{legs}\",\"torso\":\"{torso}\",\"bones\":[{}]}}",
            joints.join(",")
        ));
    };
    let idle = MoveInput { msec: MS, ..Default::default() };
    rig.frames(idle, 30);
    shot(&rig, "parado");
    rig.frames(MoveInput { msec: MS, forward: 127, ..Default::default() }, 40);
    shot(&rig, "correndo");
    rig.frames(MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() }, 40);
    shot(&rig, "sprint");
    rig.frames(MoveInput { msec: MS, buttons: button::CROUCH, ..Default::default() }, 60);
    shot(&rig, "agachado");
    rig.frames(MoveInput { msec: MS, buttons: button::PRONE, ..Default::default() }, 90);
    shot(&rig, "deitado");
    rig.frames(idle, 90);
    rig.body.event(&mut rig.player, Some(RIFLE), event::RELOAD);
    rig.frames(idle, 40);
    shot(&rig, "recarregando");
    std::fs::write(path, format!("[{}]", frames.join(","))).unwrap();
}

#[test]
fn print_mw2_bind_axes() {
    let Some(src) = sources() else { return };
    let d = &src.dobj;
    let b = d.compose(&d.bind_locals(), Mat4::IDENTITY);
    for n in ["j_mainroot", "j_hip_le", "j_hip_ri", "j_ankle_le", "j_ball_le", "j_head", "j_wrist_le", "j_wrist_ri"] {
        println!("{n}: {:?}", b[d.find(n).unwrap()].w_axis.truncate());
    }
}
