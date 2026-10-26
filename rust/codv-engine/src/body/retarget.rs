//! MW2's posed soldier on the GTA ped's skeleton.
//!
//! The method is SK8V's (`rust/skatev-runtime/src/ped.rs`, Apache-2.0,
//! NOTICE), with MW2's soldier as the source instead of Skate's skater and
//! without the board contacts:
//!
//! * each mapped GTA bone wears its MW2 joint's change from bind (`posed *
//!   bind^-1`) carried through a bind fit that swings the GTA bind segment
//!   onto MW2's (`fit`), so wrists, head, spine twist and feet follow MW2;
//! * GTA keeps its own bone lengths: the pelvis is placed so the GTA hip
//!   joints straddle MW2's (lowered towards the feet for a shorter ped), and
//!   two-bone solves put the GTA ankles and wrists on MW2's, bending in the
//!   plane MW2's limb bends in (feet on the ground, both hands on the gun);
//! * GTA's expression-driven roll bones take the swing / twist split their
//!   bind position along the limb implies; shirt and belt helpers under the
//!   legs follow the pelvis.
//!
//! Spaces: the MW2 pose is model space (IW4 units, feet at the origin, facing
//! +X); the result is the GTA ped's object space (metres, the entity's
//! frame, front +Y), what the skeleton's object matrices hold.
use glam::{Mat3, Mat4, Quat, Vec3};

/// One GTA bone as crSkeletonData holds it: parent, local bind transform.
#[derive(Clone, Debug)]
pub struct GtaBone {
    pub name: String,
    pub parent: Option<usize>,
    pub t: Vec3,
    pub r: Quat,
    pub s: Vec3,
}

type Segment = Option<(&'static str, &'static str)>;
/// (GTA bone, MW2 joint, fit segment), owned: MAP plus the fingers.
type MapEntry = (String, String, Option<(String, String)>);

/// GTA bone, the MW2 joint it wears, and the bind segment (GTA child, MW2
/// child) the fit swings together (`None`: a chain end, fitted like the bone
/// named in `TERMINAL_FIT`).
const MAP: [(&str, &str, Segment); 23] = [
    ("SKEL_Pelvis", "j_mainroot", None),
    ("SKEL_Spine0", "j_spinelower", Some(("SKEL_Spine2", "j_spineupper"))),
    ("SKEL_Spine1", "j_spinelower", Some(("SKEL_Spine2", "j_spineupper"))),
    ("SKEL_Spine2", "j_spineupper", Some(("SKEL_Spine3", "j_spine4"))),
    ("SKEL_Spine3", "j_spine4", Some(("SKEL_Neck_1", "j_neck"))),
    ("SKEL_Neck_1", "j_neck", Some(("SKEL_Head", "j_head"))),
    ("SKEL_Head", "j_head", None),
    ("SKEL_L_Clavicle", "j_clavicle_le", Some(("SKEL_L_UpperArm", "j_shoulder_le"))),
    ("SKEL_L_UpperArm", "j_shoulder_le", Some(("SKEL_L_Forearm", "j_elbow_le"))),
    ("SKEL_L_Forearm", "j_elbow_le", Some(("SKEL_L_Hand", "j_wrist_le"))),
    ("SKEL_L_Hand", "j_wrist_le", None),
    ("SKEL_R_Clavicle", "j_clavicle_ri", Some(("SKEL_R_UpperArm", "j_shoulder_ri"))),
    ("SKEL_R_UpperArm", "j_shoulder_ri", Some(("SKEL_R_Forearm", "j_elbow_ri"))),
    ("SKEL_R_Forearm", "j_elbow_ri", Some(("SKEL_R_Hand", "j_wrist_ri"))),
    ("SKEL_R_Hand", "j_wrist_ri", None),
    ("SKEL_L_Thigh", "j_hip_le", Some(("SKEL_L_Calf", "j_knee_le"))),
    ("SKEL_L_Calf", "j_knee_le", Some(("SKEL_L_Foot", "j_ankle_le"))),
    ("SKEL_L_Foot", "j_ankle_le", Some(("SKEL_L_Toe0", "j_ball_le"))),
    ("SKEL_L_Toe0", "j_ball_le", None),
    ("SKEL_R_Thigh", "j_hip_ri", Some(("SKEL_R_Calf", "j_knee_ri"))),
    ("SKEL_R_Calf", "j_knee_ri", Some(("SKEL_R_Foot", "j_ankle_ri"))),
    ("SKEL_R_Foot", "j_ankle_ri", Some(("SKEL_R_Toe0", "j_ball_ri"))),
    ("SKEL_R_Toe0", "j_ball_ri", None),
];
/// Fingers: GTA SKEL_<side>_Finger<f><k> wears MW2 j_<finger>_<side>_<k+1>
/// (thumb, index, middle, ring, pinky), each fitted along its own segment.
const FINGERS: [&str; 5] = ["thumb", "index", "mid", "ring", "pinky"];
const TERMINAL_FIT: [(&str, &str); 3] =
    [("SKEL_Head", "SKEL_Neck_1"), ("SKEL_L_Hand", "SKEL_L_Forearm"), ("SKEL_R_Hand", "SKEL_R_Forearm")];
