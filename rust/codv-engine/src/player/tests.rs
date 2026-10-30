//! iw4L's pmove on a synthetic world, checked against Modern Warfare 2's
//! numbers: 190 u/s run, 1.5x sprint for 4 s, a 39-unit jump at gravity 800,
//! 60 / 40 / 11 unit eye heights, 18-unit steps.
use super::*;
use crate::player::{Foley, StepKind};
use crate::world::{BoxWorld, SolidBox};

const MS: u32 = 16;

fn floor() -> BoxWorld {
    BoxWorld::floor(0.0, 20000.0)
}

/// Settles the player onto the floor, then runs `frames` of `input`.
fn run(p: &mut Mw2Player, w: &BoxWorld, input: MoveInput, frames: usize) -> Vec<MoveOutput> {
    (0..frames).map(|_| p.step(&input, w)).collect()
}

fn settled(w: &BoxWorld) -> Mw2Player {
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    run(&mut p, w, MoveInput { msec: MS, ..Default::default() }, 30);
    p
}

fn run_dyn(p: &mut Mw2Player, w: &dyn World, input: MoveInput, n: usize) {
    for _ in 0..n {
        p.step(&input, w);
    }
}

fn horizontal(v: [f32; 3]) -> f32 {
    v[0].hypot(v[1])
}

#[test]
fn stands_on_the_floor_with_retail_eye_height() {
    let w = floor();
    let mut p = settled(&w);
    let o = p.step(&MoveInput { msec: MS, ..Default::default() }, &w);
    assert!(o.on_ground, "{o:?}");
    assert!(o.origin[2].abs() < 0.5, "feet on the floor top: {:?}", o.origin);
    assert_eq!(o.view_height, 60.0);
    assert_eq!(o.stance, Stance::Stand);
}

#[test]
fn runs_at_190_units_per_second_facing_its_yaw() {
    let w = floor();
    let mut p = settled(&w);
    let outs = run(&mut p, &w, MoveInput { msec: MS, forward: 127, ..Default::default() }, 90);
    let o = outs.last().unwrap();
    let speed = horizontal(o.velocity);
    assert!((speed - 190.0).abs() < 3.0, "retail g_speed 190: {speed}");
    // Yaw 0 looks along +X.
    assert!(o.velocity[0] > 180.0 && o.velocity[1].abs() < 1.0, "{:?}", o.velocity);
    assert!(o.origin[0] > 200.0);
}

#[test]
fn backpedal_and_strafe_use_retail_scales() {
    let w = floor();
    let mut back = settled(&w);
    let b = *run(&mut back, &w, MoveInput { msec: MS, forward: -127, ..Default::default() }, 90).last().unwrap();
    let mut side = settled(&w);
    let s = *run(&mut side, &w, MoveInput { msec: MS, right: 127, ..Default::default() }, 90).last().unwrap();
    let (bs, ss) = (horizontal(b.velocity), horizontal(s.velocity));
    assert!((bs - 190.0 * 0.7).abs() < 4.0, "back 0.7x: {bs}");
    assert!((ss - 190.0 * 0.8).abs() < 4.0, "strafe 0.8x: {ss}");
    assert!(b.velocity[0] < 0.0 && s.velocity[1] < 0.0, "back is -X, right is -Y when facing +X");
}

#[test]
fn sprint_is_faster_and_runs_out_after_four_seconds() {
    let w = floor();
    let mut p = settled(&w);
    let input = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let outs = run(&mut p, &w, input, 60);
    let o = outs.last().unwrap();
    assert!(o.sprinting, "{o:?}");
    let speed = horizontal(o.velocity);
    assert!(speed > 250.0, "sprint scale 1.5: {speed}");
    // 4 s of sprint at most.
    let outs = run(&mut p, &w, input, 250);
    assert!(!outs.last().unwrap().sprinting, "the sprint meter runs out");
}

/// Sprint over aim: the aim held, a sprint press sprints (MW2 kept walking); the aim is back once the
/// sprint ends with the button still held.
#[test]
fn a_sprint_pressed_while_aiming_wins_and_the_aim_comes_back_after() {
    let w = floor();
    let mut p = settled(&w);
    let aim = MoveInput { msec: MS, forward: 127, buttons: button::ADS, ..Default::default() };
    run(&mut p, &w, aim, 20);
    assert!(!p.sprint_over_ads);
    let both = MoveInput { buttons: button::ADS | button::SPRINT, ..aim };
    let o = *run(&mut p, &w, both, 40).last().unwrap();
    assert!(o.sprinting, "the sprint wins over the aim: {o:?}");
    assert!(p.sprint_over_ads);
    // Stop running, the aim still held: the sprint ends and the aim is back.
    let still = MoveInput { forward: 0, buttons: button::ADS, ..aim };
    let o = *run(&mut p, &w, still, 40).last().unwrap();
    assert!(!o.sprinting && !p.sprint_over_ads, "aiming again: {o:?}");
    // Let go of the aim mid-sprint and press it again: MW2's, the aim ends the sprint.
    run(&mut p, &w, both, 40);
    run(&mut p, &w, MoveInput { buttons: button::SPRINT, ..aim }, 5);
    let o = *run(&mut p, &w, both, 10).last().unwrap();
    assert!(!p.sprint_over_ads && !o.sprinting, "a fresh aim press ends the sprint: {o:?}");
}

#[test]
fn sprint_meter_drains_and_recharges_and_the_speed_scale_applies() {
    let w = floor();
    let mut p = settled(&w);
    assert!((p.sprint_left() - 1.0).abs() < 1e-3);
    let input = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let base = horizontal(run(&mut p, &w, input, 60).last().unwrap().velocity);
    let left = p.sprint_left();
    assert!(left < 0.9 && left > 0.5, "a second of sprint used: {left}");
    run(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 300);
    assert!(p.sprint_left() > left, "recharges");
    let mut q = settled(&w);
    q.sprint_speed_scale = 1.65;
    let fast = horizontal(run(&mut q, &w, input, 60).last().unwrap().velocity);
    assert!(fast > base * 1.05, "{fast} vs {base}");
}

#[test]
fn a_sprint_jump_keeps_its_speed_on_landing_unless_mw2s_slowdown_is_on() {
    let w = floor();
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let landing = |slowdown: bool| {
        let mut p = settled(&w);
        p.jump_slowdown = slowdown;
        run(&mut p, &w, sprint, 40);
        let before = horizontal(p.peek().velocity);
        p.step(&MoveInput { buttons: button::SPRINT | button::JUMP, ..sprint }, &w);
        let mut min = f32::MAX;
        let mut landed = false;
        for _ in 0..80 {
            let o = p.step(&sprint, &w);
            landed |= o.on_ground;
            if landed {
                min = min.min(horizontal(o.velocity));
            }
        }
        (before, min)
    };
    let (before, kept) = landing(false);
    assert!(kept > before * 0.95, "{kept} of {before}");
    let (before, mw2) = landing(true);
    assert!(mw2 < before * 0.7, "MW2's slowdown: {mw2} of {before}");
}

