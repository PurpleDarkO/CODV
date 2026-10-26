//! A triangle soup (GTA's collision, IW4 units) as Modern Warfare 2's own
//! collision mesh: the retail `ClipMeshTables` layout (vertex segments of
//! 1024, partitions of at most 255 triangles, an AABB tree over the
//! partitions), traced with iw4L's retail capsule-vs-mesh walk
//! (`clipmap_iw4::trace_through_mesh_into`). pmove then collides with GTA's
//! streets the same way it collides with a MW2 map's terrain.
//!
//! Retail meshes are one-sided (a capsule moving into a triangle from behind
//! passes), and GTA authors thin surfaces (fences, signs, railings) as single
//! layers, so every triangle goes in once per side.
//!
//! The retail tables index vertices with u16 inside a segment and a segment
//! with u8, which caps one table at 256 x 1024 vertices: larger soups are
//! split spatially into several tables (chunks), traced in turn into one
//! result.
use crate::world::World;
use clipmap_iw4::{ClipAabbNode, ClipMeshTables, ClipPartition, MeshWalkCensus, TraceExtents, VERTS_PER_SEGMENT};
use std::collections::HashMap;
use trace_iw4::Trace;

/// Triangles per partition (AABB leaf). Retail partitions are small; 24 keeps
/// the leaves tight around the capsule's sweep.
const LEAF_TRIS: usize = 24;
/// Retail walkable slope (normal z), the bit an edge or vertex hit inherits.
const WALKABLE_Z: f32 = 0.7;
/// Triangles per table: 3 vertices each plus segment padding always fits in
/// 256 segments (80k * 3 + 256 * 72 < 262144).
const CHUNK_TRIS: usize = 80_000;

/// One triangle, IW4 units, wound however the source wound it.
pub type Triangle = [[f32; 3]; 3];

struct Chunk {
    tables: ClipMeshTables,
    mins: [f32; 3],
    maxs: [f32; 3],
}

/// GTA's collision as MW2 collision meshes.
#[derive(Default)]
pub struct MeshWorld {
    chunks: Vec<Chunk>,
    triangles: usize,
}

#[derive(Clone, Copy)]
struct Item {
    v: Triangle,
    lo: [f32; 3],
    hi: [f32; 3],
    centre: [f32; 3],
}

fn item(v: Triangle) -> Item {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in v {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let centre = [0, 1, 2].map(|k| (v[0][k] + v[1][k] + v[2][k]) / 3.0);
    Item { v, lo, hi, centre }
}

fn bounds(items: &[Item]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for it in items {
        for k in 0..3 {
            lo[k] = lo[k].min(it.lo[k]);
            hi[k] = hi[k].max(it.hi[k]);
        }
    }
    (lo, hi)
}

/// Orders `items` so the first half and the second half are spatially apart:
/// median of the centroids along the widest axis of their spread.
fn split(items: &mut [Item]) -> usize {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for it in items.iter() {
        for k in 0..3 {
            lo[k] = lo[k].min(it.centre[k]);
            hi[k] = hi[k].max(it.centre[k]);
        }
    }
    let axis = (0..3).max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b]))).unwrap_or(0);
    let mid = items.len() / 2;
    items.select_nth_unstable_by(mid, |a, b| a.centre[axis].total_cmp(&b.centre[axis]));
    mid
}

/// Area normal as retail computes it (`cross(v0 - v2, v0 - v1)`): the side a
/// capsule collides from.
fn retail_normal(v: &Triangle) -> Option<[f32; 3]> {
    let a = [v[0][0] - v[2][0], v[0][1] - v[2][1], v[0][2] - v[2][2]];
    let b = [v[0][0] - v[1][0], v[0][1] - v[1][1], v[0][2] - v[1][2]];
    let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    (len.is_finite() && len > 1e-6).then(|| n.map(|c| c / len))
}

struct TableBuilder {
    t: ClipMeshTables,
    segment: usize,
    used: usize,
}

impl TableBuilder {
    fn new() -> TableBuilder {
        TableBuilder { t: ClipMeshTables::default(), segment: 0, used: 0 }
    }

