# Weapons and handling

Everything here lives in `data/weapons.json` (installed as `<GTA>\CODV\weapons.json`). Edit it and restart GTA;
a mistake is named in `CodVRuntime.log` (weapon and field) and the built-in table is used instead. Units are
metres, seconds, degrees and rounds per minute.

## The shipped table

| Weapon | Fire | RPM | Damage near - far | Range (m) | Head | Mag + reserve | Reload tactical / empty (s) | ADS (s) | Sprint-out (s) | Hip spread (deg) | ADS spread | Move |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Carbine | auto | 750 | 30 - 20 | 25 - 45 | x1.4 | 30 + 120 | 2.2 / 2.9 | 0.22 | 0.25 | 2.5 - 6.0 | 0.1 | 1.0 |
| Assault Rifle | auto | 630 | 35 - 25 | 22 - 42 | x1.4 | 30 + 120 | 2.4 / 3.1 | 0.25 | 0.28 | 3.0 - 6.5 | 0.15 | 0.97 |
| Burst Rifle | burst x3 | 900 | 40 - 30 | 30 - 50 | x1.5 | 30 + 120 | 2.3 / 3.0 | 0.24 | 0.27 | 3.0 - 6.5 | 0.08 | 0.97 |
| SMG | auto | 900 | 30 - 18 | 10 - 25 | x1.2 | 30 + 150 | 2.0 / 2.6 | 0.17 | 0.2 | 2.0 - 4.5 | 0.3 | 1.06 |
| Machine Pistol | auto | 1000 | 24 - 14 | 8 - 18 | x1.2 | 20 + 120 | 1.7 / 2.2 | 0.15 | 0.15 | 2.0 - 4.0 | 0.6 | 1.08 |
| Light Machine Gun | auto | 700 | 40 - 30 | 30 - 55 | x1.3 | 100 + 200 | 6.0 / 7.0 | 0.38 | 0.4 | 4.0 - 8.0 | 0.2 | 0.88 |
| Pump Shotgun | semi, 8 pellets | 75 | 40 - 10 | 5 - 16 | x1.0 | 8 + 32 | 0.5 per shell | 0.25 | 0.25 | 5.0 - 7.0 | 4.5 | 1.0 |
| Bolt Sniper | semi | 48 | 98 - 70 | 60 - 120 | x1.5 | 5 + 25 | 3.0 / 3.8 | 0.42 | 0.4 | 8.0 - 12.0 | 0.0 | 0.95 |
| Pistol | semi | 450 | 40 - 20 | 10 - 25 | x1.4 | 12 + 48 | 1.6 / 2.0 | 0.15 | 0.12 | 2.0 - 4.0 | 0.3 | 1.1 |
| Magnum | semi | 240 | 55 - 35 | 12 - 28 | x1.4 | 9 + 36 | 1.9 / 2.3 | 0.17 | 0.14 | 2.5 - 4.5 | 0.35 | 1.08 |

Damage is per bullet (per pellet for the shotgun) against GTA peds, which have about 100 health to lose, so a
Carbine kills in four body shots up close and five at range. GTA adds its own body-part multipliers on top
(head shots on peds are usually fatal anyway).

## How a shot works

1. **Timing.** Automatic weapons fire while the trigger is held, exactly at their RPM (the overshoot of each
   frame carries into the next, so 750 rpm is 12.5 shots a second at any frame rate). Semi-automatic weapons
   fire once per press; a press up to 0.12 s before the gun is ready still counts. Burst weapons fire their
   burst on one press, even if you let go, then wait `burst_delay`.
2. **Where it goes.** The cone half-angle is the hip spread, from `hip_spread[0]` standing still to
   `hip_spread[1]` (plus `move_spread`) at walking speed and above, times 0.7 crouched, plus `bloom` per shot
   (decaying at `bloom_decay` per second), plus 4 degrees in the air. Aiming down sights blends it to
   `ads_spread` over `ads_time`. Each bullet lands uniformly inside the cone.
3. **Damage.** Full `damage[0]` up to `range[0]`, falling linearly to `damage[1]` at `range[1]` and beyond;
   `head` multiplies a headshot (within 0.2 m of the head bone).
4. **Kick.** Each shot kicks the view up by `kick_pitch` and sideways by `kick_yaw` (random inside each range),
   scaled by `ads_kick` when aiming down sights, 0.85 crouched, and the menu's recoil setting. The kick reaches
   the camera over a few frames; `kick_recover` is the share that drifts back once you stop firing.

## Handling

- **Magazines.** A reload refills the magazine from your reserve at the end (`reload` with rounds left,
  `reload_empty` when empty); with `reload_round` the weapon loads one round at a time and firing interrupts.
  Sprinting cancels a reload. Pulling the trigger on an empty magazine reloads.
- **Swaps.** The swap button drops the weapon in hand (`drop`), equips the other loadout slot and raises it
  (`raise`); a weapon that arrives any other way (weapon wheel, pickup) also takes its raise time.
- **Sprint.** 4 seconds of sprint (`player.sprint_seconds`); empty, it is blocked until 1 second has come back,
  which starts 1.2 s after you stop. Firing out of a sprint waits `sprint_out`. Pulling the trigger stops the
  sprint.
- **Movement.** `move` scales GTA's movement rate with the weapon in hand; aiming down sights scales it by
  `player.ads_move_scale` (times `ads_move`). The sprint multiplier is `player.sprint_multiplier` times `move`.
- **Melee.** A short lunge and strike for `player.melee_damage` within `player.melee_range`.
- **Health.** With regeneration on, health returns over `player.regen_seconds` after `player.regen_delay`
  without damage.

## Adding a weapon

Any GTA weapon name works in `gta` (one entry per GTA weapon). Copy a similar entry, give it a new `name`, set
`gta`, tune the numbers, and pick it as primary or secondary in the menu. `py tools\check-data.py` confirms the
GTA weapon name exists.