#[test]
fn jumps_39_units_and_lands() {
    let w = floor();
    let mut p = settled(&w);
    let mut peak: f32 = 0.0;
    let mut left_ground = false;
    let mut input = MoveInput { msec: MS, buttons: button::JUMP, ..Default::default() };
    for f in 0..120 {
        if f == 2 {
            input.buttons = 0;
        }
        let o = p.step(&input, &w);
        peak = peak.max(o.origin[2]);
        left_ground |= !o.on_ground;
    }
    assert!(left_ground);
    assert!((peak - 39.0).abs() < 3.0, "retail jump_height 39: {peak}");
    let o = p.step(&MoveInput { msec: MS, ..Default::default() }, &w);
    assert!(o.on_ground && o.origin[2].abs() < 0.5);
}

#[test]
fn crouch_and_prone_lower_the_eye_and_slow_the_walk() {
    let w = floor();
    let mut p = settled(&w);
    let crouch = MoveInput { msec: MS, buttons: button::CROUCH, ..Default::default() };
    let o = *run(&mut p, &w, crouch, 40).last().unwrap();
    assert_eq!(o.stance, Stance::Crouch);
    assert!((o.view_height - 40.0).abs() < 0.5, "crouch eye 40: {}", o.view_height);
    let walk = *run(&mut p, &w, MoveInput { forward: 127, ..crouch }, 60).last().unwrap();
    assert!(horizontal(walk.velocity) < 150.0, "crouched walk is slower");

    let mut q = settled(&w);
    let prone = MoveInput { msec: MS, buttons: button::PRONE, ..Default::default() };
    let o = *run(&mut q, &w, prone, 90).last().unwrap();
    assert_eq!(o.stance, Stance::Prone);
    assert!((o.view_height - 11.0).abs() < 0.5, "prone eye 11: {}", o.view_height);
}

#[test]
fn walls_stop_and_steps_are_climbed() {
    let mut w = floor();
    w.add(SolidBox { mins: [200.0, -500.0, 0.0], maxs: [264.0, 500.0, 200.0] });
    let mut p = settled(&w);
    let o = *run(&mut p, &w, MoveInput { msec: MS, forward: 127, ..Default::default() }, 150).last().unwrap();
    assert!(o.origin[0] < 186.0 && o.origin[0] > 180.0, "stops at the wall minus the 15-unit hull: {:?}", o.origin);

    let mut s = floor();
    s.add(SolidBox { mins: [100.0, -500.0, 0.0], maxs: [600.0, 500.0, 16.0] });
    let mut q = settled(&s);
    let o = *run(&mut q, &s, MoveInput { msec: MS, forward: 127, ..Default::default() }, 90).last().unwrap();
    assert!(o.origin[0] > 200.0 && (o.origin[2] - 16.0).abs() < 0.5, "walks up a 16-unit step: {:?}", o.origin);
}

#[test]
fn mouse_turns_the_view_and_the_run() {
    let w = floor();
    let mut p = settled(&w);
    p.step(&MoveInput { msec: MS, yaw_delta: 90.0, pitch_delta: 20.0, ..Default::default() }, &w);
    let o = *run(&mut p, &w, MoveInput { msec: MS, forward: 127, ..Default::default() }, 90).last().unwrap();
    assert!((o.view_angles[1] - 90.0).abs() < 0.1 && (o.view_angles[0] - 20.0).abs() < 0.1, "{:?}", o.view_angles);
    assert!(o.velocity[1] > 180.0, "yaw 90 runs along +Y: {:?}", o.velocity);
    p.step(&MoveInput { msec: MS, pitch_delta: 120.0, ..Default::default() }, &w);
    let o = p.step(&MoveInput { msec: MS, ..Default::default() }, &w);
    assert!(o.view_angles[0] <= 85.01, "pitch clamps at 85: {:?}", o.view_angles);
}

#[test]
fn falls_from_a_ledge_and_takes_retail_fall_damage_from_high_up() {
    let w = floor();
    let mut p = Mw2Player::new([0.0, 0.0, 600.0], 0.0);
    let mut damage = 0;
    for _ in 0..200 {
        damage += p.step(&MoveInput { msec: MS, ..Default::default() }, &w).fall_damage;
    }
    assert!(damage > 0, "a 600-unit fall hurts");
    let mut q = Mw2Player::new([0.0, 0.0, 60.0], 0.0);
    let mut small = 0;
    for _ in 0..100 {
        small += q.step(&MoveInput { msec: MS, ..Default::default() }, &w).fall_damage;
    }
    assert_eq!(small, 0, "a short drop does not");
}

#[test]
fn lighter_fall_damage_hurts_only_from_higher_up() {
    let w = floor();
    // CODV's light setting: from 6 m (236 units) to death at 20 m (787 units).
    let light = Some((236.0, 787.0));
    let fall = |from: f32, heights: Option<(f32, f32)>| {
        let mut p = Mw2Player::new([0.0, 0.0, from], 0.0);
        p.fall_heights = heights;
        (0..300).map(|_| p.step(&MoveInput { msec: MS, ..Default::default() }, &w).fall_damage).sum::<i32>()
    };
    assert!(fall(200.0, Some(MW2_FALL_HEIGHTS)) > 0, "MW2: a 5 m drop hurts");
    assert_eq!(fall(200.0, light), 0, "light: it does not");
    let d = fall(500.0, light);
    assert!((40..=60).contains(&d), "light: a 12.7 m drop takes about half: {d}");
    assert_eq!(fall(900.0, light), 100);
    assert_eq!(fall(900.0, None), 0, "off");
    assert_eq!(fall_damage(511.5, (236.0, 787.0)), 50);
}

#[test]
fn crouch_while_sprinting_slides_faster_then_slows_and_jump_ends_it() {
    let w = floor();
    let mut p = settled(&w);
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    run(&mut p, &w, sprint, 40);
    let crouch = MoveInput { buttons: button::CROUCH | button::SPRINT, ..sprint };
    let first = p.step(&crouch, &w);
    assert!(first.sliding, "{first:?}");
    let fast = horizontal(first.velocity);
    assert!(fast > 300.0, "faster than the sprint: {fast}");
    let mut last = first;
    let mut steps = 1;
    while last.sliding && steps < 100 {
        last = p.step(&crouch, &w);
        steps += 1;
    }
    let ms = steps * MS as i32;
    assert!((SLIDE_MS - 50..=SLIDE_MS + 50).contains(&ms), "lasts its time: {ms}");
    assert_eq!(last.stance, Stance::Crouch);
    // No new slide until crouch is pressed again (from a sprint).
    assert!(!p.step(&crouch, &w).sliding);

    // A jump out of a slide: crouch released (the host stands up) and jump pressed.
    let mut q = settled(&w);
    run(&mut q, &w, sprint, 40);
    assert!(q.step(&crouch, &w).sliding);
    run(&mut q, &w, crouch, 10);
    let jump = MoveInput { buttons: button::JUMP, ..sprint };
    let mut rose = false;
    for _ in 0..10 {
        let o = q.step(&jump, &w);
        assert!(!o.sliding);
        rose |= o.velocity[2] > 50.0;
    }
    assert!(rose, "jumps out of the slide");

    // Off: crouch just crouches.
    let mut r = settled(&w);
    r.slide_enabled = false;
    run(&mut r, &w, sprint, 40);
    assert!(!r.step(&crouch, &w).sliding);
}

