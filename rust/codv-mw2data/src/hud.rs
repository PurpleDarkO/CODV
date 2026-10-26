//! MW2's HUD from the player's zones (F5b): the fonts (glyph tables and the
//! atlas each draws from), the localized strings the HUD shows, the layout
//! of the HUD menus (item rectangles, alignment, colours, fonts, owner
//! draws), and the images of the HUD materials (ammo counter rounds, hit
//! marker, damage direction, grenade indicator).
//!
//! The capture uses `fastfile_iw4`'s font, menu and localize records (iw4L's
//! retail zone loader) and joins materials to images by asset slot, as
//! `viewmodels` does. iw4L is Apache-2.0 (NOTICE).
use asset_iw4::size as sz;
use fastfile_iw4::{
    AssetLinkSink, AssetSink, AssetType, FontCapture, GlyphCapture, MenuDefCapture, MenuItemLayout, Ptr, ScriptStrings,
    ZoneStream,
};
use std::collections::HashMap;

/// One glyph of a font (GlyphCapture).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glyph {
    pub letter: u16,
    pub x0: i8,
    pub y0: i8,
    pub dx: u8,
    pub width: u8,
    pub height: u8,
    pub st: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Font {
    pub name: String,
    pub pixel_height: i32,
    /// The atlas image (the material's colour map).
    pub image: String,
    pub glyphs: Vec<Glyph>,
}

/// One HUD menu item (MenuItemLayout).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub menu: String,
    pub name: String,
    pub text: String,
    pub owner_draw: i32,
    pub rect: [f32; 4],
    pub align: [u8; 2],
    pub fore_color: [f32; 4],
    pub back_color: [f32; 4],
    pub text_scale: f32,
    pub font: i32,
    pub text_align: i32,
    pub text_align_xy: [f32; 2],
    pub text_style: i32,
    pub style: i32,
    /// The background material's image.
    pub background: String,
}

/// What a zone held for the HUD.
#[derive(Default)]
pub struct Found {
    pub fonts: Vec<Font>,
    pub strings: HashMap<String, String>,
    pub menus: Vec<(String, [f32; 4])>,
    pub items: Vec<Item>,
    /// Material name -> its colour map image.
    pub materials: HashMap<String, String>,
    /// Material name -> its texture atlas (rows, columns) when it is a flipbook (F5c: the animated dpad icons).
    pub atlas: HashMap<String, [u8; 2]>,
    /// The HUD menus' item expressions in capture order, after the item they belong to (menu, kind, dump).
    pub exprs: Vec<(String, String, String)>,
    pub raw: HashMap<String, Vec<u8>>,
    /// String tables (CSV assets) by name: columns and the cells row by row.
    pub tables: HashMap<String, (usize, Vec<String>)>,
    /// Font atlases the zone carries itself (name -> D3D9 format, size, levels, pixels).
    pub images: HashMap<String, (u32, u16, u16, u8, Vec<u8>)>,
}

struct Sink {
    found: Found,
    /// Material asset slot -> (name, image).
    material_slots: HashMap<Ptr, (String, String)>,
    image_slots: HashMap<Ptr, String>,
    material_by_name: HashMap<String, String>,
}

const TS_2D: u8 = 0;
const TS_COLOR_MAP: u8 = 2;

fn ascii(s: &ZoneStream<'_>, p: Option<Ptr>) -> Option<String> {
    let name = s.cstr(p?).ok()?;
    let name = name.trim_start_matches(',');
    (!name.is_empty()).then(|| name.to_owned())
}

impl Sink {
    fn material(&mut self, s: &ZoneStream<'_>) -> Option<(String, String)> {
        let g = s.latest_material()?;
        let name = ascii(s, g.name)?;
        let mut image = String::new();
        if let Some(arr) = g.textures {
            for i in 0..g.texture_count {
                let b = arr.at(i * g.texture_stride);
                let semantic = s.u8_at(b, 7).ok()?;
                if (semantic == TS_2D || semantic == TS_COLOR_MAP)
                    && let Some(img) = self.image_slots.get(&b.at(8))
                {
                    image = img.clone();
                    break;
                }
            }
        }
        Some((name, image))
    }

