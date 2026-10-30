//! The weapon state machine on a test gun (numbers written for the test,
//! not MW2's), and, when the player's imported table is at
//! `CODV_MW2_TABLE`, on MW2's own M4.
use super::*;
use crate::player::{MoveInput, button};
use crate::world::BoxWorld;
use codv_mw2data::table::{Row, Table};

const MS: u32 = 10;

fn test_gun(name: &str, fire_type: i32) -> Row {
    let n = |pairs: &[(&str, f64)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<Vec<_>>();
    Row {
        name: name.into(),
        display_name: "TEST".into(),
        numbers: n(&[
            ("fire_time_ms", 100.0),
            ("raise_time_ms", 300.0),
            ("drop_time_ms", 200.0),
            ("first_raise_time_ms", 500.0),
            ("reload_time_ms", 1500.0),
            ("reload_empty_time_ms", 2000.0),
            ("clip_size", 10.0),
            ("start_ammo", 30.0),
            ("max_ammo", 60.0),
            ("fire_type", fire_type as f64),
            ("weap_type", 0.0),
            ("weap_class", 0.0),
            ("inventory_type", 0.0),
            ("shots_per_fire", 1.0),
            ("damage", 30.0),
            ("min_damage", 20.0),
            ("max_damage_range", 1000.0),
            ("min_damage_range", 2000.0),
            ("hip_spread_stand_min", 3.0),
            ("hip_spread_stand_max", 7.0),
            ("hip_spread_ducked_min", 2.5),
            ("hip_spread_ducked_max", 6.0),
            ("hip_spread_prone_min", 2.0),
            ("hip_spread_prone_max", 5.0),
            ("hip_spread_decay_rate", 4.0),
            ("hip_spread_fire_add", 0.6),
            ("hip_spread_move_add", 5.0),
            ("hip_spread_ducked_decay", 1.05),
            ("hip_spread_prone_decay", 1.1),
            ("ads_spread", 0.0),
            ("aim_down_sight", 1.0),
            ("ads_in_rate", 1.0 / 250.0),
            ("ads_out_rate", 1.0 / 200.0),
            ("rechamber_while_ads", 1.0),
            ("move_speed_scale", 0.9),
            ("ads_move_speed_scale", 0.5),
            ("sprint_duration_scale", 1.0),
            ("melee_damage", 135.0),
            ("melee_time_ms", 800.0),
            ("kick.hip_view_kick_pitch_min", 30.0),
            ("kick.hip_view_kick_pitch_max", 50.0),
            ("kick.ads_view_kick_pitch_min", 20.0),
            ("kick.ads_view_kick_pitch_max", 40.0),
            ("select_requires_ammo", -1.0),
            ("offhand_hold_is_cancelable", -1.0),
        ]),
        strings: vec![],
        location: None,
    }
}

fn test_frag() -> Row {
    let mut r = test_gun("frag_grenade_mp", 0);
    for (k, v) in [
        ("weap_type", 1.0),
        ("weap_class", 6.0),
        ("inventory_type", 1.0),
        ("offhand_class", 1.0),
        ("fire_time_ms", 360.0),
        ("fire_delay_ms", 129.0),
        ("hold_fire_time_ms", 600.0),
        ("fuse_time_ms", 3500.0),
        ("clip_size", 1.0),
        ("start_ammo", 1.0),
        ("max_ammo", 1.0),
        ("projectile_speed", 940.0),
        ("explosion_radius", 256.0),
        ("explosion_inner_damage", 150.0),
        ("explosion_outer_damage", 55.0),
        ("quick_raise_time_ms", 500.0),
        ("quick_drop_time_ms", 500.0),
        ("offhand_hold_is_cancelable", 0.0),
    ] {
        match r.numbers.iter_mut().find(|(n, _)| n == k) {
            Some(e) => e.1 = v,
            None => r.numbers.push((k.into(), v)),
        }
    }
    r
}

fn table() -> Table {
    let mut loc = [1.0f32; HITLOCS];
    loc[2] = 1.4; // head
    loc[18] = 0.0;
    Table {
        source: "test".into(),
        location: Some(loc),
        weapons: vec![test_gun("auto_mp", 0), test_gun("semi_mp", 1), test_frag()],
    }
}

struct Rig {
    world: BoxWorld,
    player: Mw2Player,
    gunner: Gunner,
    arsenal: Arsenal,
}

impl Rig {
    fn new(t: &Table, loadout: &[&str]) -> Rig {
        let (arsenal, refused) = Arsenal::from_table(t);
        assert!(refused.is_empty(), "{refused:?}");
        let ids: Vec<u32> = loadout.iter().map(|n| arsenal.id(n).unwrap()).collect();
        let gunner = Gunner::new(&arsenal, &ids);
        let mut rig =
            Rig { world: BoxWorld::floor(0.0, 20000.0), player: Mw2Player::new([0.0, 0.0, 1.0], 0.0), gunner, arsenal };
        rig.frames(0, 100);
        rig
    }

    /// One frame of pmove and the weapon with these retail buttons.
    fn frame(&mut self, buttons: u32) -> GunFrame {
        self.gunner.arm(&mut self.player, &self.arsenal);
        self.player.step(&MoveInput { msec: MS, buttons, ..Default::default() }, &self.world);
        self.gunner.step(&mut self.player, &self.arsenal, MS, buttons)
    }

    /// A frame running forward.
    fn run(&mut self, buttons: u32) -> GunFrame {
        self.gunner.arm(&mut self.player, &self.arsenal);
        self.player.step(&MoveInput { msec: MS, buttons, forward: 127, ..Default::default() }, &self.world);
        // As the runtime: a sprint over the aim takes the aim from the weapon too.
        let buttons = if self.player.sprint_over_ads { buttons & !button::ADS } else { buttons };
        self.gunner.step(&mut self.player, &self.arsenal, MS, buttons)
    }

    fn frames(&mut self, buttons: u32, n: usize) -> Vec<GunFrame> {
        (0..n).map(|_| self.frame(buttons)).collect()
    }

    fn frames_run(&mut self, buttons: u32, n: usize) -> Vec<GunFrame> {
        (0..n).map(|_| self.run(buttons)).collect()
    }
}

#[test]
fn full_auto_fires_at_its_fire_time_and_empties_the_magazine() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    let out = rig.frames(button::ATTACK, 100); // one second
    let shots: usize = out.iter().map(|f| f.shots.len()).sum();
    assert!((10..=10).contains(&shots), "10 rounds in the magazine at 100 ms each: {shots}");
    let last = out.last().unwrap();
    assert_eq!(last.clip, 0);
    assert_eq!(last.stock, 20);
    // Spread grows with each hip shot and stays inside the hip cone.
    let spreads: Vec<f32> = out.iter().filter(|f| f.fired).map(|f| f.spread).collect();
    assert!(spreads.first().unwrap() < spreads.last().unwrap(), "{spreads:?}");
    assert!(out.iter().flat_map(|f| &f.shots).all(|s| s.yaw.hypot(s.pitch) <= 7.5 + 8.0), "inside the cone (+ kick)");
    // The view kick rises then returns to centre.
    assert!(out.iter().any(|f| f.kick[0] > 0.1), "the view kicks up");
    let rest = rig.frames(0, 100);
    assert!(rest.last().unwrap().kick[0].abs() < 0.01, "and comes back");
}

