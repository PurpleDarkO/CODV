use super::*;
use crate::arms::{Arsenal, Gunner};
use crate::player::{MoveInput, button};
use crate::world::BoxWorld;

#[test]
fn hide_tags_hide_the_tag_and_everything_under_it() {
    // root (tag_weapon) <- tag_m203 <- j_m203_trigger ; tag_clip under root.
    let m = vm::Model {
        name: "gun".into(),
        num_root_bones: 1,
        bone_names: vec!["tag_weapon".into(), "tag_m203".into(), "j_m203_trigger".into(), "tag_clip".into()],
        parent_list: vec![1, 1, 3],
        ..Default::default()
    };
    let words = hidden_words(&m, &["TAG_M203".to_owned()]);
    let bit = |b: usize| words[b >> 5] & (0x8000_0000u32 >> (b & 31)) != 0;
    assert!(!bit(0) && bit(1) && bit(2) && !bit(3));
    assert_eq!(hidden_words(&m, &[]), [0; 6]);
}

#[test]
fn scheduler_blends_to_goal_and_wraps_loops() {
    let mut s = Scheduler::new(2);
    s.set_goal_weight(0, 1.0, 0.0);
    assert_eq!(s.weight(0), 1.0);
    s.set_goal_weight(0, 0.0, 0.2);
    s.advance(0.1);
    assert!((s.weight(0) - 0.5).abs() < 1e-5);
    s.advance(0.2);
    assert_eq!(s.weight(0), 0.0);
}

