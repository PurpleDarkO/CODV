use super::*;
use crate::world::{BoxWorld, SolidBox};

fn frag() -> Throw {
    Throw {
        kind: crate::arms::ThrowKind::Frag,
        offhand_class: 1,
        hold_fire_time_ms: 600,
        fuse_time_ms: 3500,
        cook_off_hold: true,
        speed: 940.0,
        speed_up: 0.0,
        speed_forward: 0.0,
        radius: 256.0,
        inner_damage: 150.0,
        outer_damage: 55.0,
    }
}

#[test]
fn launch_follows_the_view_and_the_throwers_motion() {
    let v = launch_velocity(forward([0.0, 90.0, 0.0]), &frag(), [0.0; 3]);
    assert!(v[0].abs() < 1e-3 && (v[1] - 940.0).abs() < 1e-3 && v[2].abs() < 1e-3, "{v:?}");
    // Pitch up 45: half up, half forward.
    let up = launch_velocity(forward([-45.0, 0.0, 0.0]), &frag(), [0.0; 3]);
    assert!((up[0] - up[2]).abs() < 1e-2 && up[2] > 600.0, "{up:?}");
    // Running forward adds speed along the throw.
    let run = launch_velocity(forward([0.0, 0.0, 0.0]), &frag(), [190.0, 0.0, 0.0]);
    assert!((run[0] - 1130.0).abs() < 1e-2, "{run:?}");
}

#[test]
fn bounces_lose_more_speed_head_on() {
    let glancing = bounce([100.0, 0.0, -10.0], [0.0, 0.0, 1.0], 0.5, 0.25);
    let head_on = bounce([0.0, 0.0, -100.0], [0.0, 0.0, 1.0], 0.5, 0.25);
    assert!(glancing[2] > 0.0 && head_on[2] > 0.0);
    assert!((head_on[2] - 25.0).abs() < 1e-3, "{head_on:?}");
    assert!(len(glancing) > 40.0, "{glancing:?}");
}

#[test]
fn a_frag_flies_lands_rests_and_goes_off_at_its_fuse() {
    let world = BoxWorld::floor(0.0, 20000.0);
    let mut g = Grenade::thrown(1, 9, frag(), [0.0, 0.0, 60.0], [-10.0, 0.0, 0.0], [0.0; 3], None);
    assert_eq!(g.fuse_ms, 3500);
    let mut t = 0;
    let mut boom = None;
    let mut highest = 0.0f32;
    while boom.is_none() && t < 10_000 {
        boom = g.step(16, &world);
        highest = highest.max(g.origin[2]);
        assert!(g.origin[2] >= -0.5, "never under the floor: {:?}", g.origin);
        t += 16;
    }
    let e = boom.expect("it went off");
    assert!((3488..=3504).contains(&t), "at the fuse: {t}");
    assert!(g.resting, "came to rest");
    assert!(e.origin[0] > 300.0 && e.origin[2] < 5.0, "thrown forward onto the floor: {:?}", e.origin);
    assert!(highest > 60.0, "arced up");
    assert_eq!((e.radius, e.inner_damage), (256.0, 150.0));
}

#[test]
fn a_cooked_frag_keeps_only_what_is_left_and_walls_send_it_back() {
    let mut world = BoxWorld::floor(0.0, 20000.0);
    world.add(SolidBox { mins: [200.0, -500.0, 0.0], maxs: [240.0, 500.0, 500.0] });
    let mut g = Grenade::thrown(2, 9, frag(), [0.0, 0.0, 60.0], [0.0, 0.0, 0.0], [0.0; 3], Some(1200));
    assert_eq!(g.fuse_ms, 1200);
    for _ in 0..60 {
        if g.step(16, &world).is_some() {
            break;
        }
    }
    assert!(g.origin[0] < 200.0, "stopped by the wall: {:?}", g.origin);
    g.shift([10.0, 0.0, 0.0]);
    assert!(g.origin[0] < 210.0);
}

#[test]
fn a_semtex_sticks_to_the_wall_it_hits_and_goes_off_after_its_fuse() {
    let mut world = BoxWorld::floor(0.0, 20000.0);
    world.add(SolidBox { mins: [200.0, -500.0, 0.0], maxs: [240.0, 500.0, 500.0] });
    let t = Throw { kind: crate::arms::ThrowKind::Sticky, cook_off_hold: false, fuse_time_ms: 2000, ..frag() };
    let mut g = Grenade::thrown(3, 9, t, [0.0, 0.0, 60.0], [0.0, 0.0, 0.0], [0.0; 3], None);
    let mut stuck_at = None;
    let mut boom = None;
    for i in 0..400 {
        boom = g.step(16, &world);
        if g.stuck && stuck_at.is_none() {
            stuck_at = Some(i * 16);
        }
        if boom.is_some() {
            break;
        }
    }
    let at = stuck_at.expect("it stuck");
    assert!(g.origin[0] < 200.0 && g.origin[0] > 190.0 && g.origin[2] > 20.0, "on the wall's face: {:?}", g.origin);
    assert!(boom.is_some(), "it went off");
    assert!(at < 400, "stuck at once: {at} ms");
}

#[test]
fn a_c4_a_claymore_or_a_knife_lands_for_the_host_and_never_goes_off_here() {
    let world = BoxWorld::floor(0.0, 20000.0);
    for kind in [crate::arms::ThrowKind::Charge, crate::arms::ThrowKind::Mine, crate::arms::ThrowKind::Knife] {
        let speed = if kind == crate::arms::ThrowKind::Mine { 0.0 } else { 600.0 };
        let t = Throw { kind, cook_off_hold: false, speed, ..frag() };
        let mut g = Grenade::thrown(4, 9, t, [0.0, 0.0, 60.0], [0.0, 0.0, 0.0], [0.0; 3], None);
        for _ in 0..300 {
            assert!(g.step(16, &world).is_none(), "{kind:?} never goes off by itself");
            if g.landed {
                break;
            }
        }
        assert!(g.landed && g.origin[2] < 5.0, "{kind:?} on the floor: {:?}", g.origin);
    }
}

#[test]
fn a_knife_thrown_from_inside_something_still_flies_out() {
    // The eye inside a box (the thrower's own space): the knife goes on and sticks in the far wall.
    let mut world = BoxWorld::floor(-1000.0, 20000.0);
    world.add(SolidBox { mins: [-10.0, -10.0, 50.0], maxs: [10.0, 10.0, 70.0] });
    world.add(SolidBox { mins: [400.0, -500.0, -1000.0], maxs: [440.0, 500.0, 500.0] });
    let t = Throw { kind: crate::arms::ThrowKind::Knife, cook_off_hold: false, speed: 1400.0, ..frag() };
    let mut g = Grenade::thrown(5, 9, t, [0.0, 0.0, 60.0], [0.0, 0.0, 0.0], [0.0; 3], None);
    for _ in 0..100 {
        g.step(16, &world);
        if g.landed {
            break;
        }
    }
    assert!(g.landed && g.origin[0] > 390.0, "stuck in the far wall, not in the hand: {:?}", g.origin);
}
