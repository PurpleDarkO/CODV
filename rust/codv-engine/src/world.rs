//! What pmove collides with. `World` is the host's side (IW4 units); the
//! `Backend` adapter is what iw4L's `movement_iw4::pmove` calls.
//!
//! GTA's world carries none of MW2's authored collision tags, so the adapter
//! supplies two of them: every wall answers the mantle probe (MW2 tags the
//! walls a player may climb with `CONTENTS_MANTLE` and the mantle surface
//! flags), and a player whose hull ends up overlapping the world (a triangle
//! mesh has no inside for iw4L's penetration recovery to push out of) is
//! moved to the nearest free spot (`free_spot`), as retail
//! `PM_CorrectAllSolid` jitters a stuck player.
//!
//! `BoxWorld` is a world of axis-aligned solid boxes swept with iw4L's own
//! brush trace (`trace_iw4::trace_box`, the retail capsule-vs-brush test). It
//! carries the tests and serves as a fallback floor; the GTA world arrives in
//! phase F1 through the same trait.
use movement_iw4::{CONTENTS_MANTLE, CollisionBackend, CorrectSolidOutcome, GroundTraceInput, SURF_MANTLE_ON_OR_OVER};
use trace_iw4::{BrushRef, ENTITYNUM_WORLD, HITTYPE_ENTITY, Trace};

/// iw4L's trace types, for hosts that implement `World`.
pub mod trace {
    pub use trace_iw4::{ENTITYNUM_NONE, ENTITYNUM_WORLD, HITTYPE_ENTITY, Trace};
}

/// Retail CONTENTS_SOLID: inside every player movement trace mask.
pub const CONTENTS_SOLID: u32 = 1;

pub trait World {
    /// Sweep a box (`mins`/`maxs` relative to the path) from `start` to `end`,
    /// IW4 units, against everything whose contents meet `mask`.
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Trace;

    /// The same sweep meets a moving body (a vehicle) before anything else: CODV's auto vault leaves
    /// those to the jump key. Worlds without movers say no.
    fn mover_hit(&self, _start: [f32; 3], _end: [f32; 3], _mins: [f32; 3], _maxs: [f32; 3], _mask: u32) -> bool {
        false
    }
}

/// pmove's view of a `World`.
pub struct Backend<'a, W: World + ?Sized>(pub &'a W);

/// Walls steeper than this (normal z) can be mantled.
const MANTLE_WALL_Z: f32 = 0.7;
/// How far `free_spot` looks for room, units.
pub const UNSTICK_REACH: f32 = 32.0;

impl<W: World + ?Sized> CollisionBackend for Backend<'_, W> {
    fn trace(&self, input: GroundTraceInput) -> Trace {
        if input.tracemask & CONTENTS_MANTLE != 0 && input.tracemask & CONTENTS_SOLID == 0 {
            // The mantle's wall probe: solid walls, tagged as MW2 tags climbable ones.
            let mask = (input.tracemask & !CONTENTS_MANTLE) | CONTENTS_SOLID;
            let mut t = self.0.trace(input.start, input.end, input.mins, input.maxs, mask);
            if t.fraction < 1.0 && t.startsolid == 0 && t.normal[2].abs() < MANTLE_WALL_Z {
                t.surface_flags |= SURF_MANTLE_ON_OR_OVER;
                t.contents |= CONTENTS_MANTLE;
            }
            return t;
        }
        with_surface(self.0.trace(input.start, input.end, input.mins, input.maxs, input.tracemask))
    }

    fn correct_solid(
        &self,
        origin: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        tracemask: u32,
    ) -> Option<CorrectSolidOutcome> {
        let at = free_spot(self.0, origin, mins, maxs, tracemask, UNSTICK_REACH)?;
        // The ground trace pmove would have made from there.
        let trace = with_surface(self.0.trace(
            [at[0], at[1], at[2] + 0.25],
            [at[0], at[1], at[2] - 0.25],
            mins,
            maxs,
            tracemask,
        ));
        Some(CorrectSolidOutcome { origin: at, trace })
    }
}

/// GTA's collision carries no MW2 surface type: what it hits is concrete (MW2's footsteps and
/// landings need one; type 0 makes no sound).
const DEFAULT_SURFACE: u32 = 5 << 20;
fn with_surface(mut t: Trace) -> Trace {
    if t.fraction < 1.0 && (t.surface_flags >> 20) & 0x1f == 0 {
        t.surface_flags |= DEFAULT_SURFACE;
    }
    t
}