#[test]
fn reloads_take_the_authored_times() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    rig.frames(button::ATTACK, 30); // 3 shots
    let out = rig.frames(button::RELOAD, 1);
    assert!(out[0].reload_started, "{:?}", out[0]);
    let mut t = 0;
    loop {
        let f = rig.frame(0);
        t += MS;
        if f.clip == 10 {
            break;
        }
        assert!(t < 5000, "reload never ended");
    }
    assert!((1400..=1600).contains(&t), "partial reload ~1500 ms: {t}");
    let f = rig.frame(0);
    assert_eq!((f.clip, f.stock), (10, 17));
    // Empty, trigger still held: MW2 starts the reload by itself, and it
    // takes the empty reload's longer time.
    let out = rig.frames(button::ATTACK, 130);
    let started = out.iter().position(|f| f.reload_started).expect("an automatic reload on the empty magazine");
    let mut t = (out.len() - 1 - started) as u32 * MS;
    while rig.frame(0).clip == 0 {
        t += MS;
        assert!(t < 5000);
    }
    assert!((1900..=2100).contains(&t), "empty reload ~2000 ms: {t}");
}

#[test]
fn sprinting_waits_for_the_reload_instead_of_cancelling_it() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    rig.frames(button::ATTACK, 30); // 3 shots
    assert!(rig.run(button::RELOAD).reload_started);
    // Sprint held from the next frame on: the reload carries on.
    let mut t = 0;
    loop {
        let f = rig.run(button::SPRINT);
        t += MS;
        assert!(!rig.player.peek().sprinting || f.clip == 10, "no sprint mid-reload ({t} ms)");
        if f.clip == 10 {
            break;
        }
        assert!(t < 3000, "the sprint cancelled the reload");
    }
    assert!((1400..=1600).contains(&t), "the reload's own time: {t}");
    // Still held: the sprint starts once the reload is done.
    let sprinted = (0..50).any(|_| {
        rig.run(button::SPRINT);
        rig.player.peek().sprinting
    });
    assert!(sprinted);
}

