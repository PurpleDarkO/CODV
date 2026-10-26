//! CODV's import of the player's own Modern Warfare 2: the multiplayer
//! weapons' definitions read from their fastfiles with iw4L's retail zone
//! loader (`fastfile_iw4`) and written as CODV's weapon table (`table`).
//! Runs on the player's machine at setup; no MW2 data is part of CODV.
#[cfg(feature = "import")]
pub mod hud;
pub mod hudfile;
#[cfg(feature = "import")]
pub mod iwd;
pub mod killstreaks;
pub mod playeranim;
#[cfg(feature = "import")]
pub mod sounds;
pub mod table;
#[cfg(feature = "import")]
pub mod viewmodels;
pub mod vm;
#[cfg(feature = "import")]
pub mod weapons;
#[cfg(feature = "import")]
pub mod zone;

/// Reads `<mw2>/zone/english/common_mp.ff` (and `patch_mp.ff`, which can
/// override it) into the weapon table.
#[cfg(feature = "import")]
pub fn import(mw2: &std::path::Path) -> Result<table::Table, String> {
    let zones = mw2.join("zone").join("english");
    let mut table = table::Table::default();
    let mut sources = Vec::new();
    for name in ["common_mp.ff", "patch_mp.ff"] {
        let path = zones.join(name);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if name == "common_mp.ff" => return Err(format!("{}: {e}", path.display())),
            Err(_) => continue,
        };
        let image = zone::open(&bytes).map_err(|e| format!("{name}: {e}"))?;
        let found = weapons::capture(&image).map_err(|e| format!("{name}: {e}"))?;
        sources.push(format!("{name} ({} weapons)", found.weapons.len()));
        if found.location.is_some() {
            table.location = found.location;
        }
        for row in found.weapons {
            match table.weapons.iter_mut().find(|w| w.name == row.name) {
                Some(w) => *w = row,
                None => table.weapons.push(row),
            }
        }
    }
    table.source = sources.join(", ");
    Ok(table)
}

/// The guns CODV can hold (the runtime's arsenal rule: bullet weapons with
/// a magazine and a fire time, in the primary inventory, and the launchers).
pub fn is_holdable_gun(row: &table::Row) -> bool {
    (row.i("weap_type") == 0 && holdable(row)) || is_launcher(row)
}

/// MW2's launchers a player carries (RPG-7, AT4, Stinger, Javelin, Thumper): projectile
/// weapons of the rocket launcher (7) or grenade launcher (6) class in the main inventory
/// (the guns' underslung launchers are alt modes, the killstreaks' guns other inventories).
pub fn is_launcher(row: &table::Row) -> bool {
    row.i("weap_type") == 2
        && matches!(row.i("weap_class"), 6 | 7)
        && row.i("clip_size") > 0
        && row.i("fire_time_ms") > 0
        && row.i("inventory_type") == 0
}

/// Whether a row is a gun or launcher with a magazine, a fire time and a main inventory slot.
fn holdable(row: &table::Row) -> bool {
    row.i("clip_size") > 0 && row.i("fire_time_ms") > 0 && row.i("inventory_type") == 0
}

/// MW2's default first-person arms (iw4L asset_model::fpv_catalog VIEWHANDS_NAME).
pub const DEFAULT_HANDS: &str = "viewmodel_base_viewhands";

/// What the viewmodel import wrote.
#[derive(Debug, Default)]
pub struct ViewmodelSummary {
    pub models: usize,
    pub anims: usize,
    pub images: usize,
    pub bytes: u64,
    pub missing_images: Vec<String>,
    pub missing_models: Vec<String>,
    pub skipped_models: usize,
    /// Third person (F4b): the soldier skeleton's model and zone, and how
    /// many character animations were written (`body/`); empty when absent.
    pub body_model: String,
    pub body_zone: String,
    pub body_anims: usize,
    pub body_error: Option<String>,
}

/// The soldier whose skeleton the character animations are retargeted from,
/// and the zones searched for it (common_mp has no bodies; maps do).
pub const BODY_MODEL: &str = "mp_body_desert_tf141_assault_a";
#[cfg(feature = "import")]
const BODY_ZONES: [&str; 4] = ["mp_rust.ff", "mp_terminal.ff", "mp_highrise.ff", "mp_afghan.ff"];