#[test]
fn weapon_crate_links_in() {
    // F0 only proves weapon_iw4 builds into the runtime; F3 drives it.
    assert_eq!(weapon_iw4::recoil_ads_weight(0.0), weapon_iw4::recoil_ads_weight(0.0));
}

#[test]
fn retail_button_bits_match_the_host_header() {
    // host/include/codv_runtime.h CvMw2Button mirrors these.
    assert_eq!(
        [
            button::ATTACK,
            button::SPRINT,
            button::MELEE_CHARGE,
            button::RELOAD,
            button::PRONE,
            button::CROUCH,
            button::JUMP,
            button::ADS
        ],
        [0x1, 0x2, 0x4, 0x10, 0x100, 0x200, 0x400, 0x800]
    );
}

/// A closed box as triangles (GTA's collision arrives as triangle soup).
fn cube(lo: [f32; 3], hi: [f32; 3]) -> Vec<crate::mesh::Triangle> {
    let c = |i: usize| {
        [
            if i & 1 != 0 { hi[0] } else { lo[0] },
            if i & 2 != 0 { hi[1] } else { lo[1] },
            if i & 4 != 0 { hi[2] } else { lo[2] },
        ]
    };
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let mut out = Vec::new();
    for q in quads {
        out.push([c(q[0]), c(q[1]), c(q[2])]);
        out.push([c(q[0]), c(q[2]), c(q[3])]);
    }
    out
}

fn mesh_with(block_lo: [f32; 3], block_hi: [f32; 3]) -> crate::mesh::MeshWorld {
    let mut tris = cube([-2000.0, -2000.0, -64.0], [2000.0, 2000.0, 0.0]);
    tris.extend(cube(block_lo, block_hi));
    crate::mesh::MeshWorld::build(&tris)
}

#[test]
fn a_hull_overlapping_a_wall_is_freed_and_moves_again() {
    // The hull (15 units each way) reaches into the wall's face at x = 20.
    let w = mesh_with([20.0, -100.0, 0.0], [60.0, 100.0, 100.0]);
    for x in [8.0f32, 30.0] {
        let mut p = Mw2Player::new([x, 0.0, 0.0], 0.0);
        let mut last = p.peek();
        for _ in 0..40 {
            last = p.step(&MoveInput { msec: MS, forward: -127, ..Default::default() }, &w);
        }
        assert!(p.unstuck >= 1, "freed at x {x}");
        assert!(last.origin[0] < -40.0, "walks away from the wall (from x {x}): {:?}", last.origin);
    }
}

fn jump_at(w: &dyn World, frames: usize) -> (MoveOutput, bool) {
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    for _ in 0..30 {
        p.step(&MoveInput { msec: MS, ..Default::default() }, w);
    }
    let mut mantled = false;
    let mut last = p.peek();
    for i in 0..frames {
        // Walk to the wall, then hold jump.
        let buttons = if i > 20 { button::JUMP } else { 0 };
        last = p.step(&MoveInput { msec: MS, forward: 127, buttons, ..Default::default() }, w);
        mantled |= last.mantling;
    }
    (last, mantled)
}

#[test]
fn sprinting_into_a_low_block_vaults_it_without_the_jump_key() {
    let mut w = floor();
    w.add(SolidBox { mins: [120.0, -200.0, 0.0], maxs: [400.0, 200.0, 40.0] });
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let run_in = |auto: bool, speed: f32| {
        let mut p = settled(&w);
        p.auto_vault = auto;
        p.mantle_speed = speed;
        let mut mantle_frames = 0;
        let mut last = p.peek();
        for _ in 0..120 {
            last = p.step(&sprint, &w);
            mantle_frames += usize::from(last.mantling);
        }
        (last, mantle_frames)
    };
    let (o, frames) = run_in(true, 1.0);
    assert!(frames > 0 && (o.origin[2] - 40.0).abs() < 1.0, "vaulted onto it: {:?}", o.origin);
    // CODV's vault keeps the run's speed over the ledge (MW2 stops dead and starts again).
    let mut p = settled(&w);
    p.auto_vault = true;
    p.mantle_momentum = true;
    p.mantle_speed = 1.8;
    let mut was = false;
    let mut after = None;
    for _ in 0..120 {
        let o = p.step(&sprint, &w);
        if was && !o.mantling && after.is_none() {
            after = Some(horizontal(o.velocity));
        }
        was = o.mantling;
    }
    let after = after.expect("vaulted");
    assert!(after > 250.0, "on at sprint speed: {after}");
    let (_, quick) = run_in(true, 1.5);
    assert!(quick < frames, "quicker: {quick} vs {frames} frames");
    let (o, frames) = run_in(false, 1.0);
    assert!(frames == 0 && o.origin[2] < 1.0, "MW2: needs the jump key: {:?}", o.origin);
}

#[test]
fn a_jump_at_a_ledge_above_mw2s_mantle_climbs_it_with_the_high_climb() {
    let mut w = floor();
    w.add(SolidBox { mins: [80.0, -200.0, 0.0], maxs: [400.0, 200.0, 80.0] });
    let attempt = |high: bool| {
        let mut p = settled(&w);
        p.high_climb = high;
        let mut climbed = false;
        let mut last = p.peek();
        for i in 0..120 {
            let buttons = if i == 25 { button::JUMP } else { 0 };
            last = p.step(&MoveInput { msec: MS, forward: 127, buttons, ..Default::default() }, &w);
            climbed |= p.climbing();
        }
        (last, climbed)
    };
    let (o, climbed) = attempt(true);
    assert!(climbed && (o.origin[2] - 80.0).abs() < 1.5 && o.origin[0] > 95.0, "on the 2 m ledge: {:?}", o.origin);
    let (o, climbed) = attempt(false);
    assert!(!climbed && o.origin[2] < 1.0, "MW2 alone: no: {:?}", o.origin);
    // GTA's collision is triangles: the same block as a mesh.
    let mesh = mesh_with([80.0, -200.0, 0.0], [400.0, 200.0, 80.0]);
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    p.high_climb = true;
    run_dyn(&mut p, &mesh, MoveInput { msec: MS, ..Default::default() }, 30);
    run_dyn(&mut p, &mesh, MoveInput { msec: MS, forward: 127, ..Default::default() }, 25);
    p.step(&MoveInput { msec: MS, forward: 127, buttons: button::JUMP, ..Default::default() }, &mesh);
    assert!(p.climbing(), "climbs the mesh too");
    run_dyn(&mut p, &mesh, MoveInput { msec: MS, forward: 127, ..Default::default() }, 60);
    assert!((p.peek().origin[2] - 80.0).abs() < 1.5, "{:?}", p.peek().origin);
    // Too high: no climb.
    let mut tall = floor();
    tall.add(SolidBox { mins: [80.0, -200.0, 0.0], maxs: [400.0, 200.0, 140.0] });
    let mut p = settled(&tall);
    p.high_climb = true;
    run(&mut p, &tall, MoveInput { msec: MS, forward: 127, ..Default::default() }, 25);
    p.step(&MoveInput { msec: MS, forward: 127, buttons: button::JUMP, ..Default::default() }, &tall);
    assert!(!p.climbing());
}

