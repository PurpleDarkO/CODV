# Changelog

All notable changes to CODV. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
versions follow [Semantic Versioning](https://semver.org/). The runtime's C ABI version (`CODV_ABI_VERSION`) is
noted with each release: a host and a runtime only work together on the same ABI.

## [Unreleased]

## [0.3.0] - 2026-11-05

Runtime ABI 10, unchanged (the runtime's additions are optional exports, options and flags). Run the setup
again: it imports the new equipment's animations and icons from your MW2.

### Added
- Lethal on G / RB (menu Weapons, `Lethal`), with MW2's throw animation and HUD icon: the frag, the Semtex
  (sticks to what it hits, cars and people too, and goes off 2 s later), the C4 (sticks where it lands; key 6
  or a double tap of reload sets them all off, even from a vehicle), the Claymore (put down ahead of you; goes
  off when a hostile walks into its lasers) or the throwing knife (kills in one hit, silently). "Grenades
  carried" (`Frags`) sets how many; Scavenger gives one back.
- Tactical on T / D-pad left (menu Weapons, `Tactical`), two carried, with MW2's animation and icon: the
  flashbang (hostiles who see it cower, blinded; your screen goes white if you look at it), the stun (shorter)
  or the smoke (a cloud for about 16 s).
- Damage (menu Weapons, `Damage`): CODV's bullets do x1.5 MW2's damage to people by default (GTA's people,
  with armour and limb hits, took more shots than MW2's players); MW2's numbers or x2 to choose.
- Gun lighting (menu HUD, `ViewmodelLight`): the hands and gun follow the light around you (a dark alley, a
  night street, a tunnel), the time of day only, or fixed daylight.
- Gun over radar (menu HUD, `GunOverRadar`): under GTA's minimap the gun fades to a faint ghost by default, so
  the map stays readable; drawn over it, or hidden, to choose.
- Locked Stinger and Javelin missiles fly with a sound, loud at the launcher and fading as they fly off.
- Ladders by holding C: standing still right at a ladder and facing it.
- "JUGGERNAUT DOWN" shows over the score bar once his death's cinematic is over.
- The log names what holds the player still (a vehicle, a person, an object or a map triangle).
- Runtime (optional exports): `cv_mw2_text`, `cv_mw2_scorebar`, `cv_mw2_board_notice`, `cv_mw2_blocked_why`,
  `cv_alias_loop`, `cv_alias_level`; options `CV_OPT_LETHAL`, `CV_OPT_TACTICAL`; `CV_GRENADE_LANDED` and
  `CV_MW2_SMOKE`.

### Changed
- Police Juggernauts come far less often at five stars: after one joins, the next waits 1:30 to 2:30 at
  least; up to three can still be up at once, but over minutes.
- After a Tactical Nuke's blast no police are sent for 30 s, and no Juggernaut joins: a head start while the
  stars run out.

### Fixed
- An invisible wall tens of metres wide around some props (a park sprinkler).
- Invisible walls where a person lies or crawls on the ground.

### Known issues
- GTA's phone shows under MW2's HUD and gun.

## [0.2.0] - 2026-10-30

Runtime ABI 10, unchanged (the runtime's additions are an optional export and optional flags).

### Added
- Reload while sprinting (menu Movement, `ReloadSprint`, on by default): a sprint no longer waits for the
  reload; the reload goes on at a slightly slower sprint, and the gun comes up for the sprint once it ends.
  A reload pressed mid-sprint reloads on the run. Off: the sprint waits for the reload, as in 0.1.0.
  Runtime option 46 (`CV_OPT_RELOAD_SPRINT`); ABI 10 unchanged.
- Sprint and aim, the last pressed wins while both are held (MW2 walked with neither): sprinting, an aim
  press aims, and the held sprint is back when the aim is let go; aiming, a sprint press sprints, and the
  held aim is back when the sprint is let go (or ends).
- The Tactical Nuke can kill the player too, with their vehicle (menu Killstreaks, `NukeKillsPlayer`;
  MW2's rule). Off by default: CODV spares the player, as in 0.1.0.
- The Tactical Nuke starts the streak over (MW2 ends the match on it): its blast's kills count, then, once
  it is over, the count goes back to zero and the rewards still held are cleared. Nothing else can be called
  in while it is inbound. Runtime: `CV_STREAK_NUKE_OVER` (an optional flag); ABI 10 unchanged.
- Rounds: each run a Tactical Nuke ends is a round. The score bar shows "Round N" in small under the kill count, one
  more after every nuke. Switching the mode off starts again at Round 1.
- Police Juggernauts (Free Roam; menu Game, `PoliceJuggernauts`, on by default): at four stars a Juggernaut
  may join the police (one at a time), at five up to three at once; every one still alive counts, one
  left far behind too, so a long chase never piles them up. After one dies the next waits 45 s, 1:15 or 1:45 at least (at random),
  and none comes while the stars flash (the police searching). Heavy armour and a heavy machine gun in bursts (a loose aim), slow, 1500
  health, no ragdoll and no head shot kill; a bigger orange dot on the radar. His entrance plays in three
  cuts (4 s, with a pose picked at random: a knuckle crunch, a slow clap, a taunt, a salute, a flex, a
  bouncer's stance) and his death in
  slow motion around him, with a letterbox; the player is frozen and
  cannot be hurt meanwhile (`JuggernautCinematics`, on by default). The menu's "Test: spawn a Juggernaut"
  puts one ahead of the player, cinematics included.
- A new look for the CODV menu: the CODV mark on black with the green rule, a bar with where you are and
  the position in the list, short setting names with the value on the right (ON in green, the selected row
  in green with < > around its value), each section's size, a scroll bar, a line of help for the selected
  setting, and the controls for the keyboard or the pad. Tests are actions now ("Test: spawn a
  Juggernaut" no longer shows On / Off).
- The Juggernaut now and then (30%) skips the pose and just comes on, gun up.
- The Juggernaut's entrance plays without words; his death shows "JUGGERNAUT DOWN" in MW2's own killstreak
  splash, with its fonts and glow (GTA's text only with MW2's HUD off). Runtime: `cv_mw2_splash` (an
  optional export) and `CV_HUD_CINEMATIC`; ABI 10 unchanged.
- A rare Juggernaut (1%) carries a minigun instead, sprayed even looser; the menu
  has a test for him too.

### Fixed
- Opening the CODV menu no longer hands the player back to GTA: CODV stays on (view, gun, HUD) and the
  player stands still while the keys drive the menu.
- Kills while a camera has the view count again: the Juggernaut dying in his cinematic, and anything killed
  from a killstreak's view (Predator, AC-130, Chopper Gunner). A view longer than 2 s made the next scan skip
  the deaths in it. Who counts is unchanged: hostile people only, never civilians or animals.
- Invisible obstacles: a loose object GTA lets people walk through (a dropped weapon, debris after an
  explosion) was a solid box under MW2 movement; it is passable now, as in GTA. Vehicles always keep their
  collision, and an object whose collision cannot be read keeps its box.
- Invisible walls where a breakable prop stood: a fence blown apart by a frag or a rocket still blocked the
  player (and was mantled) where it had been; a broken prop is passable now, its pieces on the ground
  as before.
- Animals no longer count as hostile kills (a mountain lion or a dog going for the player added to the
  streak and showed on the UAV).
- A weapon switch mid-reload waits no more: the reload is cancelled and the other gun comes up at once
  (MW2's rule; a reload cut short before its add point puts no rounds in). GTA's own reload task is no
  longer started on the character under MW2.

### Known issues
- Now and then a rifle-armed NPC (SWAT, soldiers) fires a burst at a minigun's rate. This is a GTA V bug in
  two of its NPC rifle fire animations, not CODV's.

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

[Unreleased]: https://github.com/PurpleDarkO/CODV/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/PurpleDarkO/CODV/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/PurpleDarkO/CODV/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/PurpleDarkO/CODV/releases/tag/v0.1.0
