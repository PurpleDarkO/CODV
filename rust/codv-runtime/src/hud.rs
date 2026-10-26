//! MW2's hip crosshair and ADS overlay (the sniper scope) for the host's HUD
//! pass, computed the way iw4L's HUD does (`hud/src/reticle.rs`,
//! `hud/src/iris.rs` on `hud_iw4`'s retail quantities): the reticle's four
//! side images open with the weapon's spread cone and the aim spread scale
//! and fade as the spread grows; the centre image grows while a grenade
//! cooks; the scope overlay fades in over the ADS window and hides the view
//! model while it shows.
//!
//! Sizes and offsets are in MW2's virtual screen units: 480 units are the
//! screen's height, centred.
use codv_engine::Mw2Player;
use codv_mw2data::table::Row;
use hud_iw4 as h;
use std::collections::HashMap;
use weapon_iw4 as w;

/// The per-weapon facts the HUD reads (the imported row).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub center_image: String,
    pub side_image: String,
    pub overlay_image: String,
    pub center_size: f32,
    pub reticle: h::WeaponReticleFacts,
    pub ads: h::WeaponAdsCrosshairFacts,
    pub overlay: h::WeaponAdsOverlayFacts,
    pub spread: w::WeaponSpreadFacts,
    pub cooks: bool,
}

impl Facts {
    pub fn from_row(r: &Row) -> Facts {
        let overlay_image = r.s("overlay_image").to_owned();
        Facts {
            center_image: r.s("reticle_center_image").to_owned(),
            side_image: r.s("reticle_side_image").to_owned(),
            center_size: r.f("reticle_center_size"),
            reticle: h::WeaponReticleFacts {
                i_reticle_min_ofs: r.i("reticle_min_ofs"),
                hip_reticle_side_pos: r.f("hip_reticle_side_pos"),
                i_reticle_side_size: r.i("reticle_side_size"),
            },
            ads: h::WeaponAdsCrosshairFacts {
                ads_aim_pitch: r.f("ads_aim_pitch"),
                ads_crosshair_in_frac: r.f("ads_crosshair_in_frac"),
                ads_crosshair_out_frac: r.f("ads_crosshair_out_frac"),
            },
            overlay: h::WeaponAdsOverlayFacts {
                ads_zoom_in_frac: r.f("ads_zoom_in_frac"),
                ads_zoom_out_frac: r.f("ads_zoom_out_frac"),
                // The overlay is drawn when the weapon names an image (a material handle stand-in).
                overlay_material: u32::from(!overlay_image.is_empty()),
                overlay_material_low_res: 0,
                overlay_material_emp: 0,
                overlay_material_emp_low_res: 0,
                overlay_reticle: r.i("overlay_reticle"),
                ads_overlay_width: r.f("ads_overlay_width"),
                ads_overlay_height: r.f("ads_overlay_height"),
            },
            spread: w::WeaponSpreadFacts {
                stand_min: r.f("hip_spread_stand_min"),
                ducked_min: r.f("hip_spread_ducked_min"),
                prone_min: r.f("hip_spread_prone_min"),
                stand_max: r.f("hip_spread_stand_max"),
                ducked_max: r.f("hip_spread_ducked_max"),
                prone_max: r.f("hip_spread_prone_max"),
            },
            cooks: r.i("weap_type") == 1 && r.i("offhand_class") == 1,
            overlay_image,
        }
    }
}

/// What the host draws this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub crosshair: bool,
    pub alpha: f32,
    /// Side offsets from the centre (x, y) and the side / centre image sizes.
    pub gap: [f32; 2],
    pub side_size: f32,
    pub center_size: f32,
    /// The scope overlay: its opacity (0: none) and size; four mirrored quads when small.
    pub overlay_alpha: f32,
    pub overlay_size: [f32; 2],
    pub overlay_four_quads: bool,
    /// The view model is hidden behind the overlay.
    pub scoped: bool,
    pub center_image: String,
    pub side_image: String,
    pub overlay_image: String,
}

