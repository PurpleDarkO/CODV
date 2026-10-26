//! Every weapon asset of a zone, as `fastfile_iw4` decodes it, as rows of
//! CODV's table; plus MW2's global hit-location table when the zone has it.
use crate::table::{HITLOCS, Row};
use fastfile_iw4::{AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, WeaponGeometry, ZoneStream};

/// What one zone held.
pub struct Found {
    pub weapons: Vec<Row>,
    pub location: Option<[f32; HITLOCS]>,
}

struct Sink {
    found: Found,
    walked: usize,
    /// Model names by shared weapon definition (variants alias the first).
    defs: std::collections::HashMap<Ptr, Vec<(String, String)>>,
    /// XModel names by asset slot, for weapons naming models loaded earlier.
    xmodel_names: std::collections::HashMap<Ptr, Ptr>,
}

fn text(s: &ZoneStream<'_>, p: Option<Ptr>) -> String {
    p.and_then(|p| s.cstr(p).ok()).unwrap_or("").to_owned()
}

/// The retail numbers CODV uses, under their `WeaponGeometry` names.
fn numbers(g: &WeaponGeometry) -> Vec<(String, f64)> {
    [
        ("fire_time_ms", g.fire_time_ms as f64),
        ("fire_delay_ms", g.fire_delay_ms as f64),
        ("raise_time_ms", g.raise_time_ms as f64),
        ("drop_time_ms", g.drop_time_ms as f64),
        ("alternate_raise_time_ms", g.alternate_raise_time_ms as f64),
        ("alternate_drop_time_ms", g.alternate_drop_time_ms as f64),
        ("first_raise_time_ms", g.first_raise_time_ms as f64),
        ("hold_fire_time_ms", g.hold_fire_time_ms as f64),
        ("reload_time_ms", g.reload_time_ms as f64),
        ("reload_empty_time_ms", g.reload_empty_time_ms as f64),
        ("reload_add_time_ms", g.reload_add_time_ms as f64),
        ("reload_start_time_ms", g.reload_start_time_ms as f64),
        ("reload_start_add_time_ms", g.reload_start_add_time_ms as f64),
        ("reload_end_time_ms", g.reload_end_time_ms as f64),
        ("reload_ammo_add", g.reload_ammo_add as f64),
        ("reload_start_add", g.reload_start_add as f64),
        ("clip_size", g.clip_size as f64),
        ("start_ammo", g.start_ammo as f64),
        ("max_ammo", g.max_ammo as f64),
        ("ammo_index", g.ammo_index as f64),
        ("clip_index", g.clip_index as f64),
        ("ammo_counter_clip", g.ammo_counter_clip as f64),
        ("low_ammo_warning_threshold", g.low_ammo_warning_threshold as f64),
        ("hud_icon_ratio", g.hud_icon_ratio as f64),
        ("fire_type", g.fire_type as f64),
        ("weap_type", g.weap_type as f64),
        ("weap_class", g.weap_class as f64),
        ("player_anim_type", g.player_anim_type as f64),
        ("inventory_type", g.inventory_type as f64),
        ("offhand_class", g.offhand_class as f64),
        ("impact_type", g.impact_type as f64),
        ("shots_per_fire", g.shots_per_fire as f64),
        ("rechamber_time_ms", g.rechamber_time_ms as f64),
        ("rechamber_bolt_time_ms", g.rechamber_bolt_time_ms as f64),
        ("rechamber_bolt_delay_ms", g.rechamber_bolt_delay_ms as f64),
        ("sprint_raise_time_ms", g.sprint_raise_time_ms as f64),
        ("sprint_loop_time_ms", g.sprint_loop_time_ms as f64),
        ("sprint_drop_time_ms", g.sprint_drop_time_ms as f64),
        ("stunned_start_time_ms", g.stunned_start_time_ms as f64),
        ("stunned_end_time_ms", g.stunned_end_time_ms as f64),
        ("damage", g.damage as f64),
        ("min_damage", g.min_damage as f64),
        ("min_player_damage", g.min_player_damage as f64),
        ("max_damage_range", g.max_damage_range as f64),
        ("min_damage_range", g.min_damage_range as f64),
        ("melee_damage", g.melee_damage as f64),
        ("melee_time_ms", g.melee_time_ms as f64),
        ("melee_delay_ms", g.melee_delay_ms as f64),
        ("melee_charge_time_ms", g.melee_charge_time_ms as f64),
        ("melee_charge_delay_ms", g.melee_charge_delay_ms as f64),
        ("quick_raise_time_ms", g.quick_raise_time_ms as f64),
        ("quick_drop_time_ms", g.quick_drop_time_ms as f64),
        ("ads_reload_trans_time_ms", g.ads_reload_trans_time_ms as f64),
        ("ads_in_rate", g.ads_in_rate as f64),
        ("ads_out_rate", g.ads_out_rate as f64),
        ("ads_spread", g.ads_spread as f64),
        ("ads_zoom_fov", g.ads_zoom_fov as f64),
        ("ads_zoom_in_frac", g.ads_zoom_in_frac as f64),
        ("ads_zoom_out_frac", g.ads_zoom_out_frac as f64),
        // The hip crosshair and the ADS overlay (MW2's reticle and scope, hud_iw4).
        ("reticle_center_size", g.reticle_center_size as f64),
        ("reticle_side_size", g.i_reticle_side_size as f64),
        ("reticle_min_ofs", g.i_reticle_min_ofs as f64),
        ("hip_reticle_side_pos", g.hip_reticle_side_pos as f64),
        ("ads_crosshair_in_frac", g.ads_crosshair_in_frac as f64),
        ("ads_crosshair_out_frac", g.ads_crosshair_out_frac as f64),
        ("ads_overlay_width", g.ads_overlay_width as f64),
        ("ads_overlay_height", g.ads_overlay_height as f64),
        ("overlay_reticle", g.overlay_reticle as f64),
        // 1: the Javelin's CLU (WEAPOVERLAYINTERFACE_JAVELIN, hud `ads_javelin`).
        ("overlay_interface", g.overlay_interface as f64),
        ("hip_spread_stand_min", g.hip_spread_stand_min as f64),
        ("hip_spread_ducked_min", g.hip_spread_ducked_min as f64),
        ("hip_spread_prone_min", g.hip_spread_prone_min as f64),
        ("hip_spread_stand_max", g.hip_spread_stand_max as f64),
        ("hip_spread_ducked_max", g.hip_spread_ducked_max as f64),
        ("hip_spread_prone_max", g.hip_spread_prone_max as f64),
        ("hip_spread_decay_rate", g.hip_spread_decay_rate as f64),
        ("hip_spread_fire_add", g.hip_spread_fire_add as f64),
        ("hip_spread_turn_add", g.hip_spread_turn_add as f64),
        ("hip_spread_move_add", g.hip_spread_move_add as f64),
        ("hip_spread_ducked_decay", g.hip_spread_ducked_decay as f64),
        ("hip_spread_prone_decay", g.hip_spread_prone_decay as f64),
        ("move_speed_scale", g.move_speed_scale as f64),
        ("ads_move_speed_scale", g.ads_move_speed_scale as f64),
        ("sprint_duration_scale", g.sprint_duration_scale as f64),
        ("penetrate_type", g.penetrate_type as f64),
        ("penetrate_multiplier", g.penetrate_multiplier as f64),
        ("explosion_radius", g.explosion_radius as f64),
        ("explosion_inner_damage", g.explosion_inner_damage as f64),
        ("explosion_outer_damage", g.explosion_outer_damage as f64),
        ("projectile_speed", g.projectile_speed as f64),
        ("fuse_time_ms", g.fuse_time_ms as f64),
        ("auto_aim_range", g.auto_aim_range as f64),
        ("aim_assist_range", g.aim_assist_range as f64),
        ("aim_assist_range_ads", g.aim_assist_range_ads as f64),
        ("bolt_action", f64::from(u8::from(g.bolt_action))),
        ("segmented_reload", f64::from(u8::from(g.segmented_reload))),
        ("no_partial_reload", f64::from(u8::from(g.no_partial_reload))),
        ("aim_down_sight", f64::from(u8::from(g.aim_down_sight))),
        ("no_ads_when_mag_empty", f64::from(u8::from(g.no_ads_when_mag_empty))),
        ("inherits_perks", f64::from(u8::from(g.inherits_perks))),
        ("rechamber_while_ads", f64::from(u8::from(g.rechamber_while_ads))),
        ("ads_fire_only", f64::from(u8::from(g.ads_fire_only))),
        ("rifle_bullet", f64::from(u8::from(g.rifle_bullet))),
        ("clip_only", f64::from(u8::from(g.clip_only))),
        ("no_dual_wield", f64::from(u8::from(g.no_dual_wield))),
        ("thermal_scope", f64::from(u8::from(g.thermal_scope))),
        ("select_requires_ammo", g.select_requires_ammo.map_or(-1.0, |b| f64::from(u8::from(b)))),
        ("offhand_hold_is_cancelable", g.offhand_hold_is_cancelable.map_or(-1.0, |b| f64::from(u8::from(b)))),
        ("kick.f_ads_view_kick_center_speed", g.kick.f_ads_view_kick_center_speed as f64),
        ("kick.f_hip_view_kick_center_speed", g.kick.f_hip_view_kick_center_speed as f64),
        ("kick.gun_max_pitch", g.kick.gun_max_pitch as f64),
        ("kick.gun_max_yaw", g.kick.gun_max_yaw as f64),
        ("kick.ads_gun_kick_reduced_kick_bullets", g.kick.ads_gun_kick_reduced_kick_bullets as f64),
        ("kick.ads_gun_kick_reduced_kick_percent", g.kick.ads_gun_kick_reduced_kick_percent as f64),
        ("kick.ads_gun_kick_pitch_min", g.kick.ads_gun_kick_pitch_min as f64),
        ("kick.ads_gun_kick_pitch_max", g.kick.ads_gun_kick_pitch_max as f64),
        ("kick.ads_gun_kick_yaw_min", g.kick.ads_gun_kick_yaw_min as f64),
        ("kick.ads_gun_kick_yaw_max", g.kick.ads_gun_kick_yaw_max as f64),
        ("kick.ads_gun_kick_accel", g.kick.ads_gun_kick_accel as f64),
        ("kick.ads_gun_kick_speed_max", g.kick.ads_gun_kick_speed_max as f64),
        ("kick.ads_gun_kick_speed_decay", g.kick.ads_gun_kick_speed_decay as f64),
        ("kick.ads_gun_kick_static_decay", g.kick.ads_gun_kick_static_decay as f64),
        ("kick.ads_view_kick_pitch_min", g.kick.ads_view_kick_pitch_min as f64),
        ("kick.ads_view_kick_pitch_max", g.kick.ads_view_kick_pitch_max as f64),
        ("kick.ads_view_kick_yaw_min", g.kick.ads_view_kick_yaw_min as f64),
        ("kick.ads_view_kick_yaw_max", g.kick.ads_view_kick_yaw_max as f64),
        ("kick.hip_gun_kick_reduced_kick_bullets", g.kick.hip_gun_kick_reduced_kick_bullets as f64),
        ("kick.hip_gun_kick_reduced_kick_percent", g.kick.hip_gun_kick_reduced_kick_percent as f64),
        ("kick.hip_gun_kick_pitch_min", g.kick.hip_gun_kick_pitch_min as f64),
        ("kick.hip_gun_kick_pitch_max", g.kick.hip_gun_kick_pitch_max as f64),
        ("kick.hip_gun_kick_yaw_min", g.kick.hip_gun_kick_yaw_min as f64),
        ("kick.hip_gun_kick_yaw_max", g.kick.hip_gun_kick_yaw_max as f64),
        ("kick.hip_gun_kick_accel", g.kick.hip_gun_kick_accel as f64),
        ("kick.hip_gun_kick_speed_max", g.kick.hip_gun_kick_speed_max as f64),
        ("kick.hip_gun_kick_speed_decay", g.kick.hip_gun_kick_speed_decay as f64),
        ("kick.hip_gun_kick_static_decay", g.kick.hip_gun_kick_static_decay as f64),
        ("kick.hip_view_kick_pitch_min", g.kick.hip_view_kick_pitch_min as f64),
        ("kick.hip_view_kick_pitch_max", g.kick.hip_view_kick_pitch_max as f64),
        ("kick.hip_view_kick_yaw_min", g.kick.hip_view_kick_yaw_min as f64),
        ("kick.hip_view_kick_yaw_max", g.kick.hip_view_kick_yaw_max as f64),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .chain(placement_numbers(g))
    .collect()
}

/// The viewmodel placement facts (weapon_iw4::placement inputs and sway).
fn placement_numbers(g: &WeaponGeometry) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut v3 = |name: &str, v: [f32; 3]| {
        for (i, x) in v.iter().enumerate() {
            out.push((format!("{name}.{i}"), f64::from(*x)));
        }
    };
    let m = &g.movement;
    v3("ducked_ofs", g.ducked_ofs);
    v3("prone_ofs", g.prone_ofs);
    v3("movement.stand_move", m.stand_move);
    v3("movement.stand_rot", m.stand_rot);
    v3("movement.strafe_move", m.strafe_move);
    v3("movement.strafe_rot", m.strafe_rot);
    v3("movement.ducked_move", m.ducked_move);
    v3("movement.ducked_rot", m.ducked_rot);
    v3("movement.prone_move", m.prone_move);
    v3("movement.prone_rot", m.prone_rot);
    let i = &g.idle;
    let w = &g.sway;
    out.extend(
        [
            ("ads_aim_pitch", g.ads_aim_pitch),
            ("ads_bob_factor", g.ads_bob_factor),
            ("ads_view_bob_mult", g.ads_view_bob_mult),
            ("dual_wield_view_model_offset", g.dual_wield_view_model_offset),
            ("movement.pos_move_rate", m.pos_move_rate),
            ("movement.pos_prone_move_rate", m.pos_prone_move_rate),
            ("movement.stand_move_min_speed", m.stand_move_min_speed),
            ("movement.ducked_move_min_speed", m.ducked_move_min_speed),
            ("movement.prone_move_min_speed", m.prone_move_min_speed),
            ("movement.pos_rot_rate", m.pos_rot_rate),
            ("movement.pos_prone_rot_rate", m.pos_prone_rot_rate),
            ("idle.ads_idle_amount", i.ads_idle_amount),
            ("idle.hip_idle_amount", i.hip_idle_amount),
            ("idle.ads_idle_speed", i.ads_idle_speed),
            ("idle.hip_idle_speed", i.hip_idle_speed),
            ("idle.idle_crouch_factor", i.idle_crouch_factor),
            ("idle.idle_prone_factor", i.idle_prone_factor),
            ("sway.sway_max_angle", w.sway_max_angle),
            ("sway.sway_lerp_speed", w.sway_lerp_speed),
            ("sway.sway_pitch_scale", w.sway_pitch_scale),
            ("sway.sway_yaw_scale", w.sway_yaw_scale),
            ("sway.sway_horiz_scale", w.sway_horiz_scale),
            ("sway.sway_vert_scale", w.sway_vert_scale),
            ("sway.ads_sway_max_angle", w.ads_sway_max_angle),
            ("sway.ads_sway_lerp_speed", w.ads_sway_lerp_speed),
            ("sway.ads_sway_pitch_scale", w.ads_sway_pitch_scale),
            ("sway.ads_sway_yaw_scale", w.ads_sway_yaw_scale),
            ("sway.ads_sway_horiz_scale", w.ads_sway_horiz_scale),
            ("sway.ads_sway_vert_scale", w.ads_sway_vert_scale),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), f64::from(v))),
    );
    out.push(("night_vision_wear_time_ms".into(), f64::from(g.night_vision_wear_time)));
    out
}

