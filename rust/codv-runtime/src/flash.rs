//! MW2's view muzzle flash: the held gun's `viewFlashEffect` (imported as `viewmodels/fx/*.cfx`) played at
//! its `tag_flash` on every shot, simulated as IW4's effect system runs an FxEffectDef's elements, and
//! handed to the host as sprite quads in the view model's space plus the lights it casts.
//!
//! What is followed: the one-shot and looping elements of an effect and its runners (the child effects a
//! runner picks at random), each particle's spawn delay, life, spawn offset, velocity graph (the
//! displacement samples, local frame), gravity, visual-state samples (colour, size, rotation, scale)
//! with their per-particle randoms, the atlas (a random or fixed start, played at its rate or over the
//! life) and the material's blend (added or alpha-blended). Elements that run relative to the effect
//! follow the tag; the others stay where they were spawned in the world while the view turns.
//! Billboards face the view, tails stretch along their velocity, omni lights become lights in GTA's
//! world. A shell eject plays the same way, its model elements (the brass, spun by their angular velocity
//! and pulled by gravity) handed out as placed models; each stops at the player's floor, as there is no
//! collision here. Decals, sounds and trails are left out.
use codv_mw2data::vm::{self, Effect, FxElem, fx_kind};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// FxElemDef flags the simulation reads (IW4 FX_ELEM_*).
mod flag {
    pub const RUN_MASK: u32 = 0xC0;
    pub const RUN_RELATIVE_TO_EFFECT: u32 = 0x80;
    pub const HAS_GRAVITY: u32 = 0x0400_0000;
    pub const NONUNIFORM_SCALE: u32 = 0x1000_0000;
}
/// FxElemAtlas behaviour bits.
mod atlas {
    pub const START_MASK: u8 = 3;
    pub const START_RANDOM: u8 = 1;
    pub const START_INDEXED: u8 = 2;
    pub const PLAY_OVER_LIFE: u8 = 4;
    pub const LOOP_ONLY_N_TIMES: u8 = 8;
}

/// MW2's gravity (g_gravity, inches per second squared): an element's gravity is a fraction of it.
const GRAVITY: f32 = 800.0;
/// The most particles alive at once (a full-auto burst of the busiest flash stays well under it).
const MAX_PARTICLES: usize = 512;
/// A runner's children nest at most this deep.
const MAX_DEPTH: u32 = 3;
/// Looping elements of a view flash spawn at most this many particles per shot.
const MAX_LOOPED: i32 = 8;

/// One sprite for the host: corners in the view model's space (D3D view axes, inches), counter-clockwise
/// from the bottom left; its atlas cell; its colour (MW2's, 0..255); added (true) or alpha-blended.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub corners: [[f32; 3]; 4],
    pub uv: [f32; 4],
    pub rgba: [u8; 4],
    pub additive: bool,
    pub image: Arc<str>,
}

/// A light the flash casts: in MW2's world (inches), its colour (0..1), radius (inches) and intensity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
}

/// Per particle random numbers (0..1), one per thing IW4 randomises independently.
#[derive(Clone, Copy, Debug, Default)]
struct Rand {
    origin: [f32; 3],
    motion: f32,
    color: f32,
    size: [f32; 2],
    rotation: f32,
    initial_rotation: f32,
    scale: f32,
    gravity: f32,
    atlas: f32,
    material: f32,
    angles: [f32; 3],
    spin: [f32; 3],
}

#[derive(Clone, Debug)]
struct Particle {
    effect: Arc<Effect>,
    elem: usize,
    /// Milliseconds since it was spawned (negative while its delay runs).
    age: f32,
    life: f32,
    rand: Rand,
    /// Its spawn offset in the effect's frame (inches).
    origin: Vec3,
    /// The effect's frame: in view space (relative to the effect: the tag now) or in MW2's world (the
    /// frame at the spawn, kept while the view moves).
    frame: Mat4,
    follows_tag: bool,
    /// The effect's frame relative to the tag (a runner's child sits at the runner's offset).
    local: Mat4,
}

/// The effects on disk, read once each.
#[derive(Default)]
struct Library {
    dir: PathBuf,
    effects: HashMap<String, Option<Arc<Effect>>>,
}

