use super::*;

/// A synthetic soldier: (MW2 joint, parent, position in IW4 units: x
/// forward, y left, z up, feet on the ground) and the GTA bone at it.
const RIG: [(&str, &str, [f32; 3], &str, &str); 24] = [
    ("tag_origin", "", [0., 0., 0.], "SKEL_ROOT", ""),
    ("j_mainroot", "tag_origin", [0., 0., 40.], "SKEL_Pelvis", "SKEL_ROOT"),
    ("j_spinelower", "j_mainroot", [0., 0., 44.], "SKEL_Spine0", "SKEL_Spine_Root"),
    ("j_spineupper", "j_spinelower", [0.5, 0., 50.], "SKEL_Spine2", "SKEL_Spine1"),
    ("j_spine4", "j_spineupper", [0.5, 0., 56.], "SKEL_Spine3", "SKEL_Spine2"),
    ("j_neck", "j_spine4", [0.8, 0., 61.], "SKEL_Neck_1", "SKEL_Spine3"),
    ("j_head", "j_neck", [1.5, 0., 65.], "SKEL_Head", "SKEL_Neck_1"),
    ("j_clavicle_le", "j_spine4", [0.5, 2., 58.], "SKEL_L_Clavicle", "SKEL_Spine3"),
    ("j_shoulder_le", "j_clavicle_le", [0., 7., 58.], "SKEL_L_UpperArm", "SKEL_L_Clavicle"),
    ("j_elbow_le", "j_shoulder_le", [2., 9., 47.], "SKEL_L_Forearm", "SKEL_L_UpperArm"),
    ("j_wrist_le", "j_elbow_le", [8., 9., 40.], "SKEL_L_Hand", "SKEL_L_Forearm"),
    ("j_clavicle_ri", "j_spine4", [0.5, -2., 58.], "SKEL_R_Clavicle", "SKEL_Spine3"),
    ("j_shoulder_ri", "j_clavicle_ri", [0., -7., 58.], "SKEL_R_UpperArm", "SKEL_R_Clavicle"),
    ("j_elbow_ri", "j_shoulder_ri", [2., -9., 47.], "SKEL_R_Forearm", "SKEL_R_UpperArm"),
    ("j_wrist_ri", "j_elbow_ri", [8., -9., 40.], "SKEL_R_Hand", "SKEL_R_Forearm"),
    ("j_hip_le", "j_mainroot", [0., 4., 38.], "SKEL_L_Thigh", "SKEL_Pelvis"),
    ("j_knee_le", "j_hip_le", [1., 4., 20.], "SKEL_L_Calf", "SKEL_L_Thigh"),
    ("j_ankle_le", "j_knee_le", [0., 4., 3.], "SKEL_L_Foot", "SKEL_L_Calf"),
    ("j_ball_le", "j_ankle_le", [5., 4., 0.5], "SKEL_L_Toe0", "SKEL_L_Foot"),
    ("j_hip_ri", "j_mainroot", [0., -4., 38.], "SKEL_R_Thigh", "SKEL_Pelvis"),
    ("j_knee_ri", "j_hip_ri", [1., -4., 20.], "SKEL_R_Calf", "SKEL_R_Thigh"),
    ("j_ankle_ri", "j_knee_ri", [0., -4., 3.], "SKEL_R_Foot", "SKEL_R_Calf"),
    ("j_ball_ri", "j_ankle_ri", [5., -4., 0.5], "SKEL_R_Toe0", "SKEL_R_Foot"),
    ("", "", [0., 0., 0.], "", ""),
];

fn mw2() -> (Vec<String>, Vec<Mat4>) {
    let rig: Vec<_> = RIG.iter().filter(|r| !r.0.is_empty()).collect();
    (
        rig.iter().map(|r| r.0.to_owned()).collect(),
        rig.iter().map(|r| Mat4::from_translation(Vec3::from(r.2))).collect(),
    )
}

