//! CODV's viewmodel files: what the importer writes under
//! `<GTA>/CODV/mw2/viewmodels/` from the player's own Modern Warfare 2, and
//! the runtime reads back. Plain little-endian records, versioned:
//!
//! * `models/<name>.cvm`: one XModel's skeleton (the fields iw4L's
//!   `xmodel_runtime::ModelPoseSrc` takes) and its LOD 0 surfaces, with the
//!   vertices unpacked (position, normal, tangent, UV, colour, up to four bone
//!   influences) and each surface's material (its colour, normal and specular
//!   map image names and its state bits);
//! * `anims/<name>.cva`: one XAnimParts as iw4L's `RawXAnimParts` holds it
//!   (the retail curve pools, decoded at runtime by `xmodel_runtime`);
//! * `images/<name>.dds`: the material images, BC1/BC2/BC3 or RGBA8 with
//!   their mips, as DirectX loads them.

pub const MODEL_MAGIC: &[u8; 4] = b"CVM2";
pub const ANIM_MAGIC: &[u8; 4] = b"CVA1";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Tangent (unit) and the binormal's sign (binormal = sign * cross(normal, tangent)).
    pub tangent: [f32; 3],
    pub binormal_sign: f32,
    pub uv: [f32; 2],
    pub color: [u8; 4],
    pub bones: [u8; 4],
    /// Sum to 1 (unused influences 0).
    pub weights: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    pub name: String,
    pub color_map: String,
    pub normal_map: String,
    pub specular_map: String,
    /// GfxStateBits loadBits of the material's first state entry.
    pub state_bits: [u32; 2],
    pub sort_key: u8,
}

impl Material {
    /// Alpha test (GFXS0_ATEST_*): 0 none, else the retail mode.
    pub fn alpha_test(&self) -> u32 {
        // GFXS0_ATEST_DISABLE 0x800; mode field 0x3000: 1 GT_0, 2 LT_128, 3 GE_128.
        if self.state_bits[0] & 0x800 != 0 { 0 } else { (self.state_bits[0] >> 12) & 3 }
    }
    /// Blending on (GFXS0 blend op set) and other than src ONE, dst ZERO.
    pub fn blended(&self) -> bool {
        let src = self.state_bits[0] & 0xF;
        let dst = (self.state_bits[0] >> 4) & 0xF;
        let op = (self.state_bits[0] >> 8) & 0x7;
        // GFXS_BLEND_ZERO = 1, GFXS_BLEND_ONE = 2; blend op 0 is disabled.
        op != 0 && !(src == 2 && dst == 1)
    }
    /// GFXS0 cull field: 1 none, 2 back, 3 front (0 unset).
    pub fn cull(&self) -> u32 {
        (self.state_bits[0] >> 14) & 3
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    pub material: Material,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub part_bits: [u32; 6],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub name: String,
    pub num_root_bones: u32,
    pub scale: f32,
    pub no_scale_part_bits: [u32; 6],
    pub bone_names: Vec<String>,
    pub parent_list: Vec<u8>,
    pub quats: Vec<[i16; 4]>,
    pub trans: Vec<[f32; 3]>,
    /// Bind pose in model space (quaternion xyzw, translation).
    pub base_mat: Vec<([f32; 4], [f32; 3])>,
    pub surfaces: Vec<Surface>,
    pub vertices: Vec<Vertex>,
    /// Model-wide vertex indices.
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notify {
    pub name: String,
    pub time: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeltaTrans {
    pub size: u16,
    pub small: bool,
    pub mins: [f32; 3],
    pub step: [f32; 3],
    pub indices: Vec<u16>,
    pub packed: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anim {
    pub name: String,
    pub numframes: u16,
    pub flags: u8,
    pub bone_count: [u8; 10],
    pub framerate: f32,
    pub names: Vec<String>,
    pub notifies: Vec<Notify>,
    pub data_byte: Vec<u8>,
    pub data_short: Vec<u16>,
    pub data_int: Vec<u32>,
    pub random_data_byte: Vec<u8>,
    pub random_data_short: Vec<u16>,
    pub random_data_int: Vec<u32>,
    pub indices: Vec<u16>,
    pub delta_trans: Option<DeltaTrans>,
}

// ---- writer -----------------------------------------------------------------

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u32(n as u32);
    }
    fn str(&mut self, s: &str) {
        self.len(s.len());
        self.0.extend_from_slice(s.as_bytes());
    }
    fn f32s(&mut self, v: &[f32]) {
        for x in v {
            self.f32(*x);
        }
    }
    fn u32s(&mut self, v: &[u32]) {
        for x in v {
            self.u32(*x);
        }
    }
    fn bytes(&mut self, v: &[u8]) {
        self.len(v.len());
        self.0.extend_from_slice(v);
    }
    fn shorts(&mut self, v: &[u16]) {
        self.len(v.len());
        for x in v {
            self.u16(*x);
        }
    }
    fn ints(&mut self, v: &[u32]) {
        self.len(v.len());
        self.u32s(v);
    }
}

// ---- reader -----------------------------------------------------------------

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

type Res<T> = Result<T, String>;

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or("truncated file")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Res<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Res<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn len(&mut self, unit: usize) -> Res<usize> {
        let n = self.u32()? as usize;
        // A count can never promise more bytes than are left.
        if n.saturating_mul(unit.max(1)) > self.b.len() - self.at {
            return Err("count past the end of the file".into());
        }
        Ok(n)
    }
    fn str(&mut self) -> Res<String> {
        let n = self.len(1)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "bad text".into())
    }
    fn f32a<const N: usize>(&mut self) -> Res<[f32; N]> {
        let mut a = [0.0; N];
        for x in &mut a {
            *x = self.f32()?;
        }
        Ok(a)
    }
    fn u32a<const N: usize>(&mut self) -> Res<[u32; N]> {
        let mut a = [0; N];
        for x in &mut a {
            *x = self.u32()?;
        }
        Ok(a)
    }
    fn bytes(&mut self) -> Res<Vec<u8>> {
        let n = self.len(1)?;
        Ok(self.take(n)?.to_vec())
    }
    fn shorts(&mut self) -> Res<Vec<u16>> {
        let n = self.len(2)?;
        (0..n).map(|_| self.u16()).collect()
    }
    fn ints(&mut self) -> Res<Vec<u32>> {
        let n = self.len(4)?;
        (0..n).map(|_| self.u32()).collect()
    }
    fn magic(&mut self, m: &[u8; 4]) -> Res<()> {
        if self.take(4)? != m {
            return Err(format!("not a {} file (re-run the MW2 import)", String::from_utf8_lossy(m)));
        }
        Ok(())
    }
}