    /// One partition of `items` (welded within the partition), returning its index.
    fn partition(&mut self, items: &[Item]) -> i32 {
        let mut local: Vec<[f32; 3]> = Vec::with_capacity(items.len() * 2);
        let mut index: HashMap<[u32; 3], u16> = HashMap::with_capacity(items.len() * 2);
        let mut tris: Vec<([u16; 3], bool)> = Vec::with_capacity(items.len());
        for it in items {
            let mut ids = [0u16; 3];
            for (k, p) in it.v.iter().enumerate() {
                let key = p.map(f32::to_bits);
                ids[k] = *index.entry(key).or_insert_with(|| {
                    local.push(*p);
                    (local.len() - 1) as u16
                });
            }
            let walkable = retail_normal(&it.v).is_some_and(|n| n[2] >= WALKABLE_Z);
            tris.push((ids, walkable));
        }
        if self.used + local.len() > VERTS_PER_SEGMENT {
            self.segment += 1;
            self.used = 0;
            self.t.verts.resize(self.segment * VERTS_PER_SEGMENT, [0.0; 3]);
        }
        let base = self.used as u16;
        self.t.verts.extend_from_slice(&local);
        self.used += local.len();
        let first_tri = self.t.tri_indices.len() / 3;
        for (ids, walkable) in tris {
            let ti = self.t.tri_indices.len() / 3;
            self.t.tri_indices.extend(ids.map(|i| i + base));
            self.t.tri_surface_flags.push(0);
            let needed = (ti * 3 + 3).div_ceil(8);
            if self.t.tri_edge_is_walkable.len() < needed {
                self.t.tri_edge_is_walkable.resize(needed, 0);
            }
            if walkable {
                for k in 0..3 {
                    let bit = ti * 3 + k;
                    self.t.tri_edge_is_walkable[bit >> 3] |= 1 << (bit & 7);
                }
            }
        }
        self.t.partitions.push(ClipPartition {
            tri_count: items.len() as u8,
            first_tri: first_tri as i32,
            first_vert_segment: self.segment as u8,
            border_count: 0,
            first_border: 0,
        });
        (self.t.partitions.len() - 1) as i32
    }

    /// Fills node `at` with the subtree over `items`.
    fn node(&mut self, at: usize, items: &mut [Item]) {
        let (lo, hi) = bounds(items);
        let pad = 0.01;
        let origin = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
        let half_size = [0, 1, 2].map(|k| (hi[k] - lo[k]) * 0.5 + pad);
        if items.len() <= LEAF_TRIS {
            let part = self.partition(items);
            self.t.aabb_trees[at] = ClipAabbNode { origin, half_size, material_index: 0, child_count: 0, u: part };
            return;
        }
        let first = self.t.aabb_trees.len();
        self.t.aabb_trees.extend([ClipAabbNode::default(); 2]);
        self.t.aabb_trees[at] = ClipAabbNode { origin, half_size, material_index: 0, child_count: 2, u: first as i32 };
        let mid = split(items);
        let (a, b) = items.split_at_mut(mid);
        self.node(first, a);
        self.node(first + 1, b);
    }

    fn finish(mut self, items: &mut [Item]) -> Chunk {
        self.t.aabb_trees.push(ClipAabbNode::default());
        self.node(0, items);
        self.t.aabb_roots = vec![0];
        let (mins, maxs) = bounds(items);
        Chunk { tables: self.t, mins, maxs }
    }
}

fn build_chunks(items: &mut [Item], out: &mut Vec<Chunk>) {
    if items.is_empty() {
        return;
    }
    if items.len() <= CHUNK_TRIS {
        out.push(TableBuilder::new().finish(items));
        return;
    }
    let mid = split(items);
    let (a, b) = items.split_at_mut(mid);
    build_chunks(a, out);
    build_chunks(b, out);
}

impl MeshWorld {
    /// Builds the mesh from `triangles` (IW4 units). Degenerate and
    /// non-finite triangles are dropped.
    pub fn build(triangles: &[Triangle]) -> MeshWorld {
        let mut items = Vec::with_capacity(triangles.len() * 2);
        for t in triangles {
            if !t.iter().flatten().all(|c| c.is_finite()) || retail_normal(t).is_none() {
                continue;
            }
            items.push(item(*t));
            items.push(item([t[0], t[2], t[1]]));
        }
        let mut chunks = Vec::new();
        build_chunks(&mut items, &mut chunks);
        MeshWorld { chunks, triangles: items.len() }
    }