#[test]
fn holding_crouch_through_a_slide_dives_and_lands_prone() {
    let w = floor();
    let mut p = settled(&w);
    p.dive_enabled = true;
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    run(&mut p, &w, sprint, 40);
    assert!(p.step(&MoveInput { buttons: button::CROUCH | button::SPRINT, ..sprint }, &w).sliding);
    let o = p.step(&MoveInput { buttons: button::PRONE, ..sprint }, &w);
    assert!(!o.on_ground && o.velocity[2] > 100.0 && horizontal(o.velocity) >= 300.0, "{o:?}");
    let prone = MoveInput { buttons: button::PRONE, ..sprint };
    let mut last = o;
    for _ in 0..80 {
        last = p.step(&prone, &w);
    }
    assert!(last.on_ground && last.stance == Stance::Prone, "{last:?}");
}

#[test]
fn a_drop_light_fall_damage_spares_lands_without_mw2s_hard_landing() {
    let w = floor();
    let mut p = Mw2Player::new([0.0, 0.0, 200.0], 0.0);
    p.fall_heights = Some((236.0, 787.0));
    let mut hurt = 0;
    let mut landed = false;
    for _ in 0..80 {
        let o = p.step(&MoveInput { msec: MS, ..Default::default() }, &w);
        hurt += o.fall_damage;
        if o.on_ground && !landed {
            landed = true;
            assert_eq!(p.ps.pm_flags & pm_flags::TIME_HARDLANDING, 0, "no hard-landing slowdown");
        }
    }
    assert!(landed);
    assert_eq!(hurt, 0);
    // MW2's heights: the same drop hurts and slows.
    let mut q = Mw2Player::new([0.0, 0.0, 200.0], 0.0);
    let mut slowed = false;
    for _ in 0..80 {
        let o = q.step(&MoveInput { msec: MS, ..Default::default() }, &w);
        slowed |= o.on_ground && q.ps.pm_flags & pm_flags::TIME_HARDLANDING != 0;
    }
    assert!(slowed);
}

#[test]
fn jumping_at_a_wall_climbs_onto_it() {
    // A car-high block (40 units = 1 m) and a chest-high wall (56 units), as boxes and as triangles.
    for h in [40.0f32, 56.0] {
        let mut boxes = floor();
        boxes.add(SolidBox { mins: [60.0, -200.0, 0.0], maxs: [400.0, 200.0, h] });
        let mesh = mesh_with([60.0, -200.0, 0.0], [400.0, 200.0, h]);
        for (name, w) in [("boxes", &boxes as &dyn World), ("mesh", &mesh as &dyn World)] {
            let (o, mantled) = jump_at(w, 120);
            assert!(mantled, "{name} {h}: mantles");
            assert!(o.origin[0] > 75.0 && (o.origin[2] - h).abs() < 1.0, "{name} {h}: on top: {:?}", o.origin);
        }
    }
}

#[test]
fn walking_into_a_wall_without_jumping_does_not_climb() {
    let mesh = mesh_with([60.0, -200.0, 0.0], [400.0, 200.0, 40.0]);
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    let mut last = p.peek();
    for _ in 0..120 {
        last = p.step(&MoveInput { msec: MS, forward: 127, ..Default::default() }, &mesh);
        assert!(!last.mantling);
    }
    assert!(last.origin[0] < 46.0 && last.origin[2].abs() < 1.0, "{:?}", last.origin);
}

#[test]
fn a_wall_too_high_is_not_climbed() {
    let mesh = mesh_with([60.0, -200.0, 0.0], [400.0, 200.0, 200.0]);
    let (o, mantled) = jump_at(&mesh, 120);
    assert!(!mantled && o.origin[0] < 46.0, "{o:?}");
}

#[test]
fn a_climb_that_ends_under_something_crouches_instead_of_going_back() {
    // A 40-unit ledge with a beam over it: room for a crouched hull (50) on top, not a standing one (70).
    let mut tris = cube([-2000.0, -2000.0, -64.0], [2000.0, 2000.0, 0.0]);
    tris.extend(cube([60.0, -200.0, 0.0], [400.0, 200.0, 40.0]));
    tris.extend(cube([70.0, -200.0, 96.0], [400.0, 200.0, 110.0]));
    let w = crate::mesh::MeshWorld::build(&tris);
    let (o, mantled) = jump_at(&w, 150);
    assert!(mantled);
    assert!(o.origin[0] > 75.0 && (o.origin[2] - 40.0).abs() < 1.0, "stays on the ledge: {:?}", o.origin);
    assert_eq!(o.stance, Stance::Crouch);
}

#[test]
fn a_vault_over_a_railing_lands_beyond_it() {
    let w = mesh_with([60.0, -200.0, 0.0], [64.0, 200.0, 40.0]);
    let (o, mantled) = jump_at(&w, 150);
    assert!(mantled && o.origin[0] > 120.0 && o.origin[2].abs() < 1.0, "{o:?}");
}

#[test]
fn the_jump_height_can_be_raised() {
    let w = floor();
    let peak = |height: f32| {
        let mut p = settled(&w);
        p.jump_height = height;
        let mut top = 0.0f32;
        for i in 0..90 {
            let buttons = if i < 3 { button::JUMP } else { 0 };
            top = top.max(p.step(&MoveInput { msec: MS, buttons, ..Default::default() }, &w).origin[2]);
        }
        top
    };
    let mw2 = peak(MW2_JUMP_HEIGHT);
    let higher = peak(46.0);
    assert!((mw2 - 39.0).abs() < 1.5, "{mw2}");
    assert!((higher - 46.0).abs() < 1.5, "{higher}");
}

#[test]
fn the_sprint_can_last_longer() {
    let w = floor();
    let sprint_ms = |seconds: f32| {
        let mut p = settled(&w);
        p.sprint_seconds = seconds;
        let mut t = 0;
        for i in 0..1500 {
            let o = p.step(&MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() }, &w);
            if o.sprinting {
                t = i;
            } else if i > 20 && t > 0 {
                break;
            }
        }
        t as u32 * MS
    };
    let mw2 = sprint_ms(MW2_SPRINT_SECONDS);
    let doubled = sprint_ms(8.0);
    assert!((3800..=4200).contains(&mw2), "{mw2}");
    assert!((7800..=8200).contains(&doubled), "{doubled}");
}

fn rot_box(c: [f32; 2], half: [f32; 2], h: f32, deg: f32) -> Vec<crate::mesh::Triangle> {
    let (s, co) = deg.to_radians().sin_cos();
    cube([-half[0], -half[1], 0.0], [half[0], half[1], h])
        .into_iter()
        .map(|t| t.map(|v| [c[0] + v[0] * co - v[1] * s, c[1] + v[0] * s + v[1] * co, v[2]]))
        .collect()
}

