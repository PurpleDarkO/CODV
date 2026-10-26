//! The Modern Warfare 2 player (iw4L's pmove through codv-engine) behind the
//! C ABI. Its world is GTA's static map as MW2 collision meshes
//! (`collision.rs`), plus, when the host sets one, a trace callback answered
//! by the host in GTA metres.
//!
//! The player moves in a frame of its own (`Mw2State::frame`, GTA XY metres)
//! that re-centres on it every `REBASE` metres, so pmove's f32 coordinates
//! stay small anywhere on the map.
use codv_engine::units::{METRES_PER_UNIT, gta_pitch, heading_from_yaw, to_metres, to_units, yaw_from_heading};
use codv_engine::world::trace as tr;
use codv_engine::{MoveInput, Mw2Player, Stance, World};
use std::ffi::c_void;

pub const CV_MOVE_ON_GROUND: u32 = 1 << 0;
pub const CV_MOVE_SPRINTING: u32 = 1 << 1;
pub const CV_MOVE_MANTLING: u32 = 1 << 2;
pub const CV_MOVE_JUMPING: u32 = 1 << 3;
/// No collision around the player yet: the step did not move it (the host
/// keeps GTA's own movement until this clears).
pub const CV_MOVE_WAITING: u32 = 1 << 4;
/// The output carries MW2's third-person camera.
pub const CV_MOVE_THIRD_PERSON: u32 = 1 << 5;
/// CODV's slide is under way (crouch while sprinting; the host lets a jump stand up out of it).
pub const CV_MOVE_SLIDING: u32 = 1 << 6;
/// Diagnostics (the host's movement panel): this step the hull was freed from geometry it overlapped
/// (moved back to its last free spot or the nearest one), and the snag guard turned an edge's stop
/// into a run on (or round a corner).
pub const CV_MOVE_FREED: u32 = 1 << 7;
pub const CV_MOVE_SNAGGED: u32 = 1 << 8;

/// Distance from the frame's origin (metres) at which it re-centres.
const REBASE: f64 = 256.0;

/// One box sweep, GTA world metres; `mins`/`maxs` are relative to the path.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CvTraceQuery {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub mask: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CvTraceResult {
    /// 0..1 along the path; 1 = nothing hit.
    pub fraction: f32,
    pub normal: [f32; 3],
    pub start_solid: u32,
    pub all_solid: u32,
    /// GTA entity handle, 0 = the map.
    pub entity: u32,
    pub surface: u32,
}