    /// Triangles in the mesh (each side counted).
    pub fn triangle_count(&self) -> usize {
        self.triangles
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// The box the mesh spans, IW4 units (None when empty).
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self.chunks.iter();
        let first = it.next()?;
        let (mut lo, mut hi) = (first.mins, first.maxs);
        for c in it {
            for k in 0..3 {
                lo[k] = lo[k].min(c.mins[k]);
                hi[k] = hi[k].max(c.maxs[k]);
            }
        }
        Some((lo, hi))
    }
}

impl MeshWorld {
    /// The retail walk: the capsule against every chunk it may reach.
    fn trace_raw(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Trace {
        let ext = TraceExtents::new(start, end, mins, maxs, mask);
        let mut best = Trace { fraction: 1.0, endpos: end, ..Trace::default() };
        let mut census = MeshWalkCensus::default();
        let reach = [0, 1, 2].map(|k| mins[k].abs().max(maxs[k].abs()) + 1.0);
        for c in &self.chunks {
            let overlaps = (0..3)
                .all(|k| start[k].min(end[k]) - reach[k] <= c.maxs[k] && start[k].max(end[k]) + reach[k] >= c.mins[k]);
            if overlaps {
                clipmap_iw4::trace_through_mesh_into(&c.tables.as_ref(), &ext, &mut best, &mut census);
                if best.allsolid != 0 {
                    break;
                }
            }
        }
        best
    }
}

/// Sphere heights between the capsule's two end spheres (units) for the normal of a contact the
/// retail sweep missed.
const CYLINDER_STEP: f32 = 4.0;
/// Bisection steps backing a sweep off to free space.
const BACK_OFF_STEPS: usize = 12;

impl World for MeshWorld {
    /// The retail sweep, made to agree with the retail position test. Against a triangle the sweep
    /// moves only one of the capsule's end spheres (the lower for an upright face), never the cylinder
    /// between them: a face whose lower edge is above that sphere's centre (a car's body over its
    /// wheels, a bin's rim, a fence's rail, a stair's nosing: GTA's world is all mesh, MW2's walls are
    /// brushes) let the cylinder through, and the next position test found the hull inside. pmove then
    /// freed it and lost the ground every frame: the grab and the shake against cars. A sweep that ends
    /// inside is backed off to the last free point, with the face's normal from sphere sweeps along the
    /// cylinder, so the player stops or slides there.
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Trace {
        let t = self.trace_raw(start, end, mins, maxs, mask);
        if start == end || t.startsolid != 0 || t.allsolid != 0 {
            return t;
        }
        let inside = |p: [f32; 3]| self.trace_raw(p, p, mins, maxs, mask).startsolid != 0;
        if !inside(t.endpos) {
            return t;
        }
        let at = |f: f32| [0, 1, 2].map(|k| start[k] + (end[k] - start[k]) * f);
        if inside(start) {
            return t;
        }
        let (mut lo, mut hi) = (0.0f32, t.fraction);
        for _ in 0..BACK_OFF_STEPS {
            let mid = (lo + hi) * 0.5;
            if inside(at(mid)) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        // A hair short of the contact, as the sweep stops short of a face.
        let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let f = if len > 0.0 { (lo - 0.125 / len).max(0.0) } else { 0.0 };
        // The face: sphere sweeps of the capsule's radius at heights along its cylinder.
        let half = [0, 1, 2].map(|k| (maxs[k] - mins[k]) * 0.5);
        let centre = [0, 1, 2].map(|k| (maxs[k] + mins[k]) * 0.5);
        let radius = half[0].min(half[2]);
        let reach = half[2] - radius;
        let ball = [radius; 3];
        let mut normal = None;
        let mut first = f32::MAX;
        let mut h = -reach;
        while h <= reach + 1e-3 {
            let lift = [centre[0], centre[1], centre[2] + h];
            let s = [0, 1, 2].map(|k| start[k] + lift[k]);
            let e = [0, 1, 2].map(|k| end[k] + lift[k]);
            let b = self.trace_raw(s, e, ball.map(|c| -c), ball, mask);
            if b.startsolid == 0 && b.fraction < first {
                first = b.fraction;
                normal = Some(b.normal);
            }
            h += CYLINDER_STEP;
        }
        let normal = normal.unwrap_or_else(|| {
            let hz = d[0].hypot(d[1]).max(1e-6);
            [-d[0] / hz, -d[1] / hz, 0.0]
        });
        Trace { fraction: f, endpos: at(f), normal, walkable: u8::from(normal[2] >= WALKABLE_Z), ..t }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{PLAYER_MAXS, PLAYER_MINS, PLAYER_TRACE_MASK};

    /// A flat grid of `n` x `n` quads, `size` units each, at height `z`.
    fn grid(n: usize, size: f32, z: f32) -> Vec<Triangle> {
        let mut out = Vec::new();
        let o = -(n as f32) * size * 0.5;
        for i in 0..n {
            for j in 0..n {
                let (x0, y0) = (o + i as f32 * size, o + j as f32 * size);
                let (x1, y1) = (x0 + size, y0 + size);
                out.push([[x0, y0, z], [x1, y0, z], [x1, y1, z]]);
                out.push([[x0, y0, z], [x1, y1, z], [x0, y1, z]]);
            }
        }
        out
    }

    #[test]
    fn tables_respect_the_retail_limits() {
        // 2 x 250 x 250 x 2 = 250k triangle sides: several tables.
        let world = MeshWorld::build(&grid(250, 40.0, 0.0));
        assert_eq!(world.triangle_count(), 250 * 250 * 2 * 2);
        assert!(world.chunk_count() >= 4, "{} chunks", world.chunk_count());
        for c in &world.chunks {
            let t = &c.tables;
            assert!(t.verts.len() <= 256 * VERTS_PER_SEGMENT);
            for p in &t.partitions {
                assert!(p.tri_count as usize <= LEAF_TRIS && p.tri_count > 0);
                let base = p.first_vert_segment as usize * VERTS_PER_SEGMENT;
                for ti in p.first_tri as usize..p.first_tri as usize + p.tri_count as usize {
                    for k in 0..3 {
                        assert!(base + (t.tri_indices[ti * 3 + k] as usize) < t.verts.len());
                        assert!((t.tri_indices[ti * 3 + k] as usize) < VERTS_PER_SEGMENT);
                    }
                }
            }
            assert_eq!(t.partitions.len() * 2 - 1, t.aabb_trees.len(), "binary tree over the partitions");
        }
    }

    #[test]
    fn the_player_hull_lands_on_the_mesh_from_either_side_of_a_wall() {
        let mut tris = grid(8, 64.0, 0.0);
        // A thin wall at x = 100, from y -200..200, 0..120 high, wound one way only.
        tris.push([[100.0, -200.0, 0.0], [100.0, 200.0, 0.0], [100.0, 200.0, 120.0]]);
        tris.push([[100.0, -200.0, 0.0], [100.0, 200.0, 120.0], [100.0, -200.0, 120.0]]);
        let w = MeshWorld::build(&tris);
        let down = w.trace([0.0, 0.0, 50.0], [0.0, 0.0, -50.0], PLAYER_MINS, PLAYER_MAXS, PLAYER_TRACE_MASK);
        assert!(down.fraction < 1.0 && down.walkable == 1, "{down:?}");
        assert!((down.endpos[2] - 0.0).abs() < 0.2, "feet on the floor: {down:?}");
        assert!(down.normal[2] > 0.99);
        for (from, to) in [(0.0f32, 200.0f32), (200.0, 0.0)] {
            let t = w.trace([from, 0.0, 10.0], [to, 0.0, 10.0], PLAYER_MINS, PLAYER_MAXS, PLAYER_TRACE_MASK);
            assert!(t.fraction < 1.0, "wall stops the hull going {from} -> {to}: {t:?}");
            let gap = (t.endpos[0] - 100.0).abs();
            assert!((14.5..15.5).contains(&gap), "stops a radius away: {t:?}");
            assert!(t.walkable == 0 && t.normal[2].abs() < 0.01);
        }
        let miss = w.trace([0.0, 0.0, 10.0], [50.0, 0.0, 10.0], PLAYER_MINS, PLAYER_MAXS, PLAYER_TRACE_MASK);
        assert_eq!(miss.fraction, 1.0);
        assert_eq!(miss.endpos, [50.0, 0.0, 10.0]);
    }

    /// A solid box as 12 triangles (outward CCW, the way GTA's box polygons
    /// decode), IW4 units.
    fn block(lo: [f32; 3], hi: [f32; 3]) -> Vec<Triangle> {
        let c = |x: usize, y: usize, z: usize| [[lo[0], hi[0]][x], [lo[1], hi[1]][y], [lo[2], hi[2]][z]];
        let quads = [
            [c(1, 0, 0), c(1, 1, 0), c(1, 1, 1), c(1, 0, 1)],
            [c(0, 0, 0), c(0, 0, 1), c(0, 1, 1), c(0, 1, 0)],
            [c(0, 1, 0), c(0, 1, 1), c(1, 1, 1), c(1, 1, 0)],
            [c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)],
            [c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)],
            [c(0, 0, 0), c(0, 1, 0), c(1, 1, 0), c(1, 0, 0)],
        ];
        quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect()
    }

