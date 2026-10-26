//! A GTA physics level laid out in this process's own memory, for tests of
//! everything downstream of the reader: the table of 0x30-byte records, the
//! instances, archetypes and BVH bounds `live.rs` walks, holding triangle
//! meshes the caller supplies (GTA world metres).
use crate::live::Level;

/// 8-byte-aligned scratch memory standing in for GTA's heap.
struct Mem(Vec<u64>);

impl Mem {
    fn new(bytes: usize) -> Mem {
        Mem(vec![0; bytes.div_ceil(8).max(1)])
    }
    fn at(&self) -> u64 {
        self.0.as_ptr() as u64
    }
    fn put(&mut self, o: usize, v: &[u8]) {
        let bytes: &mut [u8] = bytemuck(&mut self.0);
        bytes[o..o + v.len()].copy_from_slice(v);
    }
    fn f32s(&mut self, o: usize, v: &[f32]) {
        for (k, c) in v.iter().enumerate() {
            self.put(o + k * 4, &c.to_le_bytes());
        }
    }
}

fn bytemuck(v: &mut [u64]) -> &mut [u8] {
    let len = v.len() * 8;
    // SAFETY: u64 storage viewed as bytes; u8 has no alignment or validity requirements.
    unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr().cast(), len) }
}

const STRIDE: usize = 0x30;
/// Map collision: ped and vehicle include flags, a map type (SK8V's tests).
const MAP_FLAGS: (u32, u32) = (0x3e, 0x07f3_bec0);
/// Stand-in for an address inside GTA's image (an instance's vtable).
const VTABLE: u64 = 0x2000;

pub struct FakeLevel {
    table: Mem,
    used: usize,
    keep: Vec<Mem>,
}

impl Default for FakeLevel {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeLevel {
    pub fn new() -> FakeLevel {
        // Room for 0x400 instances and the 4096+ empty slots that end a table.
        let mut level = FakeLevel { table: Mem::new(0x1800 * STRIDE), used: 0, keep: Vec::new() };
        // The reader wants a table that looks like one (8+ live instances):
        // a few far-off pebbles.
        for i in 0..8 {
            let x = 50_000.0 + i as f32 * 10.0;
            level.add_mesh(&[[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]], &[[0, 1, 2]], 1);
        }
        level
    }

    /// Adds one fixed BVH instance holding `tris` over `verts` (GTA world
    /// metres), all of GTA material `material`, to the table.
    /// Returns the table slot it went into.
    pub fn add_mesh(&mut self, verts: &[[f32; 3]], tris: &[[u16; 3]], material: u8) -> usize {
        let index = self.used;
        assert!(index < 0x400, "fake table full");
        let inst = self.instance(verts, tris, material, index as u16);
        self.table.put(index * STRIDE, &(inst as u64 | 2).to_le_bytes());
        self.used += 1;
        index
    }

    /// Empties a table slot (GTA streamed that collision out).
    pub fn clear(&mut self, index: usize) {
        self.table.put(index * STRIDE, &0u64.to_le_bytes());
    }

