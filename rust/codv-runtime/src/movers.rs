//! Moving GTA entities (vehicles, props, peds) as MW2 collision, every frame.
//!
//! The host lists the entities near the player (`cv_set_movers`): model,
//! GTA's matrix, the model's box and the entity's fragInst. A vehicle or prop
//! collides with GTA's own bound for its model, read once per model by the
//! collision worker from that instance (`codv_gtaworld::live::instance_model`,
//! the static map's decoder and flag rule); until it is in, or when it can't
//! be read, the model's box stands in. A ped is a box the size of MW2's
//! player hull (30 x 30 units, its model's height): players in Modern Warfare 2
//! collide with each other as hulls.
//!
//! Each body's triangles are turned by its matrix into IW4 units around the
//! entity's position and kept as a small MW2 collision mesh; a body that only
//! moves reuses its mesh (a sweep is shifted into it), one that turns gets a
//! new one. Static map props never come here: GTA keeps them in the physics
//! level's fixed state, which the area reader takes.
use crate::collision::{self, Streamer, TemplateRequest};
use codv_engine::mesh::Triangle;
use codv_engine::units::METRES_PER_UNIT;
use codv_engine::world::trace::Trace;
use codv_engine::{MeshWorld, World};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CV_MOVER_VEHICLE: u32 = 0;
pub const CV_MOVER_PED: u32 = 1;
pub const CV_MOVER_OBJECT: u32 = 2;

/// At most this many movers per frame (the host sends the nearest).
pub const MAX_MOVERS: usize = 96;
/// A list older than this is stale (the host stopped sending): no movers.
const STALE: Duration = Duration::from_millis(500);
/// MW2's player hull half width, metres (15 units).
const HULL_HALF: f32 = 15.0 * METRES_PER_UNIT;

/// One entity near the player, as the host sees it this frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CvMover {
    /// GTA entity handle: reported back with hits.
    pub entity: u32,
    pub model: u32,
    /// CV_MOVER_VEHICLE / PED / OBJECT.
    pub kind: u32,
    pub flags: u32,
    /// The entity's fragInst (phInst) address; 0 when unknown.
    pub instance: u64,
    /// GTA's matrix: position and the (scaled) right, forward, up axes.
    pub position: [f32; 3],
    pub right: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    /// GET_MODEL_DIMENSIONS, model space metres.
    pub box_min: [f32; 3],
    pub box_max: [f32; 3],
}