/// MW2's hit locations, in weapon_iw4::HITLOC_NAMES order.
const HITLOC_NAMES: [&str; HITLOCS] = [
    "none",
    "helmet",
    "head",
    "neck",
    "torso_upper",
    "torso_lower",
    "right_arm_upper",
    "left_arm_upper",
    "right_arm_lower",
    "left_arm_lower",
    "right_hand",
    "left_hand",
    "right_leg_upper",
    "left_leg_upper",
    "right_leg_lower",
    "left_leg_lower",
    "right_foot",
    "left_foot",
    "gun",
    "shield",
];

/// `LOCDMGTABLE\key\value\...` (iw4L asset_game::lochit).
pub fn parse_lochit(text: &str) -> Option<[f32; HITLOCS]> {
    let rest = text.trim().trim_end_matches('\0').strip_prefix("LOCDMGTABLE")?;
    let mut table = [1.0f32; HITLOCS];
    let mut parts = rest.split('\\').filter(|p| !p.is_empty());
    while let Some(key) = parts.next() {
        let value: f32 = parts.next()?.trim().parse().ok()?;
        let slot = HITLOC_NAMES.iter().position(|n| *n == key)?;
        table[slot] = value;
    }
    table.iter().all(|v| v.is_finite() && *v >= 0.0).then_some(table)
}