fn write_material(w: &mut W, m: &Material) {
    w.str(&m.name);
    w.str(&m.color_map);
    w.str(&m.normal_map);
    w.str(&m.specular_map);
    w.u32s(&m.state_bits);
    w.u8(m.sort_key);
}

fn read_material(r: &mut R<'_>) -> Res<Material> {
    Ok(Material {
        name: r.str()?,
        color_map: r.str()?,
        normal_map: r.str()?,
        specular_map: r.str()?,
        state_bits: r.u32a()?,
        sort_key: r.u8()?,
    })
}

pub fn write_model(m: &Model) -> Vec<u8> {
    let mut w = W::default();
    w.0.extend_from_slice(MODEL_MAGIC);
    w.str(&m.name);
    w.u32(m.num_root_bones);
    w.f32(m.scale);
    w.u32s(&m.no_scale_part_bits);
    w.len(m.bone_names.len());
    for n in &m.bone_names {
        w.str(n);
    }
    w.bytes(&m.parent_list);
    w.len(m.quats.len());
    for q in &m.quats {
        for x in q {
            w.i16(*x);
        }
    }
    w.len(m.trans.len());
    for t in &m.trans {
        w.f32s(t);
    }
    w.len(m.base_mat.len());
    for (q, t) in &m.base_mat {
        w.f32s(q);
        w.f32s(t);
    }
    w.len(m.surfaces.len());
    for s in &m.surfaces {
        write_material(&mut w, &s.material);
        w.u32s(&[s.first_vertex, s.vertex_count, s.first_index, s.index_count]);
        w.u32s(&s.part_bits);
    }
    w.len(m.vertices.len());
    for v in &m.vertices {
        w.f32s(&v.position);
        w.f32s(&v.normal);
        w.f32s(&v.tangent);
        w.f32(v.binormal_sign);
        w.f32s(&v.uv);
        w.0.extend_from_slice(&v.color);
        w.0.extend_from_slice(&v.bones);
        w.f32s(&v.weights);
    }
    w.ints(&m.indices);
    w.0
}