impl Library {
    fn get(&mut self, name: &str, log: &dyn Fn(&str)) -> Option<Arc<Effect>> {
        let key = name.to_ascii_lowercase();
        if let Some(e) = self.effects.get(&key) {
            return e.clone();
        }
        let path = self.dir.join(format!("{}.cfx", vm::file_stem(&key)));
        let e = match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| vm::read_effect(&b)) {
            Ok(e) => Some(Arc::new(e)),
            Err(why) => {
                log(&format!("mw2: muzzle flash {name}: {} ({why}); re-run the MW2 import", path.display()));
                None
            }
        };
        self.effects.insert(key, e.clone());
        e
    }
}

/// The colour maps an effect and the effects its runners spawn draw with (file stems), read from `dir`.
pub fn images(dir: &std::path::Path, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut more = vec![name.to_ascii_lowercase()];
    while let Some(n) = more.pop() {
        if !seen.insert(n.clone()) || seen.len() > 32 {
            continue;
        }
        let Ok(bytes) = std::fs::read(dir.join(format!("{}.cfx", vm::file_stem(&n)))) else { continue };
        let Ok(e) = vm::read_effect(&bytes) else { continue };
        for x in &e.elems {
            more.extend(x.runs.iter().map(|r| r.to_ascii_lowercase()));
            for m in &x.materials {
                let stem = vm::file_stem(&m.color_map.to_ascii_lowercase());
                if !stem.is_empty() && !out.contains(&stem) {
                    out.push(stem);
                }
            }
        }
    }
    out
}

/// The models an effect and the effects its runners spawn throw (lower-case XModel names), read from `dir`.
pub fn models(dir: &std::path::Path, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut more = vec![name.to_ascii_lowercase()];
    while let Some(n) = more.pop() {
        if !seen.insert(n.clone()) || seen.len() > 32 {
            continue;
        }
        let Ok(bytes) = std::fs::read(dir.join(format!("{}.cfx", vm::file_stem(&n)))) else { continue };
        let Ok(e) = vm::read_effect(&bytes) else { continue };
        for x in &e.elems {
            more.extend(x.runs.iter().map(|r| r.to_ascii_lowercase()));
            for m in &x.models {
                let m = m.to_ascii_lowercase();
                if !out.contains(&m) {
                    out.push(m);
                }
            }
        }
    }
    out
}

/// A small, fast generator (xorshift32): the flash only needs variety, not quality.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    fn rand(&mut self) -> Rand {
        Rand {
            origin: [self.next(), self.next(), self.next()],
            motion: self.next(),
            color: self.next(),
            size: [self.next(), self.next()],
            rotation: self.next(),
            initial_rotation: self.next(),
            scale: self.next(),
            gravity: self.next(),
            atlas: self.next(),
            material: self.next(),
            angles: [self.next(), self.next(), self.next()],
            spin: [self.next(), self.next(), self.next()],
        }
    }
}

/// The view's frame in MW2's world: D3D view axes (x right, y up, z forward) to world inches, from the
/// eye and the view angles (pitch down positive, yaw, roll: degrees).
pub fn view_to_world(eye: [f32; 3], angles: [f32; 3]) -> Mat4 {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    let forward = Vec3::new(cp * cy, cp * sy, -sp);
    let right = Vec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = Vec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    Mat4::from_cols(right.extend(0.0), up.extend(0.0), forward.extend(0.0), Vec3::from_array(eye).extend(1.0))
}

/// A model an effect threw (the brass): its XModel name and where it is (model to view space: D3D view
/// axes, inches).
#[derive(Clone, Debug, PartialEq)]
pub struct Thrown {
    pub model: Arc<str>,
    pub at: Mat4,
}

/// The flashes in flight.
pub struct Flashes {
    library: Library,
    particles: Vec<Particle>,
    rng: Rng,
    view_to_world: Mat4,
    tag: Option<Mat4>,
    /// Model elements only (a shell eject), or everything else (a muzzle flash).
    models: bool,
    /// The player's floor in MW2's world (inches): thrown models stop there.
    floor: Option<f32>,
}

