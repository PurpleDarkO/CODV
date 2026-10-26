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
}