pub fn read_model(b: &[u8]) -> Res<Model> {
    let mut r = R { b, at: 0 };
    r.magic(MODEL_MAGIC)?;
    let mut m = Model {
        name: r.str()?,
        num_root_bones: r.u32()?,
        scale: r.f32()?,
        no_scale_part_bits: r.u32a()?,
        ..Default::default()
    };
    let n = r.len(4)?;
    m.bone_names = (0..n).map(|_| r.str()).collect::<Res<_>>()?;
    m.parent_list = r.bytes()?;
    let n = r.len(8)?;
    m.quats = (0..n).map(|_| Ok([r.i16()?, r.i16()?, r.i16()?, r.i16()?])).collect::<Res<_>>()?;
    let n = r.len(12)?;
    m.trans = (0..n).map(|_| r.f32a()).collect::<Res<_>>()?;
    let n = r.len(28)?;
    m.base_mat = (0..n).map(|_| Ok((r.f32a()?, r.f32a()?))).collect::<Res<_>>()?;
    let n = r.len(40)?;
    for _ in 0..n {
        let material = read_material(&mut r)?;
        let [first_vertex, vertex_count, first_index, index_count] = r.u32a()?;
        m.surfaces.push(Surface {
            material,
            first_vertex,
            vertex_count,
            first_index,
            index_count,
            part_bits: r.u32a()?,
        });
    }
    let n = r.len(72)?;
    for _ in 0..n {
        let position = r.f32a()?;
        let normal = r.f32a()?;
        let tangent = r.f32a()?;
        let binormal_sign = r.f32()?;
        let uv = r.f32a()?;
        let color = r.take(4)?.try_into().unwrap();
        let bones = r.take(4)?.try_into().unwrap();
        m.vertices.push(Vertex { position, normal, tangent, binormal_sign, uv, color, bones, weights: r.f32a()? });
    }
    m.indices = r.ints()?;
    validate_model(&m)?;
    Ok(m)
}

fn validate_model(m: &Model) -> Res<()> {
    let bones = m.bone_names.len();
    let child = bones.saturating_sub(m.num_root_bones as usize);
    if m.base_mat.len() != bones || m.parent_list.len() != child || m.quats.len() != child || m.trans.len() != child {
        return Err(format!("{}: skeleton tables disagree", m.name));
    }
    if m.indices.iter().any(|i| *i as usize >= m.vertices.len()) {
        return Err(format!("{}: index past the vertices", m.name));
    }
    if m.vertices.iter().any(|v| v.bones.iter().any(|b| *b as usize >= bones.max(1))) {
        return Err(format!("{}: vertex bone past the skeleton", m.name));
    }
    for s in &m.surfaces {
        let v_end = s.first_vertex as usize + s.vertex_count as usize;
        let i_end = s.first_index as usize + s.index_count as usize;
        if v_end > m.vertices.len() || i_end > m.indices.len() {
            return Err(format!("{}: surface past the buffers", m.name));
        }
    }
    Ok(())
}

pub fn write_anim(a: &Anim) -> Vec<u8> {
    let mut w = W::default();
    w.0.extend_from_slice(ANIM_MAGIC);
    w.str(&a.name);
    w.u16(a.numframes);
    w.u8(a.flags);
    w.0.extend_from_slice(&a.bone_count);
    w.f32(a.framerate);
    w.len(a.names.len());
    for n in &a.names {
        w.str(n);
    }
    w.len(a.notifies.len());
    for n in &a.notifies {
        w.str(&n.name);
        w.f32(n.time);
    }
    w.bytes(&a.data_byte);
    w.shorts(&a.data_short);
    w.ints(&a.data_int);
    w.bytes(&a.random_data_byte);
    w.shorts(&a.random_data_short);
    w.ints(&a.random_data_int);
    w.shorts(&a.indices);
    match &a.delta_trans {
        None => w.u8(0),
        Some(d) => {
            w.u8(1);
            w.u16(d.size);
            w.u8(u8::from(d.small));
            w.f32s(&d.mins);
            w.f32s(&d.step);
            w.shorts(&d.indices);
            w.bytes(&d.packed);
        }
    }
    w.0
}