/// Sprints at the corner of a car-sized block and of a fence turned at various angles (GTA's
/// collision is triangles): the longest run of steps with the player held still, not mantling.
fn longest_hold(half: [f32; 2], h: f32, deg: f32, off: f32, guard: bool) -> usize {
    let mut tris = cube([-3000.0, -3000.0, -64.0], [3000.0, 3000.0, 0.0]);
    let (s, co) = deg.to_radians().sin_cos();
    let cx = 300.0 - (half[0] * co - half[1] * s);
    let cy = off - (half[0] * s + half[1] * co);
    tris.extend(rot_box([cx, cy], half, h, deg + 180.0));
    let w = crate::mesh::MeshWorld::build(&tris);
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    p.auto_vault = true;
    p.mantle_momentum = true;
    p.mantle_speed = 1.8;
    p.snag_guard = guard;
    run_dyn(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 20);
    let (mut held, mut worst) = (0, 0);
    for _ in 0..150 {
        let o = p.step(&MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() }, &w);
        if horizontal(o.velocity) < 40.0 && !o.mantling {
            held += 1;
            worst = worst.max(held);
        } else {
            held = 0;
        }
    }
    worst
}

#[test]
fn the_snag_guard_keeps_a_run_going_past_car_and_fence_corners() {
    for (half, h) in [([90.0f32, 40.0], 56.0f32), ([120.0, 2.0], 44.0)] {
        for deg in [20.0f32, 37.0, 45.0, 63.0] {
            for off in [-20.0f32, -10.0, 10.0, 20.0, 30.0] {
                let held = longest_hold(half, h, deg, off, true);
                assert!(held <= 3, "{half:?} at {deg} deg, offset {off}: held {held} steps");
                // Without the guard too: these catches came from the auto vault trying a ledge met
                // at a slant, which it no longer does.
                let bare = longest_hold(half, h, deg, off, false);
                assert!(bare <= 3, "{half:?} at {deg} deg, offset {off}, no guard: held {bare} steps");
            }
        }
    }
    // Head on, a wall still stops the player.
    let w = mesh_with([300.0, -500.0, 0.0], [340.0, 500.0, 200.0]);
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    p.snag_guard = true;
    run_dyn(&mut p, &w, MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() }, 150);
    assert!(p.peek().origin[0] < 290.0 && horizontal(p.peek().velocity) < 5.0);
}

#[test]
fn movement_sounds_come_from_steps_landings_slides_and_climbs() {
    let mut w = floor();
    w.add(SolidBox { mins: [600.0, -200.0, 0.0], maxs: [900.0, 200.0, 80.0] });
    let mut p = settled(&w);
    p.take_foley();
    p.high_climb = true;
    p.jump_slowdown = false;
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    run(&mut p, &w, sprint, 40);
    let f = p.take_foley();
    assert!(f.contains(&Foley::Step(StepKind::Sprint)), "{f:?}");
    p.step(&MoveInput { buttons: button::SPRINT | button::JUMP, ..sprint }, &w);
    run(&mut p, &w, sprint, 40);
    assert!(p.take_foley().contains(&Foley::Land { hard: false }));
    let o = p.step(&MoveInput { buttons: button::SPRINT | button::CROUCH, ..sprint }, &w);
    let f = p.take_foley();
    assert!(f.contains(&Foley::Slide), "{f:?} {o:?}");
    // On to the 2 m block (x 600): a jump there climbs it.
    let walk = MoveInput { msec: MS, forward: 127, ..Default::default() };
    for _ in 0..200 {
        p.step(&walk, &w);
        if p.peek().origin[0] > 560.0 {
            break;
        }
    }
    p.take_foley();
    run(&mut p, &w, walk, 10);
    p.step(&MoveInput { buttons: button::JUMP, ..walk }, &w);
    assert!(p.climbing() && p.take_foley().contains(&Foley::Mantle));
}

#[test]
fn the_slide_and_the_climb_take_their_tuning() {
    let w = floor();
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let slide = |boost: f32, ms: i32| {
        let mut p = settled(&w);
        p.slide_boost = boost;
        p.slide_ms = ms;
        run(&mut p, &w, sprint, 40);
        let first = p.step(&MoveInput { buttons: button::CROUCH | button::SPRINT, ..sprint }, &w);
        let mut n = 1;
        while p.step(&MoveInput { buttons: button::CROUCH | button::SPRINT, ..sprint }, &w).sliding && n < 300 {
            n += 1;
        }
        (horizontal(first.velocity), n)
    };
    let (fast, long) = slide(1.6, 1500);
    let (normal, short) = slide(1.3, 750);
    assert!(fast > normal * 1.1 && long > short * 3 / 2, "{fast} {normal} {long} {short}");
    // A 2 m ledge is past a 1.8 m climb limit.
    let mut wall = floor();
    wall.add(SolidBox { mins: [80.0, -200.0, 0.0], maxs: [400.0, 200.0, 80.0] });
    let mut p = settled(&wall);
    p.high_climb = true;
    p.climb_max = 70.0;
    run(&mut p, &wall, MoveInput { msec: MS, forward: 127, ..Default::default() }, 25);
    p.step(&MoveInput { msec: MS, forward: 127, buttons: button::JUMP, ..Default::default() }, &wall);
    assert!(!p.climbing());
}

#[test]
fn leaning_moves_the_eye_sideways_short_of_a_wall_and_slows_the_walk() {
    use crate::player::{LEAN_LEFT, LEAN_RIGHT};
    // A crate ahead (cover within reach) to lean out from.
    let mut w = floor();
    w.add(SolidBox { mins: [30.0, -10.0, 0.0], maxs: [60.0, 10.0, 80.0] });
    let mut p = settled(&w);
    p.lean_enabled = true;
    // Facing +X (yaw 0): right is -Y.
    let right = MoveInput { msec: MS, buttons: LEAN_RIGHT, ..Default::default() };
    let o = *run(&mut p, &w, right, 20).last().unwrap();
    assert!((o.lean_offset[1] + LEAN_DIST).abs() < 0.5 && o.lean_offset[0].abs() < 0.5, "{:?}", o.lean_offset);
    // The head swings about the hips: a little lower, the view rolled the same way.
    assert!(o.lean_offset[2] < -1.0 && o.lean_offset[2] > -6.0, "{:?}", o.lean_offset);
    assert!(o.lean_roll > 8.0 && o.lean_roll < 20.0, "{}", o.lean_roll);
    let o = *run(&mut p, &w, MoveInput { msec: MS, buttons: LEAN_LEFT, ..Default::default() }, 30).last().unwrap();
    assert!((o.lean_offset[1] - LEAN_DIST).abs() < 0.5 && o.lean_roll < -8.0, "{:?}", o.lean_offset);
    // Back upright with neither held.
    let o = *run(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 20).last().unwrap();
    assert_eq!(o.lean_offset, [0.0; 3]);
    // Leaning, the run slows to MW2's lean speed and no sprint starts (strafing past the crate).
    run(&mut p, &w, right, 20);
    let lean_run = MoveInput { msec: MS, right: -127, buttons: LEAN_RIGHT | button::SPRINT, ..Default::default() };
    let o = *run(&mut p, &w, lean_run, 40).last().unwrap();
    assert!(!o.sprinting && o.lean_offset != [0.0; 3] && horizontal(o.velocity) < 150.0, "{o:?}");
    // In the open a lean does not start.
    let open = floor();
    let mut alone = settled(&open);
    alone.lean_enabled = true;
    let o = *run(&mut alone, &open, right, 20).last().unwrap();
    assert_eq!(o.lean_offset, [0.0; 3]);
    // A wall 3 units past the hull on the right: the eye stops short of it.
    let mut walled = floor();
    walled.add(SolidBox { mins: [-200.0, -40.0, 0.0], maxs: [200.0, -18.0, 200.0] });
    let mut q = settled(&walled);
    q.lean_enabled = true;
    let o = *run(&mut q, &walled, right, 20).last().unwrap();
    assert!(o.lean_offset[1] > -18.0 + 5.5 && o.lean_offset[1] < -1.0, "{:?}", o.lean_offset);
    // Off: nothing.
    let mut r = settled(&w);
    let o = *run(&mut r, &w, right, 20).last().unwrap();
    assert_eq!(o.lean_offset, [0.0; 3]);
}

