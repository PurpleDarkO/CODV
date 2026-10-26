//! Thrown grenades in flight (MW2's frag): launched from the view the way
//! iw4L's simulation launches them (`sim/src/equipment.rs`
//! `grenade_launch_velocity`: speed along the view, the authored extra up and
//! forward speed, the thrower's velocity along the throw), falling under
//! MW2's gravity, bouncing off the world (`bounce_reflected_scale`: the
//! reflection scaled between the parallel and perpendicular coefficients by
//! the incidence) and coming to rest on the floor (`floor && speed < 20`),
//! until the fuse runs out.
//!
//! The world is the same `World` pmove collides with (IW4 units, in the
//! player's frame). Damage is the host's (GTA's explosion at the point).
use crate::arms::Throw;
use crate::world::{CONTENTS_SOLID, World};

#[cfg(test)]
mod tests;

/// MW2's g_gravity, units per second squared.
pub const GRAVITY: f32 = 800.0;
/// Surface bounce coefficients for every surface (CODV's choice: the import
/// does not carry MW2's per-surface tables).
pub const PARALLEL_BOUNCE: f32 = 0.5;
pub const PERPENDICULAR_BOUNCE: f32 = 0.25;
/// Half size of the box the grenade sweeps (units).
const HALF: f32 = 1.5;
/// Fuse when none is authored, and the longest one (iw4L GRENADE_*_FUSE_MS).
const DEFAULT_FUSE_MS: i32 = 3500;
const FUSE_CAP_MS: i32 = 30_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Grenade {
    pub id: u32,
    pub weapon: u32,
    /// IW4 units in the player's frame.
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// Milliseconds until it goes off.
    pub fuse_ms: i32,
    pub resting: bool,
    /// Spin for the host's model (degrees: pitch, roll, yaw).
    pub angles: [f32; 3],
    pub throw: Throw,
}

/// A grenade that went off: where (IW4 units, player's frame) and how big.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Explosion {
    pub id: u32,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub radius: f32,
    pub inner_damage: f32,
    pub outer_damage: f32,
}