pub fn read_anim(b: &[u8]) -> Res<Anim> {
    let mut r = R { b, at: 0 };
    r.magic(ANIM_MAGIC)?;
    let mut a = Anim { name: r.str()?, numframes: r.u16()?, flags: r.u8()?, ..Default::default() };
    a.bone_count = r.take(10)?.try_into().unwrap();
    a.framerate = r.f32()?;
    let n = r.len(4)?;
    a.names = (0..n).map(|_| r.str()).collect::<Res<_>>()?;
    let n = r.len(8)?;
    a.notifies = (0..n).map(|_| Ok(Notify { name: r.str()?, time: r.f32()? })).collect::<Res<_>>()?;
    a.data_byte = r.bytes()?;
    a.data_short = r.shorts()?;
    a.data_int = r.ints()?;
    a.random_data_byte = r.bytes()?;
    a.random_data_short = r.shorts()?;
    a.random_data_int = r.ints()?;
    a.indices = r.shorts()?;
    if r.u8()? != 0 {
        a.delta_trans = Some(DeltaTrans {
            size: r.u16()?,
            small: r.u8()? != 0,
            mins: r.f32a()?,
            step: r.f32a()?,
            indices: r.shorts()?,
            packed: r.bytes()?,
        });
    }
    Ok(a)
}

// ---- effects ----------------------------------------------------------------

/// `fx/<name>.cfx`: one FxEffectDef (a gun's view muzzle flash), its elements as IW4's FxElemDef holds
/// them. Distances are inches in the effect's frame (MW2's: X along the barrel, Y left, Z up); times
/// milliseconds; every range is `base + amplitude * random(0..1)`.
pub const EFFECT_MAGIC: &[u8; 4] = b"CFX1";

/// FxElemType values the runtime knows.
pub mod fx_kind {
    pub const BILLBOARD: u8 = 0;
    pub const ORIENTED: u8 = 1;
    pub const TAIL: u8 = 2;
    pub const TRAIL: u8 = 3;
    pub const CLOUD: u8 = 4;
    pub const SPARK_CLOUD: u8 = 5;
    pub const SPARK_FOUNTAIN: u8 = 6;
    pub const MODEL: u8 = 7;
    pub const OMNI_LIGHT: u8 = 8;
    pub const SPOT_LIGHT: u8 = 9;
    pub const SOUND: u8 = 10;
    pub const DECAL: u8 = 11;
    pub const RUNNER: u8 = 12;
}

/// One FxElemVisualState (a sample's base or amplitude).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxVisual {
    pub color: [u8; 4],
    pub rotation_delta: f32,
    pub rotation_total: f32,
    pub size: [f32; 2],
    pub scale: f32,
}

/// One FxElemVelStateSample: velocity and the displacement so far (base, amplitude), in the effect's
/// frame (`local`) and the world's (`world`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxMotion {
    pub local_velocity: [[f32; 3]; 2],
    pub local_delta: [[f32; 3]; 2],
    pub world_velocity: [[f32; 3]; 2],
    pub world_delta: [[f32; 3]; 2],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FxElem {
    pub kind: u8,
    pub flags: u32,
    /// 0 looping, 1 one-shot, 2 emitted (by another element).
    pub category: u8,
    /// Looping: interval (ms) and count; one-shot: count base and amplitude.
    pub spawn: [i32; 2],
    pub fade_in: [f32; 2],
    pub fade_out: [f32; 2],
    pub delay_ms: [i32; 2],
    pub life_ms: [i32; 2],
    pub origin: [[f32; 2]; 3],
    pub offset_radius: [f32; 2],
    pub offset_height: [f32; 2],
    pub angles: [[f32; 2]; 3],
    pub angular_velocity: [[f32; 2]; 3],
    pub initial_rotation: [f32; 2],
    pub gravity: [f32; 2],
    /// FxElemAtlas: behaviour, start index, frames per second, loop count, column bits, row bits.
    pub atlas: [u8; 6],
    pub atlas_entries: u16,
    pub motion: Vec<FxMotion>,
    /// (base, amplitude) per sample, spread evenly over the life.
    pub visuals: Vec<[FxVisual; 2]>,
    /// The sprite's materials (one picked at random per particle); none for lights.
    pub materials: Vec<Material>,
    /// A runner's effects (one picked at random), by name: spawned where the runner is.
    pub runs: Vec<String>,
    pub sort_order: u8,
    pub lighting_frac: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub name: String,
    pub looping_life_ms: i32,
    pub elems: Vec<FxElem>,
}