    #[test]
    fn pmove_runs_climbs_and_stops_on_a_triangle_mesh() {
        use crate::player::{MoveInput, Mw2Player};
        let mut tris = grid(40, 64.0, 0.0);
        tris.extend(block([300.0, -100.0, 0.0], [400.0, 100.0, 16.0])); // a curb-like 16-unit step
        tris.extend(block([700.0, -100.0, 0.0], [720.0, 100.0, 200.0])); // a wall
        let w = MeshWorld::build(&tris);
        let mut p = Mw2Player::new([0.0, 0.0, 2.0], 0.0);
        let idle = MoveInput { msec: 16, ..Default::default() };
        let mut o = p.step(&idle, &w);
        for _ in 0..30 {
            o = p.step(&idle, &w);
        }
        assert!(o.on_ground && o.origin[2].abs() < 0.5, "settles on the mesh: {o:?}");
        let run = MoveInput { msec: 16, forward: 127, ..Default::default() };
        let mut top = 0.0f32;
        let mut speed = 0.0f32;
        for _ in 0..140 {
            o = p.step(&run, &w);
            if o.origin[0] < 280.0 {
                speed = speed.max(o.velocity[0]);
            }
            if (300.0..400.0).contains(&o.origin[0]) {
                top = top.max(o.origin[2]);
            }
        }
        assert!((speed - 190.0).abs() < 4.0, "runs at 190 on the mesh: {speed}");
        assert!((top - 16.0).abs() < 0.5, "steps onto the 16-unit block: top {top}");
        for _ in 0..150 {
            o = p.step(&run, &w);
        }
        assert!(o.on_ground, "{o:?}");
        assert!((683.0..=686.0).contains(&o.origin[0]), "stops a hull radius from the wall at 700: {o:?}");
    }