/// The 12 outward triangles of a box (model space).
fn box_tris(lo: [f32; 3], hi: [f32; 3]) -> Vec<[[f32; 3]; 3]> {
    let c = |i: usize| [[lo[0], hi[0]][i & 1], [lo[1], hi[1]][i >> 1 & 1], [lo[2], hi[2]][i >> 2]];
    let quads = [[1, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

/// A parked car or a prop at rest still moves by millimetres in GTA's physics frame after frame (settling,
/// or the frozen player ped leaning on it). Followed, it crept into the MW2 hull every frame: the player
/// was freed and lost the ground each step, a grab and a shake against every car. Vehicles and props
/// keep their pose until they move this far (peds follow every frame).
const SETTLE_MOVE: f64 = 0.03;
const SETTLE_TURN: f32 = 0.01;

fn settled(old: &Body, kind: u32, model: u32, shape: Shape, axes: &Axes, position: [f64; 3]) -> bool {
    kind != CV_MOVER_PED
        && old.kind == kind
        && old.model == model
        && old.shape == shape
        && (0..3).all(|i| (0..3).all(|k| (old.axes[i][k] - axes[i][k]).abs() < SETTLE_TURN))
        && (0..3).map(|k| (old.position[k] - position[k]).powi(2)).sum::<f64>() < SETTLE_MOVE * SETTLE_MOVE
}

fn finite(v: &[f32]) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// A body's (scaled) right, forward, up axes.
type Axes = [[f32; 3]; 3];
/// A model's bound triangles, model space metres.
type Template = Arc<Vec<codv_gtaworld::Tri>>;

/// Shape source of a body, so a cached mesh is rebuilt when it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Exact,
    Box,
    Hull,
}

#[derive(Clone)]
pub struct Body {
    pub entity: u32,
    pub kind: u32,
    model: u32,
    shape: Shape,
    axes: [[f32; 3]; 3],
    /// GTA world metres.
    pub position: [f64; 3],
    /// Triangles turned by `axes`, IW4 units around `position`.
    mesh: Arc<MeshWorld>,
    /// Farthest mesh point from `position`, IW4 units.
    reach: f32,
}

/// What the last list held (debug overlay, status line).
#[derive(Clone, Copy, Debug, Default)]
pub struct MoverStats {
    pub vehicles: u32,
    pub peds: u32,
    pub objects: u32,
    pub exact: u32,
    pub boxed: u32,
    pub skipped: u32,
}

#[derive(Default)]
pub struct Movers {
    bodies: Vec<Body>,
    at: Option<Instant>,
    pub stats: MoverStats,
}

impl Movers {
    /// Replaces the list with this frame's entities.
    pub fn update(&mut self, list: &[CvMover], collision: &Streamer) {
        // Last frame's bodies by entity, for mesh reuse.
        let mut previous: HashMap<u32, Body> =
            std::mem::take(&mut self.bodies).into_iter().map(|b| (b.entity, b)).collect();
        let mut stats = MoverStats::default();
        for m in list.iter().take(MAX_MOVERS) {
            match body(m, collision, previous.remove(&m.entity)) {
                Some(b) => {
                    match b.kind {
                        CV_MOVER_VEHICLE => stats.vehicles += 1,
                        CV_MOVER_PED => stats.peds += 1,
                        _ => stats.objects += 1,
                    }
                    match b.shape {
                        Shape::Exact => stats.exact += 1,
                        Shape::Box => stats.boxed += 1,
                        Shape::Hull => {}
                    }
                    self.bodies.push(b);
                }
                None => stats.skipped += 1,
            }
        }
        self.stats = stats;
        self.at = Some(Instant::now());
    }

    /// This frame's bodies (none when the host stopped sending).
    pub fn bodies(&self) -> &[Body] {
        if self.at.is_some_and(|t| t.elapsed() < STALE) { &self.bodies } else { &[] }
    }

    pub fn summary(&self) -> String {
        let s = self.stats;
        if self.bodies().is_empty() {
            return "no movers".into();
        }
        format!("{} cars, {} peds, {} props ({} exact, {} boxes)", s.vehicles, s.peds, s.objects, s.exact, s.boxed)
    }
}

/// One mover as a body, reusing `old`'s mesh when its shape and turn match.
fn body(m: &CvMover, collision: &Streamer, old: Option<Body>) -> Option<Body> {
    if !(finite(&m.position)
        && finite(&m.right)
        && finite(&m.forward)
        && finite(&m.up)
        && finite(&m.box_min)
        && finite(&m.box_max))
    {
        return None;
    }
    let (lo, hi) = (m.box_min, m.box_max);
    let size = [0, 1, 2].map(|k| hi[k] - lo[k]);
    if size.iter().any(|&s| !(0.0..=60.0).contains(&s)) {
        return None;
    }
    let (shape, axes, tris): (Shape, Axes, Option<Template>) = match m.kind {
        CV_MOVER_PED => (Shape::Hull, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], None),
        _ => {
            // Litter (cans, cups, bottles) is stepped over or kicked: no collision.
            if size.iter().all(|&s| s < 0.3) {
                return None;
            }
            let det = {
                let (a, b, c) = (m.right, m.forward, m.up);
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            };
            if !det.is_finite() || det.abs() < 1e-6 {
                return None;
            }
            let request = TemplateRequest { model: m.model, instance: m.instance as usize, lo, hi };
            match collision.template(request) {
                collision::Template::Ready(t) => (Shape::Exact, [m.right, m.forward, m.up], Some(t)),
                collision::Template::Pending | collision::Template::Box => {
                    (Shape::Box, [m.right, m.forward, m.up], None)
                }
            }
        }
    };
    let position = [m.position[0] as f64, m.position[1] as f64, m.position[2] as f64];
    if let Some(old) = &old
        && settled(old, m.kind, m.model, shape, &axes, position)
    {
        return Some(old.clone());
    }
    if let Some(old) = old
        && old.model == m.model
        && old.shape == shape
        && (0..3).all(|i| (0..3).all(|k| (old.axes[i][k] - axes[i][k]).abs() < 1e-3))
    {
        return Some(Body { position, kind: m.kind, ..old });
    }
    let local: Vec<[[f32; 3]; 3]> = match (&tris, shape) {
        (Some(t), _) => t.iter().map(|t| t.v).collect(),
        (None, Shape::Hull) => box_tris([-HULL_HALF, -HULL_HALF, lo[2]], [HULL_HALF, HULL_HALF, hi[2]]),
        (None, _) => box_tris(lo, hi),
    };
    let k = 1.0 / METRES_PER_UNIT;
    let turn = |p: [f32; 3]| -> [f32; 3] {
        [0, 1, 2].map(|c| (axes[0][c] * p[0] + axes[1][c] * p[1] + axes[2][c] * p[2]) * k)
    };
    let units: Vec<Triangle> = local.iter().map(|t| t.map(turn)).collect();
    let reach = units.iter().flatten().map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()).fold(0.0f32, f32::max);
    Some(Body {
        entity: m.entity,
        kind: m.kind,
        model: m.model,
        shape,
        axes,
        position,
        mesh: Arc::new(MeshWorld::build(&units)),
        reach,
    })
}