/// CODV's reload while sprinting: the sprint starts mid-reload, the reload goes on at its own time and
/// the sprint is a little slower until it ends.
#[test]
fn reload_while_sprinting_keeps_the_reload_and_slows_the_sprint() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    rig.gunner.reload_sprint = true;
    rig.frames(button::ATTACK, 30); // 3 shots
    assert!(rig.run(button::RELOAD).reload_started);
    let (mut t, mut sprinted_mid, mut slow) = (0, false, 0.0f32);
    loop {
        let f = rig.run(button::SPRINT);
        t += MS;
        if f.clip == 10 {
            break;
        }
        if rig.player.peek().sprinting {
            sprinted_mid = true;
            slow = rig.player.ps.velocity[0].hypot(rig.player.ps.velocity[1]);
        }
        assert!(t < 3000, "the sprint cancelled the reload");
    }
    assert!(sprinted_mid, "the sprint starts mid-reload");
    assert!((1400..=1600).contains(&t), "the reload's own time: {t}");
    // After the reload: still sprinting, at the full sprint speed.
    rig.frames_run(button::SPRINT, 40);
    assert!(rig.player.peek().sprinting);
    let fast = rig.player.ps.velocity[0].hypot(rig.player.ps.velocity[1]);
    assert!(slow < fast * 0.95 && slow > fast * 0.8, "slower while reloading: {slow} vs {fast}");
}

/// Reload pressed mid-sprint: the reload starts and the held sprint goes on.
#[test]
fn reload_pressed_mid_sprint_reloads_on_the_run() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    rig.gunner.reload_sprint = true;
    rig.frames(button::ATTACK, 30);
    rig.frames_run(button::SPRINT, 40);
    assert!(rig.player.peek().sprinting);
    let started = (0..10).any(|_| rig.run(button::SPRINT | button::RELOAD).reload_started);
    assert!(started, "the reload starts on the run");
    let mut t = 0;
    let mut sprinting = 0;
    loop {
        let f = rig.run(button::SPRINT);
        t += MS;
        if rig.player.peek().sprinting {
            sprinting += 1;
        }
        if f.clip == 10 {
            break;
        }
        assert!(t < 3000, "the reload never ended");
    }
    assert!(
        sprinting * MS as i32 > t as i32 / 2,
        "sprinting through most of the reload ({sprinting} frames of {t} ms)"
    );
}

#[test]
fn semi_auto_needs_a_press_per_shot_and_ads_slows_and_tightens() {
    let mut rig = Rig::new(&table(), &["semi_mp", "auto_mp"]);
    let held: usize = rig.frames(button::ATTACK, 50).iter().map(|f| f.shots.len()).sum();
    assert_eq!(held, 1, "held trigger: one shot");
    rig.frames(0, 15);
    let mut taps = 0;
    for _ in 0..5 {
        taps += rig.frames(button::ATTACK, 2).iter().map(|f| f.shots.len()).sum::<usize>();
        rig.frames(0, 15);
    }
    assert_eq!(taps, 5);
    // ADS: the frac reaches 1 in ~250 ms; spread falls to the ADS spread.
    let out = rig.frames(button::ADS, 40);
    let full = out.iter().position(|f| f.ads >= 1.0).expect("fully aimed") as u32 * MS;
    assert!((200..=300).contains(&full), "ADS in ~250 ms: {full}");
    assert!(out.last().unwrap().spread < 0.5, "ADS spread: {}", out.last().unwrap().spread);
    assert!(rig.player.weapon.ads_allowed);
}

