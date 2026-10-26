# Changelog

All notable changes to CODV. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
versions follow [Semantic Versioning](https://semver.org/). The runtime's C ABI version (`CODV_ABI_VERSION`) is
noted with each release: a host and a runtime only work together on the same ABI.

## [Unreleased]

## [0.1.0] - 2026-10-26

First public release. Runtime ABI 10.

### Movement
- MW2's player movement (`movement_iw4`) drives the GTA player in Story Mode: run, sprint, jump, crouch, prone
  and mantle, with MW2's eye height, field of view and camera.
- GTA's loaded collision becomes the MW2 world, read live (credit Sol4ra, via SK8V); vehicles, props and peds
  collide with their own GTA bounds.
- CODV's additions: slide, dive, auto vault, high climb, lean (Q / E), a sprint meter that refills 30% faster
  than MW2's, lighter fall damage, MW2's footstep and landing sounds.
- GTA's ladders and parachute hand the player to GTA with MW2's view and HUD kept.
- A real-time debug overlay (`Debug=1` or the menu) shows what the movement meets.

### Weapons
- The setup imports MW2's multiplayer weapons from the player's own copy; `weapon_iw4` runs fire, aim down
  sights, spread, view kick, reloads, swaps and melee with MW2's numbers and hit locations.
- Weapon sets (`WeaponSets=1`, default): three different random guns behind each number key, rolled anew at
  each switch-on: key 1 primaries, key 2 secondaries, key 3 sniper rifles, key 5 launchers. The same key
  again takes the group's next gun; every gun keeps its own rounds. `WeaponSets=0`: one gun per key.
- The knife is on F (every press, sprinting too) and ends with the gun's quick raise, as in MW2. F never
  enters a vehicle while MW2 drives; GTA's own enter key is off in the mode.
- MW2's frag on G / RB: pin, cook, throw and cook-off, with MW2's bounce rules.
- Launchers: the Stinger locks on aircraft up to 2500 m, the Javelin on ground vehicles with its top attack.
  A locked Stinger or Javelin flies CODV's own missile (about 3 km, MW2's proximity fuse; `Mw2Missiles=1`);
  the RPG-7, the AT4 and unlocked shots fire GTA's rocket.
- The RPG-7 shows its rocket while loaded and gets it back during the reload (after the setup imports MW2 again).
- Shooting from a vehicle's seat with MW2's HUD and gun; GTA's arms hidden in first person.

### View, sound and HUD
- MW2's first-person view models (hands and gun) posed by iw4L's animation code, with MW2's bob, sway and kick.
- MW2's character animation retargeted onto the GTA ped (third person and other cameras).
- MW2's weapon sounds inside GTA's audio engine (SK8V's bridge), with MW2's alias rules.
- MW2's HUD: compass, crosshair, sniper scope, weapon bar, hit marker, damage direction, grenade indicator,
  kill feedback and blood.
- The view model and HUD hide under GTA's pause menu, its warning screens and while the game window is in
  the background.

### Killstreaks
- All ten of MW2's killstreaks for hostile kills: UAV, Care Package, Predator Missile, Sentry Gun, Precision
  Airstrike, Attack Helicopter, AC-130, Chopper Gunner, EMP and the Tactical Nuke, with MW2's splashes,
  overlays and sounds. Key 4 / D-pad right calls in the top reward.
- All ten in play by default (`AllKillstreaks=1`), on spaced kill counts (`SpacedKillstreaks=1`): UAV 3, Care
  Package 6, Predator Missile 9, Sentry Gun 12, Precision Airstrike 15, Attack Helicopter 18, AC-130 22,
  Chopper Gunner 26, EMP 30, Tactical Nuke 35. Off: MW2's three picks and kill counts.
- The streak counts per life; unused rewards are lost on death (`KeepKillstreaksOnDeath=0`).
- The Tactical Nuke leaves a long aftermath (orange sky, smoke, empty streets) without killing the player or
  ending anything.
- Never in story missions, never raising the wanted level.

### Setup
- `CODV Setup.cmd` opens CODV's setup window: it finds GTA V (Steam, Rockstar Games Launcher, Epic) and MW2
  (Steam), imports MW2 from the player's own copy with a progress bar and installs next to `GTA5.exe`.
  `INSTALL.bat` does the same from source.
- The release carries `THIRD-PARTY-NOTICES.txt` (every Rust crate in the shipped binaries with its license)
  and `SHA256SUMS.txt`.

### Known issues
- Small hitches remain in some movement cases (car corners, low fences); tuning continues.
- CODV's own missiles have no in-flight sound yet.
- GTA V Enhanced and GTA Online are not supported.

[Unreleased]: https://github.com/PurpleDarkO/CODV/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/PurpleDarkO/CODV/releases/tag/v0.1.0