/// Reads the first-person meshes, animations and images the holdable guns of
/// `table` use from the player's MW2 (`zone/english/*.ff`, `main/iw_*.iwd`)
/// into `out/viewmodels/{models,anims,images}`, and records on each gun its
/// models, hidden tags and animation names (`gun_model`, `hand_model`,
/// `hide_tags`, `xanim.<slot>`).
#[cfg(feature = "import")]
pub fn import_viewmodels(
    mw2: &std::path::Path,
    table: &mut table::Table,
    out: &std::path::Path,
) -> Result<ViewmodelSummary, String> {
    use std::collections::{BTreeSet, HashMap};
    let zones = mw2.join("zone").join("english");
    let mut found = viewmodels::Found::default();
    for name in ["common_mp.ff", "patch_mp.ff"] {
        let Ok(bytes) = std::fs::read(zones.join(name)) else { continue };
        let image = zone::open(&bytes).map_err(|e| format!("{name}: {e}"))?;
        let f = viewmodels::capture(&image).map_err(|e| format!("{name}: {e}"))?;
        for m in f.models {
            found.models.retain(|x| x.name != m.name);
            found.models.push(m);
        }
        found.anims.extend(f.anims.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)));
        found.weapon_anims.extend(f.weapon_anims);
        found.weapon_models.extend(f.weapon_models);
        found.skipped.extend(f.skipped);
        found.raw.extend(f.raw);
        found.weapon_notetracks.extend(f.weapon_notetracks);
        found.weapon_hud.extend(f.weapon_hud);
    }
    let models: HashMap<String, &vm::Model> = found.models.iter().map(|m| (m.name.to_ascii_lowercase(), m)).collect();
    let mut want_models = BTreeSet::new();
    let mut want_anims = BTreeSet::new();
    let mut want_images = BTreeSet::new();
    let mut summary = ViewmodelSummary { skipped_models: found.skipped.len(), ..Default::default() };
    // Guns, and the thrown frag-class grenades (their throw animations).
    for row in
        table.weapons.iter_mut().filter(|r| is_holdable_gun(r) || (r.i("weap_type") == 1 && r.i("offhand_class") == 1))
    {
        let Some(wm) = found.weapon_models.get(&row.name) else { continue };
        let hands = if wm.hands.is_empty() { DEFAULT_HANDS.to_owned() } else { wm.hands.clone() };
        row.strings.retain(|(k, _)| {
            k != "gun_model" && k != "hand_model" && k != "hide_tags" && k != "rocket_model" && !k.starts_with("xanim.")
        });
        row.strings.push(("gun_model".into(), wm.gun.clone()));
        row.strings.push(("hand_model".into(), hands.clone()));
        row.strings.push(("hide_tags".into(), wm.hide_tags.join(",")));
        // The rocket on a loaded launcher (the RPG-7's), when the zones hold its model.
        if !wm.rocket.is_empty() && models.contains_key(&wm.rocket.to_ascii_lowercase()) {
            row.strings.push(("rocket_model".into(), wm.rocket.clone()));
            want_models.insert(wm.rocket.to_ascii_lowercase());
        }
        row.strings.retain(|(k, _)| k != "notetrack_sounds" && !k.ends_with("_image"));
        // MW2's hip crosshair and scope images (drawn by the host's HUD pass).
        if let Some(hud) = found.weapon_hud.get(&row.name) {
            for (key, img) in
                ["reticle_center_image", "reticle_side_image", "overlay_image", "hud_icon_image"].iter().zip(hud)
            {
                if !img.is_empty() && !img.starts_with('$') {
                    row.strings.push(((*key).into(), img.to_ascii_lowercase()));
                    want_images.insert(img.to_ascii_lowercase());
                }
            }
        }
        if let Some(map) = found.weapon_notetracks.get(&row.name) {
            let text: Vec<String> = map.iter().map(|(k, v)| format!("{k}={v}")).collect();
            row.strings.push(("notetrack_sounds".into(), text.join(",")));
        }
        for m in [&wm.gun, &hands] {
            if models.contains_key(&m.to_ascii_lowercase()) {
                want_models.insert(m.to_ascii_lowercase());
            } else if !summary.missing_models.contains(m) {
                summary.missing_models.push(m.clone());
            }
        }
        if let Some(anims) = found.weapon_anims.get(&row.name) {
            for (slot, a) in anims.iter().enumerate() {
                let key = a.to_ascii_lowercase();
                if !a.is_empty() && found.anims.contains_key(&key) {
                    row.strings.push((format!("xanim.{slot}"), key.clone()));
                    want_anims.insert(key);
                }
            }
        }
    }
    // The killstreaks' turret reticles: the Chopper Gunner's gun (remote_chopper_overlay_hd's
    // VehicleWeaponReticle, owner draw 190, draws its centre reticle at reticle_center_size).
    for row in table.weapons.iter_mut().filter(|r| KILLSTREAK_TURRETS.contains(&r.name.as_str())) {
        let Some(img) = found.weapon_hud.get(&row.name).map(|h| h[0].to_ascii_lowercase()) else { continue };
        if img.is_empty() || img.starts_with('$') {
            continue;
        }
        row.strings.retain(|(k, _)| k != "reticle_center_image");
        row.strings.push(("reticle_center_image".into(), img.clone()));
        want_images.insert(img);
    }
    let dir = out.join("viewmodels");
    for sub in ["models", "anims", "images"] {
        std::fs::create_dir_all(dir.join(sub)).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let write = |path: std::path::PathBuf, bytes: &[u8], summary: &mut ViewmodelSummary| -> Result<(), String> {
        summary.bytes += bytes.len() as u64;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    for name in &want_models {
        let m = models[name];
        for s in &m.surfaces {
            for img in [&s.material.color_map, &s.material.normal_map] {
                if !img.is_empty() && !img.starts_with('$') {
                    want_images.insert(img.to_ascii_lowercase());
                }
            }
        }
        write(dir.join("models").join(format!("{}.cvm", vm::file_stem(name))), &vm::write_model(m), &mut summary)?;
        summary.models += 1;
    }
    for name in &want_anims {
        write(
            dir.join("anims").join(format!("{}.cva", vm::file_stem(name))),
            &vm::write_anim(&found.anims[name]),
            &mut summary,
        )?;
        summary.anims += 1;
    }
    let images = iwd::Images::open(mw2)?;
    for name in &want_images {
        let dds = match images.read(name)? {
            Some(iwi) => iwd::iwi_to_dds(&iwi).map_err(|e| format!("{name}: {e}")),
            None => Err(format!("{name}: not in main/iw_*.iwd")),
        };
        match dds {
            Ok(d) => {
                write(dir.join("images").join(format!("{}.dds", vm::file_stem(name))), &d, &mut summary)?;
                summary.images += 1;
            }
            Err(e) => summary.missing_images.push(e),
        }
    }
    match import_body(mw2, &found, out) {
        Ok((model, zone, anims, bytes)) => {
            summary.body_model = model;
            summary.body_zone = zone;
            summary.body_anims = anims;
            summary.bytes += bytes;
        }
        Err(e) => summary.body_error = Some(e),
    }
    Ok(summary)
}

/// Third person (F4b): writes `out/body/` with MW2's character animation
/// tree and player script (`multiplayer.atr`, `playeranim.script`), every
/// animation the compiled tree uses (`anims/*.cva`) and one soldier's
/// skeleton (`skeleton.cvm`, surfaces left out: the GTA ped is what is seen).
/// Returns the model, the zone it came from, the animation count and bytes.
#[cfg(feature = "import")]
fn import_body(
    mw2: &std::path::Path,
    found: &viewmodels::Found,
    out: &std::path::Path,
) -> Result<(String, String, usize, u64), String> {
    let atr =
        found.raw.get(playeranim::MULTIPLAYER_ANIMTREE_PATH).ok_or("no animtrees/multiplayer.atr in common_mp.ff")?;
    let script = found.raw.get(playeranim::PLAYERANIM_SCRIPT_PATH).ok_or("no mp/playeranim.script in common_mp.ff")?;
    let (tree, _) = playeranim::compile(atr, script)?;
    // The skeleton: the first map zone that has the soldier (else any body).
    let zones = mw2.join("zone").join("english");
    let mut candidates: Vec<std::path::PathBuf> = BODY_ZONES.iter().map(|z| zones.join(z)).collect();
    if let Ok(dir) = std::fs::read_dir(&zones) {
        let mut more: Vec<_> = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("mp_") && n.ends_with(".ff") && !n.ends_with("_load.ff"))
            })
            .collect();
        more.sort();
        candidates.extend(more);
    }
    let mut skeleton = None;
    for path in candidates.iter().filter(|p| p.is_file()) {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(image) = zone::open(&bytes) else { continue };
        let Ok(f) = viewmodels::capture(&image) else { continue };
        let mut bodies: Vec<vm::Model> = f.models.into_iter().filter(|m| viewmodels::is_body_model(&m.name)).collect();
        if bodies.is_empty() {
            continue;
        }
        let i = bodies.iter().position(|m| m.name == BODY_MODEL).unwrap_or(0);
        let mut m = bodies.swap_remove(i);
        m.surfaces.clear();
        let zone_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();
        skeleton = Some((m, zone_name));
        break;
    }
    let (model, zone_name) = skeleton.ok_or("no mp_body_* soldier in zone/english/mp_*.ff")?;
    let dir = out.join("body");
    std::fs::create_dir_all(dir.join("anims")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut bytes = 0u64;
    let mut write = |path: std::path::PathBuf, data: &[u8]| -> Result<(), String> {
        bytes += data.len() as u64;
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))
    };
    write(dir.join("multiplayer.atr"), atr)?;
    write(dir.join("playeranim.script"), script)?;
    write(dir.join("skeleton.cvm"), &vm::write_model(&model))?;
    let mut anims = 0;
    for node in tree.nodes().iter().filter(|n| n.child_count == 0) {
        if let Some(a) = found.anims.get(&node.name) {
            write(dir.join("anims").join(format!("{}.cva", vm::file_stem(&node.name))), &vm::write_anim(a))?;
            anims += 1;
        }
    }
    Ok((model.name, zone_name, anims, bytes))
}

