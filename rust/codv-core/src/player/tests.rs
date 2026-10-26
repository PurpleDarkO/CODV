use super::*;
use crate::defs::Table;

const DT: f32 = 1.0 / 60.0;

fn player() -> Player {
    Player::new(Table::embedded(), 7)
}

fn hash(name: &str) -> u32 {
    let t = Table::embedded();
    t.weapons[t.by_name(name).unwrap()].gta_hash
}

fn input(weapon: &str, held: u32, ammo: u32) -> FrameIn {
    FrameIn { dt: DT, held, on_foot: true, gta_weapon: hash(weapon), gta_ammo: ammo, health: 1.0, ..Default::default() }
}

/// Runs `seconds` with the same input, feeding GTA's ammo back like the host does.
fn run(p: &mut Player, mut i: FrameIn, seconds: f32) -> (Vec<FrameOut>, u32) {
    let mut outs = Vec::new();
    for _ in 0..(seconds / DT).round() as usize {
        let o = p.step(&i);
        i.gta_ammo -= o.ammo_used;
        outs.push(o);
    }
    (outs, i.gta_ammo)
}

fn shots(outs: &[FrameOut]) -> usize {
    outs.iter().map(|o| o.shots.len()).sum()
}

/// Puts the carbine in hand and waits out its raise.
fn ready(p: &mut Player, weapon: &str) {
    run(p, input(weapon, 0, 120), 1.0);
}

#[test]
fn auto_fire_keeps_the_rate_of_fire() {
    let mut p = player();
    ready(&mut p, "carbine");
    let (outs, ammo) = run(&mut p, input("carbine", FIRE, 120), 1.0);
    // 750 rpm: 12.5 shots a second; the cadence carries across frames.
    let n = shots(&outs);
    assert!((12..=13).contains(&n), "{n} shots");
    assert_eq!(ammo, 120 - n as u32);
    assert!(outs[0].shots.len() == 1, "the first shot leaves at once");
}

#[test]
fn magazine_empties_then_reloads_and_refills() {
    let mut p = player();
    ready(&mut p, "carbine");
    let (outs, ammo) = run(&mut p, input("carbine", FIRE, 120), 3.0);
    assert_eq!(shots(&outs[..]) as u32, 30, "one magazine before the reload");
    assert!(outs.iter().any(|o| o.reload_started), "empty magazine reloads by itself");
    assert_eq!(ammo, 90);
    // The empty reload (2.9 s) then refills to 30.
    let (_, _) = run(&mut p, input("carbine", 0, ammo), 3.0);
    let o = p.step(&input("carbine", 0, ammo));
    assert_eq!(o.hud.mag, 30);
    assert_eq!(o.hud.reserve, 60);
}

#[test]
fn tactical_reload_is_faster_and_sprint_cancels_it() {
    let mut p = player();
    ready(&mut p, "carbine");
    run(&mut p, input("carbine", FIRE, 120), 0.3);
    let o = p.step(&input("carbine", RELOAD, 116));
    assert!(o.reload_started);
    let (outs, _) = run(&mut p, input("carbine", 0, 116), 2.3);
    assert!(outs.last().unwrap().hud.mag == 30, "2.2 s tactical reload done");
    // Sprinting during a reload cancels it.
    run(&mut p, input("carbine", FIRE, 116), 0.3);
    p.step(&input("carbine", RELOAD, 112));
    let mut i = input("carbine", SPRINT, 112);
    i.sprinting = true;
    i.speed = 6.0;
    let (outs, _) = run(&mut p, i, 0.5);
    assert_eq!(outs.last().unwrap().state, Some(State::Sprinting));
    assert!(outs.last().unwrap().hud.mag < 30);
}

#[test]
fn semi_auto_needs_a_press_per_shot_and_buffers_early_presses() {
    let mut p = player();
    ready(&mut p, "pistol");
    let (outs, _) = run(&mut p, input("pistol", FIRE, 48), 1.0);
    assert_eq!(shots(&outs), 1, "holding fires once");
    // Release, then press every 0.1 s (frames 6, 12, 18, 24; the trigger was
    // still held at frame 0). The pistol cycles in 0.133 s: the presses at 12
    // and 18 come early and are buffered, firing at frames 14 and 22.
    let mut fired = Vec::new();
    for f in 0..30 {
        let held = if f % 6 < 3 { FIRE } else { 0 };
        if !p.step(&input("pistol", held, 47)).shots.is_empty() {
            fired.push(f);
        }
    }
    assert_eq!(fired, [6, 14, 22]);
}

#[test]
fn burst_fires_three_then_waits() {
    let mut p = player();
    ready(&mut p, "burst_rifle");
    let mut i = input("burst_rifle", FIRE, 120);
    let o = p.step(&i);
    assert_eq!(o.shots.len(), 1);
    i.held = 0; // the burst finishes even when the trigger is released
    let (outs, _) = run(&mut p, i, 0.3);
    assert_eq!(1 + shots(&outs), 3);
}