/// True when a hull at `at` overlaps nothing.
pub fn is_free<W: World + ?Sized>(world: &W, at: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> bool {
    let t = world.trace(at, at, mins, maxs, mask);
    t.startsolid == 0 && t.allsolid == 0
}

/// Room a spot found by `free_spot` should have beyond it, units: a hull
/// freed into the hollow inside a closed mesh (a wall, a car) has none.
const ROOM: f32 = 16.0;

/// The nearest spot within `reach` units of `origin` where the hull overlaps
/// nothing (`origin` itself when free): retail's one-unit jitter first, then
/// rings outwards, each ring trying straight up, then the eight directions
/// around, then around and up, then straight down. Spots with room beyond
/// them (the same way, `ROOM` units) are preferred over the nearest one.
pub fn free_spot<W: World + ?Sized>(
    world: &W,
    origin: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    reach: f32,
) -> Option<[f32; 3]> {
    if is_free(world, origin, mins, maxs, mask) {
        return Some(origin);
    }
    search(world, origin, mins, maxs, mask, reach, true)
        .or_else(|| search(world, origin, mins, maxs, mask, reach, false))
}

fn search<W: World + ?Sized>(
    world: &W,
    origin: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    reach: f32,
    roomy: bool,
) -> Option<[f32; 3]> {
    let try_at = |d: [f32; 3]| {
        let at = [origin[0] + d[0], origin[1] + d[1], origin[2] + d[2]];
        if !is_free(world, at, mins, maxs, mask) {
            return None;
        }
        if roomy {
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len > 0.0 {
                let k = ROOM / len;
                let beyond = [at[0] + d[0] * k, at[1] + d[1] * k, at[2] + d[2] * k];
                let t = world.trace(at, beyond, mins, maxs, mask);
                if t.fraction < 1.0 || t.startsolid != 0 {
                    return None;
                }
            }
        }
        Some(at)
    };
    // PM_CorrectAllSolid's jitter: -1, 0, 1 on each axis.
    for z in [0.0, 1.0, -1.0] {
        for y in [0.0, -1.0, 1.0] {
            for x in [0.0, -1.0, 1.0] {
                if let Some(at) = try_at([x, y, z]) {
                    return Some(at);
                }
            }
        }
    }
    const DIAG: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let around =
        [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0], [DIAG, DIAG], [-DIAG, DIAG], [DIAG, -DIAG], [-DIAG, -DIAG]];
    for r in [2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0] {
        if r > reach {
            break;
        }
        if let Some(at) = try_at([0.0, 0.0, r]) {
            return Some(at);
        }
        for a in around {
            if let Some(at) = try_at([a[0] * r, a[1] * r, 0.0]) {
                return Some(at);
            }
        }
        for a in around {
            if let Some(at) = try_at([a[0] * r, a[1] * r, r * 0.5]) {
                return Some(at);
            }
        }
        if let Some(at) = try_at([0.0, 0.0, -r]) {
            return Some(at);
        }
    }
    None
}

/// An axis-aligned solid box, IW4 units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolidBox {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

impl SolidBox {
    /// Six outward planes (normal, distance), as a retail brush stores them.
    fn planes(&self) -> [[f32; 4]; 6] {
        let (a, b) = (self.mins, self.maxs);
        [
            [1.0, 0.0, 0.0, b[0]],
            [-1.0, 0.0, 0.0, -a[0]],
            [0.0, 1.0, 0.0, b[1]],
            [0.0, -1.0, 0.0, -a[1]],
            [0.0, 0.0, 1.0, b[2]],
            [0.0, 0.0, -1.0, -a[2]],
        ]
    }
}

#[derive(Clone, Debug, Default)]
pub struct BoxWorld {
    planes: Vec<[[f32; 4]; 6]>,
    flags: [u32; 6],
}

impl BoxWorld {
    pub fn new() -> BoxWorld {
        BoxWorld::default()
    }

    pub fn add(&mut self, b: SolidBox) -> &mut BoxWorld {
        self.planes.push(b.planes());
        self
    }

    /// A flat floor whose top is at `z`, `half` units out from the origin each way.
    pub fn floor(z: f32, half: f32) -> BoxWorld {
        let mut w = BoxWorld::new();
        w.add(SolidBox { mins: [-half, -half, z - 64.0], maxs: [half, half, z] });
        w
    }
}

impl World for BoxWorld {
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Trace {
        let brushes = self.planes.iter().map(|p| BrushRef {
            planes: p,
            contents: CONTENTS_SOLID,
            plane_surface_flags: &self.flags,
            glass_encoded: 0,
        });
        let mut t = trace_iw4::trace_box(brushes, start, end, mins, maxs, mask);
        if t.fraction < 1.0 || t.startsolid != 0 {
            // A world brush, as iw4L's simulation tags it.
            t.hit_type = HITTYPE_ENTITY;
            t.hit_id = ENTITYNUM_WORLD;
            t.contents = CONTENTS_SOLID;
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_dropped_onto_the_floor_stops_on_it() {
        let w = BoxWorld::floor(0.0, 1000.0);
        let t = w.trace([0.0, 0.0, 100.0], [0.0, 0.0, -100.0], [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0], 0x0281_0011);
        assert!(t.fraction < 1.0 && t.startsolid == 0);
        assert!(t.endpos[2].abs() < 1.0, "lands at the floor's top: {:?}", t.endpos);
        assert!(t.normal[2] > 0.99);
        assert_eq!(t.hit_id, ENTITYNUM_WORLD);
        let open = w.trace([0.0, 0.0, 100.0], [50.0, 0.0, 100.0], [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0], 0x0281_0011);
        assert_eq!(open.fraction, 1.0, "nothing in the air");
        let masked = w.trace([0.0, 0.0, 100.0], [0.0, 0.0, -100.0], [-15.0; 3], [15.0; 3], 0x0200_0000);
        assert_eq!(masked.fraction, 1.0, "contents outside the mask are ignored");
    }
}