/// What the HUD import wrote.
#[derive(Debug, Default)]
pub struct HudSummary {
    pub fonts: usize,
    pub items: usize,
    pub strings: usize,
    pub images: usize,
    /// F5c: the killstreaks read from mp/killstreakTable.csv.
    pub killstreaks: usize,
    pub missing: Vec<String>,
}

/// F5c: MW2's killstreak overlay menus (common_mp.ff), the Javelin's CLU view
/// (`javelin_overlay_hd`, shown while the Javelin is fully aimed: hud `ads_javelin`) and the
/// score bar (`scorebar_hd`: CODV's kill counter in its art).
pub const KILLSTREAK_MENUS: [&str; 6] = [
    "missilecam_hud_hd",
    "ac130_hud_hd",
    "remote_chopper_overlay_hd",
    "targetmap_fullscreen",
    "javelin_overlay_hd",
    "scorebar_hd",
];

/// F5c: the killstreaks' turret guns whose reticle the views draw (the Chopper Gunner's).
pub const KILLSTREAK_TURRETS: [&str; 1] = ["cobra_player_minigun_mp"];

/// F5c: materials the killstreak views draw outside those menus: the thermal FOF box on hostiles
/// (cg ThermalVisionFOFOverlay) and the static after the Predator hits (_remotemissile.gsc).
pub const KILLSTREAK_MATERIALS: [&str; 2] = ["hud_fofbox_hostile", "ac130_overlay_grain"];

/// MW2's damage blood (cg's blood overlay, the splatter_alt material): its colour image and the
/// mask that spreads it in from the edges as health drops. Images read as they are.
pub const BLOOD_IMAGES: [&str; 2] = ["blood_defocus_color", "blood_defocus_mask"];