/// A car-like block (a 36-unit body, a 58-unit cabin) turned `deg` with its centre `off` across the path.
fn car_world(deg: f32, off: f32) -> crate::mesh::MeshWorld {
    let mut tris = cube([-3000.0, -3000.0, -64.0], [3000.0, 3000.0, 0.0]);
    tris.extend(rot_box([300.0, off], [90.0, 38.0], 36.0, deg));
    let (s, c) = deg.to_radians().sin_cos();
    tris.extend(
        cube([-40.0, -34.0, 36.0], [40.0, 34.0, 58.0])
            .into_iter()
            .map(|t| t.map(|v| [300.0 + v[0] * c - v[1] * s, off + v[0] * s + v[1] * c, v[2]])),
    );
    crate::mesh::MeshWorld::build(&tris)
}

#[test]
fn running_into_a_cars_corner_slips_around_it() {
    for deg in [0.0f32, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0] {
        for off in [-60.0f32, -30.0, 0.0, 30.0, 60.0] {
            for sprint in [false, true] {
                let w = car_world(deg, off);
                let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
                p.auto_vault = true;
                p.mantle_momentum = true;
                p.mantle_speed = 1.8;
                p.snag_guard = true;
                p.jump_slowdown = false;
                run_dyn(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 20);
                let buttons = if sprint { button::SPRINT } else { 0 };
                let input = MoveInput { msec: MS, forward: 127, buttons, ..Default::default() };
                let (mut held, mut worst) = (0, 0);
                for _ in 0..200 {
                    let o = p.step(&input, &w);
                    if horizontal(o.velocity) < 30.0 && !o.mantling {
                        held += 1;
                        worst = worst.max(held);
                    } else {
                        held = 0;
                    }
                }
                // Jogging (no auto vault) straight into the body stops the player, as a wall does;
                // only its corners have to be slipped around. Sprinting vaults the mesh block.
                let (s, c) = deg.to_radians().sin_cos();
                let square = (0..300).any(|i| {
                    let (dx, dy) = (i as f32 * 2.0 - 300.0, -off);
                    (dx * c + dy * s).abs() <= 90.0 && (-dx * s + dy * c).abs() <= 38.0
                });
                if sprint || !square {
                    assert!(worst <= 6, "car at {deg} deg, offset {off}, sprint {sprint}: held {worst} steps");
                }
            }
        }
    }
}

/// CODV's held sprint: aiming ends the sprint, and with the key still down it comes back by
/// itself once the aim is let go (MW2 waits for a fresh press: `hold_to_sprint` off).
#[test]
fn a_held_sprint_key_resumes_the_sprint_after_whatever_ended_it() {
    let w = floor();
    for hold in [true, false] {
        let mut p = settled(&w);
        p.hold_to_sprint = hold;
        let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
        assert!(run(&mut p, &w, sprint, 20).last().unwrap().sprinting);
        let aim = MoveInput { buttons: button::SPRINT | button::ADS, ..sprint };
        assert!(!run(&mut p, &w, aim, 10).last().unwrap().sprinting, "aiming ends it");
        let again = run(&mut p, &w, sprint, 20).last().unwrap().sprinting;
        assert_eq!(again, hold, "hold_to_sprint {hold}");
    }
}

/// A world whose boxes are vehicles (movers) for the auto vault's question.
struct CarWorld(BoxWorld);