    fn image_of(&self, material: &str) -> String {
        self.material_by_name.get(&material.to_ascii_lowercase()).cloned().unwrap_or_default()
    }
}

impl AssetSink for Sink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

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
                if let Some(g) = s.latest_image()
                    && let Some(name) = ascii(s, g.name)
                    && name.contains("font")
                    && g.source_len > 0
                    && let Ok(px) = s.source_slice(g.source_offset, g.source_len)
                {
                    self.found.images.insert(name, (g.format, g.width, g.height, g.level_count, px.to_vec()));
                }
                if let Some(name) = s.latest_image().and_then(|g| ascii(s, g.name)) {
                    self.image_slots.insert(slot, name.clone());
                    if let Some(p) = insert {
                        self.image_slots.insert(p, name);
                    }
                }
            }
            AssetType::Material => {
                let header = s.latest_material().and_then(|g| g.header);
                let atlas = s.latest_material().map(|g| g.texture_atlas).unwrap_or([0, 0]);
                if let Some(m) = self.material(s) {
                    if atlas[0] > 1 || atlas[1] > 1 {
                        self.found.atlas.insert(m.0.to_ascii_lowercase(), atlas);
                    }
                    self.material_by_name.insert(m.0.to_ascii_lowercase(), m.1.clone());
                    self.found.materials.insert(m.0.to_ascii_lowercase(), m.1.clone());
                    for p in [Some(slot), insert, header].into_iter().flatten() {
                        self.material_slots.insert(p, m.clone());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        match ty {
            AssetType::Material => {
                if let Some(m) = self.material_slots.get(&target).cloned() {
                    self.material_slots.insert(slot, m);
                }
            }
            AssetType::Image => {
                if let Some(m) = self.image_slots.get(&target).cloned() {
                    self.image_slots.insert(slot, m);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        self.material_slots.get(&slot).map(|(n, _)| n.as_str())
    }

    fn capture_localize(&mut self, name: &str, value: &[u8]) -> fastfile_iw4::Result<()> {
        // Latin-1 as MW2 stores it.
        let text: String = value.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        self.found.strings.insert(name.to_ascii_uppercase(), text);
        Ok(())
    }

    fn capture_string_table(&mut self, s: &ZoneStream<'_>, header: Ptr) -> fastfile_iw4::Result<()> {
        // asset_game menu_catalog capture_string_table.
        let Ok(fastfile_iw4::ZonePtr::Offset(np)) = s.ptr_at(header, 0) else { return Ok(()) };
        let name = s.cstr(s.resolve_alias(np)).unwrap_or("").to_ascii_lowercase();
        let columns = s.i32_at(header, s.layout(4, 8)).unwrap_or(0).max(0) as usize;
        let rows = s.i32_at(header, s.layout(8, 12)).unwrap_or(0).max(0) as usize;
        let mut cells = Vec::new();
        if let Ok(fastfile_iw4::ZonePtr::Offset(ap)) = s.ptr_at(header, s.layout(12, 16)) {
            let arr = s.resolve_alias(ap);
            for i in 0..columns.saturating_mul(rows).min(1 << 20) {
                let cell = arr.at(i * s.layout(sz::STRING_TABLE_CELL, 16));
                cells.push(match s.ptr_at(cell, 0) {
                    Ok(fastfile_iw4::ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).unwrap_or("").to_owned(),
                    _ => String::new(),
                });
            }
        }
        if !name.is_empty() {
            self.found.tables.insert(name, (columns, cells));
        }
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> fastfile_iw4::Result<()> {
        let keep = ["damagefeedback", "_hud", "healthoverlay", "killstreak", "_uav", "_emp", "_rank", "_music"];
        if keep.iter().any(|k| name.contains(k)) || std::env::var_os("CODV_KEEP_ALL_RAW").is_some() {
            let bytes = if zlib_compressed {
                miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap_or_default()
            } else {
                data.to_vec()
            };
            self.found.raw.insert(name.to_owned(), bytes);
        }
        Ok(())
    }

    fn capture_font(&mut self, rec: &FontCapture<'_>) -> fastfile_iw4::Result<()> {
        let mut glyphs = Vec::with_capacity(rec.glyphs.len() / sz::GLYPH);
        for chunk in rec.glyphs.chunks_exact(sz::GLYPH) {
            let mut row = [0u8; sz::GLYPH];
            row.copy_from_slice(chunk);
            let g = GlyphCapture::from_row(&row);
            glyphs.push(Glyph {
                letter: g.letter,
                x0: g.x0,
                y0: g.y0,
                dx: g.dx,
                width: g.pixel_width,
                height: g.pixel_height,
                st: [g.s0, g.t0, g.s1, g.t1],
            });
        }
        self.found.fonts.push(Font {
            name: rec.name.to_ascii_lowercase(),
            pixel_height: rec.pixel_height,
            image: self.image_of(rec.material),
            glyphs,
        });
        Ok(())
    }

    fn capture_menu_def(&mut self, rec: &MenuDefCapture<'_>) -> fastfile_iw4::Result<()> {
        let r = rec.rect;
        self.found.menus.push((rec.name.to_ascii_lowercase(), [r.x, r.y, r.w, r.h]));
        Ok(())
    }

    fn capture_menu_script(
        &mut self,
        menu: &str,
        item: &str,
        kind: fastfile_iw4::MenuScriptKind,
        script: &str,
    ) -> fastfile_iw4::Result<()> {
        if [
            "missilecam_hud_hd",
            "ac130_hud_hd",
            "ac130timer",
            "remote_chopper_overlay_hd",
            "killstreak_pickup",
            "targetmap_fullscreen",
        ]
        .contains(&menu)
        {
            self.found.exprs.push((menu.to_owned(), format!("script {kind:?} {item}"), script.to_owned()));
        }
        Ok(())
    }

    fn capture_item_visible_exp(&mut self, menu: &str, _item: &str, dump: &str) -> fastfile_iw4::Result<()> {
        self.found.exprs.push((menu.to_ascii_lowercase(), "visible".into(), dump.to_owned()));
        Ok(())
    }

    fn capture_item_float_exp(&mut self, menu: &str, _item: &str, key: u32, dump: &str) -> fastfile_iw4::Result<()> {
        self.found.exprs.push((menu.to_ascii_lowercase(), format!("float{key}"), dump.to_owned()));
        Ok(())
    }

    fn capture_item_text_exp(&mut self, menu: &str, _item: &str, dump: &str) -> fastfile_iw4::Result<()> {
        self.found.exprs.push((menu.to_ascii_lowercase(), "text".into(), dump.to_owned()));
        Ok(())
    }

    fn capture_item_material_exp(&mut self, menu: &str, _item: &str, dump: &str) -> fastfile_iw4::Result<()> {
        self.found.exprs.push((menu.to_ascii_lowercase(), "material".into(), dump.to_owned()));
        Ok(())
    }

    fn capture_menu_item_layout(&mut self, rec: &MenuItemLayout<'_>) -> fastfile_iw4::Result<()> {
        let r = rec.rect;
        let text: String = rec.text.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        let background = if rec.background.is_empty() { String::new() } else { self.image_of(rec.background) };
        self.found.exprs.push((
            rec.menu.to_ascii_lowercase(),
            "ITEM".into(),
            format!("od{} {:?}", rec.owner_draw, [r.x, r.y]),
        ));
        self.found.items.push(Item {
            menu: rec.menu.to_ascii_lowercase(),
            name: rec.name.to_owned(),
            text,
            owner_draw: rec.owner_draw,
            rect: [r.x, r.y, r.w, r.h],
            align: [r.horz_align, r.vert_align],
            fore_color: rec.fore_color,
            back_color: rec.back_color,
            text_scale: rec.text_scale,
            font: rec.font_enum,
            text_align: rec.text_align_mode,
            text_align_xy: [rec.text_align_x, rec.text_align_y],
            text_style: rec.text_style,
            style: rec.style,
            background: if background.is_empty() { rec.background.to_ascii_lowercase() } else { background },
        });
        Ok(())
    }
}

/// The HUD content of one zone image (`zone::open`).
pub fn capture(image: &[u8]) -> Result<Found, String> {
    let header = fastfile_iw4::parse_zone_header(image).map_err(|e| format!("zone header: {e:?}"))?;
    let mut memory = crate::zone::Memory::for_header(&header);
    let mut stream = memory.stream(image)?;
    let mut sink = Sink {
        found: Found::default(),
        material_slots: HashMap::new(),
        image_slots: HashMap::new(),
        material_by_name: HashMap::new(),
    };
    fastfile_iw4::load_zone(&mut stream, &mut sink).map_err(|e| format!("zone walk stopped: {e:?}"))?;
    Ok(sink.found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lists what the player's zones hold for the HUD (CODV_MW2 = the MW2 folder).
    #[test]
    fn probe_hud() {
        let Ok(mw2) = std::env::var("CODV_MW2") else { return };
        for zone in [
            "common_mp.ff",
            "patch_mp.ff",
            "code_post_gfx_mp.ff",
            "ui_mp.ff",
            "mp_rust.ff",
            "localized_common_mp.ff",
            "localized_code_post_gfx_mp.ff",
            "localized_ui_mp.ff",
        ] {
            let bytes = std::fs::read(std::path::Path::new(&mw2).join("zone/english").join(zone)).unwrap();
            let image = crate::zone::open(&bytes).unwrap();
            let f = capture(&image).unwrap();
            eprintln!(
                "== {zone}: {} fonts, {} strings, {} menus, {} items, {} materials",
                f.fonts.len(),
                f.strings.len(),
                f.menus.len(),
                f.items.len(),
                f.materials.len()
            );
            for font in &f.fonts {
                eprintln!(
                    "font {} px{} image '{}' glyphs {}",
                    font.name,
                    font.pixel_height,
                    font.image,
                    font.glyphs.len()
                );
            }
            for (m, r) in &f.menus {
                if m.starts_with("weaponbar") || m.starts_with("dpad") {
                    eprintln!("MENU {m} {r:?}");
                }
            }
            if let Ok(dir) = std::env::var("CODV_RAW_DUMP") {
                for (n, b) in &f.raw {
                    let p = std::path::Path::new(&dir).join(n);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, b).unwrap();
                }
            }
            if let Ok(dir) = std::env::var("CODV_RAW_DUMP") {
                for (n, (cols, cells)) in &f.tables {
                    let p = std::path::Path::new(&dir).join("tables").join(n);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    let text: Vec<String> = cells.chunks((*cols).max(1)).map(|r| r.join(",")).collect();
                    std::fs::write(p, text.join("\n")).unwrap();
                }
            }
            if std::env::var("CODV_PROBE_KS").is_ok() {
                let mut t: Vec<_> =
                    f.tables.keys().filter(|k| k.contains("killstreak") || k.contains("splash")).collect();
                t.sort();
                eprintln!(
                    "KS TABLES {zone}: {t:?} killstreak menu items: {}",
                    f.items.iter().filter(|i| i.menu == "killstreak").count()
                );
                eprintln!("KS ATLAS {zone} count {} {:?}", f.atlas.len(), f.atlas.iter().take(8).collect::<Vec<_>>());
                for (m, img) in &f.materials {
                    if m == "specialty_uav"
                        || m == "dpad_killstreak_uav"
                        || m == "specialty_emp"
                        || m == "dpad_killstreak_emp"
                    {
                        eprintln!("KS MAT {zone} {m} -> {img}");
                    }
                }
                let mut ks: Vec<_> = f
                    .strings
                    .iter()
                    .filter(|(k, _)| {
                        k.starts_with("KILLSTREAKS_")
                            || k.starts_with("MP_EARNED_")
                            || k.starts_with("MP_KILLSTREAK")
                            || k.contains("ACTIONSLOT")
                            || k.contains("EMP")
                    })
                    .collect();
                ks.sort();
                for (k, v) in ks {
                    eprintln!("KS string {k} = {v:?}");
                }
                let mut rs: Vec<_> = f
                    .strings
                    .iter()
                    .filter(|(k, _)| {
                        k.starts_with("AC130_")
                            || k.starts_with("PLATFORM_PREDATOR")
                            || k.starts_with("PLATFORM_UI_AC130")
                            || k.contains("BUTTONLOOK")
                            || k.contains("CONFIRM_TARGET")
                            || k.contains("NUKE")
                            || k.contains("CRATE")
                            || k.contains("SENTRY")
                            || k.contains("AIRSTRIKE")
                            || k.contains("HELI")
                    })
                    .collect();
                rs.sort();
                for (k, v) in rs {
                    eprintln!("RS string {k} = {v:?}");
                }
                for (m, img) in &f.materials {
                    if m.starts_with("specialty_uav")
                        || m.starts_with("specialty_emp")
                        || m.starts_with("dpad_killstreak_uav")
                        || m.starts_with("dpad_killstreak_emp")
                        || m.contains("killstreak") && !m.contains("crate")
                    {
                        eprintln!("KS material {m} -> {img}");
                    }
                }
                for it in f.items.iter().filter(|i| {
                    [
                        "missilecam_hud_hd",
                        "ac130_hud_hd",
                        "ac130timer",
                        "remote_chopper_overlay_hd",
                        "killstreak_pickup",
                        "targetmap_fullscreen",
                    ]
                    .contains(&i.menu.as_str())
                        || (i.menu == "weaponbar_hd"
                            && (i.owner_draw == 0
                                || (180..230).contains(&i.owner_draw)
                                || i.name.contains("action")
                                || i.name.contains("dpad")))
                }) {
                    eprintln!(
                        "KS ITEM {} '{}' od{} rect {:?} al {:?} fore {:?} scale {} font {} talign {} txy {:?} tstyle {} style {} bg '{}' text {:?}",
                        it.menu,
                        it.name,
                        it.owner_draw,
                        it.rect,
                        it.align,
                        it.fore_color,
                        it.text_scale,
                        it.font,
                        it.text_align,
                        it.text_align_xy,
                        it.text_style,
                        it.style,
                        it.background,
                        it.text
                    );
                }
                for (m, k, d) in f.exprs.iter().filter(|e| {
                    [
                        "missilecam_hud_hd",
                        "ac130_hud_hd",
                        "ac130timer",
                        "remote_chopper_overlay_hd",
                        "killstreak_pickup",
                        "targetmap_fullscreen",
                    ]
                    .contains(&e.0.as_str())
                }) {
                    eprintln!("KS EXPR {m} {k}: {d}");
                }
                for (m, r) in f.menus.iter().filter(|m| {
                    [
                        "missilecam_hud_hd",
                        "ac130_hud_hd",
                        "ac130timer",
                        "remote_chopper_overlay_hd",
                        "killstreak_pickup",
                        "targetmap_fullscreen",
                    ]
                    .contains(&m.0.as_str())
                }) {
                    eprintln!("KS MENU {m} {r:?}");
                }
            }
            for (m, k, d) in f.exprs.iter().filter(|e| e.0 == "weaponbar_hd_x") {
                eprintln!("EXPR {m} {k}: {d}");
            }
            for it in f.items.iter().filter(|i| i.menu == "weaponbar_hd_x") {
                eprintln!(
                    "ITEM {} '{}' od{} rect {:?} al {:?} fore {:?} back {:?} scale {} font {} talign {} txy {:?} tstyle {} style {} bg '{}' text {:?}",
                    it.menu,
                    it.name,
                    it.owner_draw,
                    it.rect,
                    it.align,
                    it.fore_color,
                    it.back_color,
                    it.text_scale,
                    it.font,
                    it.text_align,
                    it.text_align_xy,
                    it.text_style,
                    it.style,
                    it.background,
                    it.text
                );
            }
            let mut menus: Vec<&String> = f.menus.iter().map(|(n, _)| n).collect();
            menus.sort();
            eprintln!("menus: {menus:?}");
            for k in
                ["PLATFORM_RELOAD", "PLATFORM_LOW_AMMO_NO_RELOAD", "WEAPON_NO_AMMO", "WEAPON_M4_CARBINE", "WEAPON_USP"]
            {
                if let Some(v) = f.strings.get(k) {
                    eprintln!("string {k} = {v:?}");
                }
            }
            for (m, img) in &f.materials {
                if m.contains("ammo_counter")
                    || m.contains("damage_feedback")
                    || m.contains("hit_direction")
                    || m.contains("grenade")
                    || m.starts_with("hud_")
                    || m.contains("blood")
                {
                    eprintln!("material {m} -> {img}");
                }
            }
        }
    }
}
