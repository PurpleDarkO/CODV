# Changelog

All notable changes to CODV. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
versions follow [Semantic Versioning](https://semver.org/). The runtime's C ABI version (`CODV_ABI_VERSION`) is
noted with each release: a host and a runtime only work together on the same ABI.

## [Unreleased]

## [0.6.0] - 2026-11-25

Runtime ABI 10, unchanged (the view model frame's first reserved field now names the brass bones; an older host
leaves it alone). **Run the setup again** to install it: it now also imports MW2's shell eject effects and their
models, and reads the current Steam build of MW2 (your `CodVLegacy.ini` settings are kept).

### Added
- MW2's shell casings: every shot throws the gun's brass from its ejection port (its `viewShellEjectEffect`,
  read from your MW2 at setup, with the effect's own models), spinning and falling as MW2's effect has it, until
  it reaches the floor. Menu HUD, `ShellEject` (on by default).
- A flashlight on the gun, CODV's own (MW2's guns have none): H turns it on and off while MW2 drives you
  (`FlashlightKey`; not in a killstreak's camera). Menu HUD, `FlashlightBrightness` (1 to 12, 4 by default).
- The AC-130 and the Chopper Gunner fly a GTA aircraft on their orbit, your view at its gun: the Titan 250 D
  (GTA's AC-130 gunship, its guns on the left, the side facing the centre) and the Hunter (its chin gun),
  banked into the turn. Before, they were the camera alone. On a game without them, the Titan and the Buzzard.
  Killstreaks menu, `GunshipAircraft` (on by default).

### Fixed
- No MW2 sounds with the current Steam build of MW2 (setup said "0 files"), and only part of its muzzle
  flashes: its zones use the x64 layout, which the sound and effect readers did not follow (models and the
  HUD already did). Both layouts now read the same sounds and effects. Run the setup again.
- A reinstall keeps every setting of `CodVLegacy.ini`, whether the menu saved it or you edited it. Before, it
  kept only a fixed list, and some menu options went back to their defaults: field of view, mouse sensitivity,
  invert look, MW2 movement, body pose and the muzzle flash options, among others.

## [0.5.0] - 2026-11-21

Runtime ABI 10, unchanged (the runtime's additions are optional options and requests). **Run the setup again**
to install it: it now also imports MW2's muzzle flash effects, and writes the new options (your `CodVLegacy.ini`
settings are kept).

### Added
- Walk: hold Caps Lock and MW2's player walks instead of running (40% of the run, MW2's own walking scale); on a
  controller the stick pushed part of the way already does. At a walk (slower than 2.6 m/s, aiming down the
  sights too) the steps make no sound. Under MW2's movement Caps Lock no longer triggers
  GTA's special ability.
- MW2's muzzle flash: every shot plays the gun's own view flash from MW2 (its `viewFlashEffect`, read from your
  MW2 at setup) at the barrel's tip, with MW2's flares, star spikes, glow and smoke, its atlas animation and
  blend, and the silencer's own smaller flash at the silencer's tip. Its light flashes on the world around you
  through GTA's lights. Menu HUD, `MuzzleFlash` (on by default) and `MuzzleFlashStrength` (0.1 to 1.5; 1.0 is
  MW2's own, 0.6 by default). Needs the MW2 import run again
  (INSTALL.bat), which now also writes the effects (`viewmodels/fx`).
- Next to a vehicle, hold F to get in (a tap is still the knife; before, the mode had to be switched off).
- Doors open under MW2's movement: walk into one and it swings away from you (through GTA's door system), and
  shuts again once you are past. They were walls before (GTA's doors swing when the ped's body pushes them; under
  MW2 the ped is placed, not pushed), except the few the game registers. A door the game locked (a mission's)
  stays shut and solid. The log names each door it opens.
- GTA's shops under MW2's movement: in one (Ammu-Nation, clothes, barber, tattoo, masks: an interior with the
  shop's radar icon near, or GTA's shop script running, as in a story mission that hides the icons), hold E a
  moment (D-pad right on a controller) and GTA has the player (its camera and body; the ped stands still
  meanwhile), so GTA offers its "press E" and a tap of E uses it. CODV takes the player back once GTA gives it
  back (the shop closed), as soon as the player moves, or when nothing was offered within 2.5 s. Elsewhere E
  held is only the lean. GTA offered none of them before: its scripts look for its own camera and a ped it
  moves. Not at a care package (E held opens it), nor on a controller while a killstreak waits. The log notes
  each hand-over.
- Killstreaks in story missions, as an option (Killstreaks menu, `KillstreaksInMissions=1`; off by default): the
  kills count and the killstreaks can be called in during a mission, as a real risk (a strike that kills the
  mission's allies or targets fails it). The Tactical Nuke there too with `NukeInMissions=1` (off: it waits
  for free roam). A death there ends the streak; a cutscene, a character switch or a fade ends the one
  running.
- The CODV menu shows the mod's version in its header's corner.
- A mission log for debugging story missions: during a mission or a cutscene, `CodVLegacy.log` notes what changes
  for the player as it happens (a cutscene, control taken and back, a screen fade, a character switch, a
  checkpoint teleport, a weapon or vehicle the mission gives, the wanted level, CODV's state and why it keeps
  its hands off), with a full line at the mission's start and every 30 s, and who killed the player. Free roam
  stays quiet.

### Fixed
- Franklin's sniper nest in Three's Company: the mission holds him there (attached to a prop) behind GTA's own
  scope, and CODV took him: he fell, GTA had him standing again, over and over, and MW2's gun and HUD showed
  inside GTA's scope. A player held on foot by a mission (attached to something) is GTA's now; CODV takes him
  back once the mission lets go. After a character switch, or the player placed far off at once, CODV also
  waits for GTA to settle him (not in the air, the collision around him loaded, a second standing still; 8 s
  at most). As a last resort, two quick falls back to the same spot leave the player to GTA until he walks
  3 m off or 6 s have passed.
- In a story mission MW2 starts on the slot matching the GTA weapon the mission gave (after a character switch
  or a new weapon while CODV's hands were off): a sniper rifle on the sniper (3), a launcher on the launcher
  (5), a pistol on the secondary (2); anything else keeps MW2's.
- A death with the mode off (a failed story mission, say) showed MW2's "Round Over" once the mode came on, and
  counted in the kill counter's deaths. A death counts now as a kill does: the mode on, and no story mission
  unless killstreaks count there.
- On a controller D-pad right (GTA's context action) is GTA's again under MW2's movement, unless a killstreak
  waits to be called in; it was taken whenever the lean was on (the lean is the keyboard's).

### Changed
- A vehicle is GTA's alone: its camera, HUD, body and weapons from the walk to the door until you are out again
  (MW2's player, gun and magazine wait for the way out). MW2's gun and HUD no longer stay on screen in the seat,
  and GTA's first-person vehicle camera is no longer forced. MW2 takes the player back once GTA's exit
  animation is over, the camera eases into the eye, and the gun comes on screen when it is there. Getting out
  no longer starts a ladder try (it left MW2's gun and HUD frozen over GTA's exit animation for up to three
  seconds), and the vehicle just left is solid again (the player walked through it).

## [0.4.0] - 2026-11-13

Runtime ABI 10, unchanged (the runtime's additions are optional options, flags and requests). **Run the setup
again** to install it: it now imports MW2's underbarrel weapons too (your `CodVLegacy.ini` settings are kept).

### Added
- MW2's attachments (menu Weapons, `Mw2PrimaryAttach` / `Mw2PrimaryAttach2` / `Mw2SecondaryAttach` /
  `Mw2SecondaryAttach2`): two per gun, as MW2 offers them (red dot, holographic, ACOG, thermal, silencer, FMJ,
  extended mags, heartbeat sensor, grenade launcher, shotgun, rapid fire, grip, akimbo, tactical knife). The gun
  becomes MW2's own variant with them, imported from your copy: the parts on the gun, its numbers (the silencer's
  shorter range, the extended mags) and its sounds. A gun without an attachment (or that pair) keeps the closest
  variant it has. Akimbo shows one gun for now.
- The sights' reticles: the red dot's dot, the holographic's ring and the ACOG's chevron are drawn as light on the
  gun as MW2 draws them (unlit, MW2's red for the red dot and the holographic, still visible against a bright
  sky); the built-in red dots (the TAR-21's) too.
- The thermal scope goes thermal: scoped through it, GTA's thermal view is on (off again as the scope goes; a
  killstreak's camera keeps its own).
- The heartbeat sensor works: its screen on the gun shows the ground ahead as MW2's does (you at the bottom, 40 m
  ahead at the top, range rings), and every 3 s a sweep runs up it and shows the hostiles there as red dots
  (police after you, anyone fighting you), fading until the next sweep.
- The underbarrel grenade launcher and shotgun: X (D-pad down) switches to them and back as MW2 does (its switch
  animations and times), each with its own rounds (the launcher's grenades go off as GTA's grenade launcher's,
  the shotgun fires MW2's pellets).
- Random guns (`RandomLoadout`, weapon sets) come with random attachments: each gun is one of MW2's variants of
  it, bare now and then, mostly with one or two attachments (not akimbo yet).

### Changed
- The menu's guns and attachments hold whenever random guns (`RandomLoadout`) is off, weapon sets included: they
  lead keys 1 and 2 (the other guns of each key are rolled, with random attachments). Turning random guns off in
  the menu puts them on at once.

### Fixed
- Attachment variants that sound as their base gun (a red dot or ACOG M4, for one) were silent: MW2's zones leave
  their sounds to the base gun's, which the import does not follow. The runtime now takes them from the base gun
  (from `<gun>_silencer_mp` for a silenced variant).

## [0.3.1] - 2026-11-10

Runtime ABI 10, unchanged. Run the setup again to install it (your `CodVLegacy.ini` settings are kept).

### Added
- Tracers (menu Weapons, `Tracers`): MW2's bullet tracers, a short bright streak racing from the gun to where
  the shot struck (from the muzzle's side of the view from the hip, just under the centre aiming down the
  sights), on about one shot in five like MW2, and on every round of the AC-130, the Chopper Gunner, the
  Attack Helicopter and the Sentry Gun (longer, hotter). Off, MW2's (the default) or every shot.

### Changed
- The minimap turns with where you look under MW2's movement, like MW2's radar (menu HUD, `MinimapFollowsView`):
  it followed GTA's camera, which CODV's view no longer matches, so up on the map was not always ahead. GTA's
  own minimap in a vehicle, in a killstreak's camera, or with the option off.
- The release carries its own Python (python.org's embeddable one, in `python\`): setup downloads nothing.

### Fixed
- Windows Defender flagged the 0.3.0 zip as `Trojan:Script/Sabsik.FL.A!ml` (a false positive of its
  machine-learning heuristic) and blocked the setup, which closed at once. The setup no longer downloads Python, the
  pattern the heuristic took for a trojan.

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

[Unreleased]: https://github.com/PurpleDarkO/CODV/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/PurpleDarkO/CODV/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/PurpleDarkO/CODV/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/PurpleDarkO/CODV/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/PurpleDarkO/CODV/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/PurpleDarkO/CODV/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/PurpleDarkO/CODV/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/PurpleDarkO/CODV/releases/tag/v0.1.0