pub fn write_effect(e: &Effect) -> Vec<u8> {
    let mut w = W::default();
    w.0.extend_from_slice(EFFECT_MAGIC);
    w.str(&e.name);
    w.u32(e.looping_life_ms as u32);
    w.len(e.elems.len());
    for x in &e.elems {
        w.u8(x.kind);
        w.u32(x.flags);
        w.u8(x.category);
        for v in [x.spawn, x.delay_ms, x.life_ms] {
            w.u32s(&[v[0] as u32, v[1] as u32]);
        }
        w.f32s(&x.fade_in);
        w.f32s(&x.fade_out);
        for r in x.origin.iter().chain(&x.angles).chain(&x.angular_velocity) {
            w.f32s(r);
        }
        for r in [x.offset_radius, x.offset_height, x.initial_rotation, x.gravity] {
            w.f32s(&r);
        }
        w.0.extend_from_slice(&x.atlas);
        w.u16(x.atlas_entries);
        w.len(x.motion.len());
        for m in &x.motion {
            for v in [m.local_velocity, m.local_delta, m.world_velocity, m.world_delta] {
                w.f32s(&v[0]);
                w.f32s(&v[1]);
            }
        }
        w.len(x.visuals.len());
        for pair in &x.visuals {
            for v in pair {
                w.0.extend_from_slice(&v.color);
                w.f32s(&[v.rotation_delta, v.rotation_total, v.size[0], v.size[1], v.scale]);
            }
        }
        w.len(x.materials.len());
        for m in &x.materials {
            write_material(&mut w, m);
        }
        w.len(x.runs.len());
        for n in &x.runs {
            w.str(n);
        }
        w.u8(x.sort_order);
        w.u8(x.lighting_frac);
    }
    w.0
}

pub fn read_effect(b: &[u8]) -> Res<Effect> {
    let mut r = R { b, at: 0 };
    r.magic(EFFECT_MAGIC)?;
    let name = r.str()?;
    let looping_life_ms = r.u32()? as i32;
    let n = r.len(64)?;
    let mut elems = Vec::with_capacity(n);
    for _ in 0..n {
        let kind = r.u8()?;
        let flags = r.u32()?;
        let category = r.u8()?;
        let mut ints = [[0i32; 2]; 3];
        for v in &mut ints {
            *v = [r.u32()? as i32, r.u32()? as i32];
        }
        let fade_in = r.f32a()?;
        let fade_out = r.f32a()?;
        let mut ranges = [[0.0f32; 2]; 9];
        for v in &mut ranges {
            *v = r.f32a()?;
        }
        let mut more = [[0.0f32; 2]; 4];
        for v in &mut more {
            *v = r.f32a()?;
        }
        let mut atlas = [0u8; 6];
        atlas.copy_from_slice(r.take(6)?);
        let atlas_entries = r.u16()?;
        let m = r.len(96)?;
        let mut motion = Vec::with_capacity(m);
        for _ in 0..m {
            let mut v = [[[0.0f32; 3]; 2]; 4];
            for pair in &mut v {
                pair[0] = r.f32a()?;
                pair[1] = r.f32a()?;
            }
            motion.push(FxMotion { local_velocity: v[0], local_delta: v[1], world_velocity: v[2], world_delta: v[3] });
        }
        let k = r.len(48)?;
        let mut visuals = Vec::with_capacity(k);
        for _ in 0..k {
            let mut pair = [FxVisual::default(); 2];
            for v in &mut pair {
                v.color.copy_from_slice(r.take(4)?);
                let f: [f32; 5] = r.f32a()?;
                (v.rotation_delta, v.rotation_total, v.size, v.scale) = (f[0], f[1], [f[2], f[3]], f[4]);
            }
            visuals.push(pair);
        }
        let count = r.len(16)?;
        let materials = (0..count).map(|_| read_material(&mut r)).collect::<Res<Vec<_>>>()?;
        let count = r.len(4)?;
        let runs = (0..count).map(|_| r.str()).collect::<Res<Vec<_>>>()?;
        elems.push(FxElem {
            kind,
            flags,
            category,
            spawn: ints[0],
            fade_in,
            fade_out,
            delay_ms: ints[1],
            life_ms: ints[2],
            origin: [ranges[0], ranges[1], ranges[2]],
            angles: [ranges[3], ranges[4], ranges[5]],
            angular_velocity: [ranges[6], ranges[7], ranges[8]],
            offset_radius: more[0],
            offset_height: more[1],
            initial_rotation: more[2],
            gravity: more[3],
            atlas,
            atlas_entries,
            motion,
            visuals,
            materials,
            runs,
            sort_order: r.u8()?,
            lighting_frac: r.u8()?,
        });
    }
    Ok(Effect { name, looping_life_ms, elems })
}

