//! From SK8V (`skatev-runtime/src/live.rs`), which adapted it for Legacy
//! from Sol4ra's LS-Skate-LiveCollision (GTA V Enhanced), shared with SK8V by
//! its author. CODV: own Vec3, Linux reader for tests (see lib.rs), CODV's
//! rustfmt.
//!
//! The static map read live from GTA's physics level: the table of every
//! collision object the game has loaded.
//!
//! Layout (GTA V Legacy 1.0.3889, proven against the Enhanced notes in
//! SK8V's `docs/PHYSICS-LEVEL.md` and its owner's
//! scan, SK8V `evidence/2026-10-06/level-probe-legacy-scan.log`):
//! - the table is an array of 0x30-byte records; record `+0` is a `phInst*`
//!   with state bits in the low 4 bits; the instance's u16 at `+0x18` is its
//!   own slot index;
//! - `phInst`: `+0x10` archetype, `+0x20` 4x4 row-vector matrix;
//! - archetype: `+0x20` bound, `+0x28` type flags, `+0x2C` include flags;
//! - geometry / BVH bound: see [`decode_geometry`].
//!
//! The host finds the table (by scanning for instances GTA's own natives
//! name, `host/src/physics_level.cpp`) and hands the runtime its address
//! (`sv_set_physics_level`); every read here goes through `ReadProcessMemory`,
//! so a stale or half-written structure fails a read instead of faulting.
//!
//! What counts as static map: BVH bounds (type 8), plus the composites and
//! authored primitives GTA keeps in its fixed state (state bits 2) with map
//! type flags (props: fences, posts, rails). Moving things (vehicles, peds,
//! objects) are other states or carry object/vehicle/ped type flags and are the
//! dynamic system's. Kept by the same rule as the offline bake (`world-cache`
//! `skater_collides`, per composite child): the include flags contain PED and
//! the type flags are not FOLIAGE. That drops GTA's bullet-only copies of the
//! map, vehicle-/cover-/animal-only bits and foliage. The decoding, shapes,
//! vegetation and repeat rules are the bake's (`svwc::bounds`, `svwc::clean`).
use crate::Tri;
use crate::bounds::{Counts, Place, skater_collides, solid_to_sight};
use crate::vec3::Vec3;
#[cfg(windows)]
use std::ffi::c_void;

const MASK: u64 = 0x7FFF_FFFF_FFF0;
const STRIDE: usize = 0x30;
const SLOTS: usize = 0x10000;
const CHUNK: usize = 1024;
const BOUND_BVH: u8 = 8;
const BOUND_COMPOSITE: u8 = 10;
/// The level's state bits for fixed (never simulated) instances.
const STATE_FIXED: u8 = 2;
/// Type flags that mark a mover (CodeWalker `EBoundCompositeFlags` bits 6..14:
/// vehicle, ped, ragdoll, animal, object, cloth, plant): never static map.
const MOVER_TYPES: u32 = 0x7FC0;
/// Composites nest this deep at most.
const MAX_DEPTH: u8 = 4;
/// Polygons whose bounding box lies further than this from the area's edge
/// are left out (slightly more than the area is always fine).
const EDGE: f32 = 32.0;

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn ReadProcessMemory(
        process: isize,
        base: *const c_void,
        buffer: *mut c_void,
        size: usize,
        read: *mut usize,
    ) -> i32;
    fn GetCurrentProcess() -> isize;
}

/// Reads GTA's memory; false when any byte is unreadable.
#[cfg(windows)]
fn read_mem(address: usize, out: &mut [u8]) -> bool {
    if out.is_empty() {
        return true;
    }
    if address < 0x10000 {
        return false;
    }
    let mut got = 0usize;
    // SAFETY: ReadProcessMemory validates both ranges and reports failure.
    let ok = unsafe {
        ReadProcessMemory(GetCurrentProcess(), address as *const c_void, out.as_mut_ptr().cast(), out.len(), &mut got)
    };
    ok != 0 && got == out.len()
}

/// Linux (CODV's tests): the same read through `/proc/self/mem`, which also
/// fails cleanly on unmapped memory.
#[cfg(target_os = "linux")]
fn read_mem(address: usize, out: &mut [u8]) -> bool {
    use std::os::unix::fs::FileExt;
    use std::sync::OnceLock;
    static MEM: OnceLock<Option<std::fs::File>> = OnceLock::new();
    if out.is_empty() {
        return true;
    }
    if address < 0x10000 {
        return false;
    }
    MEM.get_or_init(|| std::fs::File::open("/proc/self/mem").ok())
        .as_ref()
        .is_some_and(|f| f.read_exact_at(out, address as u64).is_ok())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn read_mem(_address: usize, _out: &mut [u8]) -> bool {
    false
}

fn heap(v: u64) -> bool {
    v > 0x10000 && v < 0x7FFF_FFFF_FFFF && v & 7 == 0
}
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn vec3_at(b: &[u8], o: usize) -> Vec3 {
    Vec3::new(f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8))
}

/// Where GTA's physics level is, as the host reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    /// Address of record 0 of the table.
    pub base: usize,
    /// GTA's main image (a phInst's vtable points into it).
    pub image: (usize, usize),
}

/// What one pass found (logged beside the build).
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Census {
    pub slots: usize,
    /// Slots whose own level index disagrees (freed and reused mid-read).
    pub stale: usize,
    pub bvh: usize,
    /// BVH bounds within reach of the area.
    pub in_range: usize,
    /// ... of which the include/type flags leave out.
    pub filtered: usize,
    pub bounds: usize,
    /// Static composites in reach, and their children used / left out by flags.
    pub composites: usize,
    pub children_used: usize,
    pub children_filtered: usize,
    pub failed: usize,
    pub triangles: usize,
    pub primitives: usize,
    pub vegetation: usize,
    pub repeats: usize,
    pub over_ground: usize,
    /// Bullet/camera-only bounds read as gap filler, and the triangles of
    /// theirs that went in (`crate::clean::fill_gaps`).
    pub spare: usize,
    pub filled: usize,
}

