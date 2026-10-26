//! The first-person meshes, animations and material images of a zone, as
//! CODV's viewmodel files (`vm`). The capture follows iw4L's own readers,
//! ported here without their Bevy and catalog layers:
//!
//! * XModels: `asset_model::model_skel` (`capture_model_skel_iw4`,
//!   `capture_pose_src`, `decode_surface_skin`), LOD 0 only, the packed
//!   vertices unpacked as `asset_model::packed_vertex` does;
//! * XAnimParts: `asset_anim::xanim_catalog` (`capture_xanim`,
//!   `copy_delta_trans`);
//! * materials and images: the texture table and GfxImage records
//!   `fastfile_iw4` records, joined by asset slot as `asset_material`'s link
//!   map does.
//!
//! iw4L is Apache-2.0 (NOTICE).
use crate::vm;
use asset_iw4::size as sz;
use fastfile_iw4::{
    AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, XAnimDeltaTransGeometry, XAnimPartsGeometry,
    XModelGeometry, ZonePtr, ZoneStream,
};
use std::collections::HashMap;

/// A GfxImage: its name and, when the zone carries the pixels, those.
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub name: String,
    pub width: u16,
    pub height: u16,
    pub level_count: u8,
    /// D3D9 format (FourCC for DXTn).
    pub format: u32,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponModels {
    pub gun: String,
    pub hands: String,
    pub hide_tags: Vec<String>,
    /// The rocket shown on the gun while it is loaded (WeaponDef rocketModel: the RPG-7's
    /// viewmodel_rpg7_rocket on tag_rocket); empty: none.
    pub rocket: String,
}

/// What a zone held for first person.
#[derive(Default)]
pub struct Found {
    pub models: Vec<vm::Model>,
    pub anims: HashMap<String, vm::Anim>,
    pub images: HashMap<String, Image>,
    /// Weapon name -> its 37 XAnim names (empty where unset).
    pub weapon_anims: HashMap<String, Vec<String>>,
    /// Weapon name -> (gun model, hand model, hidden gun tags); variants that
    /// share a weapon definition get its values.
    pub weapon_models: HashMap<String, WeaponModels>,
    /// Weapon name -> its notetrack sound map (animation notetrack -> sound
    /// alias, WeaponDef notetrackSoundMapKeys / Values); variants share it.
    pub weapon_notetracks: HashMap<String, Vec<(String, String)>>,
    /// Weapon name -> the images of its hip crosshair (centre, side), ADS
    /// overlay (the scope) and HUD icon materials; "" where unset. Variants share them.
    pub weapon_hud: HashMap<String, [String; 4]>,
    pub skipped: Vec<String>,
    /// The character animation sources (playeranim::MULTIPLAYER_ANIMTREE_PATH,
    /// playeranim::PLAYERANIM_SCRIPT_PATH), inflated, by rawfile name.
    pub raw: HashMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
struct MaterialRec {
    material: vm::Material,
}

struct Sink {
    strings: Option<ScriptStrings>,
    materials: Vec<MaterialRec>,
    material_slots: HashMap<Ptr, usize>,
    image_slots: HashMap<Ptr, usize>,
    images: Vec<Image>,
    defs: HashMap<Ptr, (String, String, String)>,
    notetrack_defs: HashMap<Ptr, Vec<(String, String)>>,
    hud_defs: HashMap<Ptr, [String; 4]>,
    found: Found,
    /// XModel names by asset slot, for weapons naming models loaded earlier.
    xmodel_names: std::collections::HashMap<Ptr, Ptr>,
}

// MaterialTextureDef semantics (TS_*).
const TS_2D: u8 = 0;
const TS_COLOR_MAP: u8 = 2;
const TS_NORMAL_MAP: u8 = 5;
const TS_SPECULAR_MAP: u8 = 8;
const TS_WATER_MAP: u8 = 11;
/// XRigidVertList / blend bone offsets are DObjSkelMat strides.
const BONE_STRIDE: u16 = 64;
/// Blend weights are 1/65536 units (dpvs_iw4 SKIN_BLEND_WEIGHT_SCALE).
const WEIGHT_SCALE: f32 = 1.0 / 65536.0;

fn ascii_name(s: &ZoneStream<'_>, p: Option<Ptr>) -> Option<String> {
    let name = s.cstr(p?).ok()?;
    let name = name.trim_start_matches(',');
    (!name.is_empty()).then(|| name.to_owned())
}

/// First-person models by iw4L's naming rule (asset_model::model_kind Fpv).
pub fn is_fpv_model(name: &str) -> bool {
    name.starts_with("viewmodel_") || name.starts_with("viewhands_") || name.contains("_viewmodel")
}

/// Multiplayer soldier bodies (their skeleton is what the character
/// animations are authored on; F4b retargets it onto the GTA ped).
pub fn is_body_model(name: &str) -> bool {
    name.starts_with("mp_body_")
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = u32::from(bits & 0x03ff);
    let value = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let mut mantissa = mantissa;
            let mut exponent = -14i32;
            while mantissa & 0x0400 == 0 {
                mantissa <<= 1;
                exponent -= 1;
            }
            sign | (((exponent + 127) as u32) << 23) | ((mantissa & 0x03ff) << 13)
        }
        0x1f => sign | 0x7f80_0000 | (mantissa << 13),
        _ => sign | ((u32::from(exponent) + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(value)
}

/// dpvs_iw4::skin_unpack_unit_vec.
fn unit_vec(packed: u32) -> [f32; 3] {
    let b = packed.to_le_bytes();
    let scale = (f32::from(b[3]) + 192.0) / (127.0 * 255.0);
    let n = [(f32::from(b[0]) - 127.0) * scale, (f32::from(b[1]) - 127.0) * scale, (f32::from(b[2]) - 127.0) * scale];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 1e-6 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 1.0] }
}