    /// One instance (identity placement) holding `tris` over `verts`, not in
    /// the table: a mover's fragInst, as the host would name it. Vertices are
    /// quantised the way GTA stores them (i16 x 3 about the bound's centre).
    pub fn instance(&mut self, verts: &[[f32; 3]], tris: &[[u16; 3]], material: u8, index: u16) -> usize {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in verts {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
        let quantum = [0, 1, 2].map(|k| ((hi[k] - lo[k]) * 0.5 / 32000.0).max(1e-4));
        let mut vb = Mem::new(verts.len() * 6);
        for (i, v) in verts.iter().enumerate() {
            for k in 0..3 {
                let q = ((v[k] - centre[k]) / quantum[k]).round() as i16;
                vb.put(i * 6 + k * 2, &q.to_le_bytes());
            }
        }
        let mut pb = Mem::new(tris.len() * 16);
        for (i, t) in tris.iter().enumerate() {
            for (k, &ix) in t.iter().enumerate() {
                pb.put(i * 16 + 4 + k * 2, &ix.to_le_bytes());
            }
        }
        let mut mats = Mem::new(8);
        mats.put(0, &[material]);
        let pmi = Mem::new(tris.len());
        let mut bound = Mem::new(0x130);
        bound.put(0x10, &[8]);
        bound.f32s(0x20, &hi);
        bound.f32s(0x30, &lo);
        bound.put(0x88, &pb.at().to_le_bytes());
        bound.f32s(0x90, &quantum);
        bound.f32s(0xA0, &centre);
        bound.put(0xB0, &vb.at().to_le_bytes());
        bound.put(0xD0, &(verts.len() as u32).to_le_bytes());
        bound.put(0xD4, &(tris.len() as u32).to_le_bytes());
        bound.put(0xF0, &mats.at().to_le_bytes());
        bound.put(0x118, &pmi.at().to_le_bytes());
        bound.put(0x120, &[1]);
        let mut arch = Mem::new(0x30);
        arch.put(0x20, &bound.at().to_le_bytes());
        arch.put(0x28, &MAP_FLAGS.0.to_le_bytes());
        arch.put(0x2C, &MAP_FLAGS.1.to_le_bytes());
        let mut inst = Mem::new(0x60);
        inst.put(0, &VTABLE.to_le_bytes());
        inst.put(0x10, &arch.at().to_le_bytes());
        inst.put(0x18, &index.to_le_bytes());
        // Row-vector matrix: identity (the bound holds world coordinates).
        inst.f32s(0x20, &[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]);
        let at = inst.at() as usize;
        self.keep.extend([vb, pb, mats, pmi, bound, arch, inst]);
        at
    }

    /// A flat grid floor of `cells` x `cells` squares of `size` metres, its
    /// south-west corner at `corner`, height `z`.
    pub fn add_floor(&mut self, corner: [f32; 2], cells: usize, size: f32, z: f32) -> usize {
        let n = cells + 1;
        let verts: Vec<[f32; 3]> =
            (0..n * n).map(|i| [corner[0] + (i % n) as f32 * size, corner[1] + (i / n) as f32 * size, z]).collect();
        let mut tris = Vec::new();
        for y in 0..cells {
            for x in 0..cells {
                let a = (y * n + x) as u16;
                let (b, c, d) = (a + 1, a + 1 + n as u16, a + n as u16);
                tris.extend([[a, b, c], [a, c, d]]);
            }
        }
        self.add_mesh(&verts, &tris, 4)
    }

    /// An axis-aligned solid block (12 outward triangles).
    pub fn add_block(&mut self, lo: [f32; 3], hi: [f32; 3]) -> usize {
        let verts: Vec<[f32; 3]> =
            (0..8).map(|i| [[lo[0], hi[0]][i & 1], [lo[1], hi[1]][i >> 1 & 1], [lo[2], hi[2]][i >> 2]]).collect();
        let quads = [[1, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]];
        let tris: Vec<[u16; 3]> =
            quads.iter().flat_map(|q: &[u16; 4]| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        self.add_mesh(&verts, &tris, 1)
    }

    /// Where the table is, as the host would report it.
    pub fn level(&self) -> Level {
        Level { base: self.table.at() as usize, image: (0x1000, 0x7FFF_0000_0000) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reader_finds_what_the_fake_level_holds() {
        let mut f = FakeLevel::new();
        f.add_floor([1000.0, 2000.0], 10, 4.0, 30.0);
        f.add_block([1010.0, 2010.0, 30.0], [1012.0, 2012.0, 31.0]);
        let (tris, census, _) = f.level().walk([1020.0, 2020.0], 64.0).expect("walk");
        assert_eq!(tris.len(), 10 * 10 * 2 + 12, "{census}");
        let floor = tris.iter().filter(|t| t.material == 4).flat_map(|t| t.v).all(|p| (p[2] - 30.0).abs() < 0.01);
        assert!(floor, "floor at 30 m after quantisation");
    }

    #[test]
    fn the_incremental_check_sees_only_what_changed_near() {
        use crate::live::changes_near;
        use std::collections::HashSet;
        let mut f = FakeLevel::new();
        f.add_floor([0.0, 0.0], 4, 10.0, 0.0);
        let level = f.level();
        let before = level.words().unwrap();
        let (_, _, seen) = level.walk([20.0, 20.0], 64.0).unwrap();
        let check = |level: &Level, before: &[u64]| {
            let now = level.words().unwrap();
            let changed: Vec<usize> = (0..now.len()).filter(|&i| now.get(i) != before.get(i)).collect();
            let ids = level.near_in(&now, &changed, [20.0, 20.0], 64.0);
            let set: HashSet<usize> = changed.iter().copied().collect();
            (changes_near(&seen, &ids, [20.0, 20.0], 64.0, Some(&set)), changed.len())
        };
        assert_eq!(check(&level, &before), ((0, 0), 0), "nothing changed");
        f.add_block([9000.0, 0.0, 0.0], [9001.0, 1.0, 1.0]);
        assert_eq!(check(&level, &before), ((0, 0), 1), "a change far away does not count");
        let near = f.add_block([10.0, 10.0, 0.0], [11.0, 11.0, 1.0]);
        assert_eq!(check(&level, &before).0, (1, 0), "a bound streamed in near");
        f.clear(near);
        f.clear(8); // the floor (slot 8, after the eight pebbles)
        assert_eq!(check(&level, &before).0, (0, 1), "the floor streamed out");
    }

    #[test]
    fn a_named_instance_reads_in_model_space() {
        let mut f = FakeLevel::new();
        let verts = [[-1.0, -2.0, 0.0], [1.0, -2.0, 0.0], [1.0, 2.0, 1.5], [-1.0, 2.0, 1.5]];
        let inst = f.instance(&verts, &[[0, 1, 2], [0, 2, 3]], 1, 0);
        let tris = crate::live::instance_model(inst).expect("model");
        assert_eq!(tris.len(), 2);
        assert!(
            tris.iter().flat_map(|t| t.v).all(|p| verts.iter().any(|v| (0..3).all(|k| (v[k] - p[k]).abs() < 1e-3)))
        );
        assert!(crate::live::instance_model(0).is_err() && crate::live::instance_model(0x10).is_err());
    }
}