impl AssetSink for Sink {
    fn set_script_strings(&mut self, _: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        fastfile_iw4::load_asset_at_observed(s, ty, slot, self)?;
        self.walked += 1;
        Ok(())
    }
}

impl AssetLinkSink for Sink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        _slot: Ptr,
        _insert: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        if ty != AssetType::Weapon {
            return Ok(());
        }
        let Some(g) = s.weapon() else { return Ok(()) };
        let name = text(s, g.name);
        if name.is_empty() {
            return Ok(());
        }
        let strings = [
            ("gun_model", g.gun_xmodel_name.or(g.gun_xmodel_names[0])),
            ("hand_model", g.hand_xmodel_name),
            ("world_model", g.world_model_name),
            ("alternate", g.alternate_weapon_name),
            ("fire_sound", g.fire_sound_name),
            ("fire_sound_player", g.fire_sound_player_name),
            ("fire_last_sound_player", g.fire_last_sound_player_name),
            ("empty_fire_sound_player", g.empty_fire_sound_player_name),
            ("reload_sound_player", g.reload_sound_player_name),
            ("reload_empty_sound_player", g.reload_empty_sound_player_name),
            ("rechamber_sound_player", g.rechamber_sound_player_name),
            ("raise_sound_player", g.raise_sound_player_name),
            ("putaway_sound_player", g.putaway_sound_player_name),
            ("melee_swipe_sound_player", g.melee_swipe_sound_player_name),
            ("melee_hit_sound", g.melee_hit_sound_name),
        ]
        .into_iter()
        .map(|(k, p)| (k.to_owned(), text(s, p)))
        .filter(|(_, v)| !v.is_empty())
        .collect::<Vec<(String, String)>>();
        let models: Vec<(String, String)> = strings.iter().filter(|(k, _)| k.ends_with("_model")).cloned().collect();
        let mut strings = strings;
        if let Some(def) = g.weap_def {
            if models.is_empty() {
                strings.extend(self.defs.get(&def).cloned().unwrap_or_default());
            } else {
                self.defs.entry(def).or_insert(models);
            }
        }
        self.found.weapons.push(Row {
            name,
            display_name: text(s, g.display_name),
            numbers: numbers(&g),
            strings,
            location: g.location_damage_mult,
        });
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

    fn alias(&mut self, _: AssetType, _: Ptr, _: Ptr) -> fastfile_iw4::Result<()> {
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> fastfile_iw4::Result<()> {
        if name == "info/mp_lochit_dmgtable" {
            let bytes = if zlib_compressed {
                miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap_or_default()
            } else {
                data.to_vec()
            };
            if let Some(t) = std::str::from_utf8(&bytes).ok().and_then(parse_lochit) {
                self.found.location = Some(t);
            }
        }
        Ok(())
    }
}