/// One vertex's influences: up to four (bone, weight).
#[derive(Clone, Copy, Default)]
struct Skin {
    bones: [u8; 4],
    weights: [f32; 4],
}

fn rigid(bone: u8) -> Skin {
    Skin { bones: [bone, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] }
}

fn decode_surface_skin(
    s: &ZoneStream<'_>,
    surface: Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
) -> Option<Vec<Skin>> {
    let mut skins = vec![Skin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u8> {
        if !raw.is_multiple_of(BONE_STRIDE) {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        ((bone as usize) < num_bones).then(|| u8::try_from(bone).ok()).flatten()
    };
    if vert_list_count > 0
        && let Ok(ZonePtr::Offset(list_ptr)) = s.ptr_at(surface, s.layout(36, 56))
    {
        let list = s.resolve_alias(list_ptr);
        let mut vertex = 0usize;
        for i in 0..vert_list_count {
            let entry = list.at(i * s.layout(sz::XRIGID_VERT_LIST, 16));
            let bone = bone_at(s.u16_at(entry, 0).ok()?)?;
            let run = s.u16_at(entry, 2).ok()? as usize;
            for _ in 0..run {
                if vertex >= vertex_count {
                    break;
                }
                skins[vertex] = rigid(bone);
                vertex += 1;
            }
        }
        if vertex == vertex_count {
            return Some(skins);
        }
    }
    let vi = surface.at(s.layout(16, 24));
    let counts = [
        s.i16_at(vi, 0).ok()?.max(0) as usize,
        s.i16_at(vi, 2).ok()?.max(0) as usize,
        s.i16_at(vi, 4).ok()?.max(0) as usize,
        s.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let blend = match s.ptr_at(vi, 8).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        ZonePtr::Null if counts.iter().all(|&c| c == 0) => {
            // No ownership recorded: everything on the root (iw4L's typed gap).
            return Some(vec![rigid(0); vertex_count]);
        }
        _ => return None,
    };
    let mut cursor = 0usize;
    let mut vertex = 0usize;
    for (bucket, count) in counts.iter().enumerate() {
        let influences = bucket + 1;
        for _ in 0..*count {
            let start = cursor;
            cursor += 1 + (influences - 1) * 2;
            let mut skin = Skin::default();
            skin.bones[0] = bone_at(s.u16_at(blend, start * 2).ok()?)?;
            let mut remaining = 1.0f32;
            for extra in 1..influences {
                let off = start + 1 + (extra - 1) * 2;
                skin.bones[extra] = bone_at(s.u16_at(blend, off * 2).ok()?)?;
                let w = f32::from(s.u16_at(blend, off * 2 + 2).ok()?) * WEIGHT_SCALE;
                skin.weights[extra] = w;
                remaining -= w;
            }
            skin.weights[0] = remaining.max(0.0);
            if vertex >= vertex_count {
                return None;
            }
            skins[vertex] = skin;
            vertex += 1;
        }
    }
    (vertex == vertex_count).then_some(skins)
}

impl Sink {
    fn script_string(&self, s: &ZoneStream<'_>, id: u16) -> String {
        self.strings.as_ref().and_then(|t| t.get(s, id)).unwrap_or("").to_owned()
    }

    fn capture_material(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let g = s.latest_material()?;
        let name = ascii_name(s, g.name)?;
        let mut material = vm::Material { name, sort_key: g.sort_key, ..Default::default() };
        if let Some(arr) = g.textures {
            for i in 0..g.texture_count {
                let b = arr.at(i * g.texture_stride);
                let semantic = s.u8_at(b, 7).ok()?;
                if semantic == TS_WATER_MAP {
                    continue;
                }
                let Some(&image) = self.image_slots.get(&b.at(8)) else { continue };
                let image = self.images[image].name.clone();
                let slot = match semantic {
                    // HUD materials (the crosshair, the scope) carry their image as TS_2D.
                    TS_COLOR_MAP | TS_2D => &mut material.color_map,
                    TS_NORMAL_MAP => &mut material.normal_map,
                    TS_SPECULAR_MAP => &mut material.specular_map,
                    _ => continue,
                };
                if slot.is_empty() {
                    *slot = image;
                }
            }
        }
        if let (Some(bits), true) = (g.state_bits, g.state_bits_count > 0) {
            material.state_bits = [s.u32_at(bits, 0).unwrap_or(0), s.u32_at(bits, 4).unwrap_or(0)];
        }
        self.materials.push(MaterialRec { material });
        Some(self.materials.len() - 1)
    }

    fn capture_image(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let g = s.latest_image()?;
        let name = ascii_name(s, g.name)?;
        let payload =
            if g.source_len == 0 { Vec::new() } else { s.source_slice(g.source_offset, g.source_len).ok()?.to_vec() };
        self.images.push(Image {
            name,
            width: g.width,
            height: g.height,
            level_count: g.level_count,
            format: g.format,
            payload,
        });
        Some(self.images.len() - 1)
    }

    fn capture_model(&mut self, s: &ZoneStream<'_>, g: XModelGeometry) -> Result<Option<vm::Model>, String> {
        let Some(name) = ascii_name(s, g.name) else { return Ok(None) };
        if !is_fpv_model(&name) && !is_body_model(&name) {
            return Ok(None);
        }
        let e = |what: &str| format!("{name}: {what}");
        let bone_names_ptr = g.bone_names.ok_or_else(|| e("no bone names"))?;
        let base_mat = g.base_mat.ok_or_else(|| e("no bind pose"))?;
        let mut model = vm::Model {
            name: name.clone(),
            num_root_bones: g.num_root_bones as u32,
            scale: g.scale,
            no_scale_part_bits: g.no_scale_part_bits,
            ..Default::default()
        };
        for i in 0..g.num_bones {
            let id = s.u16_at(bone_names_ptr, i * 2).map_err(|_| e("bone names"))?;
            model.bone_names.push(self.script_string(s, id));
            let m = base_mat.at(i * sz::DOBJ_ANIM_MAT);
            let f = |o: usize| s.f32_at(m, o).map_err(|_| e("bind pose"));
            model.base_mat.push(([f(0)?, f(4)?, f(8)?, f(12)?], [f(16)?, f(20)?, f(24)?]));
        }
        let child = g.num_bones.saturating_sub(g.num_root_bones);
        if child > 0 {
            let (parents, quats, trans) = (
                g.parent_list.ok_or_else(|| e("no parent list"))?,
                g.quats.ok_or_else(|| e("no bind rotations"))?,
                g.trans.ok_or_else(|| e("no bind translations"))?,
            );
            for i in 0..child {
                model.parent_list.push(s.u8_at(parents, i).map_err(|_| e("parent list"))?);
                let o = i * sz::XMODEL_QUAT;
                let q = |k: usize| s.i16_at(quats, o + k * 2).map_err(|_| e("bind rotations"));
                model.quats.push([q(0)?, q(1)?, q(2)?, q(3)?]);
                let t = |k: usize| s.f32_at(trans, i * 12 + k * 4).map_err(|_| e("bind translations"));
                model.trans.push([t(0)?, t(1)?, t(2)?]);
            }
        }
        // LOD 0 surfaces.
        let count = usize::from(g.lod_numsurfs[0]);
        let handle_base = usize::from(g.lod_surf_index[0]);
        let material_at = |sink: &Sink, slot: usize| -> vm::Material {
            (slot < g.material_handle_count)
                .then_some(g.material_handles)
                .flatten()
                .and_then(|h| sink.material_slots.get(&h.at(slot * s.pointer_bytes())))
                .map(|&i| sink.materials[i].material.clone())
                .unwrap_or_default()
        };
        let Some(surfaces) = g.lod_xsurfaces[0] else {
            // Camouflage variants reuse another model's surfaces (named in the LOD
            // info) with materials of their own.
            let source = g.lod_surface_names[0]
                .and_then(|p| s.cstr(p).ok())
                .map(|n| n.trim_start_matches(',').to_owned())
                .ok_or_else(|| e("LOD 0 without surfaces"))?;
            let shared = self
                .found
                .models
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(&source))
                .ok_or_else(|| e(&format!("shares the surfaces of {source}, which this zone did not hold")))?;
            if shared.surfaces.len() != count || shared.bone_names.len() != model.bone_names.len() {
                return Err(e(&format!("does not match the surfaces of {source}")));
            }
            model.vertices = shared.vertices.clone();
            model.indices = shared.indices.clone();
            model.surfaces = shared.surfaces.clone();
            for (k, surface) in model.surfaces.iter_mut().enumerate() {
                surface.material = material_at(self, handle_base + k);
            }
            return Ok(Some(model));
        };
        for surface_index in 0..count {
            let surface = surfaces.at(surface_index * s.layout(sz::XSURFACE, 88));
            let vertex_count = s.u16_at(surface, 2).map_err(|_| e("surface"))? as usize;
            let tri_count = s.u16_at(surface, 4).map_err(|_| e("surface"))? as usize;
            let vert_list_count = s.u32_at(surface, s.layout(32, 48)).map_err(|_| e("surface"))? as usize;
            let mut part_bits = [0u32; 6];
            for (i, w) in part_bits.iter_mut().enumerate() {
                *w = s.u32_at(surface, s.layout(40, 64) + i * 4).map_err(|_| e("part bits"))?;
            }
            let vertices = match s.ptr_at(surface, s.layout(28, 40)).map_err(|_| e("vertices"))? {
                ZonePtr::Offset(p) => s.resolve_alias(p),
                _ => return Err(e("surface without vertices")),
            };
            let triangles = match s.ptr_at(surface, s.layout(12, 16)).map_err(|_| e("triangles"))? {
                ZonePtr::Offset(p) => s.resolve_alias(p),
                _ => return Err(e("surface without triangles")),
            };
            let skins = decode_surface_skin(s, surface, vertex_count, vert_list_count, g.num_bones)
                .ok_or_else(|| e("vertex skinning"))?;
            let first_vertex = model.vertices.len() as u32;
            let first_index = model.indices.len() as u32;
            for (i, skin) in skins.iter().enumerate() {
                let v = vertices.at(i * sz::GFX_PACKED_VERTEX);
                let f = |o: usize| s.f32_at(v, o).map_err(|_| e("vertex"));
                let u = |o: usize| s.u32_at(v, o).map_err(|_| e("vertex"));
                let color = u(16)?.to_le_bytes();
                let uv = u(20)?;
                model.vertices.push(vm::Vertex {
                    position: [f(0)?, f(4)?, f(8)?],
                    color: [color[2], color[1], color[0], color[3]],
                    uv: [half_to_f32((uv >> 16) as u16), half_to_f32(uv as u16)],
                    normal: unit_vec(u(24)?),
                    tangent: unit_vec(u(28)?),
                    binormal_sign: if f(12)? < 0.0 { -1.0 } else { 1.0 },
                    bones: skin.bones,
                    weights: skin.weights,
                });
            }
            for k in 0..tri_count * 3 {
                let index = s.u16_at(triangles, k * 2).map_err(|_| e("triangles"))?;
                if usize::from(index) >= vertex_count {
                    return Err(e("triangle index past the surface"));
                }
                model.indices.push(first_vertex + u32::from(index));
            }
            let material = material_at(self, handle_base + surface_index);
            model.surfaces.push(vm::Surface {
                material,
                first_vertex,
                vertex_count: vertex_count as u32,
                first_index,
                index_count: (tri_count * 3) as u32,
                part_bits,
            });
        }
        if model.surfaces.is_empty() {
            return Err(e("no LOD 0 surfaces"));
        }
        Ok(Some(model))
    }
}

fn copy_u8(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u8> {
    ptr.and_then(|p| s.slice_at(p, 0, count).ok()).map(|b| b.to_vec()).unwrap_or_default()
}

fn copy_u16(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u16> {
    let Some(b) = ptr.and_then(|p| s.slice_at(p, 0, count * 2).ok()) else { return Vec::new() };
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

fn copy_u32(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u32> {
    let Some(b) = ptr.and_then(|p| s.slice_at(p, 0, count * 4).ok()) else { return Vec::new() };
    b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn copy_f32_3(s: &ZoneStream<'_>, ptr: Ptr, off: usize) -> Option<[f32; 3]> {
    let b = s.slice_at(ptr, off, 12).ok()?;
    Some([
        f32::from_le_bytes(b[0..4].try_into().ok()?),
        f32::from_le_bytes(b[4..8].try_into().ok()?),
        f32::from_le_bytes(b[8..12].try_into().ok()?),
    ])
}

fn copy_delta_trans(s: &ZoneStream<'_>, g: XAnimDeltaTransGeometry) -> Option<vm::DeltaTrans> {
    if let Some(constant) = g.constant {
        return Some(vm::DeltaTrans { small: g.small != 0, mins: copy_f32_3(s, constant, 0)?, ..Default::default() });
    }
    let mins_step = g.mins_step?;
    let n = g.size as usize + 1;
    let indices = if g.indices_are_bytes {
        copy_u8(s, g.indices, n).into_iter().map(u16::from).collect()
    } else {
        copy_u16(s, g.indices, n)
    };
    Some(vm::DeltaTrans {
        size: g.size,
        small: g.small != 0,
        mins: copy_f32_3(s, mins_step, 0)?,
        step: copy_f32_3(s, mins_step, 12)?,
        indices,
        packed: copy_u8(s, g.frames, if g.small != 0 { 3 * n } else { 6 * n }),
    })
}

impl AssetSink for Sink {
    fn set_script_strings(&mut self, strings: ScriptStrings) {
        self.strings = Some(strings);
    }

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        fastfile_iw4::load_asset_at_observed(s, ty, slot, self)?;
        Ok(())
    }
}

impl AssetLinkSink for Sink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        match ty {
            AssetType::Image => {
                if let Some(i) = self.capture_image(s) {
                    self.image_slots.insert(slot, i);
                    if let Some(p) = insert {
                        self.image_slots.insert(p, i);
                    }
                }
            }
            AssetType::Material => {
                let header = s.latest_material().and_then(|g| g.header);
                if let Some(i) = self.capture_material(s) {
                    self.material_slots.insert(slot, i);
                    if let Some(p) = insert {
                        self.material_slots.insert(p, i);
                    }
                    if let Some(h) = header {
                        self.material_slots.insert(h, i);
                    }
                }
            }
            AssetType::XModel => {
                if let Some(g) = s.xmodel() {
                    match self.capture_model(s, g) {
                        Ok(Some(m)) => self.found.models.push(m),
                        Ok(None) => {}
                        Err(why) => self.found.skipped.push(why),
                    }
                }
            }
            AssetType::Weapon => {
                if let Some(g) = s.weapon()
                    && let Some(name) = ascii_name(s, g.name)
                {
                    // Gun and hands live in the weapon definition, which variants share;
                    // the hidden tags are each variant's own.
                    let gun = ascii_name(s, g.gun_xmodel_name).or_else(|| ascii_name(s, g.gun_xmodel_names[0]));
                    let models = match (gun, g.weap_def) {
                        (Some(gun), def) => {
                            let m = (
                                gun,
                                ascii_name(s, g.hand_xmodel_name).unwrap_or_default(),
                                ascii_name(s, g.rocket_model_name).unwrap_or_default(),
                            );
                            if let Some(def) = def {
                                self.defs.insert(def, m.clone());
                            }
                            Some(m)
                        }
                        (None, Some(def)) => self.defs.get(&def).cloned(),
                        _ => None,
                    };
                    if let Some((gun, hands, rocket)) = models {
                        let mut hide_tags = Vec::new();
                        if let Some(arr) = g.hide_tags {
                            for i in 0..32 {
                                let id = s.u16_at(arr, i * 2).unwrap_or(0);
                                let tag = self.script_string(s, id);
                                if id != 0 && !tag.is_empty() {
                                    hide_tags.push(tag);
                                }
                            }
                        }
                        self.found.weapon_models.insert(name.clone(), WeaponModels { gun, hands, hide_tags, rocket });
                    }
                }
                if let Some(g) = s.weapon()
                    && let Some(name) = ascii_name(s, g.name)
                {
                    let map = match (g.notetrack_sound_keys, g.notetrack_sound_values) {
                        (Some(keys), Some(values)) => {
                            let mut map = Vec::new();
                            for i in 0..16 {
                                let (k, v) = (s.u16_at(keys, i * 2).unwrap_or(0), s.u16_at(values, i * 2).unwrap_or(0));
                                let (k, v) = (self.script_string(s, k), self.script_string(s, v));
                                if !k.is_empty() && !v.is_empty() {
                                    map.push((k.to_ascii_lowercase(), v.to_ascii_lowercase()));
                                }
                            }
                            if let Some(def) = g.weap_def {
                                self.notetrack_defs.insert(def, map.clone());
                            }
                            Some(map)
                        }
                        _ => g.weap_def.and_then(|d| self.notetrack_defs.get(&d).cloned()),
                    };
                    if let Some(map) = map {
                        self.found.weapon_notetracks.insert(name, map);
                    }
                }
                if let Some(g) = s.weapon()
                    && let Some(name) = ascii_name(s, g.name)
                {
                    let image = |slot: Option<Ptr>| {
                        slot.and_then(|p| {
                            self.material_slots.get(&p).or_else(|| self.material_slots.get(&s.resolve_alias(p)))
                        })
                        .map(|&i| self.materials[i].material.color_map.clone())
                        .unwrap_or_default()
                    };
                    let hud = [
                        image(g.reticle_center_material_slot),
                        image(g.reticle_side_material_slot),
                        image(g.overlay_material_slot),
                        image(g.hud_icon_slot),
                    ];
                    let hud = if hud.iter().any(|h| !h.is_empty()) {
                        if let Some(def) = g.weap_def {
                            self.hud_defs.insert(def, hud.clone());
                        }
                        Some(hud)
                    } else {
                        g.weap_def.and_then(|d| self.hud_defs.get(&d).cloned())
                    };
                    if let Some(hud) = hud {
                        self.found.weapon_hud.insert(name, hud);
                    }
                }
                if let Some(g) = s.weapon()
                    && let (Some(name), Some(arr)) = (ascii_name(s, g.name), g.sz_xanims)
                {
                    let anims = (0..sz::WEAPON_ANIM_COUNT)
                        .map(|i| match s.ptr_at(arr, i * s.pointer_bytes()) {
                            Ok(ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).unwrap_or("").to_owned(),
                            _ => String::new(),
                        })
                        .collect();
                    self.found.weapon_anims.insert(name, anims);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn remember_xmodel_name(&mut self, slot: Ptr, insert_slot: Option<Ptr>, name: Ptr) {
        self.xmodel_names.insert(slot, name);
        if let Some(p) = insert_slot {
            self.xmodel_names.insert(p, name);
        }
    }

    fn xmodel_name_ptr(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_names.get(&slot).copied()
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        let map = match ty {
            AssetType::Image => &mut self.image_slots,
            AssetType::Material => &mut self.material_slots,
            _ => return Ok(()),
        };
        if let Some(&i) = map.get(&target) {
            map.insert(slot, i);
        }
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> fastfile_iw4::Result<()> {
        if name == crate::playeranim::MULTIPLAYER_ANIMTREE_PATH || name == crate::playeranim::PLAYERANIM_SCRIPT_PATH {
            let mut bytes = if zlib_compressed {
                miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap_or_default()
            } else {
                data.to_vec()
            };
            if bytes.last() == Some(&0) {
                bytes.pop();
            }
            if !bytes.is_empty() {
                self.found.raw.insert(name.to_owned(), bytes);
            }
        }
        Ok(())
    }

    fn capture_xanim(&mut self, s: &ZoneStream<'_>, g: XAnimPartsGeometry) -> fastfile_iw4::Result<()> {
        let Some(name) = ascii_name(s, g.name) else { return Ok(()) };
        let track_count = g.bone_count[9] as usize;
        let mut names = Vec::with_capacity(track_count);
        if let Some(arr) = g.names {
            for i in 0..track_count {
                let sid = s.u16_at(arr, i * 2).unwrap_or(0);
                names.push(self.script_string(s, sid));
            }
        } else if track_count > 0 {
            self.found.skipped.push(format!("{name}: animation without track names"));
            return Ok(());
        }
        let mut notifies = Vec::with_capacity(g.notify_count);
        if let Some(arr) = g.notify {
            for i in 0..g.notify_count {
                let b = arr.at(i * sz::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                notifies.push(vm::Notify { name: self.script_string(s, sid), time: s.f32_at(b, 4).unwrap_or(0.0) });
            }
        }
        let indices = if g.indices_are_bytes {
            copy_u8(s, g.indices, g.index_count).into_iter().map(u16::from).collect()
        } else {
            copy_u16(s, g.indices, g.index_count)
        };
        let anim = vm::Anim {
            name: name.clone(),
            numframes: g.numframes,
            flags: g.flags,
            bone_count: g.bone_count,
            framerate: g.framerate,
            names,
            notifies,
            data_byte: copy_u8(s, g.data_byte, g.data_byte_count),
            data_short: copy_u16(s, g.data_short, g.data_short_count),
            data_int: copy_u32(s, g.data_int, g.data_int_count),
            random_data_byte: copy_u8(s, g.random_data_byte, g.random_data_byte_count),
            random_data_short: copy_u16(s, g.random_data_short, g.random_data_short_count),
            random_data_int: copy_u32(s, g.random_data_int, g.random_data_int_count),
            indices,
            delta_trans: copy_delta_trans(s, g.delta_trans),
        };
        self.found.anims.insert(name, anim);
        Ok(())
    }
}

/// The first-person content of one zone image (`zone::open`).
pub fn capture(image: &[u8]) -> Result<Found, String> {
    let header = fastfile_iw4::parse_zone_header(image).map_err(|e| format!("zone header: {e:?}"))?;
    let mut memory = crate::zone::Memory::for_header(&header);
    let mut stream = memory.stream(image)?;
    let mut sink = Sink {
        strings: None,
        materials: Vec::new(),
        material_slots: HashMap::new(),
        image_slots: HashMap::new(),
        images: Vec::new(),
        defs: HashMap::new(),
        notetrack_defs: HashMap::new(),
        hud_defs: HashMap::new(),
        xmodel_names: HashMap::new(),
        found: Found::default(),
    };
    fastfile_iw4::load_zone(&mut stream, &mut sink).map_err(|e| format!("zone walk stopped: {e:?}"))?;
    for img in std::mem::take(&mut sink.images) {
        sink.found.images.entry(img.name.clone()).or_insert(img);
    }
    Ok(sink.found)
}

#[cfg(test)]
mod tests {
    #[test]
    fn halves_and_unit_vectors_decode() {
        assert_eq!(super::half_to_f32(0x3c00), 1.0);
        assert_eq!(super::half_to_f32(0xc000), -2.0);
        // (127, 127, 255) with scale byte 63: +z.
        let n = super::unit_vec(u32::from_le_bytes([127, 127, 255, 63]));
        assert!(n[2] > 0.99, "{n:?}");
        assert!(
            super::is_fpv_model("viewmodel_m4")
                && super::is_fpv_model("viewhands_tf141")
                && !super::is_fpv_model("weapon_m4")
        );
    }
}