#[test]
fn switching_takes_drop_then_raise_and_keeps_each_magazine() {
    let mut rig = Rig::new(&table(), &["auto_mp", "semi_mp"]);
    rig.frames(button::ATTACK, 30);
    let auto_clip = rig.frame(0).clip;
    rig.gunner.select(1);
    let semi = rig.arsenal.id("semi_mp").unwrap();
    let mut t = 0;
    loop {
        let f = rig.frame(0);
        t += MS;
        if f.weapon == semi && f.weaponstate == 0 {
            assert_eq!(f.clip, 10, "the semi's own full magazine");
            break;
        }
        assert!(t < 3000);
    }
    assert!((650..=850).contains(&t), "drop 200 + first raise 500: {t}");
    rig.gunner.select(0);
    let mut back = rig.frame(0);
    for _ in 0..200 {
        back = rig.frame(0);
    }
    assert_eq!(back.clip, auto_clip, "the auto kept its magazine");
}

#[test]
fn the_rounds_carried_survive_a_new_gunner_for_the_same_guns() {
    let mut rig = Rig::new(&table(), &["auto_mp", "semi_mp"]);
    rig.frames(button::ATTACK, 30);
    let clip = rig.frame(0).clip;
    let (ammo, frags) = rig.gunner.ammo_state();
    let loadout = rig.gunner.loadout().to_vec();
    let mut fresh = Gunner::new(&rig.arsenal, &loadout);
    fresh.restore_ammo(&ammo, frags);
    assert_eq!(fresh.ammo_state().0, ammo);
    rig.gunner = fresh;
    assert_eq!(rig.frame(0).clip, clip, "the magazine as it was, not a full one");
}

#[test]
fn damage_falls_off_with_range_and_scales_by_location() {
    let (arsenal, _) = Arsenal::from_table(&table());
    let g = arsenal.gun(arsenal.id("auto_mp").unwrap()).unwrap();
    assert_eq!((g.damage_at(500.0), g.damage_at(1500.0), g.damage_at(5000.0)), (30, 25, 20));
    assert!((g.location_scale(2) - 1.4).abs() < 1e-6 && g.location_scale(18) == 0.0 && g.location_scale(4) == 1.0);
}

/// MW2's own M4 from the player's import (`CODV_MW2_TABLE=<path to weapons.json>`).
#[test]
fn mw2_m4_from_the_players_import() {
    let Ok(path) = std::env::var("CODV_MW2_TABLE") else { return };
    let t = codv_mw2data::table::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (arsenal, refused) = Arsenal::from_table(&t);
    println!("{} MW2 guns, {} refused: {:?}", arsenal.len(), refused.len(), refused.iter().take(5).collect::<Vec<_>>());
    let mut rig = Rig::new(&t, &["m4_mp", "usp_mp"]);
    rig.frames(0, 100); // first raise
    let out = rig.frames(button::ATTACK, 100);
    let shots: usize = out.iter().map(|f| f.shots.len()).sum();
    println!("M4: {shots} shots in 1 s, clip {}", out.last().unwrap().clip);
    assert!((14..=15).contains(&shots), "M4 fires every 70 ms: {shots}");
    let m4 = arsenal.gun(arsenal.id("m4_mp").unwrap()).unwrap();
    assert_eq!((m4.damage_at(1000.0), m4.damage_at(3000.0)), (30, 20));
    assert!((m4.location_scale(2) - 1.4).abs() < 1e-3, "head x1.4");
}

#[test]
fn frags_cook_while_held_and_leave_the_hand_on_release() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    let (a, _) = Arsenal::from_table(&table());
    let frag = a.offhand_id("frag_grenade_mp").expect("the frag is an offhand weapon");
    assert!(a.gun(frag).is_none() && a.any(frag).is_some());
    rig.gunner = Gunner::new(&rig.arsenal, &[rig.arsenal.id("auto_mp").unwrap()])
        .with_reserve_scale(&rig.arsenal, 3.0)
        .with_frags(&rig.arsenal, "frag_grenade_mp", 2);
    rig.frames(0, 100);
    assert_eq!(rig.frame(0).stock, 60, "spare rounds x3");
    // Hold the frag button 1.5 s (cooking), then let go.
    let held = rig.frames(button::FRAG, 150);
    assert!(held.iter().any(|f| f.offhand_prepare), "the pin is pulled");
    assert!(held.iter().any(|f| f.offhand_view == Some(frag)), "the view shows the grenade");
    let after = rig.frames(0, 100);
    let thrown: Vec<_> = held.iter().chain(&after).filter_map(|f| f.thrown).collect();
    assert_eq!(thrown.len(), 1, "one throw");
    let (w, fuse) = thrown[0];
    assert_eq!(w, frag);
    let fuse = fuse.expect("cooked: part of the fuse is spent");
    assert!((1500..3000).contains(&fuse), "{fuse}");
    assert_eq!(after.last().unwrap().frags, 1);
    assert!(after.last().unwrap().offhand_view.is_none(), "back to the gun");
    // Held past the fuse: it goes off in the hand.
    let cooked = rig.frames(button::FRAG, 450);
    assert!(cooked.iter().any(|f| f.cooked_off == Some(frag)), "cooked off");
    rig.frames(0, 100);
    assert_eq!(rig.frame(0).frags, 0);
    // None left: the button does nothing.
    assert!(rig.frames(button::FRAG, 100).iter().all(|f| !f.offhand_prepare));
}