impl Flashes {
    pub fn new(dir: PathBuf) -> Flashes {
        Flashes {
            library: Library { dir, effects: HashMap::new() },
            particles: Vec::new(),
            rng: Rng(0x9E37_79B9),
            view_to_world: Mat4::IDENTITY,
            tag: None,
            models: false,
            floor: None,
        }
    }

    /// For shell ejects: only their model elements (the brass) play.
    pub fn for_models(dir: PathBuf) -> Flashes {
        Flashes { models: true, rng: Rng(0x85EB_CA6B), ..Flashes::new(dir) }
    }

    /// The player's floor (MW2's world z, inches), where thrown models stop; None: no floor.
    pub fn set_floor(&mut self, z: Option<f32>) {
        self.floor = z;
    }

    /// Whether any particle is alive.
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        !self.particles.is_empty()
    }

    /// Everything off (the mode stopped, the weapon changed).
    pub fn clear(&mut self) {
        self.particles.clear();
    }

    /// One frame: advance by `dt_ms`; `tag` is the gun's tag_flash in view space this frame (None: no
    /// view model), `view_to_world` the view's frame in MW2's world. With `fire`, the named effect is
    /// played at the tag.
    pub fn tick(&mut self, dt_ms: f32, tag: Option<Mat4>, view_to_world: Mat4, fire: Option<&str>, log: &dyn Fn(&str)) {
        self.view_to_world = view_to_world;
        self.tag = tag;
        for p in &mut self.particles {
            p.age += dt_ms;
        }
        self.particles.retain(|p| p.age < p.life);
        if let Some(floor) = self.floor {
            let gone: Vec<bool> = self.particles.iter().map(|p| self.below(p, floor)).collect();
            let mut k = 0;
            self.particles.retain(|_| {
                k += 1;
                !gone[k - 1]
            });
        }
        if let (Some(name), Some(tag)) = (fire, tag)
            && let Some(effect) = self.library.get(name, log)
        {
            self.spawn(effect, tag, Mat4::IDENTITY, 0.0, 0, log);
        }
    }

    /// Spawns `effect`'s elements at `tag * local`, `delay` ms from now.
    fn spawn(&mut self, effect: Arc<Effect>, tag: Mat4, local: Mat4, delay: f32, depth: u32, log: &dyn Fn(&str)) {
        for (i, e) in effect.elems.iter().enumerate() {
            let count = match e.category {
                1 => e.spawn[0] + (self.rng.next() * (e.spawn[1].max(0) + 1) as f32) as i32,
                0 => e.spawn[1].clamp(1, MAX_LOOPED),
                _ => 0,
            };
            for k in 0..count.max(0) {
                // A looping element spawns one particle per interval.
                let looped = if e.category == 0 { k as f32 * e.spawn[0].max(1) as f32 } else { 0.0 };
                let start = delay + looped + range_i(e.delay_ms, self.rng.next());
                let rand = self.rng.rand();
                let origin = Vec3::new(
                    range(e.origin[0], rand.origin[0]),
                    range(e.origin[1], rand.origin[1]),
                    range(e.origin[2], rand.origin[2]),
                );
                if e.kind == fx_kind::RUNNER {
                    if depth < MAX_DEPTH && !e.runs.is_empty() {
                        let pick = &e.runs[((self.rng.next() * e.runs.len() as f32) as usize).min(e.runs.len() - 1)];
                        if let Some(child) = self.library.get(pick, log) {
                            self.spawn(child, tag, local * Mat4::from_translation(origin), start, depth + 1, log);
                        }
                    }
                    continue;
                }
                if self.models {
                    if e.kind != fx_kind::MODEL || e.models.is_empty() {
                        continue;
                    }
                } else {
                    if !matches!(e.kind, fx_kind::BILLBOARD | fx_kind::ORIENTED | fx_kind::TAIL | fx_kind::OMNI_LIGHT) {
                        continue;
                    }
                    if e.kind != fx_kind::OMNI_LIGHT && !e.materials.iter().any(|m| !m.color_map.is_empty()) {
                        continue; // distortion: no image to draw
                    }
                }
                if self.particles.len() >= MAX_PARTICLES {
                    return;
                }
                let follows_tag = e.flags & flag::RUN_MASK == flag::RUN_RELATIVE_TO_EFFECT;
                let frame = if follows_tag { Mat4::IDENTITY } else { self.view_to_world * tag * local };
                let life = range_i(e.life_ms, self.rng.next()).max(1.0);
                self.particles.push(Particle {
                    effect: Arc::clone(&effect),
                    elem: i,
                    age: -start,
                    life,
                    rand,
                    origin,
                    frame,
                    follows_tag,
                    local,
                });
            }
        }
    }

    /// Where a particle is in view space, its frame's axes there (rotation only) and its velocity there.
    fn place(&self, p: &Particle, e: &FxElem, t: f32) -> Option<(Vec3, Mat4, Vec3)> {
        let frame = if p.follows_tag { self.tag? * p.local } else { self.view_to_world.inverse() * p.frame };
        let mut offset = p.origin;
        let mut velocity = Vec3::X;
        if e.motion.len() >= 2 {
            let intervals = (e.motion.len() - 1) as f32;
            let f = (t * intervals).clamp(0.0, intervals);
            let i = (f as usize).min(e.motion.len() - 2);
            let frac = f - i as f32;
            let sample = |v: [[f32; 3]; 2]| Vec3::from_array(v[0]) + Vec3::from_array(v[1]) * p.rand.motion;
            let (a, b) = (&e.motion[i], &e.motion[i + 1]);
            // The displacement samples are per interval of the graph: scaled by an interval's length.
            let delta = sample(a.local_delta).lerp(sample(b.local_delta), frac);
            offset += delta * (p.life / intervals);
            velocity = sample(a.local_velocity).lerp(sample(b.local_velocity), frac);
        }
        let mut position = frame.transform_point3(offset);
        if e.flags & flag::HAS_GRAVITY != 0 {
            // Gravity pulls along the world's up (a negative gravity lifts smoke).
            let g = range(e.gravity, p.rand.gravity) * GRAVITY;
            let s = p.age.max(0.0) / 1000.0;
            let up = self.view_to_world.inverse().transform_vector3(Vec3::Z);
            position -= up * (0.5 * g * s * s);
        }
        Some((position, frame, frame.transform_vector3(velocity)))
    }

    /// Whether a thrown model has fallen to `floor` (MW2's world z).
    fn below(&self, p: &Particle, floor: f32) -> bool {
        let e = &p.effect.elems[p.elem];
        if e.kind != fx_kind::MODEL || p.age <= 0.0 {
            return false;
        }
        let t = (p.age / p.life).clamp(0.0, 1.0);
        self.place(p, e, t).is_some_and(|(at, _, _)| self.view_to_world.transform_point3(at).z < floor)
    }

    /// This frame's thrown models (oldest first): spun from their spawn angles by their angular velocity
    /// (IW4 keeps both in radians, the velocity per millisecond), scaled by their visual state.
    pub fn thrown(&self) -> Vec<Thrown> {
        let mut out = Vec::new();
        for p in &self.particles {
            let e = &p.effect.elems[p.elem];
            if p.age < 0.0 || e.kind != fx_kind::MODEL || e.models.is_empty() {
                continue;
            }
            let t = (p.age / p.life).clamp(0.0, 1.0);
            let Some((at, frame, _)) = self.place(p, e, t) else { continue };
            let angle =
                |i: usize| range(e.angles[i], p.rand.angles[i]) + range(e.angular_velocity[i], p.rand.spin[i]) * p.age;
            // IW angles: pitch about y (down positive), yaw about z, roll about x.
            let spin =
                Quat::from_rotation_z(angle(1)) * Quat::from_rotation_y(angle(0)) * Quat::from_rotation_x(angle(2));
            let scale = visual(e, t, &p.rand).scale;
            let scale = if scale > 0.0 { scale } else { 1.0 };
            let axes = Mat4::from_mat3(Mat3::from_mat4(frame));
            let model = &e.models[((p.rand.material * e.models.len() as f32) as usize).min(e.models.len() - 1)];
            out.push(Thrown {
                model: Arc::from(model.to_ascii_lowercase()),
                at: Mat4::from_translation(at) * axes * Mat4::from_quat(spin) * Mat4::from_scale(Vec3::splat(scale)),
            });
        }
        out
    }

    /// This frame's sprites (in spawn order: the effect's own order, older shots first) and lights.
    pub fn draw(&self) -> (Vec<Quad>, Vec<Light>) {
        let mut quads = Vec::new();
        let mut lights = Vec::new();
        for p in &self.particles {
            if p.age < 0.0 {
                continue;
            }
            let e = &p.effect.elems[p.elem];
            let t = (p.age / p.life).clamp(0.0, 1.0);
            let Some((at, frame, velocity)) = self.place(p, e, t) else { continue };
            let v = visual(e, t, &p.rand);
            if e.kind == fx_kind::OMNI_LIGHT {
                let world = self.view_to_world.transform_point3(at);
                let c = v.color.map(|c| c as f32 / 255.0);
                lights.push(Light {
                    position: world.to_array(),
                    color: [c[0], c[1], c[2]],
                    radius: v.size[0].max(0.0),
                    intensity: v.scale.max(0.0) * c[3],
                });
                continue;
            }
            let usable: Vec<&vm::Material> = e.materials.iter().filter(|m| !m.color_map.is_empty()).collect();
            if usable.is_empty() || v.color[3] == 0 && !additive(usable[0]) {
                continue;
            }
            let m = usable[((p.rand.material * usable.len() as f32) as usize).min(usable.len() - 1)];
            let rotation = range(e.initial_rotation, p.rand.initial_rotation) + v.rotation;
            let (w, h) = (v.size[0], if e.flags & flag::NONUNIFORM_SCALE != 0 { v.size[1] } else { v.size[0] });
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let corners = match e.kind {
                fx_kind::TAIL => tail(at, velocity, w, h),
                fx_kind::ORIENTED => {
                    // In the plane across the effect's axis (its y and z), facing along the barrel.
                    let (s, c) = rotation.sin_cos();
                    let (y, z) = (
                        frame.transform_vector3(Vec3::Y).normalize_or_zero(),
                        frame.transform_vector3(Vec3::Z).normalize_or_zero(),
                    );
                    quad(at, (y * c + z * s) * w, (z * c - y * s) * h)
                }
                _ => {
                    // Facing the eye: in the view plane, turned by its rotation.
                    let (s, c) = rotation.sin_cos();
                    quad(at, Vec3::new(c, s, 0.0) * w, Vec3::new(-s, c, 0.0) * h)
                }
            };
            quads.push(Quad {
                corners,
                uv: atlas_cell(e, p.age, t, p.rand.atlas),
                rgba: v.color,
                additive: additive(m),
                image: Arc::from(vm::file_stem(&m.color_map.to_ascii_lowercase())),
            });
        }
        (quads, lights)
    }
}