/// Host callback: answer `query` into `result`. Called on the thread that
/// calls `cv_mw2_step`, several times per step.
pub type CvTraceFn = unsafe extern "C" fn(user: *mut c_void, query: *const CvTraceQuery, result: *mut CvTraceResult);

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvMoveSpawn {
    pub size: u32,
    /// Feet, GTA world metres.
    pub position: [f32; 3],
    /// GTA heading, degrees.
    pub heading: f32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CvMoveInput {
    pub size: u32,
    pub msec: u32,
    /// -127 .. 127.
    pub forward: i32,
    pub right: i32,
    /// Retail button bits (playerstate_iw4::buttons).
    pub buttons: u32,
    /// View change, degrees: yaw grows to the left, pitch grows UP (GTA's way).
    pub yaw_delta: f32,
    pub pitch_delta: f32,
    /// MW2 weapons: 0 none, 1 next weapon, 2 + n loadout slot n.
    pub weapon_request: u32,
    /// MW2's third-person camera (F4b): range behind the player in metres
    /// (0: first person) and the angle around it, degrees.
    pub third_person_range: f32,
    pub third_person_angle: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CvMoveOutput {
    pub size: u32,
    pub flags: u32,
    /// 0 stand, 1 crouch, 2 prone.
    pub stance: u32,
    pub fall_damage: i32,
    /// Feet, velocity, eye: GTA world metres (per second for velocity).
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub eye: [f32; 3],
    /// GTA heading and camera pitch (up positive), degrees.
    pub heading: f32,
    pub pitch: f32,
    pub view_height: f32,
    /// MW2 view kick to add to the camera this frame (degrees, pitch up, yaw
    /// right); it returns to zero by itself.
    pub kick: [f32; 2],
    /// With `third_person_range`: MW2's third-person camera (GTA world
    /// metres, GTA heading, pitch up positive) and CV_MOVE_THIRD_PERSON.
    pub camera: [f32; 3],
    pub camera_heading: f32,
    pub camera_pitch: f32,
    /// CODV's lean: the camera's roll, degrees (right lean positive); `eye` is already moved.
    pub roll: f32,
}

/// The host's world, as codv-engine's `World` (IW4 units in the player's
/// frame at `frame`, GTA metres to the host).
pub struct CallbackWorld {
    pub trace: CvTraceFn,
    pub user: *mut c_void,
    pub frame: [f64; 2],
}

impl CallbackWorld {
    fn gta(&self, p: [f32; 3]) -> [f32; 3] {
        let m = METRES_PER_UNIT as f64;
        [(self.frame[0] + p[0] as f64 * m) as f32, (self.frame[1] + p[1] as f64 * m) as f32, (p[2] as f64 * m) as f32]
    }
}

impl World for CallbackWorld {
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> tr::Trace {
        let q = CvTraceQuery {
            start: self.gta(start),
            end: self.gta(end),
            mins: to_metres(mins),
            maxs: to_metres(maxs),
            mask,
            reserved: 0,
        };
        let mut r = CvTraceResult { fraction: 1.0, ..Default::default() };
        unsafe { (self.trace)(self.user, &q, &mut r) };
        let fraction = if r.fraction.is_finite() { r.fraction.clamp(0.0, 1.0) } else { 1.0 };
        let n = if r.normal.iter().all(|c| c.is_finite()) { r.normal } else { [0.0, 0.0, 1.0] };
        let endpos = [0, 1, 2].map(|i| start[i] + (end[i] - start[i]) * fraction);
        let hit = fraction < 1.0 || r.start_solid != 0;
        tr::Trace {
            fraction,
            normal: if hit { n } else { [0.0; 3] },
            endpos,
            startsolid: u8::from(r.start_solid != 0),
            allsolid: u8::from(r.all_solid != 0),
            walkable: u8::from(hit && r.start_solid == 0 && n[2] >= 0.7),
            contents: if hit { 1 } else { 0 },
            hit_type: if hit { tr::HITTYPE_ENTITY } else { 0 },
            hit_id: if hit { tr::ENTITYNUM_WORLD } else { tr::ENTITYNUM_NONE },
            surface_flags: r.surface,
            ..Default::default()
        }
    }
}

/// Several worlds as one: the nearest hit wins (a start-solid one first).
pub struct Many<'a>(pub Vec<&'a dyn World>);

impl World for Many<'_> {
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> tr::Trace {
        let mut best = tr::Trace { fraction: 1.0, endpos: end, ..Default::default() };
        for w in &self.0 {
            let t = w.trace(start, end, mins, maxs, mask);
            if t.fraction < best.fraction || (t.startsolid != 0 && best.startsolid == 0) {
                best = t;
            }
            if best.allsolid != 0 {
                break;
            }
        }
        best
    }

    /// A mover is the nearest thing the sweep meets (the static world not in front of it).
    fn mover_hit(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> bool {
        let mut nearest = f32::MAX;
        let mut mover = false;
        for w in &self.0 {
            let t = w.trace(start, end, mins, maxs, mask);
            if t.fraction < 1.0 || t.startsolid != 0 {
                let f = if t.startsolid != 0 { 0.0 } else { t.fraction };
                if f < nearest {
                    nearest = f;
                    mover = w.mover_hit(start, end, mins, maxs, mask);
                }
            }
        }
        mover
    }
}

/// The player and the frame its IW4 coordinates are relative to.
pub struct Mw2State {
    pub player: Mw2Player,
    /// GTA XY (metres) of the frame's origin; IW4 z is GTA z.
    pub frame: [f64; 2],
}

impl Mw2State {
    /// Feet, GTA world metres.
    pub fn position(&self) -> [f64; 3] {
        let o = self.player.origin();
        let m = METRES_PER_UNIT as f64;
        [self.frame[0] + o[0] as f64 * m, self.frame[1] + o[1] as f64 * m, o[2] as f64 * m]
    }

    fn rebase(&mut self) {
        let p = self.position();
        if (p[0] - self.frame[0]).hypot(p[1] - self.frame[1]) < REBASE {
            return;
        }
        let to = [p[0], p[1]];
        self.player.shift(crate::collision::offset(self.frame, to));
        self.frame = to;
    }
}

pub fn spawn(s: &CvMoveSpawn) -> Option<Mw2State> {
    if !s.position.iter().all(|c| c.is_finite()) || !s.heading.is_finite() {
        return None;
    }
    let z = to_units([0.0, 0.0, s.position[2]])[2];
    Some(Mw2State {
        player: Mw2Player::new([0.0, 0.0, z], yaw_from_heading(s.heading)),
        frame: [s.position[0] as f64, s.position[1] as f64],
    })
}

/// The output for the player as it stands (`flags` added).
pub fn report(st: &Mw2State, o: &codv_engine::MoveOutput, extra: u32) -> CvMoveOutput {
    let mut flags = extra;
    if o.on_ground {
        flags |= CV_MOVE_ON_GROUND;
    }
    if o.sprinting {
        flags |= CV_MOVE_SPRINTING;
    }
    if o.mantling {
        flags |= CV_MOVE_MANTLING;
    }
    if o.sliding {
        flags |= CV_MOVE_SLIDING;
    }
    if o.jumping {
        flags |= CV_MOVE_JUMPING;
    }
    let m = METRES_PER_UNIT as f64;
    let at = |v: [f32; 3]| {
        [(st.frame[0] + v[0] as f64 * m) as f32, (st.frame[1] + v[1] as f64 * m) as f32, (v[2] as f64 * m) as f32]
    };
    // CODV's lean moves the eye sideways (the shots leave from it too).
    let eye = [
        o.origin[0] + o.lean_offset[0],
        o.origin[1] + o.lean_offset[1],
        o.origin[2] + o.view_height + o.lean_offset[2],
    ];
    CvMoveOutput {
        size: size_of::<CvMoveOutput>() as u32,
        flags,
        stance: match o.stance {
            Stance::Stand => 0,
            Stance::Crouch => 1,
            Stance::Prone => 2,
        },
        fall_damage: o.fall_damage,
        position: at(o.origin),
        velocity: to_metres(o.velocity),
        eye: at(eye),
        heading: heading_from_yaw(o.view_angles[1]),
        pitch: gta_pitch(o.view_angles[0]),
        view_height: o.view_height * METRES_PER_UNIT,
        kick: [0.0; 2],
        camera: [0.0; 3],
        camera_heading: 0.0,
        camera_pitch: 0.0,
        roll: o.lean_roll,
    }
}

/// The retail usercmd fields of a host input.
pub fn move_input(input: &CvMoveInput) -> MoveInput {
    let clamp8 = |v: i32| v.clamp(-127, 127) as i8;
    MoveInput {
        msec: input.msec,
        forward: clamp8(input.forward),
        right: clamp8(input.right),
        buttons: input.buttons,
        yaw_delta: input.yaw_delta,
        // IW4 pitch grows downward.
        pitch_delta: -input.pitch_delta,
    }
}

/// One step against `world` (in the player's frame), then re-centres the frame.
pub fn step(st: &mut Mw2State, input: &CvMoveInput, world: &dyn World) -> CvMoveOutput {
    let (unstuck, snags) = (st.player.unstuck, st.player.snags);
    let o = st.player.step(&move_input(input), world);
    let mut extra = 0;
    if st.player.unstuck != unstuck {
        extra |= CV_MOVE_FREED;
    }
    if st.player.snags != snags {
        extra |= CV_MOVE_SNAGGED;
    }
    let mut out = report(st, &o, extra);
    if input.third_person_range > 0.0 && input.third_person_range.is_finite() {
        // Before the frame moves: the world's traces are in this frame.
        let (at, angles) =
            st.player.third_person_view(input.third_person_range / METRES_PER_UNIT, input.third_person_angle, world);
        let m = METRES_PER_UNIT as f64;
        out.camera = [
            (st.frame[0] + at[0] as f64 * m) as f32,
            (st.frame[1] + at[1] as f64 * m) as f32,
            (at[2] as f64 * m) as f32,
        ];
        out.camera_heading = heading_from_yaw(angles[1]);
        out.camera_pitch = gta_pitch(angles[0]);
        out.flags |= CV_MOVE_THIRD_PERSON;
    }
    st.rebase();
    out
}

/// The player's state without moving it (no world to move in yet).
pub fn hold(st: &mut Mw2State) -> CvMoveOutput {
    let o = st.player.peek();
    report(st, &o, CV_MOVE_WAITING)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_recentres_without_moving_the_player() {
        let mut st =
            spawn(&CvMoveSpawn { size: 24, position: [-3000.0, 5000.0, 12.0], heading: 90.0, flags: 0 }).unwrap();
        st.player.ps.velocity = [100.0, 0.0, 0.0];
        st.player.shift([300.0 / METRES_PER_UNIT, -20.0 / METRES_PER_UNIT, 0.0]);
        let before = st.position();
        st.rebase();
        let after = st.position();
        assert!((0..3).all(|k| (before[k] - after[k]).abs() < 1e-3), "{before:?} -> {after:?}");
        assert!((st.frame[0] - -2700.0).abs() < 1e-3 && (st.frame[1] - 4980.0).abs() < 1e-3, "{:?}", st.frame);
        assert!(
            st.player.origin()[0].abs() < 0.05 && st.player.origin()[1].abs() < 0.05,
            "pmove's coordinates small again"
        );
        assert_eq!(st.player.ps.velocity, [100.0, 0.0, 0.0], "motion untouched");
        // Close to the frame's origin: no re-centring.
        st.player.shift([10.0 / METRES_PER_UNIT, 0.0, 0.0]);
        let frame = st.frame;
        st.rebase();
        assert_eq!(st.frame, frame);
    }
}