/// A file name for an asset name (MW2 names are plain; anything odd is replaced).
pub fn file_stem(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_and_anims_round_trip() {
        let m = Model {
            name: "viewmodel_test".into(),
            num_root_bones: 1,
            scale: 1.0,
            bone_names: vec!["tag_view".into(), "tag_weapon".into()],
            parent_list: vec![1],
            quats: vec![[0, 0, 0, 32767]],
            trans: vec![[1.0, 2.0, 3.0]],
            base_mat: vec![([0.0, 0.0, 0.0, 1.0], [0.0; 3]), ([0.0, 0.0, 0.0, 1.0], [1.0, 2.0, 3.0])],
            surfaces: vec![Surface {
                material: Material {
                    name: "mtl_x".into(),
                    color_map: "x_col".into(),
                    state_bits: [0x800, 0],
                    ..Default::default()
                },
                first_vertex: 0,
                vertex_count: 3,
                first_index: 0,
                index_count: 3,
                part_bits: [0; 6],
            }],
            vertices: vec![Vertex { weights: [1.0, 0.0, 0.0, 0.0], bones: [1, 0, 0, 0], ..Default::default() }; 3],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert_eq!(read_model(&write_model(&m)).unwrap(), m);
        assert_eq!(m.surfaces[0].material.alpha_test(), 0);
        let mut bad = m.clone();
        bad.indices[0] = 9;
        assert!(read_model(&write_model(&bad)).is_err());

        let a = Anim {
            name: "viewmodel_test_idle".into(),
            numframes: 30,
            framerate: 30.0,
            bone_count: [0, 1, 0, 0, 0, 0, 0, 0, 1, 1],
            names: vec!["tag_weapon".into()],
            notifies: vec![Notify { name: "end".into(), time: 1.0 }],
            data_short: vec![1, 2, 3],
            delta_trans: Some(DeltaTrans {
                size: 1,
                indices: vec![0, 1],
                packed: vec![1, 2, 3, 4, 5, 6],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(read_anim(&write_anim(&a)).unwrap(), a);
        assert!(read_anim(b"CVA1").is_err());
        assert_eq!(file_stem("a/b c"), "a_b_c");
    }

    #[test]
    fn effects_round_trip() {
        let e = Effect {
            name: "muzzleflashes/m4m203_flash_view".into(),
            looping_life_ms: 0,
            elems: vec![FxElem {
                kind: fx_kind::BILLBOARD,
                flags: 0x12,
                category: 1,
                spawn: [2, 1],
                fade_in: [0.0, 0.1],
                fade_out: [0.5, 0.2],
                delay_ms: [0, 10],
                life_ms: [50, 25],
                origin: [[1.0, 0.5], [0.0, 0.0], [-0.5, 1.0]],
                gravity: [0.0, 0.0],
                atlas: [1, 0, 30, 0, 1, 1],
                atlas_entries: 4,
                motion: vec![FxMotion { local_delta: [[1.0, 2.0, 3.0], [0.5, 0.0, 0.0]], ..Default::default() }; 2],
                visuals: vec![[
                    FxVisual { color: [255, 200, 100, 255], size: [2.0, 3.0], scale: 1.0, ..Default::default() },
                    FxVisual::default(),
                ]],
                materials: vec![Material {
                    name: "muzzle".into(),
                    color_map: "fx_muzzle".into(),
                    ..Default::default()
                }],
                runs: vec!["muzzleflashes/other".into()],
                sort_order: 5,
                lighting_frac: 0,
                ..Default::default()
            }],
        };
        assert_eq!(read_effect(&write_effect(&e)).unwrap(), e);
        assert!(read_effect(b"CFX1").is_err());
    }
}