fn range(r: [f32; 2], k: f32) -> f32 {
    r[0] + r[1] * k
}

fn range_i(r: [i32; 2], k: f32) -> f32 {
    r[0] as f32 + r[1] as f32 * k
}

/// Added (GFXS0 destination blend ONE) or alpha-blended.
fn additive(m: &vm::Material) -> bool {
    (m.state_bits[0] >> 4) & 0xF == 2
}

/// A particle's visual state at `t` (0..1 of its life): the samples spread evenly over the life, each a
/// base and an amplitude (colours lerp from one to the other by the particle's random; the rest add it).
struct Visual {
    color: [u8; 4],
    size: [f32; 2],
    rotation: f32,
    scale: f32,
}

fn visual(e: &FxElem, t: f32, r: &Rand) -> Visual {
    let one = |pair: &[vm::FxVisual; 2]| {
        let (b, a) = (&pair[0], &pair[1]);
        let mut color = [0f32; 4];
        for (c, x) in color.iter_mut().enumerate() {
            *x = b.color[c] as f32 + (a.color[c] as f32 - b.color[c] as f32) * r.color;
        }
        (
            color,
            [b.size[0] + a.size[0] * r.size[0], b.size[1] + a.size[1] * r.size[1]],
            b.rotation_total + a.rotation_total * r.rotation,
            b.scale + a.scale * r.scale,
        )
    };
    let Some(first) = e.visuals.first() else {
        return Visual { color: [255; 4], size: [1.0; 2], rotation: 0.0, scale: 1.0 };
    };
    let (color, size, rotation, scale) = if e.visuals.len() == 1 {
        one(first)
    } else {
        let n = (e.visuals.len() - 1) as f32;
        let f = (t * n).clamp(0.0, n);
        let i = (f as usize).min(e.visuals.len() - 2);
        let k = f - i as f32;
        let (a, b) = (one(&e.visuals[i]), one(&e.visuals[i + 1]));
        let color: [f32; 4] = std::array::from_fn(|c| a.0[c] + (b.0[c] - a.0[c]) * k);
        (
            color,
            [a.1[0] + (b.1[0] - a.1[0]) * k, a.1[1] + (b.1[1] - a.1[1]) * k],
            a.2 + (b.2 - a.2) * k,
            a.3 + (b.3 - a.3) * k,
        )
    };
    Visual { color: color.map(|c| c.round().clamp(0.0, 255.0) as u8), size, rotation, scale }
}