/// F5c: strings the killstreaks show besides the menus' own (the nuke's countdown, crate captures).
pub const KILLSTREAK_STRINGS: [&str; 23] = [
    // The kill splashes (_rank.gsc / _awards splashNotify): title and "+N" line.
    "SPLASHES_HEADSHOT",
    "SPLASHES_HEADSHOT_DESC",
    "SPLASHES_DOUBLEKILL",
    "SPLASHES_DOUBLEKILL_DESC",
    "SPLASHES_TRIPLEKILL",
    "SPLASHES_TRIPLEKILL_DESC",
    "SPLASHES_MULTIKILL",
    "SPLASHES_MULTIKILL_DESC",
    "SPLASHES_FIRSTBLOOD",
    "SPLASHES_FIRSTBLOOD_DESC",
    "SPLASHES_LONGSHOT",
    "SPLASHES_LONGSHOT_DESC",
    // The kill counter: "Kills", "Deaths", "Streak", "Round Over".
    "CGAME_SB_KILLS",
    "CGAME_SB_DEATHS",
    "MENU_STREAK",
    "MP_ROUNDEND",
    "MP_WAR_AIRSTRIKE_CROSSHAIR",
    "MP_TACTICAL_NUKE",
    "MP_FRIENDLY_TACTICAL_NUKE",
    "MP_CAPTURING_CRATE",
    "AC130_HUD_THERMAL_WHOT",
    "AC130_HUD_THERMAL_BHOT",
    "SENTRY_PLACE",
];

/// The ground surfaces CODV tells MW2's footsteps apart by (the host maps GTA's materials to
/// them; the runtime's CV_OPT_SURFACE takes the index).
pub const MOVEMENT_SURFACES: &[&str] = &[
    "concrete", "asphalt", "dirt", "grass", "gravel", "metal", "wood", "water", "sand", "glass", "carpet", "mud",
    "snow", "rock", "plastic",
];