    /// Release-build timing for a dense area (run with --release --ignored).
    #[test]
    #[ignore]
    fn dense_area_timing() {
        let mut tris = grid(200, 25.0, 0.0); // 80k floor triangles over 5000 units
        let mut seed = 1u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 10000) as f32 / 10000.0
        };
        for _ in 0..70_000 {
            let (x, y, z) = (rnd() * 5000.0 - 2500.0, rnd() * 5000.0 - 2500.0, rnd() * 300.0);
            tris.push([[x, y, z], [x + 20.0 * rnd(), y, z + 20.0 * rnd()], [x, y + 20.0 * rnd(), z + 10.0]]);
        }
        let t0 = std::time::Instant::now();
        let w = MeshWorld::build(&tris);
        let built = t0.elapsed();
        let t1 = std::time::Instant::now();
        let mut hits = 0;
        for i in 0..20_000 {
            let x = (i % 200) as f32 * 20.0 - 2000.0;
            let t = w.trace([x, 0.0, 40.0], [x + 8.0, 3.0, 30.0], PLAYER_MINS, PLAYER_MAXS, PLAYER_TRACE_MASK);
            hits += usize::from(t.fraction < 1.0);
        }
        let per = t1.elapsed() / 20_000;
        println!(
            "built {} sides in {} tables: {built:?}; {per:?} per player sweep ({hits} hits)",
            w.triangle_count(),
            w.chunk_count()
        );
    }
}