/// The player's own import (CODV_MW2_OUT = the folder holding weapons.json and
/// viewmodels/); skipped otherwise. Builds the M4's rig, raises it, fires and
/// reloads, and checks the arms and gun sit in front of the eye.
#[test]
fn m4_viewmodel_from_the_players_import() {
    let Ok(dir) = std::env::var("CODV_MW2_OUT") else { return };
    let dir = std::path::Path::new(&dir);
    let table = codv_mw2data::table::from_json(&std::fs::read_to_string(dir.join("weapons.json")).unwrap()).unwrap();
    let row = table.find("m4_mp").unwrap();
    let load_model = |n: &str| {
        vm::read_model(
            &std::fs::read(
                dir.join("viewmodels/models").join(format!("{}.cvm", vm::file_stem(&n.to_ascii_lowercase()))),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let hands = load_model(row.s("hand_model"));
    let gun = load_model(row.s("gun_model"));
    let mut clips = vec![None; SLOTS];
    for (s, c) in clips.iter_mut().enumerate() {
        let name = row.s(&format!("xanim.{s}"));
        if !name.is_empty() {
            let a = vm::read_anim(
                &std::fs::read(dir.join("viewmodels/anims").join(format!("{}.cva", vm::file_stem(name)))).unwrap(),
            )
            .unwrap();
            *c = Some(Arc::new(clip(&a).unwrap()));
        }
    }
    let loaded = clips.iter().flatten().count();
    let hide: Vec<String> = row.s("hide_tags").split(',').filter(|t| !t.is_empty()).map(str::to_owned).collect();
    let (arsenal, _) = Arsenal::from_table(&table);
    let id = arsenal.id("m4_mp").unwrap();
    let mut rig =
        Rig::build(id, &hands, &gun, &hide, clips, Timers::from_row(row), PlacementFacts::from_row(row)).unwrap();
    println!(
        "m4 rig: {} bones, {} vertices, {} draws, {loaded} clips, hide {hide:?}",
        rig.bone_count(),
        rig.mesh().vertices.len(),
        rig.mesh().draws.len()
    );
    assert!(rig.mesh().draws.len() >= 4 && rig.mesh().draws.len() < 40);
    let world = BoxWorld::floor(0.0, 20000.0);
    let mut player = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    let mut gunner = Gunner::new(&arsenal, &[id]);
    const MS: u32 = 16;
    let mut states = Vec::new();
    let mut frame_of = |buttons: u32, rig: &mut Rig, player: &mut Mw2Player| {
        gunner.arm(player, &arsenal);
        player.step(&MoveInput { msec: MS, buttons, ..Default::default() }, &world);
        let f = gunner.step(player, &arsenal, MS, buttons);
        rig.tick(player, &f, MS as f32 / 1000.0);
        f
    };
    for _ in 0..90 {
        frame_of(0, &mut rig, &mut player);
        states.push(rig.state());
    }
    assert!(states.contains(&VmState::Raising), "the first raise played: {states:?}");
    assert_eq!(rig.state(), VmState::Ready);
    // Every vertex of the posed rig in front of the eye, within arm's reach.
    let check = |rig: &Rig, what: &str| {
        let skin = rig.skin();
        let mesh = rig.mesh();
        let mut z_min = f32::MAX;
        let mut z_max = f32::MIN;
        for v in mesh.vertices.iter().step_by(7) {
            let mut p = [0.0f32; 3];
            for (b, wgt) in v.bones.iter().zip(v.weights) {
                if wgt <= 0.0 {
                    continue;
                }
                let m = &skin[*b as usize];
                for (r, out) in p.iter_mut().enumerate() {
                    *out += wgt
                        * (m[r * 4] * v.position[0]
                            + m[r * 4 + 1] * v.position[1]
                            + m[r * 4 + 2] * v.position[2]
                            + m[r * 4 + 3]);
                }
            }
            assert!(p.iter().all(|x| x.is_finite()), "{what}: {p:?}");
            z_min = z_min.min(p[2]);
            z_max = z_max.max(p[2]);
        }
        println!("{what}: depth {z_min:.1} .. {z_max:.1} in");
        if let Ok(dump) = std::env::var("CODV_VM_DUMP") {
            // Posed vertices (x y z nx ny nz u v) and indices, for an offline look.
            let mut out = Vec::new();
            out.extend_from_slice(&(mesh.vertices.len() as u32).to_le_bytes());
            for v in &mesh.vertices {
                let mut p = [0.0f32; 6];
                for (b, wgt) in v.bones.iter().zip(v.weights) {
                    let m = &skin[*b as usize];
                    for r in 0..3 {
                        p[r] += wgt
                            * (m[r * 4] * v.position[0]
                                + m[r * 4 + 1] * v.position[1]
                                + m[r * 4 + 2] * v.position[2]
                                + m[r * 4 + 3]);
                        p[3 + r] +=
                            wgt * (m[r * 4] * v.normal[0] + m[r * 4 + 1] * v.normal[1] + m[r * 4 + 2] * v.normal[2]);
                    }
                }
                for x in p.iter().chain(&v.uv) {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            out.extend_from_slice(&(mesh.draws.len() as u32).to_le_bytes());
            for d in &mesh.draws {
                let name = d.material.color_map.as_bytes();
                out.extend_from_slice(&(name.len() as u32).to_le_bytes());
                out.extend_from_slice(name);
                out.extend_from_slice(&d.index_count.to_le_bytes());
                for i in &mesh.indices[d.first_index as usize..(d.first_index + d.index_count) as usize] {
                    out.extend_from_slice(&i.to_le_bytes());
                }
            }
            std::fs::write(format!("{dump}/{}.bin", what.replace(' ', "_")), out).unwrap();
        }
        assert!(z_max > 4.0 && z_max < 60.0 && z_min > -20.0, "{what}: {z_min} .. {z_max}");
    };
    check(&rig, "idle");
    // Fire a few, then reload.
    for _ in 0..30 {
        frame_of(button::ATTACK, &mut rig, &mut player);
    }
    assert!(matches!(rig.state(), VmState::Firing | VmState::Ready));
    check(&rig, "firing");
    for _ in 0..10 {
        frame_of(0, &mut rig, &mut player);
    }
    let mut reloading = false;
    rig.take_notes();
    let mut notes = Vec::new();
    for i in 0..90 {
        let f = frame_of(if i < 2 { button::RELOAD } else { 0 }, &mut rig, &mut player);
        reloading |= rig.state() == VmState::Reloading;
        notes.extend(rig.take_notes());
        let _ = f;
    }
    assert!(reloading, "the reload animation played");
    // The reload's notetracks are its sounds (the M4's clip out, then clip in).
    println!("reload notetracks: {notes:?}");
    let out = notes.iter().position(|n| n.contains("clipout"));
    let inn = notes.iter().position(|n| n.contains("clipin"));
    assert!(out.is_some() && inn.is_some() && out < inn, "{notes:?}");
    check(&rig, "after reload");
    // ADS: the gun comes to the eye line (iron sight near the centre).
    for _ in 0..40 {
        frame_of(button::ADS, &mut rig, &mut player);
    }
    let (fov, ads) = rig.zoom();
    println!("ads {ads:.2} zoom fov {fov}");
    assert!(ads > 0.95);
    check(&rig, "ads");
}

/// The player's import (CODV_MW2_OUT): M4 -> USP -> M4 -> USP. The guns swap
/// off screen, and each raise (the first-time pullout too) ends when the
/// weapon is ready, so nothing is left to be cut short.
#[test]
fn weapon_switches_raise_in_step_with_the_weapon() {
    let Ok(dir) = std::env::var("CODV_MW2_OUT") else { return };
    let dir = std::path::Path::new(&dir);
    let table = codv_mw2data::table::from_json(&std::fs::read_to_string(dir.join("weapons.json")).unwrap()).unwrap();
    let (arsenal, _) = Arsenal::from_table(&table);
    let build = |name: &str| {
        let row = table.find(name).unwrap();
        let load_model = |n: &str| {
            vm::read_model(
                &std::fs::read(
                    dir.join("viewmodels/models").join(format!("{}.cvm", vm::file_stem(&n.to_ascii_lowercase()))),
                )
                .unwrap(),
            )
            .unwrap()
        };
        let mut clips = vec![None; SLOTS];
        for (s, c) in clips.iter_mut().enumerate() {
            let n = row.s(&format!("xanim.{s}"));
            if !n.is_empty() {
                let a = vm::read_anim(
                    &std::fs::read(dir.join("viewmodels/anims").join(format!("{}.cva", vm::file_stem(n)))).unwrap(),
                )
                .unwrap();
                *c = Some(Arc::new(clip(&a).unwrap()));
            }
        }
        let hide: Vec<String> = row.s("hide_tags").split(',').filter(|t| !t.is_empty()).map(str::to_owned).collect();
        Rig::build(
            arsenal.id(name).unwrap(),
            &load_model(row.s("hand_model")),
            &load_model(row.s("gun_model")),
            &hide,
            clips,
            Timers::from_row(row),
            PlacementFacts::from_row(row),
        )
        .unwrap()
    };
    let names = ["m4_mp", "usp_mp"];
    let ids: Vec<u32> = names.iter().map(|n| arsenal.id(n).unwrap()).collect();
    let world = BoxWorld::floor(0.0, 20000.0);
    let mut player = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    let mut gunner = Gunner::new(&arsenal, &ids);
    let mut rig = build(names[0]);
    const MS: u32 = 16;
    let centroid = |rig: &Rig| {
        let s = rig.skin();
        let mut c = [0.0f32; 3];
        for m in &s {
            c[0] += m[3];
            c[1] += m[7];
            c[2] += m[11];
        }
        let n = s.len() as f32;
        [c[0] / n, c[1] / n, c[2] / n]
    };
    let mut prev = None;
    let (mut swapped, mut last_vis, mut ready_at, mut swaps) = (false, 1.0f32, None::<usize>, 0);
    for i in 0..550 {
        if i == 100 {
            gunner.select(1);
        }
        if i == 250 {
            gunner.select(0);
        }
        if i == 400 {
            gunner.select(1);
        }
        gunner.arm(&mut player, &arsenal);
        player.step(&MoveInput { msec: MS, ..Default::default() }, &world);
        let f = gunner.step(&mut player, &arsenal, MS, 0);
        if rig.weapon != f.weapon {
            let name = names[ids.iter().position(|&x| x == f.weapon).unwrap()];
            rig = build(name);
            swapped = true;
        }
        rig.tick(&player, &f, MS as f32 / 1000.0);
        let c = centroid(&rig);
        // Fraction of vertices on screen (rigid skin by the first bone; fov_y 50, 16:9) and their mean screen y.
        let skin = rig.skin();
        let (mut vis, mut tot, mut sy) = (0usize, 0usize, 0.0f32);
        for v in rig.mesh().vertices.iter().step_by(7) {
            let m = &skin[v.bones[0] as usize];
            let p = v.position;
            let x = m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3];
            let y = m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7];
            let z = m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11];
            tot += 1;
            let k = 1.0 / (25.0f32.to_radians().tan());
            if z > 1.0 {
                let (sx, syy) = (x / z * k / 1.777, y / z * k);
                if sx.abs() < 1.0 && syy.abs() < 1.0 {
                    vis += 1;
                    sy += syy;
                }
            }
        }
        let vis_frac = vis as f32 / tot as f32;
        let sy = if vis > 0 { sy / vis as f32 } else { -9.0 };
        let jump = prev
            .map_or(0.0, |p: [f32; 3]| ((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) + (c[2] - p[2]).powi(2)).sqrt());
        prev = Some(c);
        let _ = (sy, jump);
        if swapped {
            assert!(last_vis < 0.05 && vis_frac < 0.05, "frame {i}: the swap is visible ({last_vis} -> {vis_frac})");
            swapped = false;
            swaps += 1;
        }
        if f.weaponstate == 0 && i > 100 {
            let at = *ready_at.get_or_insert(i);
            if i >= at + 3 {
                assert_eq!(rig.state(), VmState::Ready, "frame {i}: the raise outlives the weapon's raise time");
            }
        } else {
            ready_at = None;
        }
        last_vis = vis_frac;
    }
    assert_eq!(swaps, 3);
}

/// The player's import (CODV_MW2_OUT): the RPG-7 carries its rocket (viewmodel_rpg7_rocket on
/// tag_rocket), drawn while loaded, gone once fired, back during the reload when the round goes in.
#[test]
fn the_rpg_shows_its_rocket_while_loaded() {
    let Ok(dir) = std::env::var("CODV_MW2_OUT") else { return };
    let dir = std::path::Path::new(&dir);
    let table = codv_mw2data::table::from_json(&std::fs::read_to_string(dir.join("weapons.json")).unwrap()).unwrap();
    let Some(row) = table.find("rpg_mp") else { return };
    if row.s("rocket_model").is_empty() {
        return; // an import from before CODV read the rocket
    }
    let load_model = |n: &str| {
        vm::read_model(
            &std::fs::read(
                dir.join("viewmodels/models").join(format!("{}.cvm", vm::file_stem(&n.to_ascii_lowercase()))),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let mut clips = vec![None; SLOTS];
    for (s, c) in clips.iter_mut().enumerate() {
        let n = row.s(&format!("xanim.{s}"));
        if !n.is_empty() {
            let a = vm::read_anim(
                &std::fs::read(dir.join("viewmodels/anims").join(format!("{}.cva", vm::file_stem(n)))).unwrap(),
            )
            .unwrap();
            *c = Some(Arc::new(clip(&a).unwrap()));
        }
    }
    let (arsenal, _) = Arsenal::from_table(&table);
    let id = arsenal.id("rpg_mp").unwrap();
    let rocket = load_model(row.s("rocket_model"));
    let rocket_bones = rocket.bone_names.len();
    let mut rig = Rig::build_with_rocket(
        id,
        &load_model(row.s("hand_model")),
        &load_model(row.s("gun_model")),
        Some(&rocket),
        &[],
        clips,
        Timers::from_row(row),
        PlacementFacts::from_row(row),
    )
    .unwrap();
    assert!(rig.has_rocket(), "the rocket merges onto the RPG-7's bones");
    let hidden = |rig: &Rig| {
        let s = rig.skin();
        assert_eq!(s.len(), rig.bone_count() + rocket_bones, "the rocket's extra bones");
        s[s.len() - rocket_bones..].iter().all(|m| m.iter().all(|v| *v == 0.0))
    };
    let world = BoxWorld::floor(0.0, 20000.0);
    let mut player = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    let mut gunner = Gunner::new(&arsenal, &[id]);
    let mut step = |buttons: u32, rig: &mut Rig| {
        gunner.arm(&mut player, &arsenal);
        player.step(&MoveInput { msec: 16, buttons, ..Default::default() }, &world);
        let f = gunner.step(&mut player, &arsenal, 16, buttons);
        rig.tick(&player, &f, 0.016);
        f
    };
    for _ in 0..120 {
        step(0, &mut rig);
    }
    assert!(!hidden(&rig), "loaded: the rocket shows");
    let mut fired = false;
    for i in 0..40 {
        let f = step(if i < 3 { button::ATTACK } else { 0 }, &mut rig);
        fired |= f.clip == 0;
    }
    assert!(fired && hidden(&rig), "fired: the rocket is gone");
    // The reload (MW2's own, or the auto reload): the round goes in partway, the rocket comes back.
    let mut back_while_reloading = false;
    for i in 0..400 {
        step(if i < 3 { button::RELOAD } else { 0 }, &mut rig);
        if !hidden(&rig) && rig.state() == VmState::Reloading {
            back_while_reloading = true;
        }
    }
    assert!(back_while_reloading, "the rocket shows during the reload, before it ends");
    assert!(!hidden(&rig), "reloaded");
}