/// The player's movement sounds CODV can play: each surface's footsteps and landing, the hard
/// landing, the gear rattle of a mantle, a slide or a dive.
pub fn movement_sounds() -> Vec<String> {
    let mut out: Vec<String> = [
        "land_plr_damage",
        "gear_rattle_plr_land",
        "gear_rattle_plr_mantle",
        "gear_rattle_plr_prone",
        "gear_rattle_plr_run",
        "gear_rattle_plr_sprint",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for s in MOVEMENT_SURFACES {
        for kind in ["step_walk_plr", "step_run_plr", "step_sprint_plr", "step_prone_plr", "land_plr"] {
            out.push(format!("{kind}_{s}"));
        }
    }
    out
}

/// F5c: the killstreaks' own sounds (their scripts' playSound / playLocalSound aliases and the
/// killstreak weapons' fire sounds), besides the jingles and announcer lines from the table.
pub const KILLSTREAK_SOUNDS: &[&str] = &[
    // The launchers' lock-on (cg weapon lock: acquiring, then locked), the Stinger's and the Javelin's.
    "javelin_clu_aquiring_lock",
    "javelin_clu_lock",
    "stinger_clu_aquiring_lock",
    "stinger_clu_lock",
    // _remotemissile.gsc and remotemissile_projectile_mp
    "remote_missile_fire",
    "move_remotemissile_proj_loop1",
    "exp_remote_missile",
    "predator_drone_static",
    // _ac130.gsc and its three guns
    "ac130_105mm_fire",
    "ac130_40mm_fire",
    "ac130_25mm_fire",
    "ac130_105mm_reload",
    "ac130_40mm_reload",
    "ac130_weapon_switch",
    "ac130_gunready",
    "exp_ac130_105mm",
    "exp_ac130_40mm",
    "us_ac130_fco_goodkill",
    "us_ac130_fco_directhit",
    "us_ac130_fco_nice",
    "us_ac130_fco_iseepieces",
    // _helicopter.gsc: the Chopper Gunner's cobra_player_minigun_mp
    "weap_chopgunner_20mm_fire_plr",
    "littlebird_gatling_fire",
    // _airdrop.gsc
    "ammo_crate_use",
    "crate_impact",
    // _autosentry.gsc
    "sentry_gun_plant",
    "sentry_gun_beep",
    "sentry_minigun_fire",
    "sentry_explode",
    // _airstrike.gsc
    "exp_airstrike_bomb",
    // _weapons.gsc handleScavengerBagPickup
    "scavenger_pack_pickup",
    // _nuke.gsc
    "ui_mp_nukebomb_timer",
    "nuke_incoming",
    "nuke_explosion",
    "nuke_wave",
];

/// The HUD fonts CODV draws with (MW2's ui_get_font_handle names).
pub const HUD_FONTS: [&str; 7] = [
    "fonts/hudbigfont",
    "fonts/hudsmallfont",
    "fonts/bigfont",
    "fonts/smallfont",
    "fonts/normalfont",
    "fonts/objectivefont",
    "fonts/extrabigfont",
];

/// The HUD materials CODV draws (the weapon bar, its rounds, the hit marker,
/// damage direction and grenade indicator).
pub const HUD_MATERIALS: [&str; 12] = [
    "scavenger_pickup",
    "hud_weaponbar",
    "ammo_counter_bullet_mp",
    "ammo_counter_riflebullet_mp",
    "ammo_counter_shotgunshell_mp",
    "ammo_counter_rocket_mp",
    "ammo_counter_beltbullet_mp",
    "damage_feedback",
    "hit_direction",
    "hit_direction_stun",
    "hud_grenadeicon",
    "hud_grenadepointer",
];

/// The weapon bar's owner draws CODV paints (stock, frag icon, clip rounds,
/// weapon name, low ammo warning, compass ring); owner draw 0 items with an
/// image (the bar itself, the compass's backing).
#[cfg(feature = "import")]
const HUD_OWNER_DRAWS: [i32; 6] = [119, 103, 117, 81, 120, 166];

/// F5b: reads MW2's HUD (the weapon bar menu from `common_mp.ff` /
/// `patch_mp.ff`, the fonts and strings from `localized_code_post_gfx_mp.ff`)
/// into `out/hud/hud.json`, and the font atlas and HUD material images into
/// `out/viewmodels/images/`. The weapons' display names (localize keys in
/// `table`) are kept as strings. Call after `import_viewmodels`.
#[cfg(feature = "import")]
pub fn import_hud(mw2: &std::path::Path, table: &table::Table, out: &std::path::Path) -> Result<HudSummary, String> {
    use std::collections::{BTreeSet, HashMap};
    let zones = mw2.join("zone").join("english");
    let mut fonts: HashMap<String, hud::Font> = HashMap::new();
    let mut strings: HashMap<String, String> = HashMap::new();
    let mut materials: HashMap<String, String> = HashMap::new();
    // The weapon bar's items, their visibility tests and the menu's origin.
    type Bar = (Vec<hud::Item>, Vec<Option<hudfile::Test>>, [f32; 2]);
    let mut bar: Option<Bar> = None;
    let mut zone_images = HashMap::new();
    // F5c: the string tables (killstreaks, splashes), the earn splash menu and the reward slot.
    let mut tables: HashMap<String, killstreaks::Table> = HashMap::new();
    let mut atlases: HashMap<String, [u8; 2]> = HashMap::new();
    let mut splash_items: Vec<hudfile::Item> = Vec::new();
    let mut dpad_items: Vec<hudfile::Item> = Vec::new();
    let mut overlay_menus: Vec<(String, Vec<hudfile::Item>)> = Vec::new();
    let plain_item = |it: &hud::Item, origin: [f32; 2]| hudfile::Item {
        name: it.name.clone(),
        owner_draw: it.owner_draw,
        rect: [it.rect[0] + origin[0], it.rect[1] + origin[1], it.rect[2], it.rect[3]],
        align: [i32::from(it.align[0]), i32::from(it.align[1])],
        color: it.fore_color,
        text_scale: it.text_scale,
        font: it.font,
        text_align: it.text_align,
        text_align_xy: it.text_align_xy,
        text_style: it.text_style,
        style: it.style,
        background: it.background.clone(),
        visible: None,
        text: it.text.clone(),
    };
    for name in [
        "common_mp.ff",
        "patch_mp.ff",
        "code_post_gfx_mp.ff",
        "localized_common_mp.ff",
        "localized_code_post_gfx_mp.ff",
    ] {
        let Ok(bytes) = std::fs::read(zones.join(name)) else { continue };
        let image = zone::open(&bytes).map_err(|e| format!("{name}: {e}"))?;
        let f = hud::capture(&image).map_err(|e| format!("{name}: {e}"))?;
        for font in f.fonts {
            fonts.insert(font.name.clone(), font);
        }
        strings.extend(f.strings);
        zone_images.extend(f.images);
        materials.extend(f.materials.into_iter().filter(|(_, v)| !v.is_empty()));
        tables.extend(f.tables);
        atlases.extend(f.atlas);
        let origin_of = |menu: &str| f.menus.iter().find(|(m, _)| m == menu).map(|&(_, r)| [r[0], r[1]]);
        // MW2's killstreak splash (_hud_message.gsc opens the splashTable row's menu, "killstreak"):
        // its title, description and icon (the "_ss" ones are splitscreen's).
        if let Some(origin) = origin_of("killstreak") {
            splash_items = f
                .items
                .iter()
                .filter(|i| i.menu == "killstreak" && !i.name.ends_with("_ss"))
                .map(|i| plain_item(i, origin))
                .collect();
        }
        // The killstreaks' overlays (the Predator's camera, the AC-130, the Chopper Gunner, the airstrike's map).
        for name in KILLSTREAK_MENUS {
            if let Some(origin) = origin_of(name) {
                let items = f.items.iter().filter(|i| i.menu == name).map(|i| plain_item(i, origin)).collect();
                overlay_menus.retain(|(n, _)| n != name);
                overlay_menus.push((name.to_owned(), items));
            }
        }
        // dpad_hd's action slot 4 (owner draw 174: the held killstreak's icon) and its key label after it.
        if let Some(origin) = origin_of("dpad_hd") {
            let items: Vec<&hud::Item> = f.items.iter().filter(|i| i.menu == "dpad_hd").collect();
            if let Some(at) = items.iter().position(|i| i.owner_draw == 174) {
                dpad_items = vec![plain_item(items[at], origin)];
                if let Some(label) = items.get(at + 1).filter(|i| i.owner_draw == 0 && i.rect == items[at].rect) {
                    dpad_items.push(plain_item(label, origin));
                }
            }
        }
        let menu = "weaponbar_hd";
        if let Some(&(_, rect)) = f.menus.iter().find(|(m, _)| m == menu) {
            let items: Vec<hud::Item> = f.items.iter().filter(|i| i.menu == menu).cloned().collect();
            // Each item's visibility test follows its layout record.
            let mut tests = vec![None; items.len()];
            let mut at: Option<usize> = None;
            for (m, kind, dump) in &f.exprs {
                if m != menu {
                    continue;
                }
                match kind.as_str() {
                    "ITEM" => at = Some(at.map_or(0, |i| i + 1)),
                    "visible" => {
                        if let Some(slot) = at.and_then(|i| tests.get_mut(i)) {
                            *slot = hudfile::parse_test(dump);
                        }
                    }
                    _ => {}
                }
            }
            bar = Some((items, tests, [rect[0], rect[1]]));
        }
    }
    let (items, tests, origin) = bar.ok_or("no weaponbar_hd menu in zone/english/common_mp.ff")?;
    let mut want_images = BTreeSet::new();
    let mut hud_items = Vec::new();
    for (it, test) in items.into_iter().zip(tests) {
        let bar_bg = it.owner_draw == 0 && !it.background.is_empty();
        if !HUD_OWNER_DRAWS.contains(&it.owner_draw) && !bar_bg {
            continue;
        }
        if !it.background.is_empty() {
            want_images.insert(it.background.clone());
        }
        hud_items.push(hudfile::Item {
            name: it.name,
            owner_draw: it.owner_draw,
            rect: [it.rect[0] + origin[0], it.rect[1] + origin[1], it.rect[2], it.rect[3]],
            align: [i32::from(it.align[0]), i32::from(it.align[1])],
            color: it.fore_color,
            text_scale: it.text_scale,
            font: it.font,
            text_align: it.text_align,
            text_align_xy: it.text_align_xy,
            text_style: it.text_style,
            style: it.style,
            background: it.background,
            visible: test,
            text: String::new(),
        });
    }
    let mut summary = HudSummary::default();
    let mut hud_fonts = Vec::new();
    for name in HUD_FONTS {
        match fonts.remove(name) {
            Some(f) if !f.image.is_empty() => {
                want_images.insert(f.image.clone());
                hud_fonts.push(hudfile::Font {
                    name: f.name,
                    pixel_height: f.pixel_height,
                    image: f.image,
                    glyphs: f
                        .glyphs
                        .iter()
                        .map(|g| hudfile::Glyph {
                            letter: g.letter,
                            x0: g.x0,
                            y0: g.y0,
                            dx: g.dx,
                            width: g.width,
                            height: g.height,
                            st: g.st,
                        })
                        .collect(),
                });
            }
            _ => summary.missing.push(format!("font {name}")),
        }
    }
    let mut images = Vec::new();
    for m in HUD_MATERIALS {
        match materials.get(m) {
            Some(img) => {
                want_images.insert(img.clone());
                images.push((m.to_owned(), img.clone()));
            }
            None => summary.missing.push(format!("material {m}")),
        }
    }
    // F5c: the killstreaks and their icons.
    let streaks = match (tables.get("mp/killstreaktable.csv"), tables.get("mp/splashtable.csv")) {
        (Some(ks), splash) => killstreaks::from_tables(ks, splash.unwrap_or(&(0, Vec::new()))),
        (None, _) => {
            summary.missing.push("mp/killstreakTable.csv (code_post_gfx_mp.ff)".into());
            Vec::new()
        }
    };
    for k in &streaks {
        for m in [&k.icon, &k.dpad_icon] {
            if m.is_empty() || images.iter().any(|(n, _)| n == m) {
                continue;
            }
            // MW2's table names the sentry's dpad icon "dpad_killstreak_sentry"; the material is "..._gun".
            match materials.get(m.as_str()).or_else(|| materials.get(&format!("{m}_gun"))) {
                Some(img) => {
                    want_images.insert(img.clone());
                    images.push((m.clone(), img.clone()));
                }
                None => {
                    let m = format!("material {m}");
                    if !summary.missing.contains(&m) {
                        summary.missing.push(m);
                    }
                }
            }
        }
    }
    // The overlays' images and texts, and the FOF box the thermal views put on hostiles.
    for (name, items) in &overlay_menus {
        for it in items {
            // "$white" / "white" / "$black" are solid colours (the runtime fills them).
            if !it.background.is_empty() && !it.background.starts_with('$') && it.background != "white" {
                want_images.insert(it.background.clone());
            }
        }
        if items.is_empty() {
            summary.missing.push(format!("menu {name}"));
        }
    }
    for m in KILLSTREAK_MATERIALS {
        match materials.get(m) {
            Some(img) => {
                want_images.insert(img.clone());
                images.push((m.to_owned(), img.clone()));
            }
            None => summary.missing.push(format!("material {m}")),
        }
    }
    for img in BLOOD_IMAGES {
        want_images.insert(img.to_owned());
        images.push((img.to_owned(), img.to_owned()));
    }
    for name in KILLSTREAK_MENUS {
        if !overlay_menus.iter().any(|(n, _)| n == name) {
            summary.missing.push(format!("menu {name}"));
        }
    }
    if !streaks.is_empty() && splash_items.is_empty() {
        summary.missing.push("menu killstreak (the earn splash)".into());
    }
    // The strings the HUD shows: the low ammo warnings and the weapons' names.
    let mut keys: BTreeSet<String> =
        ["PLATFORM_RELOAD", "PLATFORM_LOW_AMMO_NO_RELOAD", "WEAPON_NO_AMMO"].iter().map(|k| (*k).to_owned()).collect();
    keys.extend(table.weapons.iter().map(|w| w.display_name.to_ascii_uppercase()).filter(|k| !k.is_empty()));
    for k in &streaks {
        keys.extend([k.title.clone(), k.earned.clone(), k.splash_title.clone()].into_iter().filter(|k| !k.is_empty()));
    }
    for (_, items) in &overlay_menus {
        keys.extend(items.iter().filter_map(|i| i.text.strip_prefix('@')).map(str::to_ascii_uppercase));
    }
    keys.extend(KILLSTREAK_STRINGS.iter().map(|k| (*k).to_owned()));
    // The crates' "Press and hold [use] for ..." hints.
    keys.extend(strings.keys().filter(|k| k.starts_with("MP_") && k.ends_with("_PICKUP")).cloned());
    let hud_strings: Vec<(String, String)> =
        keys.into_iter().filter_map(|k| strings.get(&k).map(|v| (k.clone(), v.clone()))).collect();
    let dir = out.join("viewmodels").join("images");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let iwds = iwd::Images::open(mw2)?;
    for name in &want_images {
        let dds = match (iwds.read(name)?, zone_images.get(name)) {
            (Some(iwi), _) => iwd::iwi_to_dds(&iwi).map_err(|e| format!("{name}: {e}")),
            (None, Some((format, w, h, levels, px))) => {
                iwd::d3d_to_dds(*format, u32::from(*w), u32::from(*h), u32::from(*levels), px)
                    .map_err(|e| format!("{name}: {e}"))
            }
            (None, None) => Err(format!("{name}: not in main/iw_*.iwd")),
        };
        match dds {
            Ok(d) => {
                let path = dir.join(format!("{}.dds", vm::file_stem(name)));
                std::fs::write(&path, d).map_err(|e| format!("{}: {e}", path.display()))?;
                summary.images += 1;
            }
            Err(e) => summary.missing.push(e),
        }
    }
    summary.killstreaks = streaks.len();
    let mut atlas: Vec<(String, [u8; 2])> = Vec::new();
    for k in &streaks {
        for m in [&k.icon, &k.dpad_icon] {
            if let Some(a) = atlases.get(m.as_str()).or_else(|| atlases.get(&format!("{m}_gun")))
                && !atlas.iter().any(|(n, _)| n == m)
            {
                atlas.push((m.clone(), *a));
            }
        }
    }
    let file = hudfile::Hud {
        fonts: hud_fonts,
        strings: hud_strings,
        items: hud_items,
        images,
        killstreaks: streaks,
        splash_items,
        dpad_items,
        atlas,
        menus: overlay_menus,
    };
    summary.fonts = file.fonts.len();
    summary.items = file.items.len();
    summary.strings = file.strings.len();
    let hud_dir = out.join("hud");
    std::fs::create_dir_all(&hud_dir).map_err(|e| format!("{}: {e}", hud_dir.display()))?;
    std::fs::write(hud_dir.join("hud.json"), hudfile::to_json(&file))
        .map_err(|e| format!("{}: {e}", hud_dir.display()))?;
    Ok(summary)
}

/// The sound fields of a weapon row the player hears (F5a).
pub const PLAYER_SOUND_FIELDS: [&str; 11] = [
    "fire_sound_player",
    "fire_last_sound_player",
    "empty_fire_sound_player",
    "reload_sound_player",
    "reload_empty_sound_player",
    "rechamber_sound_player",
    "raise_sound_player",
    "putaway_sound_player",
    "melee_swipe_sound_player",
    "melee_hit_sound",
    "first_raise_sound_player",
];

/// The WAV file a loaded sound is written to (its name, which already ends
/// in `.wav` in MW2's zones, made a safe file name).
#[cfg(feature = "import")]
fn sound_file_name(loaded: &str) -> String {
    let stem = vm::file_stem(loaded);
    if stem.to_ascii_lowercase().ends_with(".wav") { stem } else { format!("{stem}.wav") }
}

/// A RIFF WAVE file of 8 or 16-bit PCM (what the runtime's mixer reads).
#[cfg(feature = "import")]
fn is_pcm_wav(b: &[u8]) -> bool {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return false;
    }
    let mut at = 12;
    while at + 8 <= b.len() {
        let len = u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()) as usize;
        if &b[at..at + 4] == b"fmt " {
            return b.len() >= at + 10 && u16::from_le_bytes([b[at + 8], b[at + 9]]) == 1;
        }
        at += 8 + len + (len & 1);
    }
    false
}