/// Both binds stand with flat soles; feet are fitted about the vertical only.
const GROUND_FIT: [&str; 2] = ["SKEL_L_Foot", "SKEL_R_Foot"];
const TOE_FIT: [(&str, &str); 2] = [("SKEL_L_Toe0", "SKEL_L_Foot"), ("SKEL_R_Toe0", "SKEL_R_Foot")];
/// Two-bone limbs whose end joint is placed on MW2's: (upper, lower, end).
const LIMBS: [(&str, &str, &str); 4] = [
    ("SKEL_L_Thigh", "SKEL_L_Calf", "SKEL_L_Foot"),
    ("SKEL_R_Thigh", "SKEL_R_Calf", "SKEL_R_Foot"),
    ("SKEL_L_UpperArm", "SKEL_L_Forearm", "SKEL_L_Hand"),
    ("SKEL_R_UpperArm", "SKEL_R_Forearm", "SKEL_R_Hand"),
];
/// GTA's roll bones: (roll, base, twisted, carries the twisted bone's swing).
const ROLLS: [(&str, &str, &str, bool); 10] = [
    ("RB_L_ThighRoll", "SKEL_Pelvis", "SKEL_L_Thigh", true),
    ("RB_R_ThighRoll", "SKEL_Pelvis", "SKEL_R_Thigh", true),
    ("RB_L_ArmRoll", "SKEL_L_Clavicle", "SKEL_L_UpperArm", true),
    ("RB_R_ArmRoll", "SKEL_R_Clavicle", "SKEL_R_UpperArm", true),
    ("RB_L_ArmRoll_Vest", "SKEL_L_Clavicle", "SKEL_L_UpperArm", true),
    ("RB_R_ArmRoll_Vest", "SKEL_R_Clavicle", "SKEL_R_UpperArm", true),
    ("RB_L_ForeArmRoll", "SKEL_L_Forearm", "SKEL_L_Hand", false),
    ("RB_R_ForeArmRoll", "SKEL_R_Forearm", "SKEL_R_Hand", false),
    ("RB_Neck", "SKEL_Spine3", "SKEL_Neck_1", true),
    ("RB_Neck_1", "SKEL_Neck_1", "SKEL_Head", false),
];
/// Beyond reach a limb end still meets its target up to this far (metres).
const STRETCH: f32 = 0.03;
pub const METRES_PER_UNIT: f32 = 0.0254;

fn rigid(r: Mat3, t: Vec3) -> Mat4 {
    Mat4::from_cols(r.x_axis.extend(0.0), r.y_axis.extend(0.0), r.z_axis.extend(0.0), t.extend(1.0))
}

/// Orthonormal frame with `u` first and `v` (orthogonalised) second.
fn frame(u: Vec3, v: Vec3) -> Option<Mat3> {
    let u = u.try_normalize()?;
    let w = u.cross(v).try_normalize()?;
    Some(Mat3::from_cols(u, w.cross(u), w))
}

/// Rotation part of a possibly scaled matrix, orthonormalised.
fn rot_of(m: &Mat4) -> Mat3 {
    Mat3::from_quat(m.to_scale_rotation_translation().1)
}

/// The part of `q` about unit `axis` (swing-twist split), short arc.
fn twist(q: Quat, axis: Vec3) -> Quat {
    let p = axis * Vec3::new(q.x, q.y, q.z).dot(axis);
    let t = Quat::from_xyzw(p.x, p.y, p.z, q.w);
    let t = if t.length_squared() < 1e-12 { Quat::IDENTITY } else { t.normalize() };
    if t.w < 0.0 { -t } else { t }
}

struct Limb {
    upper: Mat3,
    lower: Mat3,
    end: Vec3,
}