/// The atlas cell (u0, v0, u1, v1) of a particle `age` ms old, `t` through its life.
fn atlas_cell(e: &FxElem, age: f32, t: f32, k: f32) -> [f32; 4] {
    let [behaviour, index, fps, loops, col_bits, row_bits] = e.atlas;
    let cols = 1u32 << col_bits.min(8);
    let rows = 1u32 << row_bits.min(8);
    let entries = (e.atlas_entries as u32).clamp(1, cols * rows);
    if entries <= 1 && cols * rows <= 1 {
        return [0.0, 0.0, 1.0, 1.0];
    }
    let start = match behaviour & atlas::START_MASK {
        atlas::START_RANDOM => (k * entries as f32) as u32,
        atlas::START_INDEXED => index as u32,
        _ => index as u32,
    };
    let step = if behaviour & atlas::PLAY_OVER_LIFE != 0 {
        (t * entries as f32) as u32
    } else if fps > 0 {
        (age.max(0.0) / 1000.0 * fps as f32) as u32
    } else {
        0
    };
    let step =
        if behaviour & atlas::LOOP_ONLY_N_TIMES != 0 { step.min(entries * loops.max(1) as u32 - 1) } else { step };
    let cell = (start + step) % entries;
    let (c, r) = (cell % cols, cell / cols);
    let (w, h) = (1.0 / cols as f32, 1.0 / rows as f32);
    [c as f32 * w, r as f32 * h, (c + 1) as f32 * w, (r + 1) as f32 * h]
}