/// The weapons of one zone image (`zone::open`), in asset order.
pub fn capture(image: &[u8]) -> Result<Found, String> {
    let header = fastfile_iw4::parse_zone_header(image).map_err(|e| format!("zone header: {e:?}"))?;
    let mut memory = crate::zone::Memory::for_header(&header);
    let mut stream = memory.stream(image)?;
    let mut sink = Sink {
        found: Found { weapons: Vec::new(), location: None },
        walked: 0,
        defs: Default::default(),
        xmodel_names: Default::default(),
    };
    fastfile_iw4::load_zone(&mut stream, &mut sink).map_err(|e| {
        format!("zone walk stopped after {} assets ({} weapons): {e:?}", sink.walked, sink.found.weapons.len())
    })?;
    Ok(sink.found)
}

#[cfg(test)]
mod tests {
    #[test]
    fn lochit_tables_parse() {
        let t = super::parse_lochit("LOCDMGTABLE\\head\\1.4\\neck\\1.4\\torso_upper\\1\\gun\\0\0").unwrap();
        assert_eq!((t[2], t[3], t[4], t[18], t[0]), (1.4, 1.4, 1.0, 0.0, 1.0));
        assert!(super::parse_lochit("nope").is_none());
        assert!(super::parse_lochit("LOCDMGTABLE\\nose\\2").is_none());
    }
}