/// Two-bone solve keeping both lengths (SK8V `solve_limb`).
#[allow(clippy::too_many_arguments)]
fn solve_limb(
    hip: Vec3,
    knee: Vec3,
    ankle: Vec3,
    target: Vec3,
    bend: Vec3,
    upper: Mat3,
    lower: Mat3,
    stretch: f32,
) -> Option<Limb> {
    let (a, b) = (hip.distance(knee), knee.distance(ankle));
    let reach = target - hip;
    let distance = reach.length();
    if !(distance.is_finite() && a > 1e-6 && b > 1e-6 && distance > 1e-6) {
        return None;
    }
    let u = reach / distance;
    let d = distance.clamp((a - b).abs() + 1e-5, a + b - 1e-5);
    let along = (a * a - b * b + d * d) / (2.0 * d);
    let height = (a * a - along * along).max(0.0).sqrt();
    let side = u.cross(bend).try_normalize()?;
    let new_knee = hip + u * along + side * height;
    let end = hip + u * (d + (distance - d).clamp(-stretch, stretch));
    let turn = |from: Vec3, to: Vec3| -> Option<Mat3> { Some(frame(to, bend)? * frame(from, bend)?.transpose()) };
    Some(Limb {
        upper: turn(knee - hip, new_knee - hip)? * upper,
        lower: turn(ankle - knee, end - new_knee)? * lower,
        end,
    })
}

struct Roll {
    bone: usize,
    base: usize,
    twisted: usize,
    swing: bool,
    share: f32,
    axis: Vec3,
}

/// The fitted pair of skeletons.
pub struct Retarget {
    names: Vec<String>,
    parents: Vec<Option<usize>>,
    local: Vec<Mat4>,
    bind: Vec<Mat4>,
    order: Vec<usize>,
    /// Per GTA bone: the MW2 bone index it wears.
    mapped: Vec<Option<usize>>,
    /// Per GTA bone with a source: MW2 bind -> GTA bind rotation.
    offset: Vec<Option<Mat3>>,
    followers: Vec<Option<(usize, Mat4)>>,
    pelvis: usize,
    hip_sources: [usize; 2],
    limbs: Vec<[usize; 3]>,
    hinge: Vec<Vec3>,
    rolls: Vec<Roll>,
    hips: Vec3,
    leg_ratio: f32,
    /// MW2 model space -> GTA object space.
    axes: Mat3,
    feet_z: f32,
}