#[test]
fn moving_opens_the_hip_spread_and_standing_closes_it() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    let still = rig.frames(0, 200).last().unwrap().spread;
    let mut moving = Vec::new();
    for _ in 0..100 {
        rig.gunner.set_moves(127, 0);
        rig.gunner.arm(&mut rig.player, &rig.arsenal);
        rig.player.step(&MoveInput { msec: MS, forward: 127, ..Default::default() }, &rig.world);
        moving.push(rig.gunner.step(&mut rig.player, &rig.arsenal, MS, 0).spread);
    }
    let open = *moving.last().unwrap();
    assert!((still - 3.0).abs() < 0.2, "standing: the minimum, {still}");
    assert!(open > 6.0, "running opens it towards the maximum: {open}");
    rig.gunner.set_moves(0, 0);
    let closed = rig.frames(0, 200).last().unwrap().spread;
    assert!(closed < 3.2, "standing again closes it: {closed}");
}

#[test]
fn scavenger_adds_a_magazine_up_to_the_limit() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    rig.frames(button::ATTACK, 100); // the magazine spent: 0 / 20
    rig.gunner.scavenge(&rig.arsenal, 2);
    let f = rig.frames(0, 1).pop().unwrap();
    assert_eq!((f.clip, f.stock), (0, 30), "one magazine (10) more");
    for _ in 0..10 {
        rig.gunner.scavenge(&rig.arsenal, 2);
    }
    let f = rig.frames(0, 1).pop().unwrap();
    assert_eq!(f.stock, 60, "capped at the gun's max_ammo");
}

#[test]
fn a_mantle_and_a_jump_leave_the_gun_ready_with_its_magazine() {
    let mut rig = Rig::new(&table(), &["auto_mp", "semi_mp"]);
    rig.world.add(crate::world::SolidBox { mins: [120.0, -200.0, 0.0], maxs: [400.0, 200.0, 40.0] });
    rig.player.auto_vault = true;
    rig.player.mantle_momentum = true;
    rig.player.mantle_speed = 1.8;
    rig.frames(button::ATTACK, 4);
    let clip = rig.frame(0).clip;
    let mut mantled = false;
    for _ in 0..120 {
        rig.run(button::SPRINT);
        mantled |= rig.player.peek().mantling;
    }
    assert!(mantled);
    for i in 0..60 {
        rig.frame(if i == 0 { button::JUMP } else { 0 });
    }
    let f = rig.frame(0);
    assert!(f.weaponstate == 0 && f.clip == clip, "ready, no reload: {f:?} (clip was {clip})");
}

/// The sprint key held through a weapon switch: the switch ends the sprint (no sprint while the
/// gun goes down and the other comes up) and it comes back by itself once the new gun is ready.
#[test]
fn a_held_sprint_comes_back_after_a_weapon_switch() {
    held_sprint_through_a_switch(&table(), "auto_mp", "semi_mp");
    // MW2's own M4 and USP (their sprint and switch animations' times), with the player's table.
    if let Ok(path) = std::env::var("CODV_MW2_TABLE") {
        let t = codv_mw2data::table::from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
        held_sprint_through_a_switch(&t, "m4_mp", "usp_mp");
    }
}

fn held_sprint_through_a_switch(t: &Table, first: &str, second: &str) {
    let mut rig = Rig::new(t, &[first, second]);
    rig.frames(0, 100);
    let sprinted = (0..60).any(|_| {
        rig.run(button::SPRINT);
        rig.player.peek().sprinting
    });
    assert!(sprinted);
    rig.gunner.select(1);
    let semi = rig.arsenal.id(second).unwrap();
    let mut ready = false;
    let mut back = false;
    for _ in 0..300 {
        let f = rig.run(button::SPRINT);
        ready |= f.weapon == semi && f.weaponstate == 0;
        if ready && rig.player.peek().sprinting {
            back = true;
            break;
        }
    }
    assert!(ready, "the switch finished");
    assert!(back, "the held key sprints again without a new press");
}