/// The GTA rig at the same joints (GTA axes: x right, y forward, z up;
/// metres; ped origin `lift` above the feet), `scale` times the size, plus
/// SKEL_Spine1 between Spine0 and Spine2 and a thigh roll.
fn gta(scale: f32, lift: f32) -> Vec<GtaBone> {
    let at = |p: [f32; 3]| Vec3::new(-p[1], p[0], p[2]) * METRES_PER_UNIT * scale - Vec3::new(0.0, 0.0, lift);
    let mut world: Vec<(String, String, Vec3)> = RIG
        .iter()
        .filter(|r| !r.0.is_empty())
        .map(|r| (r.3.to_owned(), r.4.to_owned(), if r.3 == "SKEL_ROOT" { Vec3::ZERO } else { at(r.2) }))
        .collect();
    // GTA's spine hangs off the root, not the pelvis.
    world.push(("SKEL_Spine_Root".into(), "SKEL_ROOT".into(), at([0., 0., 41.])));
    let mid = (at([0., 0., 44.]) + at([0.5, 0., 50.])) * 0.5;
    world.push(("SKEL_Spine1".into(), "SKEL_Spine0".into(), mid));
    let thigh = world.iter().find(|w| w.0 == "SKEL_L_Thigh").unwrap().2;
    let knee = world.iter().find(|w| w.0 == "SKEL_L_Calf").unwrap().2;
    world.push(("RB_L_ThighRoll".into(), "SKEL_L_Thigh".into(), thigh.lerp(knee, 0.3)));
    let pos = |n: &str| world.iter().find(|w| w.0 == n).map_or(Vec3::ZERO, |w| w.2);
    world
        .iter()
        .map(|(name, parent, p)| GtaBone {
            name: name.clone(),
            parent: world.iter().position(|w| &w.0 == parent),
            t: *p - pos(parent),
            r: Quat::IDENTITY,
            s: Vec3::ONE,
        })
        .collect()
}

fn find(rt: &Retarget, name: &str) -> usize {
    rt.bone_names().iter().position(|n| n == name).unwrap()
}

#[test]
fn mw2_at_bind_gives_back_the_gta_bind() {
    let (names, bind) = mw2();
    let rig = gta(1.0, 1.0);
    let rt = Retarget::new(&rig, &names, &bind, -1.0).unwrap();
    let out = rt.pose(&bind).unwrap();
    for (i, m) in out.iter().enumerate() {
        let d = m.w_axis.truncate().distance(rt.bind()[i].w_axis.truncate());
        assert!(d < 0.005, "{} {d}", rt.bone_names()[i]);
    }
}

#[test]
fn a_turned_and_raised_soldier_moves_the_ped_rigidly() {
    let (names, bind) = mw2();
    let rig = gta(1.0, 1.0);
    let rt = Retarget::new(&rig, &names, &bind, -1.0).unwrap();
    // MW2 turns 30 degrees left (+yaw) and steps up 10 units.
    let motion = Mat4::from_translation(Vec3::new(0.0, 0.0, 10.0)) * Mat4::from_rotation_z(30f32.to_radians());
    let pose: Vec<Mat4> = bind.iter().map(|b| motion * *b).collect();
    let out = rt.pose(&pose).unwrap();
    // In GTA axes the same: +30 degrees about z, up 0.254 m.
    let expect = Mat4::from_translation(Vec3::new(0.0, 0.0, 10.0 * METRES_PER_UNIT))
        * Mat4::from_translation(Vec3::new(0.0, 0.0, -1.0))
        * Mat4::from_rotation_z(30f32.to_radians())
        * Mat4::from_translation(Vec3::new(0.0, 0.0, 1.0));
    for name in ["SKEL_Pelvis", "SKEL_Head", "SKEL_L_Hand", "SKEL_R_Foot", "SKEL_L_Toe0"] {
        let i = find(&rt, name);
        let want = expect.transform_point3(rt.bind()[i].w_axis.truncate());
        let d = out[i].w_axis.truncate().distance(want);
        assert!(d < 0.006, "{name} off by {d}");
    }
}