impl Census {
    fn add(&mut self, c: Counts) {
        self.triangles += c.triangles;
        self.primitives += c.primitives;
        self.vegetation += c.vegetation;
    }
}

impl std::fmt::Display for Census {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "live physics level: {} slots ({} stale), {} BVH bounds, {} in reach: {} bounds used ({} static composites, {} children used, {} left out by flags), {} left out by include flags, {} unreadable; \
             {} triangles + {} primitive polygons, {} vegetation volumes, {} repeats and {} foliage-over-ground dropped; {} gap triangles from {} bullet/camera-only bounds",
            self.slots,
            self.stale,
            self.bvh,
            self.in_range,
            self.bounds,
            self.composites,
            self.children_used,
            self.children_filtered,
            self.filtered,
            self.failed,
            self.triangles,
            self.primitives,
            self.vegetation,
            self.repeats,
            self.over_ground,
            self.filled,
            self.spare
        )
    }
}

/// One BVH instance that passed the flag rule, or (`spare`) a bullet/camera
/// copy read only to fill the walkable collision's gaps.
struct Bvh {
    inst: usize,
    bound: usize,
    /// phBound type (4 / 8 geometry, 0 / 1 / 3 / 12 / 13 authored primitive).
    kind: u8,
    /// Row-vector matrix (rows are the instance's x, y, z axes and position).
    rows: [Vec3; 4],
    identity: bool,
    spare: bool,
    /// Box centre (GTA XY) and half-diagonal, for the reach test.
    at: [f32; 2],
    reach: f32,
}

/// A bound an area was built from: its identity (instance and bound
/// addresses) and where it sits, so streaming near the skater can be told
/// from streaming at the area's edge (`changed_near`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub id: u64,
    pub at: [f32; 2],
    pub reach: f32,
    /// The table slot it was found in (CODV: for the incremental check).
    pub slot: usize,
}