/// The weapAnim events (toggle bit off) a gun shows from a melee press on, frame by frame.
fn melee_anims(t: &Table, gun: &str) -> Vec<u32> {
    let mut rig = Rig::new(t, &[gun]);
    let mut seen = Vec::new();
    for i in 0..200 {
        let out = rig.frame(if i < 2 { button::MELEE_CHARGE } else { 0 });
        let ev = out.weap_anim as u32 & w::WEAP_ANIM_EVENT_MASK;
        if seen.last() != Some(&ev) {
            seen.push(ev);
        }
    }
    seen
}

#[test]
fn a_knife_melee_ends_with_the_guns_quick_raise() {
    const MELEE: u32 = 8;
    const QUICK_RAISE: u32 = 0x14;
    let mut knife = test_gun("knife_mp", 0);
    knife.numbers.push(("quick_raise_time_ms".into(), 750.0));
    knife.strings.push(("xanim.7".into(), "viewmodel_knife_melee_slice".into()));
    let t = Table { source: "test".into(), location: table().location, weapons: vec![knife, test_gun("auto_mp", 0)] };
    let with = melee_anims(&t, "knife_mp");
    let m = with.iter().position(|&e| e == MELEE).expect("no melee");
    assert!(with[m..].contains(&QUICK_RAISE), "{with:?}");
    // A gun without the knife clip still goes straight back to idle (nothing else changes).
    let without = melee_anims(&t, "auto_mp");
    assert!(without.contains(&MELEE) && !without.contains(&QUICK_RAISE), "{without:?}");
}

/// A switch mid-reload cancels it (MW2): the other gun comes up at once, and a reload cut short before
/// its add point puts no rounds in.
#[test]
fn a_switch_mid_reload_cancels_it_without_the_rounds() {
    let mut rig = Rig::new(&table(), &["auto_mp", "semi_mp"]);
    rig.frames(button::ATTACK, 30);
    assert!(rig.frame(button::RELOAD).reload_started);
    rig.frames(0, 30); // 0.3 s into a 1.5 s reload
    rig.gunner.select(1);
    let semi = rig.arsenal.id("semi_mp").unwrap();
    let mut t = 0;
    while rig.frame(0).weapon != semi {
        t += MS;
        assert!(t < 1000, "the switch waited for the reload");
    }
    let (ammo, _) = rig.gunner.ammo_state();
    let auto = rig.arsenal.id("auto_mp").unwrap();
    assert_eq!(ammo.iter().find(|a| a.0 == auto).unwrap().1, 7, "no rounds from the cut reload: {ammo:?}");
}

/// Sprint and aim, the last pressed wins while both are held.
#[test]
fn sprint_and_aim_the_last_pressed_wins() {
    let mut rig = Rig::new(&table(), &["auto_mp"]);
    let hold = |rig: &mut Rig, b: u32, n: usize| {
        let mut f = rig.run(b);
        for _ in 1..n {
            f = rig.run(b);
        }
        (rig.player.peek().sprinting, f.ads)
    };
    // Sprinting, the aim pressed: aiming (not a walk with neither).
    hold(&mut rig, button::SPRINT, 60);
    let (s, a) = hold(&mut rig, button::SPRINT | button::ADS, 80);
    assert!(!s && a > 0.9, "aiming over the held sprint: sprint {s} ads {a}");
    // The aim let go, the sprint still held: sprinting again.
    let (s, _) = hold(&mut rig, button::SPRINT, 40);
    assert!(s, "the held sprint is back");
    // Aiming, the sprint pressed: sprinting; the sprint let go: aiming again.
    hold(&mut rig, 0, 40);
    hold(&mut rig, button::ADS, 60);
    let (s, a) = hold(&mut rig, button::ADS | button::SPRINT, 60);
    assert!(s && a < 0.1, "sprinting over the held aim: sprint {s} ads {a}");
    let (s, a) = hold(&mut rig, button::ADS, 80);
    assert!(!s && a > 0.9, "aiming again: sprint {s} ads {a}");
}