fn len(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let l = len(v);
    (l > 1e-6).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

/// The view's forward vector (IW4 angles: pitch down +, yaw left +).
pub fn forward(angles: [f32; 3]) -> [f32; 3] {
    let (p, y) = (angles[0].to_radians(), angles[1].to_radians());
    [p.cos() * y.cos(), p.cos() * y.sin(), -p.sin()]
}

/// iw4L `grenade_launch_velocity` (with `project_owner_velocity`).
pub fn launch_velocity(direction: [f32; 3], t: &Throw, owner_velocity: [f32; 3]) -> [f32; 3] {
    let mut v = [direction[0] * t.speed, direction[1] * t.speed, direction[2] * t.speed + t.speed_up];
    if t.speed_forward != 0.0
        && let Some(flat) = normalize([direction[0], direction[1], 0.0])
    {
        v[0] += flat[0] * t.speed_forward;
        v[1] += flat[1] * t.speed_forward;
    }
    let Some(dir) = normalize(v) else { return v };
    let along = owner_velocity[0] * dir[0] + owner_velocity[1] * dir[1] + owner_velocity[2] * dir[2];
    [v[0] + dir[0] * along, v[1] + dir[1] * along, v[2] + dir[2] * along]
}

/// iw4L `bounce_reflected_scale`.
pub fn bounce(incoming: [f32; 3], normal: [f32; 3], parallel: f32, perpendicular: f32) -> [f32; 3] {
    let speed = len(incoming);
    let Some(n) = normalize(normal) else { return [0.0; 3] };
    if speed < 1e-6 {
        return [0.0; 3];
    }
    let d = incoming[0] * n[0] + incoming[1] * n[1] + incoming[2] * n[2];
    let r = [incoming[0] - 2.0 * d * n[0], incoming[1] - 2.0 * d * n[1], incoming[2] - 2.0 * d * n[2]];
    let incidence = (-d / speed).clamp(0.0, 1.0);
    let f = parallel + (perpendicular - parallel) * incidence;
    [r[0] * f, r[1] * f, r[2] * f]
}

impl Grenade {
    /// Thrown from `eye` along `angles` (the view) by a player moving at
    /// `owner_velocity`; `fuse_left` is what cooking left of the fuse.
    pub fn thrown(
        id: u32,
        weapon: u32,
        t: Throw,
        eye: [f32; 3],
        angles: [f32; 3],
        owner_velocity: [f32; 3],
        fuse_left: Option<i32>,
    ) -> Grenade {
        let authored = if t.fuse_time_ms > 0 { t.fuse_time_ms } else { DEFAULT_FUSE_MS };
        let fuse = if t.cook_off_hold { fuse_left.filter(|&ms| ms > 0).unwrap_or(authored) } else { authored };
        Grenade {
            id,
            weapon,
            origin: eye,
            velocity: launch_velocity(forward(angles), &t, owner_velocity),
            fuse_ms: fuse.min(FUSE_CAP_MS),
            resting: false,
            angles: [0.0, 0.0, angles[1]],
            throw: t,
        }
    }

    /// Moves the grenade `msec` against `world`; Some when it went off.
    pub fn step(&mut self, msec: i32, world: &dyn World) -> Option<Explosion> {
        let msec = msec.clamp(0, 200);
        self.fuse_ms -= msec;
        if !self.resting {
            // Sub-steps of at most 10 ms keep fast grenades from tunnelling.
            let mut left = msec;
            while left > 0 && !self.resting {
                let ms = left.min(10);
                left -= ms;
                self.advance(ms as f32 / 1000.0, world);
            }
        }
        (self.fuse_ms <= 0).then(|| self.explosion())
    }

    pub fn explosion(&self) -> Explosion {
        Explosion {
            id: self.id,
            weapon: self.weapon,
            origin: self.origin,
            radius: self.throw.radius,
            inner_damage: self.throw.inner_damage,
            outer_damage: self.throw.outer_damage,
        }
    }

    fn advance(&mut self, dt: f32, world: &dyn World) {
        let mut time = dt;
        for _ in 0..4 {
            if time <= 0.0 {
                break;
            }
            let v = self.velocity;
            let end = [
                self.origin[0] + v[0] * time,
                self.origin[1] + v[1] * time,
                self.origin[2] + v[2] * time - 0.5 * GRAVITY * time * time,
            ];
            let tr = world.trace(self.origin, end, [-HALF; 3], [HALF; 3], CONTENTS_SOLID);
            let f = if tr.startsolid != 0 { 0.0 } else { tr.fraction.clamp(0.0, 1.0) };
            let used = time * f;
            for (o, e) in self.origin.iter_mut().zip(end) {
                *o += (e - *o) * f;
            }
            self.velocity[2] -= GRAVITY * used;
            self.spin(used);
            time -= used;
            if f >= 1.0 {
                return;
            }
            let normal = if tr.startsolid != 0 { [0.0, 0.0, 1.0] } else { tr.normal };
            self.velocity = bounce(self.velocity, normal, PARALLEL_BOUNCE, PERPENDICULAR_BOUNCE);
            // Off the surface a little, so the next sweep does not start inside it.
            for (o, n) in self.origin.iter_mut().zip(normal) {
                *o += n * 0.1;
            }
            if normal[2] > 0.7 && len(self.velocity) < 20.0 {
                self.velocity = [0.0; 3];
                self.resting = true;
                self.angles[0] = 0.0;
                return;
            }
        }
    }

    fn spin(&mut self, dt: f32) {
        let speed = len(self.velocity);
        self.angles[0] = (self.angles[0] + speed * 1.5 * dt) % 360.0;
    }

    /// The player's frame moved by `offset` units (as `Mw2Player::shift`).
    pub fn shift(&mut self, offset: [f32; 3]) {
        for (o, d) in self.origin.iter_mut().zip(offset) {
            *o += d;
        }
    }
}