#[test]
fn a_smaller_ped_keeps_its_bone_lengths_and_plants_its_feet() {
    let (names, bind) = mw2();
    let rig = gta(0.9, 0.9);
    let rt = Retarget::new(&rig, &names, &bind, -0.9).unwrap();
    // A crouch-like pose: the hips drop 8 units, knees forward.
    let mut pose = bind.clone();
    let i = |n: &str| names.iter().position(|x| x == n).unwrap();
    for n in [
        "j_mainroot",
        "j_spinelower",
        "j_spineupper",
        "j_spine4",
        "j_neck",
        "j_head",
        "j_clavicle_le",
        "j_shoulder_le",
        "j_elbow_le",
        "j_wrist_le",
        "j_clavicle_ri",
        "j_shoulder_ri",
        "j_elbow_ri",
        "j_wrist_ri",
        "j_hip_le",
        "j_hip_ri",
    ] {
        pose[i(n)] = Mat4::from_translation(Vec3::new(0.0, 0.0, -8.0)) * pose[i(n)];
    }
    for (k, h) in [("j_knee_le", "j_hip_le"), ("j_knee_ri", "j_hip_ri")] {
        let hip = pose[i(h)].w_axis.truncate();
        let ankle = pose[i(if k.ends_with("le") { "j_ankle_le" } else { "j_ankle_ri" })].w_axis.truncate();
        let mid = (hip + ankle) * 0.5 + Vec3::new(6.0, 0.0, 0.0);
        pose[i(k)] = Mat4::from_translation(mid);
    }
    let out = rt.pose(&pose).unwrap();
    // Bone lengths: every GTA bone keeps its bind offset length from its parent.
    for (b, bone) in rig.iter().enumerate() {
        let Some(p) = bone.parent else { continue };
        let want = bone.t.length();
        let got = out[b].w_axis.truncate().distance(out[p].w_axis.truncate());
        assert!((got - want).abs() < 0.002, "{} length {got} vs {want}", bone.name);
    }
    // Feet on MW2's ankles (scaled into GTA space, same ground).
    for (gta_name, mw2_name) in [("SKEL_L_Foot", "j_ankle_le"), ("SKEL_R_Foot", "j_ankle_ri")] {
        let a = pose[i(mw2_name)].w_axis.truncate();
        let want = Vec3::new(-a.y, a.x, a.z) * METRES_PER_UNIT - Vec3::new(0.0, 0.0, 0.9);
        let d = out[find(&rt, gta_name)].w_axis.truncate().distance(want);
        assert!(d < STRETCH + 0.005, "{gta_name} {d}");
    }
}

#[test]
fn a_skeleton_without_human_bones_is_refused() {
    let (names, bind) = mw2();
    let mut rig = gta(1.0, 1.0);
    rig.retain(|b| b.name != "SKEL_Head");
    for b in &mut rig {
        b.parent = None;
    }
    assert!(Retarget::new(&rig, &names, &bind, -1.0).is_err());
}

/// GTA's ped binds face -Y in their object space (toes towards -Y, the left
/// side on +X) while the entity's front is +Y: the pose must face +Y anyway.
#[test]
fn the_pose_faces_the_entitys_front_whichever_way_the_bind_faces() {
    let (names, bind) = mw2();
    for turned in [false, true] {
        let mut rig = gta(1.0, 1.0);
        if turned {
            for b in &mut rig {
                b.t = Vec3::new(-b.t.x, -b.t.y, b.t.z);
            }
        }
        let rt = Retarget::new(&rig, &names, &bind, -1.0).unwrap();
        let out = rt.pose(&bind).unwrap();
        let p = |n: &str| out[find(&rt, n)].w_axis.truncate();
        let toes = (p("SKEL_L_Toe0") - p("SKEL_L_Foot")).y + (p("SKEL_R_Toe0") - p("SKEL_R_Foot")).y;
        let left = p("SKEL_L_Thigh").x - p("SKEL_R_Thigh").x;
        assert!(toes > 0.05 && left < -0.05, "turned {turned}: toes {toes} left {left}");
    }
}