/// The bodies seen from the player's frame (GTA XY metres at `frame`, IW4 z
/// = GTA z).
pub struct MoverView<'a> {
    pub bodies: &'a [Body],
    pub frame: [f64; 2],
}

impl MoverView<'_> {
    /// The nearest hit and the entity it belongs to (0: none).
    pub fn trace_entity(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> (Trace, u32) {
        let mut best = Trace { fraction: 1.0, endpos: end, ..Trace::default() };
        let mut entity = 0;
        let k = 1.0 / METRES_PER_UNIT as f64;
        let hull = [0, 1, 2].map(|i| mins[i].abs().max(maxs[i].abs())).iter().map(|c| c * c).sum::<f32>().sqrt();
        for b in self.bodies {
            let shift = [
                ((self.frame[0] - b.position[0]) * k) as f32,
                ((self.frame[1] - b.position[1]) * k) as f32,
                (-b.position[2] * k) as f32,
            ];
            let s = [start[0] + shift[0], start[1] + shift[1], start[2] + shift[2]];
            let e = [end[0] + shift[0], end[1] + shift[1], end[2] + shift[2]];
            // Cull: the sweep's segment against the body's sphere.
            let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
            let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let t = if len2 > 0.0 { (-(s[0] * d[0] + s[1] * d[1] + s[2] * d[2]) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let c = [s[0] + d[0] * t, s[1] + d[1] * t, s[2] + d[2] * t];
            let r = b.reach + hull + 2.0;
            if c[0] * c[0] + c[1] * c[1] + c[2] * c[2] > r * r {
                continue;
            }
            let mut hit = b.mesh.trace(s, e, mins, maxs, mask);
            if hit.fraction < best.fraction || (hit.startsolid != 0 && best.startsolid == 0) {
                hit.endpos = [hit.endpos[0] - shift[0], hit.endpos[1] - shift[1], hit.endpos[2] - shift[2]];
                best = hit;
                entity = b.entity;
            }
        }
        (best, entity)
    }
}

impl World for MoverView<'_> {
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Trace {
        self.trace_entity(start, end, mins, maxs, mask).0
    }

    fn mover_hit(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> bool {
        let (t, entity) = self.trace_entity(start, end, mins, maxs, mask);
        entity != 0 && (t.fraction < 1.0 || t.startsolid != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_car_at_rest_keeps_its_pose_through_physics_jitter() {
        let axes: Axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let car = Body {
            entity: 7,
            kind: CV_MOVER_VEHICLE,
            model: 42,
            shape: Shape::Exact,
            axes,
            position: [10.0, 20.0, 30.0],
            mesh: Arc::new(MeshWorld::default()),
            reach: 0.0,
        };
        let mut jitter = axes;
        jitter[0][1] = 0.004;
        assert!(settled(&car, CV_MOVER_VEHICLE, 42, Shape::Exact, &jitter, [10.012, 19.99, 30.005]));
        assert!(!settled(&car, CV_MOVER_VEHICLE, 42, Shape::Exact, &axes, [10.05, 20.0, 30.0]), "driven off");
        let mut turned = axes;
        turned[0][1] = 0.05;
        assert!(!settled(&car, CV_MOVER_VEHICLE, 42, Shape::Exact, &turned, [10.0, 20.0, 30.0]), "turned");
        assert!(!settled(&car, CV_MOVER_VEHICLE, 42, Shape::Box, &axes, [10.0, 20.0, 30.0]), "new shape");
        let ped = Body { kind: CV_MOVER_PED, ..car.clone() };
        assert!(!settled(&ped, CV_MOVER_PED, 42, Shape::Exact, &axes, [10.01, 20.0, 30.0]), "peds follow");
    }
}