#[test]
fn shotgun_fires_pellets_and_loads_one_round_at_a_time() {
    let mut p = player();
    ready(&mut p, "shotgun");
    let o = p.step(&input("shotgun", FIRE, 32));
    assert_eq!(o.shots.len(), 8);
    assert_eq!(o.ammo_used, 1, "one shell for eight pellets");
    run(&mut p, input("shotgun", 0, 31), 1.0);
    p.step(&input("shotgun", RELOAD, 31));
    let (outs, _) = run(&mut p, input("shotgun", 0, 31), 0.7);
    assert_eq!(outs.last().unwrap().hud.mag, 8, "one shell back in 0.5 s, then full");
    // Firing interrupts a shell-by-shell reload.
    for _ in 0..3 {
        run(&mut p, input("shotgun", FIRE, 31), 0.05);
        run(&mut p, input("shotgun", 0, 31), 0.9);
    }
    p.step(&input("shotgun", RELOAD, 28));
    run(&mut p, input("shotgun", 0, 28), 0.6);
    let o = p.step(&input("shotgun", FIRE, 28));
    assert_eq!(o.shots.len(), 8);
}

#[test]
fn ads_tightens_spread_and_slows_movement() {
    let mut p = player();
    ready(&mut p, "carbine");
    let hip = p.step(&input("carbine", 0, 120));
    let (outs, _) = run(&mut p, input("carbine", ADS, 120), 0.3);
    let ads = outs.last().unwrap();
    assert!(ads.hud.ads >= 1.0, "0.22 s to aim down sights: {} {:?}", ads.hud.ads, ads.state);
    assert!(ads.hud.spread < 0.2 && hip.hud.spread >= 2.5);
    assert!(ads.move_rate < hip.move_rate);
    // Moving and jumping widen the hip cone.
    let mut moving = input("carbine", 0, 120);
    moving.speed = 3.0;
    run(&mut p, moving, 0.5);
    let walk = p.step(&moving);
    moving.airborne = true;
    let air = p.step(&moving);
    assert!(walk.hud.spread > hip.hud.spread && air.hud.spread > walk.hud.spread);
}

#[test]
fn recoil_kicks_the_view_and_part_of_it_recovers() {
    let mut p = player();
    ready(&mut p, "carbine");
    let (outs, _) = run(&mut p, input("carbine", FIRE, 120), 0.5);
    let up: f32 = outs.iter().map(|o| o.kick[0]).sum();
    assert!(up > 2.0, "the view climbs while firing: {up}");
    let (after, _) = run(&mut p, input("carbine", 0, 110), 0.6);
    let back: f32 = after.iter().map(|o| o.kick[0]).sum();
    assert!(back < 0.0 && -back < up, "some comes back, not all: {up} {back}");
    // ADS kicks less.
    let mut a = player();
    ready(&mut a, "carbine");
    run(&mut a, input("carbine", ADS, 120), 0.3);
    let (outs, _) = run(&mut a, input("carbine", ADS | FIRE, 120), 0.5);
    let ads_up: f32 = outs.iter().map(|o| o.kick[0]).sum();
    assert!(ads_up < up);
}

#[test]
fn spread_shots_land_inside_the_cone() {
    let mut p = player();
    ready(&mut p, "carbine");
    let (outs, _) = run(&mut p, input("carbine", FIRE, 120), 2.0);
    for o in &outs {
        for s in &o.shots {
            assert!(s.yaw.hypot(s.pitch) <= 6.0 + 1e-3);
        }
    }
}

#[test]
fn swap_drops_then_equips_the_other_slot_and_raises_it() {
    let mut p = player();
    ready(&mut p, "carbine");
    let o = p.step(&input("carbine", SWAP, 120));
    assert_eq!(o.state, Some(State::Dropping));
    let (outs, _) = run(&mut p, input("carbine", 0, 120), 0.35);
    let equip = outs.iter().find_map(|o| o.equip);
    assert_eq!(equip, Some(hash("pistol")));
    // GTA now reports the pistol: it raises (0.3 s) without a second raise.
    let o = p.step(&input("pistol", FIRE, 48));
    assert_eq!(o.state, Some(State::Raising));
    assert!(o.shots.is_empty(), "no firing while raising");
    let (outs, _) = run(&mut p, input("pistol", 0, 48), 0.3);
    assert_eq!(outs.last().unwrap().state, Some(State::Ready));
    // A weapon change CODV did not ask for (weapon wheel, pickup) also raises.
    let o = p.step(&input("smg", 0, 150));
    assert_eq!(o.state, Some(State::Raising));
}