/// Per-weapon facts and the ADS direction latch (iw4L ReticleAdsLatch).
#[derive(Default)]
pub struct Hud {
    facts: HashMap<String, Facts>,
    last_frac: f32,
    to_ads: bool,
}

impl Hud {
    /// The HUD for `player` holding (or showing, mid-throw) the weapon `row`;
    /// `fov_y` the camera's vertical FOV, degrees; `third_person` hides the crosshair.
    pub fn frame(&mut self, player: &Mw2Player, row: Option<&Row>, fov_y: f32, third_person: bool) -> Frame {
        let ps = &player.ps;
        let frac = ps.f_weapon_pos_frac;
        if frac > self.last_frac + 1e-4 {
            self.to_ads = true;
        } else if frac + 1e-4 < self.last_frac {
            self.to_ads = false;
        }
        self.last_frac = frac;
        let Some(row) = row else { return Frame::default() };
        let f = self.facts.entry(row.name.clone()).or_insert_with(|| Facts::from_row(row)).clone();
        let tan_half = (fov_y.clamp(1.0, 170.0).to_radians() * 0.5).tan();
        let mut out = Frame {
            center_image: f.center_image.clone(),
            side_image: f.side_image.clone(),
            overlay_image: f.overlay_image.clone(),
            ..Frame::default()
        };

        // The scope (iris) overlay.
        let iris = h::get_weap_reticle_zoom(frac, self.to_ads, &f.overlay);
        let draw = h::draw_weap_reticle(frac, self.to_ads, 1080.0, 0, &f.overlay);
        if let Some(alpha) = draw.overlay_alpha
            && !f.overlay_image.is_empty()
            && f.overlay.ads_overlay_width > 0.0
            && f.overlay.ads_overlay_height > 0.0
        {
            out.overlay_alpha = alpha;
            out.overlay_size = [f.overlay.ads_overlay_width, f.overlay.ads_overlay_height];
            out.overlay_four_quads = h::ads_overlay_uses_four_quads(out.overlay_size[0], out.overlay_size[1]);
            out.scoped = iris.active;
        }

        // The hip crosshair.
        let gate = h::HipCrosshairGate {
            rendering_third_person: third_person,
            e_flags: ps.e_flags,
            viewmodel_weapon_index: 1,
            f_weapon_pos_frac: frac,
            bob_gate: w::should_apply_view_org_bob(
                false,
                ps.pm_type,
                ps.other_flags,
                ps.link_flags,
                ps.e_flags,
                frac,
                f.overlay.overlay_reticle,
            ),
            weaponstate_primary: ps.weaponstate_primary,
            ..h::HipCrosshairGate::default()
        };
        if h::hip_crosshair_visible(&gate) && (!f.side_image.is_empty() || !f.center_image.is_empty()) {
            let trans = h::hip_crosshair_trans_scale(frac, self.to_ads, &f.ads, tan_half);
            let side = h::reticle_draw_size(&f.reticle, trans);
            let cone = w::get_spread_for_weapon(
                ps.view_height_current,
                ps.spread_override,
                w::SpreadOverrideState::from_i32(ps.spread_override_state),
                &f.spread,
                1.0,
            );
            out.gap =
                h::calc_reticle_spread(cone.min, cone.max, ps.aim_spread_scale, trans, tan_half, &f.reticle, side);
            let cook =
                if f.cooks && ps.grenade_time_left > 0 { (ps.grenade_time_left % 1000) as f32 / 100.0 } else { 0.0 };
            out.side_size = side[0];
            out.center_size = (f.center_size + cook) * trans;
            out.alpha = h::calc_reticle_alpha(
                draw.hip_reticle_alpha,
                h::CG_CROSSHAIR_ALPHA_DEFAULT,
                h::CG_CROSSHAIR_ALPHA_MIN_DEFAULT,
                ps.aim_spread_scale,
            );
            out.crosshair = out.alpha > 0.0 && (out.side_size > 0.0 || out.center_size > 0.0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, overlay: bool) -> Row {
        let mut numbers: Vec<(String, f64)> = [
            ("reticle_center_size", 4.0),
            ("reticle_side_size", 8.0),
            ("reticle_min_ofs", 10.0),
            ("hip_reticle_side_pos", 0.0),
            ("ads_crosshair_in_frac", 1.0),
            ("ads_crosshair_out_frac", 0.1),
            ("ads_zoom_in_frac", 0.7),
            ("ads_zoom_out_frac", 0.1),
            ("hip_spread_stand_min", 3.0),
            ("hip_spread_stand_max", 8.0),
            ("hip_spread_ducked_min", 2.5),
            ("hip_spread_ducked_max", 6.0),
            ("hip_spread_prone_min", 2.0),
            ("hip_spread_prone_max", 5.0),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
        let mut strings = vec![
            ("reticle_center_image".to_string(), String::new()),
            ("reticle_side_image".to_string(), "side_small".to_string()),
        ];
        if overlay {
            numbers.push(("ads_overlay_width".into(), 480.0));
            numbers.push(("ads_overlay_height".into(), 480.0));
            numbers.push(("overlay_reticle".into(), 1.0));
            strings.push(("overlay_image".into(), "scope_overlay_m40a3_1024#0".into()));
        }
        Row { name: name.into(), numbers, strings, ..Row::default() }
    }

    fn player(frac: f32, spread: f32) -> Mw2Player {
        let mut p = Mw2Player::new([0.0, 0.0, 2.0], 0.0);
        p.ps.f_weapon_pos_frac = frac;
        p.ps.aim_spread_scale = spread;
        p
    }

    #[test]
    fn the_hip_crosshair_opens_with_the_aim_spread() {
        let mut hud = Hud::default();
        let r = row("m4_mp", false);
        let still = hud.frame(&player(0.0, 0.0), Some(&r), 65.0, false);
        let moving = hud.frame(&player(0.0, 255.0), Some(&r), 65.0, false);
        assert!(still.crosshair && moving.crosshair, "{still:?} {moving:?}");
        assert!(moving.gap[1] > still.gap[1] + 1.0, "{still:?} {moving:?}");
        assert!(still.gap[1] >= 10.0);
        assert!(still.alpha >= moving.alpha);
        assert_eq!(still.overlay_alpha, 0.0);
        assert!(!still.scoped);
    }

    #[test]
    fn third_person_and_full_ads_hide_the_hip_crosshair() {
        let mut hud = Hud::default();
        let r = row("m4_mp", false);
        assert!(!hud.frame(&player(0.0, 0.0), Some(&r), 65.0, true).crosshair);
        hud.frame(&player(0.5, 0.0), Some(&r), 65.0, false);
        assert!(!hud.frame(&player(1.0, 0.0), Some(&r), 65.0, false).crosshair);
    }

    #[test]
    fn a_sniper_at_full_ads_shows_the_scope_and_hides_the_view_model() {
        let mut hud = Hud::default();
        let r = row("cheytac_mp", true);
        let hip = hud.frame(&player(0.0, 0.0), Some(&r), 65.0, false);
        assert_eq!(hip.overlay_alpha, 0.0);
        hud.frame(&player(0.5, 0.0), Some(&r), 65.0, false);
        let ads = hud.frame(&player(1.0, 0.0), Some(&r), 10.0, false);
        assert!(ads.overlay_alpha > 0.99, "{ads:?}");
        assert!(ads.scoped);
        assert_eq!(ads.overlay_size, [480.0, 480.0]);
        assert!(!ads.overlay_four_quads);
        assert_eq!(ads.overlay_image, "scope_overlay_m40a3_1024#0");
    }
}