impl Retarget {
    /// Fits the GTA skeleton `gta` to the MW2 soldier (`mw2_names`, its bind
    /// pose in model space). `feet_z`: where the ground is in the ped's
    /// object space (minus the ped origin's height above its feet).
    pub fn new(gta: &[GtaBone], mw2_names: &[String], mw2_bind: &[Mat4], feet_z: f32) -> Result<Retarget, String> {
        let n = gta.len();
        let names: Vec<String> = gta.iter().map(|b| b.name.clone()).collect();
        let parents: Vec<Option<usize>> = gta.iter().map(|b| b.parent.filter(|&p| p < n)).collect();
        let local: Vec<Mat4> =
            gta.iter().map(|b| Mat4::from_scale_rotation_translation(b.s, b.r.normalize(), b.t)).collect();
        let mut order = Vec::with_capacity(n);
        let mut placed = vec![false; n];
        while order.len() < n {
            let before = order.len();
            for i in 0..n {
                if !placed[i] && parents[i].is_none_or(|p| placed[p]) {
                    placed[i] = true;
                    order.push(i);
                }
            }
            if order.len() == before {
                return Err("GTA skeleton has a parent cycle".into());
            }
        }
        let mut bind = vec![Mat4::IDENTITY; n];
        for &i in &order {
            bind[i] = parents[i].map_or(local[i], |p| bind[p] * local[i]);
        }
        let idx = |name: &str| names.iter().position(|x| x == name);
        let src = |name: &str| mw2_names.iter().position(|x| x == name);
        let need = |name: &str| idx(name).ok_or_else(|| format!("not a human GTA skeleton (no {name})"));
        let need_src = |name: &str| src(name).ok_or_else(|| format!("MW2 soldier has no {name}"));
        let g = |name: &str| need(name).map(|i| bind[i].w_axis.truncate());
        let m = |name: &str| need_src(name).map(|i| mw2_bind[i].w_axis.truncate());

        // MW2's axes onto the ped's object space: MW2's front (+X) onto the
        // entity's front (+Y), MW2's left (+Y) onto -X, up stays up. Not onto
        // the bind's own facing: GTA's ped binds face -Y in object space (the
        // ped then faced the camera); the per-bone fit below turns each GTA
        // bone from its bind to MW2's.
        frame(m("j_head")? - m("j_mainroot")?, m("j_hip_le")? - m("j_hip_ri")?).ok_or("degenerate MW2 bind")?;
        frame(g("SKEL_Head")? - g("SKEL_Pelvis")?, g("SKEL_L_Thigh")? - g("SKEL_R_Thigh")?)
            .ok_or("degenerate GTA bind")?;
        if (g("SKEL_Head")? - g("SKEL_Pelvis")?).z <= 0.0 {
            return Err("not an upright GTA skeleton".into());
        }
        let axes = Mat3::from_cols(Vec3::Y, -Vec3::X, Vec3::Z);
        let to_gta = |p: Vec3| axes * p * METRES_PER_UNIT + Vec3::new(0.0, 0.0, feet_z);
        let sb = |name: &str| m(name).map(to_gta);
        let source_bind_rot: Vec<Mat3> = mw2_bind.iter().map(|b| axes * rot_of(b) * axes.transpose()).collect();

        let mut map: Vec<MapEntry> = MAP
            .iter()
            .map(|(a, b, s)| (a.to_string(), b.to_string(), s.map(|(c, d)| (c.to_string(), d.to_string()))))
            .collect();
        let mut terminal: Vec<(String, String)> =
            TERMINAL_FIT.iter().chain(TOE_FIT.iter()).map(|(a, b)| (a.to_string(), b.to_string())).collect();
        for (side, s) in [("L", "le"), ("R", "ri")] {
            for (f, finger) in FINGERS.iter().enumerate() {
                for k in 0..3 {
                    let gta_name = format!("SKEL_{side}_Finger{f}{k}");
                    let mw2_name = format!("j_{finger}_{s}_{}", k + 1);
                    if idx(&gta_name).is_none() || src(&mw2_name).is_none() {
                        continue;
                    }
                    let seg = (k < 2)
                        .then(|| (format!("SKEL_{side}_Finger{f}{}", k + 1), format!("j_{finger}_{s}_{}", k + 2)));
                    let seg = seg.filter(|(a, b)| idx(a).is_some() && src(b).is_some());
                    if seg.is_none() && k > 0 {
                        terminal.push((gta_name.clone(), format!("SKEL_{side}_Finger{f}{}", k - 1)));
                    }
                    map.push((gta_name, mw2_name, seg));
                }
            }
        }
        let mut mapped = vec![None; n];
        for (bone, source, _) in &map {
            mapped[need(bone)?] = Some(need_src(source)?);
        }
        let mut fit_from: Vec<usize> = (0..n).collect();
        for (bone, from) in &terminal {
            if let (Some(b), Some(f)) = (idx(bone), idx(from)) {
                fit_from[b] = f;
            }
        }
        let facing = frame(sb("j_spine4")? - sb("j_mainroot")?, sb("j_hip_le")? - sb("j_hip_ri")?)
            .zip(frame(g("SKEL_Spine3")? - g("SKEL_Pelvis")?, g("SKEL_L_Thigh")? - g("SKEL_R_Thigh")?))
            .map(|(s, t)| s * t.transpose())
            .ok_or("degenerate pelvis frames")?;
        let horizontal = |v: Vec3| Vec3::new(v.x, v.y, 0.0).try_normalize();
        let mut fit_rotation = vec![facing; n];
        for (bone, source, segment) in &map {
            let Some((child, source_child)) = segment else { continue };
            let from = facing * (g(child)? - g(bone)?);
            let to = sb(source_child)? - sb(source)?;
            let swing = if GROUND_FIT.contains(&bone.as_str()) {
                horizontal(from).zip(horizontal(to)).map(|(a, b)| Quat::from_rotation_arc(a, b))
            } else {
                from.try_normalize().zip(to.try_normalize()).map(|(a, b)| Quat::from_rotation_arc(a, b))
            };
            fit_rotation[need(bone)?] = Mat3::from_quat(swing.unwrap_or(Quat::IDENTITY)) * facing;
        }
        // offset: (MW2 source bind rotation)^-1 * fit * GTA bind, so that at
        // MW2's bind the GTA bone shows its fitted bind.
        let offset: Vec<Option<Mat3>> = (0..n)
            .map(|i| mapped[i].map(|j| source_bind_rot[j].transpose() * fit_rotation[fit_from[i]] * rot_of(&bind[i])))
            .collect();
        let pelvis = need("SKEL_Pelvis")?;
        let thighs = [need("SKEL_L_Thigh")?, need("SKEL_R_Thigh")?];
        let p = |i: usize| bind[i].w_axis.truncate();
        let hips = rot_of(&bind[pelvis]).transpose() * ((p(thighs[0]) + p(thighs[1])) * 0.5 - p(pelvis));
        let limbs: Vec<[usize; 3]> =
            LIMBS.iter().map(|l| Ok([need(l.0)?, need(l.1)?, need(l.2)?])).collect::<Result<_, String>>()?;
        let hinge = limbs
            .iter()
            .map(|&[upper, lower, end]| {
                let axis = (p(lower) - p(upper))
                    .cross(p(end) - p(lower))
                    .try_normalize()
                    .unwrap_or(rot_of(&bind[upper]) * Vec3::Z);
                rot_of(&bind[upper]).transpose() * axis
            })
            .collect();
        let leg = |a: Vec3, b: Vec3, c: Vec3| a.distance(b) + b.distance(c);
        let mw2_leg = leg(sb("j_hip_le")?, sb("j_knee_le")?, sb("j_ankle_le")?);
        let gta_leg = leg(g("SKEL_L_Thigh")?, g("SKEL_L_Calf")?, g("SKEL_L_Foot")?);
        let leg_ratio = if mw2_leg > 1e-3 && gta_leg.is_finite() { (gta_leg / mw2_leg).clamp(0.3, 1.0) } else { 1.0 };
        // Shirt / belt / bum helpers parented under the legs follow the pelvis.
        let under_leg = |mut i: usize| {
            while let Some(pp) = parents[i] {
                if names[pp].starts_with("SKEL_") && (names[pp].contains("Thigh") || names[pp].contains("Calf")) {
                    return true;
                }
                i = pp;
            }
            false
        };
        let helper = |s: &str| ["Shirt", "Belt", "Bum"].iter().any(|k| s.contains(k));
        let followers = (0..n)
            .map(|i| {
                (!names[i].starts_with("SKEL_")
                    && helper(&names[i])
                    && parents[i].is_some_and(|pp| !helper(&names[pp]))
                    && under_leg(i))
                .then(|| (pelvis, bind[pelvis].inverse() * bind[i]))
            })
            .collect();
        let segment_end =
            |bone: usize| MAP.iter().find(|mm| mm.0 == names[bone]).and_then(|mm| mm.2).and_then(|s| idx(s.0));
        let rolls = ROLLS
            .iter()
            .filter_map(|&(roll, base, twisted, swing)| {
                let (bone, base, twisted) = (idx(roll)?, idx(base)?, idx(twisted)?);
                let (from, to) = if swing { (twisted, segment_end(twisted)?) } else { (base, twisted) };
                let axis = (p(to) - p(from)).try_normalize()?;
                let share = ((p(bone) - p(from)).dot(axis) / p(to).distance(p(from))).clamp(0.0, 1.0);
                Some(Roll { bone, base, twisted, swing, share, axis })
            })
            .collect();
        Ok(Retarget {
            names,
            parents,
            local,
            bind,
            order,
            mapped,
            offset,
            followers,
            pelvis,
            hip_sources: [need_src("j_hip_le")?, need_src("j_hip_ri")?],
            limbs,
            hinge,
            rolls,
            hips,
            leg_ratio,
            axes,
            feet_z,
        })
    }