#[test]
fn sprint_runs_out_blocks_and_comes_back() {
    let mut p = player();
    ready(&mut p, "carbine");
    let mut i = input("carbine", SPRINT, 120);
    i.sprinting = true;
    i.speed = 6.0;
    let (outs, _) = run(&mut p, i, 4.2);
    assert!(outs.last().unwrap().block_sprint, "4 s of sprint, then blocked");
    assert!(!outs[10].block_sprint);
    let (outs, _) = run(&mut p, input("carbine", 0, 120), 2.5);
    assert!(!outs.last().unwrap().block_sprint, "rests back after 1.2 s + 1 s of meter");
    // Firing out of a sprint waits for the sprint-out time.
    let mut q = player();
    ready(&mut q, "carbine");
    let mut s = input("carbine", SPRINT, 120);
    s.sprinting = true;
    s.speed = 6.0;
    run(&mut q, s, 0.5);
    let o = q.step(&input("carbine", FIRE, 120));
    assert!(o.shots.is_empty() && o.block_sprint, "pulling the trigger stops the sprint first");
    let (outs, _) = run(&mut q, input("carbine", FIRE, 120), 0.3);
    let first = outs.iter().position(|o| !o.shots.is_empty()).unwrap();
    assert!((13..=16).contains(&(first + 1)), "first shot after ~0.25 s: frame {}", first + 1);
}

#[test]
fn crouch_toggles_tightens_and_sprint_stands_up() {
    let mut p = player();
    ready(&mut p, "carbine");
    let stand = p.step(&input("carbine", 0, 120)).hud.spread;
    let o = p.step(&input("carbine", CROUCH, 120));
    assert_eq!(o.crouch_changed, Some(true));
    let crouch = p.step(&input("carbine", 0, 120)).hud.spread;
    assert!(crouch < stand);
    let o = p.step(&input("carbine", SPRINT, 120));
    assert_eq!(o.crouch_changed, Some(false));
}

#[test]
fn melee_and_quick_jump() {
    let mut p = player();
    ready(&mut p, "carbine");
    let o = p.step(&input("carbine", MELEE, 120));
    assert!(o.melee && o.state == Some(State::Melee));
    let o = p.step(&input("carbine", FIRE, 120));
    assert!(o.shots.is_empty(), "no shots during the melee");
    assert!(p.step(&input("carbine", JUMP, 120)).jump.is_none(), "quick jump is off by default");
    p.options.quick_jump = true;
    p.step(&input("carbine", 0, 120));
    assert!(p.step(&input("carbine", JUMP, 120)).jump.is_some());
}

#[test]
fn health_regenerates_after_a_pause() {
    let mut p = player();
    let mut i = input("carbine", 0, 120);
    i.health = 0.5;
    i.health_lost = 0.2;
    let o = p.step(&i);
    assert_eq!(o.heal, 0.0);
    assert!(o.hud.damage_flash > 0.5);
    i.health_lost = 0.0;
    let (outs, _) = run(&mut p, i, 4.9);
    assert!(outs.iter().all(|o| o.heal == 0.0), "5 s without damage first");
    let (outs, _) = run(&mut p, i, 0.5);
    assert!(outs.last().unwrap().heal > 0.0);
    p.options.regen = false;
    assert_eq!(p.step(&i).heal, 0.0);
}

#[test]
fn hits_use_range_falloff_and_drive_the_hitmarker() {
    let mut p = player();
    let c = p.table.by_name("carbine").unwrap();
    let d = p.report_hits(&[
        Hit { weapon: c, distance: 10.0, head: false, ped: true },
        Hit { weapon: c, distance: 100.0, head: true, ped: true },
        Hit { weapon: c, distance: 10.0, head: false, ped: false },
    ]);
    assert_eq!(d[0], 30.0);
    assert!((d[1] - 28.0).abs() < 1e-4);
    let o = p.step(&input("carbine", 0, 120));
    assert!(o.hud.hitmarker > 0.9 && o.hud.headshot && !o.hud.kill);
    p.report_kill();
    assert!(p.step(&input("carbine", 0, 120)).hud.kill);
}

#[test]
fn other_weapons_and_vehicles_are_left_to_gta() {
    let mut p = player();
    let mut i = input("carbine", FIRE, 120);
    i.gta_weapon = crate::defs::joaat("weapon_rpg");
    let o = p.step(&i);
    assert!(o.state.is_none() && o.shots.is_empty() && o.move_rate == 1.0);
    i.gta_weapon = hash("carbine");
    i.on_foot = false;
    let o = p.step(&i);
    assert!(o.state.is_none() && o.shots.is_empty());
    p.options.enabled = false;
    i.on_foot = true;
    assert!(p.step(&i).shots.is_empty());
}

#[test]
fn deterministic_for_a_seed() {
    let play = |seed| {
        let mut p = Player::new(Table::embedded(), seed);
        ready(&mut p, "carbine");
        let (outs, _) = run(&mut p, input("carbine", FIRE, 120), 1.0);
        outs.iter().flat_map(|o| o.shots.clone()).map(|s| (s.yaw, s.pitch)).collect::<Vec<_>>()
    };
    assert_eq!(play(3), play(3));
    assert_ne!(play(3), play(4));
}