/// A quad centred at `at` with half-axes `right` and `up`: bottom left, bottom right, top right, top left.
fn quad(at: Vec3, right: Vec3, up: Vec3) -> [[f32; 3]; 4] {
    [(at - right - up), (at + right - up), (at + right + up), (at - right + up)].map(|v| v.to_array())
}

/// A tail: `length` along its velocity (to where it came from), `width` across, turned to face the eye.
fn tail(at: Vec3, velocity: Vec3, width: f32, length: f32) -> [[f32; 3]; 4] {
    let dir = velocity.normalize_or(Vec3::Z);
    let across = dir.cross(at).normalize_or(Vec3::X) * (width * 0.5);
    let back = at - dir * length;
    [back - across, at - across, at + across, back + across].map(|v| v.to_array())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codv_mw2data::vm::{FxMotion, FxVisual, Material};
    use glam::Vec4;

    fn material(name: &str, additive: bool) -> Material {
        // GFXS0: src ONE / dst ONE (added) or SRC_ALPHA / INV_SRC_ALPHA, blend op ADD.
        let bits = if additive { 0x0000_0122 } else { 0x0000_0165 };
        Material { name: name.into(), color_map: name.into(), state_bits: [bits, 0], ..Default::default() }
    }

    fn sprite(kind: u8, life: i32, flags: u32) -> FxElem {
        FxElem {
            kind,
            flags,
            category: 1,
            spawn: [1, 0],
            life_ms: [life, 0],
            origin: [[2.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
            // The amplitude's colour is the other end of the random lerp: the same colour, none.
            visuals: vec![
                [
                    FxVisual { color: [255, 200, 100, 255], size: [2.0, 2.0], ..Default::default() },
                    FxVisual { color: [255, 200, 100, 255], ..Default::default() },
                ],
                [
                    FxVisual { color: [0, 0, 0, 255], size: [4.0, 4.0], ..Default::default() },
                    FxVisual { color: [0, 0, 0, 255], ..Default::default() },
                ],
            ],
            materials: vec![material("flare", true)],
            ..Default::default()
        }
    }

    fn library(dir: &std::path::Path, effects: &[Effect]) {
        std::fs::create_dir_all(dir).unwrap();
        for e in effects {
            std::fs::write(dir.join(format!("{}.cfx", vm::file_stem(&e.name))), vm::write_effect(e)).unwrap();
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("codv-flash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_shot_plays_the_effect_at_the_tag_and_its_runner_child() {
        let dir = temp("runner");
        let child = Effect {
            name: "muzzleflashes/test_a".into(),
            looping_life_ms: 0,
            elems: vec![sprite(fx_kind::BILLBOARD, 40, flag::RUN_RELATIVE_TO_EFFECT), {
                let mut light = sprite(fx_kind::OMNI_LIGHT, 40, 0x40);
                light.materials.clear();
                light.visuals[0][0].size = [200.0, 0.0];
                light.visuals[0][0].scale = 1.5;
                light
            }],
        };
        let parent = Effect {
            name: "muzzleflashes/test_view".into(),
            looping_life_ms: 0,
            elems: vec![FxElem {
                kind: fx_kind::RUNNER,
                category: 1,
                spawn: [1, 0],
                life_ms: [1000, 0],
                runs: vec!["muzzleflashes/test_a".into()],
                ..Default::default()
            }],
        };
        library(&dir, &[parent, child]);
        let mut f = Flashes::new(dir.clone());
        // The tag 20 inches ahead of the eye, its x (the barrel) along the view's forward (D3D z).
        let tag = Mat4::from_cols(Vec4::Z, -Vec4::X, Vec4::Y, Vec4::new(0.0, -5.0, 20.0, 1.0));
        let world = view_to_world([0.0, 0.0, 60.0], [0.0, 90.0, 0.0]);
        f.tick(0.0, Some(tag), world, Some("muzzleflashes/test_view"), &|_| {});
        let (quads, lights) = f.draw();
        assert_eq!(quads.len(), 1);
        // 2 inches along the barrel from the tag: 22 inches ahead.
        let centre: Vec3 = quads[0].corners.iter().map(|c| Vec3::from_array(*c)).sum::<Vec3>() / 4.0;
        assert!((centre - Vec3::new(0.0, -5.0, 22.0)).length() < 1e-3, "{centre}");
        assert!(quads[0].additive && &*quads[0].image == "flare");
        assert_eq!(quads[0].rgba, [255, 200, 100, 255]);
        assert_eq!(lights.len(), 1);
        // Yaw 90 looks along +y: 22 inches ahead, 5 below the eye.
        let p = Vec3::from_array(lights[0].position);
        assert!((p - Vec3::new(0.0, 22.0, 55.0)).length() < 1e-2, "{p}");
        assert!((lights[0].radius - 200.0).abs() < 1e-3 && (lights[0].intensity - 1.5).abs() < 1e-3);
        // Halfway through its life the sprite has grown and dimmed; past it, it is gone.
        f.tick(20.0, Some(tag), world, None, &|_| {});
        let (quads, _) = f.draw();
        let width = (Vec3::from_array(quads[0].corners[1]) - Vec3::from_array(quads[0].corners[0])).length();
        assert!((width - 6.0).abs() < 1e-3, "{width}");
        assert_eq!(quads[0].rgba[0], 128);
        f.tick(25.0, Some(tag), world, None, &|_| {});
        assert!(!f.busy());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn particles_left_in_the_world_stay_there_as_the_view_turns() {
        let dir = temp("world");
        let mut smoke = sprite(fx_kind::BILLBOARD, 400, 0x40);
        smoke.materials = vec![material("smoke", false)];
        smoke.motion = vec![FxMotion::default(); 2];
        let effect = Effect { name: "smoke".into(), looping_life_ms: 0, elems: vec![smoke] };
        library(&dir, &[effect]);
        let mut f = Flashes::new(dir.clone());
        let tag = Mat4::from_cols(Vec4::Z, -Vec4::X, Vec4::Y, Vec4::new(0.0, 0.0, 20.0, 1.0));
        f.tick(0.0, Some(tag), view_to_world([0.0; 3], [0.0, 0.0, 0.0]), Some("smoke"), &|_| {});
        // The view turns 90 degrees left: the puff, 22 inches along +x, is now to the right.
        f.tick(10.0, Some(tag), view_to_world([0.0; 3], [0.0, 90.0, 0.0]), None, &|_| {});
        let (quads, _) = f.draw();
        let centre: Vec3 = quads[0].corners.iter().map(|c| Vec3::from_array(*c)).sum::<Vec3>() / 4.0;
        assert!((centre - Vec3::new(22.0, 0.0, 0.0)).length() < 1e-3, "{centre}");
        assert!(!quads[0].additive);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_shell_eject_throws_its_brass_until_the_floor() {
        let dir = temp("brass");
        let brass = FxElem {
            kind: fx_kind::MODEL,
            flags: flag::HAS_GRAVITY,
            category: 1,
            spawn: [1, 0],
            life_ms: [2000, 0],
            gravity: [1.0, 0.0],
            models: vec!["Shell_Rifle".into()],
            ..Default::default()
        };
        let sprite = sprite(fx_kind::BILLBOARD, 100, 0);
        let effect = Effect { name: "shelleject/rifle".into(), looping_life_ms: 0, elems: vec![brass, sprite] };
        library(&dir, &[effect]);
        let mut f = Flashes::for_models(dir.clone());
        f.set_floor(Some(0.0));
        let tag = Mat4::from_cols(Vec4::Z, -Vec4::X, Vec4::Y, Vec4::new(0.0, -5.0, 20.0, 1.0));
        let world = view_to_world([0.0, 0.0, 60.0], [0.0, 0.0, 0.0]);
        f.tick(0.0, Some(tag), world, Some("shelleject/rifle"), &|_| {});
        // Only the model element plays (the sprite is the muzzle flash's business), at the tag.
        assert!(f.draw().0.is_empty());
        let thrown = f.thrown();
        assert_eq!(thrown.len(), 1);
        assert_eq!(&*thrown[0].model, "shell_rifle");
        assert!((thrown[0].at.w_axis.truncate() - Vec3::new(0.0, -5.0, 20.0)).length() < 1e-3);
        // Gravity pulls it down the world's z (the view's y here); 0.3 s in it has fallen 36 inches.
        f.tick(300.0, Some(tag), world, None, &|_| {});
        let y = f.thrown()[0].at.w_axis.y;
        assert!((y - (-5.0 - 36.0)).abs() < 1e-2, "{y}");
        // 55 inches below the eye is the floor: past it, it is gone.
        f.tick(100.0, Some(tag), world, None, &|_| {});
        assert!(f.thrown().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atlas_cells_follow_the_behaviour() {
        let mut e = sprite(fx_kind::BILLBOARD, 100, 0);
        // 4 x 1, random start, no playback.
        e.atlas = [atlas::START_RANDOM, 0, 0, 1, 2, 0];
        e.atlas_entries = 4;
        assert_eq!(atlas_cell(&e, 0.0, 0.0, 0.6), [0.5, 0.0, 0.75, 1.0]);
        // 8 x 4 at 20 fps from cell 0: 0.1 s in, the third cell.
        e.atlas = [0, 0, 20, 1, 3, 2];
        e.atlas_entries = 32;
        assert_eq!(atlas_cell(&e, 100.0, 0.25, 0.0), [0.25, 0.0, 0.375, 0.25]);
        // Played over the life: halfway, cell 16 (row 2).
        e.atlas = [atlas::PLAY_OVER_LIFE, 0, 0, 1, 3, 2];
        assert_eq!(atlas_cell(&e, 50.0, 0.5, 0.0), [0.0, 0.5, 0.125, 0.75]);
    }

    #[test]
    fn view_axes_match_the_d3d_view() {
        let m = view_to_world([1.0, 2.0, 3.0], [0.0, 0.0, 0.0]);
        // Forward (D3D z) is world +x, right (D3D x) is world -y, up is +z.
        assert!((m.transform_point3(Vec3::Z) - Vec3::new(2.0, 2.0, 3.0)).length() < 1e-5);
        assert!((m.transform_vector3(Vec3::X) - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-5);
        assert!((m.transform_vector3(Vec3::Y) - Vec3::Z).length() < 1e-5);
    }
}