impl Bvh {
    fn id(&self) -> u64 {
        let mut h = (self.inst as u64) ^ (self.bound as u64).rotate_left(23);
        h = (h ^ (h >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^ (h >> 29)
    }

    fn seen(&self, slot: usize) -> Seen {
        Seen { id: self.id(), at: self.at, reach: self.reach, slot }
    }
}

/// GTA streamed collision in or out within `half` (plus the walk's EDGE and
/// the bound's own reach, the walk's rule) of `centre` since the area holding
/// `built` was walked: a bound there now that the build did not read, or one
/// the build read that is gone. `now` is `Level::near(centre, half)`, sorted.
pub fn changed_near(built: &[Seen], now: &[u64], centre: [f32; 2], half: f32) -> bool {
    let (added, removed) = changes_near(built, now, centre, half, None);
    added + removed > 0
}

/// How many bounds came in (in `now`, not built) and went out (built, in
/// reach, not in `now`) near the area. With `slots`, only built bounds from
/// those table slots count as possibly gone (CODV's incremental check reads
/// only the slots whose words changed).
pub fn changes_near(
    built: &[Seen],
    now: &[u64],
    centre: [f32; 2],
    half: f32,
    slots: Option<&std::collections::HashSet<usize>>,
) -> (usize, usize) {
    let had = |id: &u64| built.iter().any(|b| b.id == *id);
    let added = now.iter().filter(|id| !had(id)).count();
    let removed = built
        .iter()
        .filter(|b| {
            let margin = half + EDGE + b.reach;
            slots.is_none_or(|s| s.contains(&b.slot))
                && (b.at[0] - centre[0]).abs() <= margin
                && (b.at[1] - centre[1]).abs() <= margin
                && now.binary_search(&b.id).is_err()
        })
        .count();
    (added, removed)
}

/// What the walk does with bounds of these flags: `Some(false)` walkable,
/// `Some(true)` gap filler, `None` left out.
fn usage(ty: u32, inc: u32) -> Option<bool> {
    if skater_collides(ty, inc) {
        Some(false)
    } else if solid_to_sight(ty, inc) {
        Some(true)
    } else {
        None
    }
}

impl Bvh {
    fn place(&self) -> Place {
        Place { rows: self.rows.map(|r| r.to_array()), identity: self.identity }
    }
}

impl Level {
    fn slots(&self, mut each: impl FnMut(usize, usize, u8)) -> Result<(), String> {
        let mut chunk = vec![0u8; CHUNK * STRIDE];
        let mut empties = 0usize;
        for first in (0..SLOTS).step_by(CHUNK) {
            // A chunk that runs past the end of the allocation is read in
            // pieces so the slots before the end still count.
            let mut valid = 0;
            if read_mem(self.base + first * STRIDE, &mut chunk) {
                valid = CHUNK;
            } else {
                for s in (0..CHUNK).step_by(64) {
                    if !read_mem(self.base + (first + s) * STRIDE, &mut chunk[s * STRIDE..(s + 64) * STRIDE]) {
                        break;
                    }
                    valid = s + 64;
                }
            }
            if valid == 0 {
                if first == 0 {
                    return Err(format!("physics level table at {:#x} is unreadable", self.base));
                }
                return Ok(());
            }
            for j in 0..valid {
                let inst = (u64_at(&chunk, j * STRIDE) & MASK) as usize;
                if inst == 0 {
                    empties += 1;
                    if empties > 4096 {
                        return Ok(());
                    }
                    continue;
                }
                empties = 0;
                each(first + j, inst, (u64_at(&chunk, j * STRIDE) & 0xF) as u8);
            }
            if valid < CHUNK {
                break;
            }
        }
        Ok(())
    }

    /// The BVH instances within `half` (+ margin) of `centre` that the skater
    /// collides with.
    fn bvh_in_reach(
        &self,
        centre: [f32; 2],
        half: f32,
        census: &mut Census,
        mut each: impl FnMut(Bvh, usize),
    ) -> Result<(), String> {
        self.slots(|index, inst, state| {
            self.examine(index, inst, state, centre, half, census, &mut |b| each(b, index));
        })
    }

    /// One table slot (`index`, its instance and state bits): the static
    /// bounds it holds within reach of the area, to `each`.
    #[allow(clippy::too_many_arguments)]
    fn examine(
        &self,
        index: usize,
        inst: usize,
        state: u8,
        centre: [f32; 2],
        half: f32,
        census: &mut Census,
        mut each: &mut dyn FnMut(Bvh),
    ) {
        census.slots += 1;
        let mut ih = [0u8; 0x60];
        if !heap(inst as u64) || !read_mem(inst, &mut ih) {
            return;
        }
        let vtable = u64_at(&ih, 0) as usize;
        if vtable < self.image.0 || vtable >= self.image.1 {
            return;
        }
        if u16_at(&ih, 0x18) as usize != index {
            census.stale += 1;
            return;
        }
        let arch = u64_at(&ih, 0x10);
        let mut a = [0u8; 8];
        let mut bound = [0u8; 8];
        if !heap(arch) || !read_mem(arch as usize + 0x20, &mut bound) {
            return;
        }
        let bound = u64::from_le_bytes(bound);
        let mut bh = [0u8; 0x40];
        if !heap(bound) || !read_mem(bound as usize, &mut bh) {
            return;
        }
        let is_bvh = bh[0x10] == BOUND_BVH;
        census.bvh += is_bvh as usize;
        let mut rows = [Vec3::ZERO; 4];
        for (k, row) in rows.iter_mut().enumerate() {
            *row = vec3_at(&ih, 0x20 + k * 16);
        }
        let (max, min) = (vec3_at(&bh, 0x20), vec3_at(&bh, 0x30));
        let local = (max + min) * 0.5;
        let reach = ((max - min) * 0.5).length();
        let at = rows[0] * local.x + rows[1] * local.y + rows[2] * local.z + rows[3];
        if !(at.is_finite() && reach.is_finite()) {
            return;
        }
        let margin = half + EDGE + reach;
        if (at.x - centre[0]).abs() > margin || (at.y - centre[1]).abs() > margin {
            return;
        }
        if !read_mem(arch as usize + 0x28, &mut a) {
            return;
        }
        let (ty, inc) = (u32_at(&a, 0), u32_at(&a, 4));
        let fixed = state == STATE_FIXED && ty & MOVER_TYPES == 0 && ty & 0x3F != 0;
        if !is_bvh && fixed {
            if bh[0x10] == BOUND_COMPOSITE {
                census.in_range += 1;
                census.composites += 1;
                composite(bound as usize, rows, inst, 0, centre, half + EDGE, census, &mut each);
                return;
            }
            if matches!(bh[0x10], 0 | 1 | 3 | 4 | 12 | 13) {
                census.in_range += 1;
                if let Some(spare) = usage(ty, inc) {
                    let identity =
                        rows[0] == Vec3::X && rows[1] == Vec3::Y && rows[2] == Vec3::Z && rows[3] == Vec3::ZERO;
                    each(Bvh {
                        inst,
                        bound: bound as usize,
                        kind: bh[0x10],
                        rows,
                        identity,
                        spare,
                        at: [at.x, at.y],
                        reach,
                    });
                } else {
                    census.filtered += 1;
                }
                return;
            }
        }
        if !is_bvh {
            return;
        }
        census.in_range += 1;
        let Some(spare) = usage(ty, inc) else {
            census.filtered += 1;
            return;
        };
        let identity = rows[0] == Vec3::X && rows[1] == Vec3::Y && rows[2] == Vec3::Z && rows[3] == Vec3::ZERO;
        each(Bvh { inst, bound: bound as usize, kind: BOUND_BVH, rows, identity, spare, at: [at.x, at.y], reach });
    }

    /// The table's raw slot words (instance pointer and state bits), read
    /// in large chunks: cheap enough to compare every few seconds.
    pub fn words(&self) -> Result<Vec<u64>, String> {
        // The walk's end rule: past 4096 empty slots in a row the table is over.
        let mut out = Vec::with_capacity(SLOTS);
        let mut chunk = vec![0u8; CHUNK * STRIDE];
        let mut empties = 0usize;
        'table: for first in (0..SLOTS).step_by(CHUNK) {
            if !read_mem(self.base + first * STRIDE, &mut chunk) {
                if first == 0 {
                    return Err(format!("physics level table at {:#x} is unreadable", self.base));
                }
                break;
            }
            for j in 0..CHUNK {
                let w = u64_at(&chunk, j * STRIDE);
                empties = if w & MASK == 0 { empties + 1 } else { 0 };
                out.push(w);
                if empties > 4096 {
                    break 'table;
                }
            }
        }
        out.truncate(out.len() - empties.min(out.len()));
        Ok(out)
    }

    /// The static bounds within reach of the area among `slots` only (their
    /// current `words`): ids, sorted.
    pub fn near_in(&self, words: &[u64], slots: &[usize], centre: [f32; 2], half: f32) -> Vec<u64> {
        let mut ids = Vec::new();
        let mut census = Census::default();
        for &index in slots {
            let Some(&w) = words.get(index) else { continue };
            let inst = (w & MASK) as usize;
            if inst != 0 {
                self.examine(index, inst, (w & 0xF) as u8, centre, half, &mut census, &mut |b| ids.push(b.id()));
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Every loaded bound whose box covers GTA point `p`, with what the walk
    /// does with it (`state/kind/type/include -> fate`): logged when an area
    /// has no floor under the player, to name the bound a filter dropped.
    pub fn under(&self, p: [f32; 2]) -> Result<String, String> {
        fn covers(rows: &[Vec3; 4], h: &[u8], p: [f32; 2]) -> bool {
            let (max, min) = (vec3_at(h, 0x20), vec3_at(h, 0x30));
            let local = (max + min) * 0.5;
            let reach = ((max - min) * 0.5).length();
            let at = rows[0] * local.x + rows[1] * local.y + rows[2] * local.z + rows[3];
            at.is_finite() && (at.x - p[0]).abs() <= reach && (at.y - p[1]).abs() <= reach
        }
        fn children(bound: usize, rows: [Vec3; 4], p: [f32; 2], depth: u8, out: &mut Vec<String>) {
            let mut head = [0u8; 0xB0];
            if !read_mem(bound, &mut head) {
                return;
            }
            let count = (u16_at(&head, 0xA0) as usize).min(512);
            let (list, frames, flags) = (u64_at(&head, 0x70), u64_at(&head, 0x78), u64_at(&head, 0x90));
            let mut pointers = vec![0u8; count * 8];
            if count == 0 || !heap(list) || !read_mem(list as usize, &mut pointers) {
                return;
            }
            let mut frame_bytes = vec![0u8; count * 64];
            let have_frames = heap(frames) && read_mem(frames as usize, &mut frame_bytes);
            let mut flag_bytes = vec![0u8; count * 8];
            let have_flags = heap(flags) && read_mem(flags as usize, &mut flag_bytes);
            for i in 0..count {
                let child = u64_at(&pointers, i * 8);
                let mut ch = [0u8; 0x40];
                if !heap(child) || !read_mem(child as usize, &mut ch) {
                    continue;
                }
                let frame = if have_frames {
                    [0, 1, 2, 3].map(|k| vec3_at(&frame_bytes, i * 64 + k * 16))
                } else {
                    [Vec3::X, Vec3::Y, Vec3::Z, Vec3::ZERO]
                };
                let placed = compose(&rows, &frame);
                if !covers(&placed, &ch, p) {
                    continue;
                }
                let (ty, inc) =
                    if have_flags { (u32_at(&flag_bytes, i * 8), u32_at(&flag_bytes, i * 8 + 4)) } else { (0, 0) };
                let fate = if !skater_collides(ty, inc) { "child flags" } else { "child used" };
                out.push(format!(
                    "  child {}/{ty:08x}/{inc:08x} z {:.1}..{:.1} -> {fate}",
                    ch[0x10],
                    vec3_at(&ch, 0x30).z + placed[3].z,
                    vec3_at(&ch, 0x20).z + placed[3].z
                ));
                if ch[0x10] == BOUND_COMPOSITE && depth < MAX_DEPTH && skater_collides(ty, inc) {
                    children(child as usize, placed, p, depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        self.slots(|index, inst, state| {
            let mut ih = [0u8; 0x60];
            if !heap(inst as u64) || !read_mem(inst, &mut ih) {
                return;
            }
            let arch = u64_at(&ih, 0x10);
            let (mut a, mut bound) = ([0u8; 8], [0u8; 8]);
            if !heap(arch) || !read_mem(arch as usize + 0x20, &mut bound) || !read_mem(arch as usize + 0x28, &mut a) {
                return;
            }
            let bound = u64::from_le_bytes(bound);
            let mut bh = [0u8; 0x40];
            if !heap(bound) || !read_mem(bound as usize, &mut bh) {
                return;
            }
            let rows = [0, 1, 2, 3].map(|k| vec3_at(&ih, 0x20 + k * 16));
            if !covers(&rows, &bh, p) {
                return;
            }
            let (ty, inc, kind) = (u32_at(&a, 0), u32_at(&a, 4), bh[0x10]);
            let fixed = state == STATE_FIXED && ty & MOVER_TYPES == 0 && ty & 0x3F != 0;
            let fate = if u16_at(&ih, 0x18) as usize != index {
                "stale"
            } else if kind == BOUND_BVH || (fixed && matches!(kind, 0 | 1 | 3 | 4 | 12 | 13)) {
                if skater_collides(ty, inc) { "used" } else { "include flags" }
            } else if fixed && kind == BOUND_COMPOSITE {
                "composite"
            } else {
                "not static"
            };
            out.push(format!(
                "{state}/{kind}/{ty:08x}/{inc:08x} z {:.1}..{:.1} -> {fate}",
                vec3_at(&bh, 0x30).z + rows[3].z,
                vec3_at(&bh, 0x20).z + rows[3].z
            ));
            if kind == BOUND_COMPOSITE {
                children(bound as usize, rows, p, 0, &mut out);
            }
        })?;
        Ok(out.join("; "))
    }

    /// The static bounds loaded within `half` of `centre` (by the walk's
    /// reach rule), sorted ids (`Seen::id`) for `changed_near`.
    pub fn near(&self, centre: [f32; 2], half: f32) -> Result<Vec<u64>, String> {
        let mut ids = Vec::new();
        self.bvh_in_reach(centre, half, &mut Census::default(), |b, _| ids.push(b.id()))?;
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }

    /// The collision of the instance GTA placed at `position` with these
    /// (scaled) axes, in its own model space: a prop with no template (spawned
    /// by a script or trainer, stunt props in the Rockstar Editor's director
    /// mode) is in the table like any other. Nearest match within 0.25 m.
    pub fn model_at(&self, position: Vec3, axes: [Vec3; 3]) -> Result<Vec<Tri>, String> {
        let unit = axes.map(|a| a.normalize_or_zero());
        let mut best: Option<(f32, usize)> = None;
        self.slots(|index, inst, _| {
            let mut ih = [0u8; 0x60];
            if !heap(inst as u64) || !read_mem(inst, &mut ih) || u16_at(&ih, 0x18) as usize != index {
                return;
            }
            let vtable = u64_at(&ih, 0) as usize;
            if vtable < self.image.0 || vtable >= self.image.1 {
                return;
            }
            let d = vec3_at(&ih, 0x50).distance(position);
            let aligned = (0..3).all(|k| vec3_at(&ih, 0x20 + k * 16).normalize_or_zero().dot(unit[k]) > 0.99);
            if d < 0.25 && aligned && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, inst));
            }
        })?;
        let (_, inst) = best.ok_or("no instance at the entity's position")?;
        Ok(instance_model(inst)?)
    }

    /// Triangles of the static map within `half` of `centre` (GTA space),
    /// plus what was found and the bounds read.
    pub fn walk(&self, centre: [f32; 2], half: f32) -> Result<(Vec<Tri>, Census, Vec<Seen>), String> {
        let mut census = Census::default();
        let mut tris = Vec::new();
        let mut found = Vec::new();
        self.bvh_in_reach(centre, half, &mut census, |b, slot| found.push((b, slot)))?;
        let clip = (centre[0] - half - EDGE, centre[1] - half - EDGE, centre[0] + half + EDGE, centre[1] + half + EDGE);
        let (mut extra, mut extra_census) = (Vec::new(), Census::default());
        for (b, _) in &found {
            let (out, counts) = if b.spare {
                census.spare += 1;
                (&mut extra, &mut extra_census)
            } else {
                census.bounds += 1;
                (&mut tris, &mut census)
            };
            match b.kind {
                BOUND_BVH | 4 => decode_geometry(b, clip, out, counts),
                _ => decode_primitive(b, clip, out, counts),
            }
        }
        census.failed += extra_census.failed;
        if census.slots < 8 {
            return Err(format!("physics level table at {:#x} holds only {} instances", self.base, census.slots));
        }
        census.repeats = crate::clean::drop_repeats(&mut tris);
        census.over_ground = crate::clean::drop_over_ground(&mut tris);
        census.filled = crate::clean::fill_gaps(&mut tris, extra);
        Ok((tris, census, found.iter().map(|(b, slot)| b.seen(*slot)).collect()))
    }
}

/// Why an instance gave no collision.
#[derive(Clone, Debug, PartialEq)]
pub enum InstanceError {
    /// Its bound could not be read (a freed instance, an unknown bound type).
    Unreadable(String),
    /// Read fine, but nothing in it collides with a ped: GTA lets people walk
    /// through it (a dropped weapon, debris, decorations).
    NoPedCollision { parts: usize, filtered: usize },
}

impl std::fmt::Display for InstanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceError::Unreadable(e) => f.write_str(e),
            InstanceError::NoPedCollision { parts, filtered } => {
                write!(f, "no skater collision in its bound ({parts} parts, {filtered} filtered)")
            }
        }
    }
}

impl From<InstanceError> for String {
    fn from(e: InstanceError) -> String {
        e.to_string()
    }
}

/// The collision of one live instance (`phInst*`, e.g. an entity's fragInst
/// as the host reads it) in its own model space, by the static map's flag
/// rule (what a ped collides with). CODV: split out of SK8V's `model_at` so
/// the host can name the instance and no table walk is needed.
pub fn instance_model(inst: usize) -> Result<Vec<Tri>, InstanceError> {
    if !heap(inst as u64) {
        return Err(InstanceError::Unreadable("no instance".into()));
    }
    let mut ih = [0u8; 0x18];
    let mut bound = [0u8; 8];
    let mut bh = [0u8; 0x40];
    let arch = if read_mem(inst, &mut ih) { u64_at(&ih, 0x10) } else { 0 };
    if !heap(arch)
        || !read_mem(arch as usize + 0x20, &mut bound)
        || !heap(u64::from_le_bytes(bound))
        || !read_mem(u64::from_le_bytes(bound) as usize, &mut bh)
    {
        return Err(InstanceError::Unreadable("instance bound unreadable".into()));
    }
    let bound = u64::from_le_bytes(bound) as usize;
    let rows = [Vec3::X, Vec3::Y, Vec3::Z, Vec3::ZERO];
    let (far, clip) = (1.0e5, (-1.0e5, -1.0e5, 1.0e5, 1.0e5));
    let mut census = Census::default();
    let mut found = Vec::new();
    match bh[0x10] {
        BOUND_COMPOSITE => composite(bound, rows, inst, 0, [0.0, 0.0], far, &mut census, &mut |b| found.push(b)),
        kind @ (BOUND_BVH | 4 | 0 | 1 | 3 | 12 | 13) => {
            found.push(Bvh { inst, bound, kind, rows, identity: true, spare: false, at: [0.0, 0.0], reach: 0.0 })
        }
        kind => return Err(InstanceError::Unreadable(format!("bound type {kind}"))),
    }
    let mut tris = Vec::new();
    for b in found.iter().filter(|b| !b.spare) {
        match b.kind {
            BOUND_BVH | 4 => decode_geometry(b, clip, &mut tris, &mut census),
            _ => decode_primitive(b, clip, &mut tris, &mut census),
        }
    }
    if tris.is_empty() {
        return Err(InstanceError::NoPedCollision { parts: found.len(), filtered: census.children_filtered });
    }
    Ok(tris)
}

/// `parent` placed by `child`: the child's frame first, then the parent's
/// (row vectors, `p' = x*r0 + y*r1 + z*r2 + r3`).
fn compose(parent: &[Vec3; 4], child: &[Vec3; 4]) -> [Vec3; 4] {
    let dir = |v: Vec3| parent[0] * v.x + parent[1] * v.y + parent[2] * v.z;
    [dir(child[0]), dir(child[1]), dir(child[2]), dir(child[3]) + parent[3]]
}

/// A composite's children that the skater collides with and that are within
/// reach, by the bake's per-child flag rule (`primitives.rs` composite arm):
/// children `+0x70`, their frames `+0x78` (64 B each), flags `+0x90` (type,
/// include per child), count `+0xA0`. Nested composites recurse.
#[allow(clippy::too_many_arguments)]
fn composite(
    bound: usize,
    rows: [Vec3; 4],
    inst: usize,
    depth: u8,
    centre: [f32; 2],
    half: f32,
    census: &mut Census,
    each: &mut impl FnMut(Bvh),
) {
    let mut head = [0u8; 0xB0];
    if !read_mem(bound, &mut head) {
        census.failed += 1;
        return;
    }
    let count = (u16_at(&head, 0xA0) as usize).min(512);
    let (list, frames, flags) = (u64_at(&head, 0x70), u64_at(&head, 0x78), u64_at(&head, 0x90));
    let mut pointers = vec![0u8; count * 8];
    if count == 0 || !heap(list) || !read_mem(list as usize, &mut pointers) {
        return;
    }
    let mut frame_bytes = vec![0u8; count * 64];
    let have_frames = heap(frames) && read_mem(frames as usize, &mut frame_bytes);
    let mut flag_bytes = vec![0u8; count * 8];
    let have_flags = heap(flags) && read_mem(flags as usize, &mut flag_bytes);
    for i in 0..count {
        let child = u64_at(&pointers, i * 8);
        let mut ch = [0u8; 0x40];
        if !heap(child) || !read_mem(child as usize, &mut ch) {
            continue;
        }
        let frame = if have_frames {
            [0, 1, 2, 3].map(|k| vec3_at(&frame_bytes, i * 64 + k * 16))
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z, Vec3::ZERO]
        };
        let placed = compose(&rows, &frame);
        let (max, min) = (vec3_at(&ch, 0x20), vec3_at(&ch, 0x30));
        let local = (max + min) * 0.5;
        let reach = ((max - min) * 0.5).length();
        let at = placed[0] * local.x + placed[1] * local.y + placed[2] * local.z + placed[3];
        if !(at.is_finite() && reach.is_finite())
            || (at.x - centre[0]).abs() > half + reach
            || (at.y - centre[1]).abs() > half + reach
        {
            continue;
        }
        let (ty, inc) = if have_flags { (u32_at(&flag_bytes, i * 8), u32_at(&flag_bytes, i * 8 + 4)) } else { (0, 0) };
        let Some(spare) = usage(ty, inc) else {
            census.children_filtered += 1;
            continue;
        };
        let kind = ch[0x10];
        match kind {
            BOUND_COMPOSITE if depth < MAX_DEPTH && !spare => {
                composite(child as usize, placed, inst, depth + 1, centre, half, census, each)
            }
            BOUND_BVH | 4 | 0 | 1 | 3 | 12 | 13 => {
                census.children_used += !spare as usize;
                let identity =
                    placed[0] == Vec3::X && placed[1] == Vec3::Y && placed[2] == Vec3::Z && placed[3] == Vec3::ZERO;
                each(Bvh { inst, bound: child as usize, kind, rows: placed, identity, spare, at: [at.x, at.y], reach });
            }
            _ => {}
        }
    }
}

/// An authored primitive bound (`crate::bounds::primitive`).
fn decode_primitive(b: &Bvh, clip: (f32, f32, f32, f32), out: &mut Vec<Tri>, census: &mut Census) {
    let mut h = [0u8; 0x60];
    if !read_mem(b.bound, &mut h) {
        census.failed += 1;
        return;
    }
    let mut counts = Counts::default();
    crate::bounds::primitive(&h, &b.place(), clip, out, &mut counts);
    census.add(counts);
}

/// A geometry / BVH bound (CodeWalker `Bounds.cs`; resources load in place, so
/// the file layout is the memory layout): polygons `+0x88` (16 B, type =
/// byte 0 & 7), quantum `+0x90`, centre `+0xA0`, vertices `+0xB0` (i16 x 3),
/// counts `+0xD0`/`+0xD4`, materials `+0xF0` (8 B), polygon material index
/// `+0x118`, material count `+0x120`, decoded by `crate::bounds::geometry`.
fn decode_geometry(b: &Bvh, clip: (f32, f32, f32, f32), out: &mut Vec<Tri>, census: &mut Census) {
    let mut h = [0u8; 0x130];
    if !read_mem(b.bound, &mut h) {
        census.failed += 1;
        return;
    }
    let (polys, verts, mats, pmi) = (u64_at(&h, 0x88), u64_at(&h, 0xB0), u64_at(&h, 0xF0), u64_at(&h, 0x118));
    let (quantum, centre) = (vec3_at(&h, 0x90), vec3_at(&h, 0xA0));
    let (nv, np) = (u32_at(&h, 0xD0) as usize, u32_at(&h, 0xD4) as usize);
    let nm = (h[0x120] as usize).max(4);
    if np == 0 || nv == 0 {
        return;
    }
    if !heap(polys) || !heap(verts) || nv > 0x10000 || np > 0x40000 {
        census.failed += 1;
        return;
    }
    let mut vb = vec![0u8; nv * 6];
    let mut pb = vec![0u8; np * 16];
    if !read_mem(verts as usize, &mut vb) || !read_mem(polys as usize, &mut pb) {
        census.failed += 1;
        return;
    }
    let mut material_table = vec![0u8; nm * 8];
    if !heap(mats) || !read_mem(mats as usize, &mut material_table) {
        material_table.clear();
    }
    let mut poly_material = vec![0u8; np];
    if !heap(pmi) || !read_mem(pmi as usize, &mut poly_material) {
        poly_material.clear();
    }
    let mut counts = Counts::default();
    crate::bounds::geometry(
        &vb,
        &pb,
        &material_table,
        &poly_material,
        quantum.to_array(),
        centre.to_array(),
        &b.place(),
        clip,
        out,
        &mut counts,
    );
    census.add(counts);
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    /// 8-byte-aligned scratch memory standing in for GTA's heap.
    struct Mem(Vec<u64>);
    impl Mem {
        fn new(bytes: usize) -> Self {
            Mem(vec![0; bytes.div_ceil(8)])
        }
        fn at(&self) -> u64 {
            self.0.as_ptr() as u64
        }
        fn bytes(&mut self) -> &mut [u8] {
            unsafe { std::slice::from_raw_parts_mut(self.0.as_mut_ptr().cast(), self.0.len() * 8) }
        }
        fn put(&mut self, o: usize, v: &[u8]) {
            self.bytes()[o..o + v.len()].copy_from_slice(v);
        }
    }

    /// A BVH instance (with its archetype and bound) at `translation`, holding
    /// one triangle (material 4) and one box polygon (material 17).
    struct Piece {
        inst: Mem,
        _own: Vec<Mem>,
    }

    fn bvh(index: u16, flags: (u32, u32), translation: [f32; 3], vertices: &[[i16; 3]]) -> Piece {
        let mut bound = Mem::new(0x130);
        let mut verts = Mem::new(vertices.len() * 6);
        let mut polys = Mem::new(32);
        let mut mats = Mem::new(16);
        let mut pmi = Mem::new(8);
        for (i, v) in vertices.iter().enumerate() {
            for (k, c) in v.iter().enumerate() {
                verts.put(i * 6 + k * 2, &c.to_le_bytes());
            }
        }
        for (o, i) in [(4, 0u16), (6, 1), (8, 2)] {
            polys.put(o, &i.to_le_bytes());
        }
        polys.put(16, &[3, 0]);
        for (o, i) in [(20, 0u16), (22, 1), (24, 2), (26, 3)] {
            polys.put(o, &i.to_le_bytes());
        }
        mats.put(0, &[4]);
        mats.put(8, &[17]);
        pmi.put(0, &[0, 1]);
        bound.put(0x10, &[BOUND_BVH]);
        for (o, v) in [
            (0x20, [100.0f32, 100.0, 100.0]),
            (0x30, [-100.0, -100.0, -100.0]),
            (0x90, [0.5, 0.5, 0.5]),
            (0xA0, [1.0, 2.0, 3.0]),
        ] {
            for (k, c) in v.iter().enumerate() {
                bound.put(o + k * 4, &c.to_le_bytes());
            }
        }
        bound.put(0x88, &polys.at().to_le_bytes());
        bound.put(0xB0, &verts.at().to_le_bytes());
        bound.put(0xD0, &(vertices.len() as u32).to_le_bytes());
        bound.put(0xD4, &2u32.to_le_bytes());
        bound.put(0xF0, &mats.at().to_le_bytes());
        bound.put(0x118, &pmi.at().to_le_bytes());
        bound.put(0x120, &[2]);
        let mut arch = Mem::new(0x30);
        arch.put(0x20, &bound.at().to_le_bytes());
        arch.put(0x28, &flags.0.to_le_bytes());
        arch.put(0x2C, &flags.1.to_le_bytes());
        let mut inst = Mem::new(0x60);
        inst.put(0, &0x2000u64.to_le_bytes());
        inst.put(0x10, &arch.at().to_le_bytes());
        inst.put(0x18, &index.to_le_bytes());
        let m =
            [1.0f32, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., translation[0], translation[1], translation[2], 1.];
        for (k, c) in m.iter().enumerate() {
            inst.put(0x20 + k * 4, &c.to_le_bytes());
        }
        Piece { inst, _own: vec![arch, bound, verts, polys, mats, pmi] }
    }

    #[test]
    fn reads_the_map_the_way_the_bake_does_and_bullet_only_copies_only_fill_gaps() {
        let verts = [[0i16, 0, 0], [4, 0, 0], [0, 4, 0], [4, 4, 2]];
        let map = bvh(0, (0x3e, 0x07f3_bec0), [0.0; 3], &verts);
        let bullet = bvh(1, (0x2, 0x0061_0000), [0.0; 3], &verts);
        let far = bvh(2, (0x3e, 0x07f3_bec0), [5000.0, 0.0, 0.0], &verts);
        let mut table = Mem::new(CHUNK * STRIDE);
        for (i, inst) in [&map.inst, &bullet.inst, &far.inst].into_iter().enumerate() {
            table.put(i * STRIDE, &(inst.at() | 2).to_le_bytes());
        }
        // Seven more instances (out of reach) so the table looks like one.
        let filler: Vec<_> = (3..10).map(|i| bvh(i, (0x3e, 0x07f3_bec0), [9000.0, 0.0, 0.0], &verts)).collect();
        for (k, f) in filler.iter().enumerate() {
            table.put((3 + k) * STRIDE, &(f.inst.at() | 2).to_le_bytes());
        }
        let level = Level { base: table.at() as usize, image: (0x1000, 0x7FFF_0000_0000) };
        let (tris, census, seen) = level.walk([0.0, 0.0], 128.0).expect("walk");
        assert_eq!(census.bvh, 10);
        assert_eq!(census.in_range, 2, "the far one and the fillers are out of reach");
        assert_eq!(
            (census.spare, census.filtered, census.filled),
            (1, 0, 0),
            "the bullet-only copy repeats the map: nothing to fill"
        );
        // One triangle plus a box polygon (12 triangles).
        assert_eq!(tris.len(), 13, "{census}");
        // Bit-exact dequantisation: vertex 1 = (4*0.5+1, 0*0.5+2, 0*0.5+3).
        assert_eq!(tris[0].v, [[1.0, 2.0, 3.0], [3.0, 2.0, 3.0], [1.0, 4.0, 3.0]]);
        assert_eq!(tris[0].material, 4);
        assert!(tris[1..].iter().all(|t| t.material == 17), "the box polygon takes material 1");
        // Streaming a bound in near the skater counts; nothing else does,
        // and the one far off only counts once the skater is near it.
        assert!(!changed_near(&seen, &level.near([0.0, 0.0], 16.0).unwrap(), [0.0, 0.0], 16.0));
        let late = bvh(10, (0x3e, 0x07f3_bec0), [10.0, 10.0, 0.0], &verts);
        table.put(10 * STRIDE, &(late.inst.at() | 2).to_le_bytes());
        assert!(changed_near(&seen, &level.near([0.0, 0.0], 16.0).unwrap(), [0.0, 0.0], 16.0));
        assert!(!changed_near(&seen, &level.near([9000.0, 300.0], 16.0).unwrap(), [9000.0, 300.0], 16.0));
        // Dropping a bound the build read counts too.
        table.put(STRIDE, &0u64.to_le_bytes());
        table.put(10 * STRIDE, &0u64.to_le_bytes());
        assert!(changed_near(&seen, &level.near([0.0, 0.0], 16.0).unwrap(), [0.0, 0.0], 16.0));
    }

    #[test]
    fn fixed_composites_give_their_map_children_and_leave_movers_out() {
        let verts = [[0i16, 0, 0], [4, 0, 0], [0, 4, 0], [4, 4, 2]];
        let mesh = bvh(0, (0x3e, 0x07f3_bec0), [0.0; 3], &verts);
        let bound_addr = mesh._own[1].at();
        // A box primitive child: type 3, AABB (0,0,0)..(1,1,1), material 9.
        let mut boxed = Mem::new(0x60);
        boxed.put(0x10, &[3]);
        for (o, v) in [(0x20, [1.0f32, 1.0, 1.0]), (0x30, [0.0, 0.0, 0.0])] {
            for (k, c) in v.iter().enumerate() {
                boxed.put(o + k * 4, &c.to_le_bytes());
            }
        }
        boxed.put(0x4C, &[9]);
        let make_composite = |index: u16, state: u8, kind_flags: (u32, u32)| {
            let children = [bound_addr, bound_addr, boxed.at()];
            let mut list = Mem::new(24);
            let mut frames = Mem::new(3 * 64);
            let mut flags = Mem::new(24);
            for (i, c) in children.iter().enumerate() {
                list.put(i * 8, &c.to_le_bytes());
                let m = [
                    1.0f32,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                    0.,
                    [5.0, 5.0, 0.0][i],
                    [0.0, 0.0, 50.0][i],
                    0.0,
                    1.0,
                ];
                for (k, v) in m.iter().enumerate() {
                    frames.put(i * 64 + k * 4, &v.to_le_bytes());
                }
            }
            for (i, (t, inc)) in
                [(0x3eu32, 0x07f3_bec0u32), (0x2, 0x0061_0000), (0x3e, 0x07f3_bec0)].into_iter().enumerate()
            {
                flags.put(i * 8, &t.to_le_bytes());
                flags.put(i * 8 + 4, &inc.to_le_bytes());
            }
            let mut bound = Mem::new(0xB0);
            bound.put(0x10, &[BOUND_COMPOSITE]);
            for (o, v) in [(0x20, [100.0f32, 100.0, 100.0]), (0x30, [-100.0, -100.0, -100.0])] {
                for (k, c) in v.iter().enumerate() {
                    bound.put(o + k * 4, &c.to_le_bytes());
                }
            }
            bound.put(0x70, &list.at().to_le_bytes());
            bound.put(0x78, &frames.at().to_le_bytes());
            bound.put(0x90, &flags.at().to_le_bytes());
            bound.put(0xA0, &3u16.to_le_bytes());
            let mut arch = Mem::new(0x30);
            arch.put(0x20, &bound.at().to_le_bytes());
            arch.put(0x28, &kind_flags.0.to_le_bytes());
            arch.put(0x2C, &kind_flags.1.to_le_bytes());
            let mut inst = Mem::new(0x60);
            inst.put(0, &0x2000u64.to_le_bytes());
            inst.put(0x10, &arch.at().to_le_bytes());
            inst.put(0x18, &index.to_le_bytes());
            let m = [1.0f32, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
            for (k, v) in m.iter().enumerate() {
                inst.put(0x20 + k * 4, &v.to_le_bytes());
            }
            let _ = state;
            (inst, vec![arch, bound, list, frames, flags])
        };
        let (fixed, _k1) = make_composite(1, STATE_FIXED, (0x3f, 0xffff_ffff));
        let (mover, _k2) = make_composite(2, 1, (0x2000, 0x07f3_3ec4));
        let mut table = Mem::new(CHUNK * STRIDE);
        table.put(0, &(mesh.inst.at() | 2).to_le_bytes());
        table.put(STRIDE, &(fixed.at() | 2).to_le_bytes());
        table.put(2 * STRIDE, &(mover.at() | 1).to_le_bytes());
        let filler: Vec<_> = (3..10).map(|i| bvh(i, (0x3e, 0x07f3_bec0), [9000.0, 0.0, 0.0], &verts)).collect();
        for (k, f) in filler.iter().enumerate() {
            table.put((3 + k) * STRIDE, &(f.inst.at() | 2).to_le_bytes());
        }
        let level = Level { base: table.at() as usize, image: (0x1000, 0x7FFF_0000_0000) };
        let (tris, census, _) = level.walk([0.0, 0.0], 128.0).expect("walk");
        assert_eq!(census.composites, 1, "{census}");
        assert_eq!(census.children_used, 2, "the bullet-only child is only a gap filler: {census}");
        assert_eq!((census.spare, census.filled, census.children_filtered), (1, 0, 0), "{census}");
        // The mesh itself (13), the composite's mesh child (13) and its box (12).
        assert_eq!(tris.len(), 13 + 13 + 12, "{census}");
        // The composite's mesh child sits at the child frame's offset (5, 0, 0).
        assert!(tris.iter().any(|t| t.v[0] == [6.0, 2.0, 3.0] && t.material == 4), "{census}");
        assert!(
            tris.iter().filter(|t| t.material == 9).all(|t| t.v.iter().all(|p| p[1] >= 50.0)),
            "box child offset by its frame"
        );
    }
}