    pub fn bone_names(&self) -> &[String] {
        &self.names
    }

    /// The GTA bind pose (object space).
    pub fn bind(&self) -> &[Mat4] {
        &self.bind
    }

    fn to_gta(&self, p: Vec3) -> Vec3 {
        self.axes * p * METRES_PER_UNIT + Vec3::new(0.0, 0.0, self.feet_z)
    }

    /// The GTA object matrices for MW2's pose (`mw2_pose`: model space, the
    /// soldier's bones in its order); None if the result is not finite.
    pub fn pose(&self, mw2_pose: &[Mat4]) -> Option<Vec<Mat4>> {
        let n = self.local.len();
        let src_rot = |j: usize| -> Option<Mat3> { Some(self.axes * rot_of(mw2_pose.get(j)?) * self.axes.transpose()) };
        let target = |j: usize| -> Option<Vec3> { Some(self.to_gta(mw2_pose.get(j)?.w_axis.truncate())) };
        let mut rot: Vec<Option<Mat3>> = (0..n).map(|i| Some(src_rot(self.mapped[i]?)? * self.offset[i]?)).collect();
        let mut pos: Vec<Option<Vec3>> = vec![None; n];
        let pelvis_rot = rot[self.pelvis]?;
        let mut hips = (target(self.hip_sources[0])? + target(self.hip_sources[1])?) * 0.5;
        let foot = |side: usize| self.mapped[self.limbs[side][2]].and_then(target);
        if let (true, Some(l), Some(r)) = (self.leg_ratio < 1.0, foot(0), foot(1)) {
            let feet = (l + r) * 0.5;
            hips = feet + (hips - feet) * self.leg_ratio;
        }
        let pelvis = rigid(pelvis_rot, hips - pelvis_rot * self.hips);
        let world = self.fk(pelvis, &rot, &pos);
        for (k, &[upper, lower, end]) in self.limbs.iter().enumerate() {
            let (Some(j), Some(r_upper), Some(r_lower)) = (self.mapped[end], rot[upper], rot[lower]) else { continue };
            let Some(goal) = target(j) else { continue };
            let pp = |i: usize| world[i].w_axis.truncate();
            let (hip, knee, ankle) = (pp(upper), pp(lower), pp(end));
            let fallback = (r_upper * self.hinge[k]).normalize();
            let chain = (knee - hip).cross(ankle - knee);
            let chain = if chain.dot(fallback) < 0.0 { -chain } else { chain };
            let bent = chain.length() / (hip.distance(knee) * knee.distance(ankle)).max(1e-9);
            let w = ((bent - 0.02) / 0.08).clamp(0.0, 1.0);
            let bend = (fallback * (1.0 - w) + chain.normalize_or_zero() * w).try_normalize().unwrap_or(fallback);
            if let Some(l) = solve_limb(hip, knee, ankle, goal, bend, r_upper, r_lower, STRETCH) {
                rot[upper] = Some(l.upper);
                rot[lower] = Some(l.lower);
                pos[end] = Some(l.end);
            }
        }
        let world = self.fk(pelvis, &rot, &pos);
        for r in &self.rolls {
            rot[r.bone] = Some(self.roll(r, &world));
        }
        let world = self.fk(pelvis, &rot, &pos);
        world.iter().all(|m| m.is_finite()).then_some(world)
    }