/// The killstreaks' sound aliases (code_post_gfx_mp.ff's mp/killstreakTable.csv); none without it.
#[cfg(feature = "import")]
fn killstreak_sound_aliases(zones: &std::path::Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(zones.join("code_post_gfx_mp.ff")) else { return Vec::new() };
    let Ok(image) = zone::open(&bytes) else { return Vec::new() };
    let Ok(f) = hud::capture(&image) else { return Vec::new() };
    f.tables
        .get("mp/killstreaktable.csv")
        .map(|t| killstreaks::sound_aliases(&killstreaks::from_tables(t, &(0, Vec::new()))))
        .unwrap_or_default()
}

/// What the sound import wrote.
#[derive(Debug, Default)]
pub struct SoundSummary {
    pub aliases: usize,
    pub files: usize,
    pub bytes: u64,
    pub missing: Vec<String>,
}

/// Reads the sounds the holdable guns of `table` play for the player (their
/// sound fields and animation notetracks, with secondary and chained aliases)
/// from the player's MW2 (`zone/english/localized_common_mp.ff`,
/// `common_mp.ff`, `patch_mp.ff`) into `out/sounds/`: `aliases.json` (each
/// alias's variants: file, volume and pitch ranges, chance, delay, secondary,
/// chain) and `files/*.wav`. Call after `import_viewmodels` (it records the
/// notetrack maps).
#[cfg(feature = "import")]
pub fn import_sounds(
    mw2: &std::path::Path,
    table: &table::Table,
    out: &std::path::Path,
) -> Result<SoundSummary, String> {
    use codv_core::json::{self, Value};
    use std::collections::{BTreeMap, BTreeSet};
    let zones = mw2.join("zone").join("english");
    let mut found = sounds::Found::default();
    for name in ["localized_common_mp.ff", "common_mp.ff", "patch_mp.ff"] {
        let Ok(bytes) = std::fs::read(zones.join(name)) else { continue };
        let image = zone::open(&bytes).map_err(|e| format!("{name}: {e}"))?;
        let f = sounds::capture(&image).map_err(|e| format!("{name}: {e}"))?;
        found.aliases.extend(f.aliases);
        found.loaded.extend(f.loaded);
    }
    if found.aliases.is_empty() {
        return Err("no sound aliases in zone/english/localized_common_mp.ff".into());
    }
    // MW2's hit marker sound (_damagefeedback.gsc), with the guns' sounds.
    let mut want: Vec<String> = vec!["mp_hit_alert".to_owned()];
    // F5c: the killstreaks' jingles and announcer lines (mp/killstreakTable.csv).
    let streak_sounds = killstreak_sound_aliases(&zones);
    want.extend(streak_sounds.iter().cloned());
    want.extend(KILLSTREAK_SOUNDS.iter().map(|s| (*s).to_owned()));
    want.extend(movement_sounds());
    for row in table.weapons.iter().filter(|r| is_holdable_gun(r)) {
        for f in PLAYER_SOUND_FIELDS {
            want.push(row.s(f).to_ascii_lowercase());
        }
        for pair in row.s("notetrack_sounds").split(',') {
            if let Some((_, v)) = pair.split_once('=') {
                want.push(v.to_owned());
            }
        }
    }
    let mut chosen: BTreeMap<String, Vec<sounds::Alias>> = BTreeMap::new();
    let mut summary = SoundSummary::default();
    while let Some(name) = want.pop() {
        if name.is_empty() || chosen.contains_key(&name) {
            continue;
        }
        match found.aliases.get(&name) {
            Some(rows) => {
                for r in rows {
                    want.push(r.secondary.clone());
                    want.push(r.chain.clone());
                }
                chosen.insert(name, rows.clone());
            }
            None => {
                if !summary.missing.contains(&name) {
                    summary.missing.push(name);
                }
            }
        }
    }
    let dir = out.join("sounds");
    std::fs::create_dir_all(dir.join("files")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files = BTreeSet::new();
    // Streamed sounds (the announcer, the killstreak jingles) are WAV files in main/*.iwd under sound/.
    let streamed = if chosen.values().flatten().any(|r| r.loaded.is_empty() && !r.streamed.is_empty()) {
        iwd::Images::open_prefixed(mw2, "sound/", "").ok()
    } else {
        None
    };
    let mut streamed_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut aliases = Vec::new();
    for (name, rows) in &chosen {
        let variants: Vec<Value> = rows
            .iter()
            .map(|r| {
                let file = if !r.loaded.is_empty() && found.loaded.get(&r.loaded).and_then(sounds::wav).is_some() {
                    files.insert(r.loaded.clone());
                    sound_file_name(&r.loaded)
                } else if !r.streamed.is_empty()
                    && let Some(bytes) = streamed
                        .as_ref()
                        .and_then(|z| z.read(&r.streamed.replace('\\', "/")).ok().flatten())
                        .filter(|b| is_pcm_wav(b))
                {
                    let file = sound_file_name(&r.streamed);
                    streamed_files.insert(file.clone(), bytes);
                    file
                } else {
                    String::new()
                };
                let num = |v: f32| Value::Num(f64::from(v));
                json::obj([
                    ("file", Value::Str(file)),
                    ("vol", Value::Arr(vec![num(r.vol_min), num(r.vol_max)])),
                    ("pitch", Value::Arr(vec![num(r.pitch_min), num(r.pitch_max)])),
                    ("chance", num(r.probability)),
                    ("delay_ms", Value::Num(f64::from(r.start_delay_ms))),
                    ("secondary", Value::Str(r.secondary.clone())),
                    ("chain", Value::Str(r.chain.clone())),
                    ("flags", Value::Num(f64::from(r.flags))),
                ])
            })
            .collect();
        aliases.push((name.clone(), Value::Arr(variants)));
    }
    summary.aliases = aliases.len();
    for (file, bytes) in &streamed_files {
        let path = dir.join("files").join(file);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        summary.bytes += bytes.len() as u64;
        summary.files += 1;
    }
    for name in &streak_sounds {
        let playable = chosen.get(name).is_some_and(|rows| {
            rows.iter().any(|r| {
                (!r.loaded.is_empty() && files.contains(&r.loaded))
                    || streamed_files.contains_key(&sound_file_name(&r.streamed))
            })
        });
        if !playable && !summary.missing.iter().any(|m| m.starts_with(name.as_str())) {
            summary.missing.push(format!("{name} (its file is not in main/*.iwd)"));
        }
    }
    for f in &files {
        if let Some(w) = found.loaded.get(f).and_then(sounds::wav) {
            let path = dir.join("files").join(sound_file_name(f));
            std::fs::write(&path, &w).map_err(|e| format!("{}: {e}", path.display()))?;
            summary.bytes += w.len() as u64;
            summary.files += 1;
        }
    }
    let doc = json::obj([
        ("format", Value::Str("codv-mw2-sounds".into())),
        ("version", Value::Num(1.0)),
        ("aliases", Value::Obj(aliases)),
    ]);
    let text = json::to_string_pretty(&doc);
    summary.bytes += text.len() as u64;
    std::fs::write(dir.join("aliases.json"), text).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(summary)
}