impl World for CarWorld {
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> trace_iw4::Trace {
        self.0.trace(start, end, mins, maxs, mask)
    }
    fn mover_hit(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> bool {
        let t = self.0.trace(start, end, mins, maxs, mask);
        // The floor is the world; anything above it is the car.
        (t.fraction < 1.0 || t.startsolid != 0) && t.endpos[2] > 1.0
    }
}

/// Sprinting into a car-high block that is a vehicle: no auto vault (the jump key still climbs it),
/// the same block as part of the world is vaulted.
#[test]
fn the_auto_vault_leaves_vehicles_to_the_jump_key() {
    let mut boxes = floor();
    boxes.add(SolidBox { mins: [120.0, -200.0, 0.0], maxs: [400.0, 200.0, 40.0] });
    let car = CarWorld(boxes);
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let mut p = settled(&car.0);
    p.auto_vault = true;
    let mut mantled = 0;
    for _ in 0..120 {
        mantled += usize::from(p.step(&sprint, &car).mantling);
    }
    assert_eq!(mantled, 0, "no auto vault onto a vehicle");
    // The jump key mantles it as in MW2.
    let jump = MoveInput { buttons: button::SPRINT | button::JUMP, ..sprint };
    let mut climbed = false;
    for _ in 0..90 {
        climbed |= p.step(&jump, &car).mantling;
    }
    assert!(climbed, "the jump key climbs the car");
}

/// The auto vault only takes a sprint head-on into something at least knee high: a jog, a run
/// slanting along a wall or a strafe past it, and a curb stay on the ground.
#[test]
fn the_auto_vault_wants_a_head_on_sprint_at_a_knee_high_obstacle() {
    let mantles = |h: f32, input: MoveInput, yaw: f32| {
        let mut w = floor();
        w.add(SolidBox { mins: [120.0, -400.0, 0.0], maxs: [400.0, 400.0, h] });
        let mut p = Mw2Player::new([0.0, 0.0, 1.0], yaw);
        p.auto_vault = true;
        run(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 20);
        (0..150).filter(|_| p.step(&input, &w).mantling).count()
    };
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let jog = MoveInput { buttons: 0, ..sprint };
    assert!(mantles(40.0, sprint, 0.0) > 0, "head-on sprint vaults");
    assert_eq!(mantles(40.0, jog, 0.0), 0, "a jog does not");
    assert_eq!(mantles(16.0, sprint, 0.0), 0, "a curb is stepped, not vaulted");
    assert_eq!(mantles(40.0, MoveInput { right: 127, ..sprint }, 0.0), 0, "strafing along it does not");
    assert_eq!(mantles(40.0, sprint, 50.0), 0, "a run slanting into the wall does not");
}

/// The auto vault leaves a low step with a wall behind it alone (the knee meets the wall, the
/// mantle would take the step).
#[test]
fn the_auto_vault_skips_a_low_step_before_a_wall() {
    let mut w = floor();
    w.add(SolidBox { mins: [120.0, -300.0, 0.0], maxs: [140.0, 300.0, 16.0] });
    w.add(SolidBox { mins: [150.0, -300.0, 0.0], maxs: [400.0, 300.0, 200.0] });
    let mut p = settled(&w);
    p.auto_vault = true;
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    assert_eq!((0..150).filter(|_| p.step(&sprint, &w).mantling).count(), 0);
}

/// A step (a curb, a stair of GTA's): the feet go up it at once, the eye follows smoothly; on flat
/// ground nothing is added.
#[test]
fn the_eye_follows_a_step_smoothly() {
    let mut w = floor();
    w.add(SolidBox { mins: [100.0, -200.0, 0.0], maxs: [400.0, 200.0, 14.0] });
    let mut p = settled(&w);
    let input = MoveInput { msec: MS, forward: 127, ..Default::default() };
    let eye = |o: &MoveOutput| o.origin[2] + o.view_height;
    let mut last = p.peek();
    let (mut feet_jump, mut eye_jump) = (0.0f32, 0.0f32);
    for _ in 0..60 {
        let o = p.step(&input, &w);
        feet_jump = feet_jump.max(o.origin[2] - last.origin[2]);
        eye_jump = eye_jump.max(eye(&o) - eye(&last));
        last = o;
    }
    assert!(feet_jump > 13.0, "the feet take the step at once: {feet_jump}");
    assert!(eye_jump < 5.0, "the eye does not: {eye_jump}");
    assert!((last.origin[2] - 14.0).abs() < 0.5 && last.view_height == p.ps.view_height_current, "{last:?}");
}

/// A car settling (or nudged) into the player by a hair while it pushes against it: freed by the
/// smallest push, not pulled back to its last free spot or lifted onto the car, frame after frame.
#[test]
fn a_hairs_overlap_with_a_settling_car_frees_by_a_hair() {
    let input = MoveInput { msec: MS, forward: 127, ..Default::default() };
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
    let mut face = 300.0f32;
    let mut w = mesh_with([face, -100.0, 0.0], [face + 160.0, 100.0, 56.0]);
    run_dyn(&mut p, &w, input, 150);
    assert!(p.peek().origin[0] > face - 16.0, "at the car: {:?}", p.peek().origin);
    let mut worst = 0.0f32;
    for _ in 0..30 {
        face -= 0.3;
        w = mesh_with([face, -100.0, 0.0], [face + 160.0, 100.0, 56.0]);
        let before = p.peek().origin;
        let o = p.step(&input, &w);
        let moved = [0, 1, 2].map(|k| o.origin[k] - before[k]);
        worst = worst.max(moved[0].abs().max(moved[1].abs()).max(moved[2]));
    }
    assert!(p.unstuck > 0, "it was overlapped");
    assert!(worst < 2.0, "freed by a hair each time: moved up to {worst} units in a frame");
}

/// A car's body over its wheels (a block whose lower face is off the ground), a table's top, a
/// fence's rail: the retail sweep let the capsule's cylinder into it and the player was freed and lost
/// the ground every frame (the shake against cars). Pushing into it or along it now stops or slides
/// with no hull ever inside.
#[test]
fn a_body_off_the_ground_stops_the_hull_without_letting_it_in() {
    for bottom in [18.0f32, 25.0, 35.0, 45.0] {
        for deg in [0.0f32, 30.0, 60.0] {
            let mut tris = cube([-3000.0, -3000.0, -64.0], [3000.0, 3000.0, 0.0]);
            let body = rot_box([400.0, 0.0], [80.0, 200.0], 1.0, deg);
            tris.extend(body.into_iter().map(|t| t.map(|v| [v[0], v[1], if v[2] > 0.5 { 60.0 } else { bottom }])));
            let w = crate::mesh::MeshWorld::build(&tris);
            let mut p = Mw2Player::new([0.0, 0.0, 1.0], 0.0);
            p.snag_guard = true;
            run_dyn(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 20);
            let input = MoveInput { msec: MS, forward: 127, ..Default::default() };
            let mut air = 0;
            for _ in 0..200 {
                air += usize::from(!p.step(&input, &w).on_ground);
            }
            assert_eq!(p.unstuck, 0, "bottom {bottom} at {deg} deg: freed {} times", p.unstuck);
            assert_eq!(air, 0, "bottom {bottom} at {deg} deg: off the ground {air} frames");
            if deg >= 30.0 {
                assert!(p.peek().origin[1].abs() > 40.0, "slides along it at {deg} deg: {:?}", p.peek().origin);
            }
        }
    }
}

/// Down stairs, a steep ramp or a curb the player stays on the ground (no hop, no landing, no bounce
/// of the view at each step); off a real drop it still falls.
#[test]
fn going_down_steps_stays_on_the_ground_and_a_drop_still_falls() {
    let down = |tris: Vec<crate::mesh::Triangle>, from: [f32; 3], sprint: bool| {
        let w = crate::mesh::MeshWorld::build(&tris);
        let mut p = Mw2Player::new(from, 180.0);
        run_dyn(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 20);
        let buttons = if sprint { button::SPRINT } else { 0 };
        let input = MoveInput { msec: MS, forward: 127, buttons, ..Default::default() };
        let (mut air, mut lands) = (0, 0);
        p.take_foley();
        for _ in 0..240 {
            air += usize::from(!p.step(&input, &w).on_ground);
            lands += p.take_foley().iter().filter(|f| matches!(f, Foley::Land { .. })).count();
        }
        (air, lands, p.peek().origin[2])
    };
    let ground = || cube([-3000.0, -3000.0, -64.0], [3000.0, 3000.0, 0.0]);
    // Twelve 9-unit stairs from x 100 (top at x 232, 108 units up), run down from x 400.
    let mut stairs = ground();
    for i in 0..12 {
        stairs.extend(cube([100.0 + i as f32 * 11.0, -100.0, 0.0], [600.0, 100.0, 9.0 * (i + 1) as f32]));
    }
    // A 40-degree ramp, 84 units high, its top from x 200.
    let mut ramp = ground();
    let len = 84.0 / 40f32.to_radians().tan();
    let (a, b, c, d) =
        ([200.0 - len, -100.0, 0.0], [200.0, -100.0, 84.0], [200.0, 100.0, 84.0], [200.0 - len, 100.0, 0.0]);
    ramp.extend([[a, b, c], [a, c, d]]);
    ramp.extend(cube([200.0, -100.0, 0.0], [600.0, 100.0, 84.0]));
    let mut curb = ground();
    curb.extend(cube([100.0, -200.0, 0.0], [600.0, 200.0, 14.0]));
    for sprint in [false, true] {
        for (name, tris, top) in
            [("stairs", stairs.clone(), 108.0f32), ("ramp", ramp.clone(), 84.0), ("curb", curb.clone(), 14.0)]
        {
            let (air, lands, z) = down(tris, [450.0, 0.0, top + 1.0], sprint);
            assert!(z < 1.0, "{name} sprint {sprint}: down at the bottom ({z})");
            assert!(air <= 1 && lands == 0, "{name} sprint {sprint}: {air} frames off the ground, {lands} landings");
        }
    }
    // A 40-unit ledge: a fall.
    let mut ledge = ground();
    ledge.extend(cube([100.0, -200.0, 0.0], [600.0, 200.0, 40.0]));
    let (air, _, z) = down(ledge, [450.0, 0.0, 41.0], false);
    assert!(air > 3 && z < 1.0, "off a ledge the player falls: {air} frames in the air");
}

/// The ground under a player that just lost it without jumping: within a step it is put on it; a
/// deeper drop, a jump or a steep face lets it fall.
#[test]
fn a_player_that_just_lost_the_ground_is_put_back_on_it_within_a_step() {
    let w = floor();
    let ground = 1022; // any entity number but NO_GROUND
    let lost = |z: f32, jumping: bool| {
        let mut p = Mw2Player::new([0.0, 0.0, z], 0.0);
        p.ps.ground_entity_num = NO_GROUND;
        p.ps.velocity = [190.0, 0.0, -20.0];
        if jumping {
            p.ps.pm_flags |= pm_flags::JUMPING;
        }
        p.stay_on_ground(ground, &w);
        p
    };
    let p = lost(10.0, false);
    assert!(
        p.ps.origin[2].abs() < 0.5 && p.ps.ground_entity_num == ground && p.ps.velocity[2] == 0.0,
        "{:?}",
        p.ps.origin
    );
    assert_eq!(lost(30.0, false).ps.ground_entity_num, NO_GROUND, "a drop past a step falls");
    assert_eq!(lost(10.0, true).ps.ground_entity_num, NO_GROUND, "a jump is left alone");
    let mut q = Mw2Player::new([0.0, 0.0, 10.0], 0.0);
    q.ps.ground_entity_num = NO_GROUND;
    q.stay_on_ground(NO_GROUND, &w);
    assert_eq!(q.ps.ground_entity_num, NO_GROUND, "airborne before: left alone");
}

/// A car's sill or bumper (a low step that is a vehicle): running at it at an angle never takes the
/// player up onto it (up, then off and down: a hop), it slides along; the same step in the map is
/// stepped onto as before.
#[test]
fn no_step_up_onto_a_vehicle_the_run_slides_along_it() {
    let mut boxes = floor();
    boxes.add(SolidBox { mins: [200.0, -400.0, 0.0], maxs: [600.0, 400.0, 14.0] });
    let run = MoveInput { msec: MS, forward: 127, ..Default::default() };
    // At 30 degrees into the step's face.
    let car = CarWorld(boxes.clone());
    let mut p = Mw2Player::new([0.0, 0.0, 1.0], 30.0);
    run_dyn(&mut p, &car, MoveInput { msec: MS, ..Default::default() }, 10);
    let (mut top, mut air) = (0.0f32, 0);
    for _ in 0..150 {
        let o = p.step(&run, &car);
        top = top.max(o.origin[2]);
        air += usize::from(!o.on_ground);
    }
    assert!(top < 1.0 && air == 0, "not up the car's sill: top {top}, {air} frames in the air");
    assert!(p.peek().origin[1] > 100.0, "slid along it: {:?}", p.peek().origin);
    // The same step in the map: onto it.
    let mut q = Mw2Player::new([0.0, 0.0, 1.0], 30.0);
    run_dyn(&mut q, &boxes, MoveInput { msec: MS, ..Default::default() }, 10);
    let top = (0..150).map(|_| q.step(&run, &boxes).origin[2]).fold(0.0f32, f32::max);
    assert!((top - 14.0).abs() < 1.0, "the map's step is stepped onto: {top}");
}

/// Pushing into a car (wheels, a body over them) with the snag guard on: it does not take turns with
/// the car every frame (a shake in place), and the player stays on the ground.
#[test]
fn the_snag_guard_rests_when_it_makes_no_headway() {
    let mut tris = cube([-3000.0, -3000.0, -64.0], [3000.0, 3000.0, 0.0]);
    for (x, y) in [(260.0f32, -40.0f32), (260.0, 40.0), (380.0, -40.0), (380.0, 40.0)] {
        tris.extend(cube([x - 14.0, y - 8.0, 0.0], [x + 14.0, y + 8.0, 26.0]));
    }
    tris.extend(cube([230.0, -50.0, 18.0], [410.0, 50.0, 56.0]));
    let w = crate::mesh::MeshWorld::build(&tris);
    for deg in [0.0f32, 10.0, 25.0] {
        let mut p = Mw2Player::new([0.0, 0.0, 1.0], deg);
        p.snag_guard = true;
        p.jump_slowdown = false;
        run_dyn(&mut p, &w, MoveInput { msec: MS, ..Default::default() }, 10);
        let input = MoveInput { msec: MS, forward: 127, ..Default::default() };
        let mut air = 0;
        for _ in 0..240 {
            air += usize::from(!p.step(&input, &w).on_ground);
        }
        assert!(p.snags < 40, "at {deg} deg: the guard fired {} times in 4 s", p.snags);
        assert_eq!(air, 0, "at {deg} deg: {air} frames off the ground");
        assert_eq!(p.unstuck, 0, "at {deg} deg");
    }
}

/// The sprint meter refills `sprint_recharge` times as fast as MW2's: emptied, back to full in 1 / 1.3 of
/// the time at 1.3, and the next sprint starts sooner.
#[test]
fn the_sprint_refills_faster_with_a_recharge_rate() {
    let w = floor();
    let sprint = MoveInput { msec: MS, forward: 127, buttons: button::SPRINT, ..Default::default() };
    let rest = MoveInput { msec: MS, ..Default::default() };
    let refill = |rate: f32| {
        let mut p = settled(&w);
        p.sprint_recharge = rate;
        // Run it dry (4 s of sprint), then rest until full.
        let mut n = 0;
        while p.sprint_left() > 0.01 && n < 400 {
            p.step(&sprint, &w);
            n += 1;
        }
        assert!(p.sprint_left() <= 0.01, "emptied: {}", p.sprint_left());
        let mut frames = 0;
        while p.sprint_left() < 0.999 && frames < 2000 {
            p.step(&rest, &w);
            frames += 1;
        }
        frames
    };
    let (mw2, fast) = (refill(1.0), refill(1.3));
    let ratio = fast as f32 / mw2 as f32;
    assert!((ratio - 1.0 / 1.3).abs() < 0.05, "refill {fast} vs {mw2} frames: {ratio}");
}