    /// Forward kinematics from the pelvis with overrides; every other bone
    /// keeps its bind offset from its parent (or leader), the root included.
    fn fk(&self, pelvis: Mat4, rot: &[Option<Mat3>], pos: &[Option<Vec3>]) -> Vec<Mat4> {
        let n = self.local.len();
        let mut world = vec![Mat4::IDENTITY; n];
        let mut done = vec![false; n];
        world[self.pelvis] = pelvis;
        done[self.pelvis] = true;
        // The root follows the pelvis (SK8V): GTA hangs the spine off the root
        // (SKEL_ROOT -> SKEL_Spine_Root -> SKEL_Spine0), so a root left at its
        // bind anchored the upper body while the hips moved (a stretched ped).
        if let Some(root) = self.parents[self.pelvis] {
            world[root] = pelvis * self.local[self.pelvis].inverse();
            done[root] = true;
        }
        for &i in &self.order {
            if done[i] {
                continue;
            }
            done[i] = true;
            if let Some((leader, relative)) = self.followers[i] {
                world[i] = world[leader] * relative;
                continue;
            }
            let fk = self.parents[i].map_or(self.local[i], |pp| world[pp] * self.local[i]);
            let at = pos[i].unwrap_or(fk.w_axis.truncate());
            world[i] = match rot[i] {
                Some(r) => rigid(r, at),
                None if pos[i].is_some() => rigid(rot_of(&fk), at),
                None => fk,
            };
        }
        world
    }

    fn roll(&self, r: &Roll, world: &[Mat4]) -> Mat3 {
        let rot = |i: usize| rot_of(&world[i]);
        let bind = |i: usize| rot_of(&self.bind[i]);
        let carried = rot(r.base) * bind(r.base).transpose();
        let delta = Quat::from_mat3(&(rot(r.twisted) * (carried * bind(r.twisted)).transpose())).normalize();
        let turn = twist(delta, (carried * r.axis).normalize());
        let share = Quat::IDENTITY.slerp(turn, r.share);
        let q = if r.swing { delta * turn.inverse() * share } else { share };
        Mat3::from_quat(q.normalize()) * carried * bind(r.bone)
    }
}

#[cfg(test)]
mod tests;
