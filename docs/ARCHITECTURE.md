# Architecture

CODV has the same three parts as SK8V, DOGV and FIGHTV: a ScriptHookV ASI that talks to GTA, a Rust runtime DLL
that holds the logic behind a versioned C ABI, and a player-side setup.

```
GTA V (natives) <-> CodVLegacy.asi (C++ host) <-> C ABI <-> CodVRuntime.dll (Rust) -> codv-core
   controls, weapon, ammo, health, speed  ->  cv_step  ->  shots, kick, rates, reload, swap, HUD
   line test per shot                     ->  cv_report_hits  ->  damage per hit, hitmarker
   physics level found (reader thread)    ->  cv_set_physics_level  ->  worker reads GTA's map around the player
   MW2 player input                       ->  cv_mw2_step  ->  feet, eye, heading, pitch, stance
```

## What runs where

Modern Warfare 2's own engine code, compiled from iw4L's crates, runs in `CodVRuntime.dll`: the player's
movement (pmove), the weapons' state machine, the first-person view model's animation, the character animation,
the HUD and the weapon sounds' alias rules, all on MW2's numbers. GTA is the world: the host reads GTA's loaded
collision for the MW2 player to move against, places the GTA ped where MW2 puts it, poses it with MW2's
animation, fires GTA bullets along MW2's shots so impacts, blood, ped reactions and the police stay GTA's, and
draws MW2's view model and HUD over GTA's frame. Without an imported MW2, the interim gunplay of `codv-core`
(below) runs on GTA's own movement and weapons instead.

## codv-engine (rust/codv-engine): Modern Warfare 2's movement

The engine side of the SK8V-style rebuild (docs/ROADMAP.md). `tools/prepare_overlay.py` copies the iw4L
crates CODV compiles (pinned in `upstreams.lock.json`) into `build/overlay/iw4l` under a generated workspace
manifest and applies `patches/iw4l/*.patch` in order; a failed patch keeps the previous overlay. codv-engine
depends on those crates by path.

- `player.rs`: `Mw2Player` holds a retail `playerState_s` spawned the way iw4L's simulation spawns players
  (speed 190, gravity 800, eye height 60), builds one retail `usercmd_s` per frame (buttons, forward/right,
  view angles as shorts) and calls `movement_iw4::pmove` with the pmove context iw4L's simulation uses (back
  0.7, strafe 0.8, sprint 1.5 for 4 s, jump height 39, the 30 x 30 x 70 hull and mask `0x02810011`). The
  command clock starts at a live server's scale so retail timers such as the 500 ms jump gate behave.
- `world.rs`: `World` is what the host supplies (box sweeps in IW4 units); `Backend` adapts it to pmove's
  `CollisionBackend`. `BoxWorld` is a world of solid boxes swept with iw4L's own retail brush trace
  (`trace_iw4::trace_box`) for tests.
- `mesh.rs`: `MeshWorld` turns a triangle soup into Modern Warfare 2's own collision mesh tables (vertex
  segments of 1024, partitions of at most 24 triangles, a binary AABB tree over them; tables split spatially
  past 80,000 triangles) and traces them with `clipmap_iw4::trace_through_mesh_into`, the retail
  capsule-vs-mesh walk MW2 uses for terrain. Retail triangles are one-sided and GTA authors thin fences and
  signs as single layers, so each triangle goes in once per side. A dense 300,000-side area builds in about
  130 ms and a player sweep costs about 4 microseconds (release build).
- `units.rs`: inches and metres, IW4 yaw and GTA heading, IW4 and GTA pitch.

## GTA's collision as the MW2 world (F1a)

The same route SK8V takes for its skater, ending in MW2's mesh trace instead of the Skate engine's:

1. `host/src/physics_level.*` (Sol4ra's reader, from SK8V) finds GTA's physics level, the table of every
   collision object GTA has loaded, by scanning for the instances GTA's own natives name; no game-build address
   is stored. A reader thread re-walks it every 5 s and reports its address, or its loss, with
   `cv_set_physics_level`.
2. `rust/codv-gtaworld` (SK8V's `live.rs`, `bounds.rs`, `clean.rs`) walks the table with `ReadProcessMemory`
   and decodes the static map within an area into triangles: BVH and geometry bounds, fixed composites and
   authored primitives, by the ped flag rule (what a ped collides with, minus foliage), with repeats removed,
   foliage over ground dropped and gaps filled from the bullet/camera copy.
3. `rust/codv-runtime/src/collision.rs` runs that on a worker thread for a 128 m square around the player,
   builds the `MeshWorld` (IW4 units around the area's centre) and swaps it in. A new area is asked for 24 m
   from the centre; the player may move only within 48 m of it; every 3 s the worker checks whether GTA
   streamed collision in or out nearby. The player keeps the old area until the new one is ready, and holds
   still (`CV_MOVE_WAITING`) only when no area covers it.
4. The MW2 player moves in its own frame (GTA XY metres plus IW4 units) that re-centres every 256 m, so pmove's
   f32 coordinates stay small anywhere in Los Santos (8 km would otherwise leave 1/32-unit precision).

The runtime exposes `cv_mw2_spawn`, `cv_mw2_step`, `cv_mw2_despawn`, `cv_world_trace` (a sweep against the
meshes, GTA metres; the debug overlay uses it) and `cv_mw2_set_world` (an optional host trace callback, merged
with the map: nearer hit wins). In F2 the host applies `cv_mw2_step`'s output (feet, eye, heading, pitch,
stance) to the player and camera.

## Vehicles, props and peds (F1b)

`host/src/movers.*` lists, every frame the mode or the debug probe is on, the entities within 30 m of the
player, nearest first (at most 96), the way SK8V's dynamic world gathers them: GTA's matrix, the model's box
(`GET_MODEL_DIMENSIONS`) and the entity's fragInst (`probe::FragInstOf`). It leaves out the player and its
vehicle, peds in vehicles, dead or ragdolled peds, attached props and doors (GTA swings those open).
`cv_set_movers` hands the list to `rust/codv-runtime/src/movers.rs`:

- a vehicle or prop collides with GTA's own bound for its model, read once per model by the collision worker
  from that instance (`codv_gtaworld::live::instance_model`, the static map's decoder and ped flag rule) and
  accepted only if it fits the model's box; until then, or if unreadable, the model's box stands in;
- a ped is a box the size of MW2's player hull (30 x 30 units, its model's height), as MW2 players collide;
- litter under 30 cm is left out (the player steps over it).

Each body keeps its triangles turned by its matrix as a small MW2 mesh around its position; a body that only
moves reuses it (sweeps are shifted in), one that turns gets a new one. The player's sweeps take the nearest hit
of the map and the bodies; `cv_world_trace` also names the entity hit. A list older than half a second counts
as empty. Not yet: a moving car pushing the player, and standing on a moving vehicle (F2 decides what GTA's
own reactions keep doing).

## The player is MW2 (F2)

`host/src/mw2_driver.*` runs every frame the mode is on (`MW2Movement=1`):

- **Taking over.** On foot and eligible (not in or entering a vehicle, not in water, ragdoll, climbing,
  parachuting or dead), the MW2 player spawns at the ped's feet (MW2's own ground under it via
  `cv_world_trace`) facing where GTA's camera looked. Once the first step is not `CV_MOVE_WAITING`, the ped is
  frozen in GTA's physics and a scripted camera takes over.
- **Each frame.** `host/src/mw2_input.h` (plain C++, tested) turns GTA's controls into one retail usercmd: move
  normals to forward/right, mouse (unbound normal x sensitivity) or right stick (dead zone, squared curve,
  degrees per second) to yaw/pitch, sprint and jump held, ADS and attack, and the stance kept on the client as
  MW2 does (Ctrl / L3: tap crouch, hold 0.35 s prone; jump or sprint while down stands up a step). The
  gunplay's camera kick from the last frame is added to the view. `cv_mw2_step` returns feet, eye, heading,
  pitch and stance; the ped is placed at the feet (plus its model's origin height) with MW2's heading, and the
  camera at the eye with MW2's field of view (cg_fov 65 by default, as a vertical FOV for GTA). MW2's fall
  damage comes off GTA's health (MW2's 100 = GTA's 100 .. max). The ped and its gun are hidden from the
  player's own camera (`ShowBody=0`); MW2's view model replaces them (F4a).
- **Shots.** The gunplay fires from MW2's view (`gunplay::Camera`) instead of GTA's gameplay camera; its kick
  goes to MW2's view angles; crouch clipsets, move rate, sprint multiplier and the quick jump are off (pmove
  owns them). The runtime's flags for sprinting and airborne come from MW2's output.
- **Sprint and reload.** MW2 lets a sprint cancel a reload (weapon_iw4 drops the reload for the sprint-in).
  CODV keeps the reload: while the weapon is in a reload state `WeaponMoveFacts::hold_sprint` strips the sprint
  button from pmove (and ends a sprint), and a sprint button held through it sprints as soon as it ends.
- **Climbing and getting unstuck.** GTA's collision carries none of MW2's authored tags, so
  `codv-engine::world::Backend` answers pmove's mantle probe (`CONTENTS_MANTLE`) with any solid wall
  (normal z below 0.7) tagged `SURF_MANTLE_ON_OR_OVER`: jump at a ledge up to 60 units above the feet and MW2's
  mantle climbs onto it, or vaults over when nothing is beyond it. The climb's root motion is
  `player::ClimbRootDelta` (iw4L's mantle end deltas, up first then onto the ledge; the retail one comes from
  the mp_mantle_* animations). A hull overlapping the world cannot move (every sweep starts solid, and iw4L's
  penetration recovery has nothing to push out of in a triangle mesh): before each pmove a player whose
  standing hull is blocked but whose crouched one fits crouches (pmove stands it back up when there is room);
  otherwise it goes back to where it last stood free (forgotten when a climb starts, so a climb never sends it
  back before the ledge), or to the nearest free spot with room beyond it (`world::free_spot`,
  retail's one-unit jitter first, then rings up to 32 units); `Backend::correct_solid` does the same for
  pmove's all-solid ground trace. The debug line counts these. The jump height is
  the host's `JumpHeight` (CV_OPT_JUMP_HEIGHT, default 45 units against MW2's 39). The sprint length is the host's `SprintTime`
  (CV_OPT_SPRINT_TIME: MW2's player_sprintTime, default 8 s against MW2's 4, times the weapon's sprint scale).
- **Handing back.** Enter (F / Y) hands the player to GTA for 2.5 s to get into the vehicle; a teleport by GTA
  (more than 3 m from where MW2 put the ped) respawns MW2 there; the lifecycle blocks (missions off, cutscenes,
  pause, screen fades, character switches, scripted scenes that take the player's control, death) and the mode
  toggle stop it. The view model and MW2 HUD passes also skip any frame the script thread has not refreshed in
  200 ms (ScriptHookV's thread stops in the pause menu while the game keeps presenting). Stopping unfreezes the ped and gives GTA's camera MW2's pitch.

## MW2 weapons (F3)

- **Import (setup, on the player's machine).** `rust/codv-mw2data` (`codv-mw2import.exe`, run by
  `tools/prepare.py`'s `mw2` stage) opens `zone/english/common_mp.ff` (and `patch_mp.ff`, whose rows win) from
  the player's MW2: the signed fastfile's hash chunks are dropped, the zone inflated (miniz_oxide) and loaded by
  iw4L's `fastfile_iw4`. Every weapon's `WeaponGeometry` numbers, kick ranges, model and sound names and its own
  hit-location multipliers go to `<GTA>/CODV/mw2/weapons.json`, with MW2's global hit-location table
  (`info/mp_lochit_dmgtable`). Nothing from MW2 is stored in the repository or a release.
- **Arsenal.** `codv-engine::arms` turns the table's bullet guns into `weapon_iw4` combat facts (fire and burst
  timing, clip and reloads, ADS, spread, damage and ranges, baked location damage) and pmove facts (move, ADS
  and sprint scales, ADS rates). `Gunner` keeps the loadout and each weapon's ammo, and each frame runs
  `weapon_hands` (fire, ADS, reload, sprint, raise and drop, melee), MW2's aim spread
  (`adjust_aim_spread_scale`, `fire_weapon_spread_degrees`) and view kick (`weapon_fire_recoil`, a damped return
  to centre). Shots are given relative to the view without the kick.
- **Runtime.** `rust/codv-runtime/src/arms.rs` loads the table and `<GTA>/CODV/mw2.json` (loadout, the GTA
  weapon each MW2 base name borrows, damage scale). `cv_mw2_step` handles `weapon_request`, sets the held
  weapon's facts on the player before pmove and ticks the weapon after it, returning `CvMoveOutput.kick`.
  `cv_step` then answers the host's gunplay from that frame (shots with the stand-in GTA hash, HUD with the MW2
  name, `mag_size`, reload / melee / equip flags) and `cv_report_hits` returns MW2's damage: falloff by range x
  the hit location's multiplier (`CvHit.hitloc`, else head / upper torso) x CODV's scale.
- **Host.** `mw2_input.h` adds reload, melee and the next-weapon press; `mw2_driver` adds the kick to the
  scripted camera only; `gunplay` traces each shot from MW2's view, picks the MW2 hit location from the nearest
  GTA ped bone segment (`aim::HitLocation`), fires GTA's bullet with MW2's damage and gives the stand-in GTA
  weapon when needed.
- **Not yet.** Grenades, launchers and equipment; attachments; MW2's bullet penetration and its own world
  trace for bullets; MW2 sounds and effects (F5).

## MW2 view model (F4a)

- **Import.** `codv-mw2data::import_viewmodels` (run by `codv-mw2import` after the weapons) walks the zones
  with ports of iw4L's readers (`asset_model::model_skel`, `asset_anim::xanim_catalog`, the material and
  image records of `fastfile_iw4`) and keeps what the holdable guns use: the gun and hands XModels (LOD 0,
  vertices unpacked, skeleton as `xmodel_runtime::ModelPoseSrc` takes it), every szXAnims clip
  (`RawXAnimParts`, decoded at runtime) and the colour and normal maps. Images come from `main/iw_*.iwd`
  (zip) as `.iwi` and are written as `.dds` (BC1/2/3 or RGBA8 with mips). Files: `viewmodels/models/*.cvm`,
  `anims/*.cva`, `images/*.dds` (`codv-mw2data::vm`); each gun's row gains `gun_model`, `hand_model`,
  `hide_tags` and `xanim.<slot>`. Camouflage model variants are skipped.
- **Rig.** `codv-engine::viewmodel` ports iw4L's view model path without Bevy: the clip scheduler
  (`asset_anim::ClipScheduler`), the controller choosing the szXAnims slot from the weapon state edges and
  shots (`render_anim::viewmodel_controller`, `anim::fpv`, the ADS up/down scrub), the assembly of hands and
  gun on `tag_weapon` as one `xmodel_runtime::DObj` with hide tags (`asset_game::FpvAssembly`,
  `anim::fpv_rig`), and MW2's placement (`weapon_iw4::weapon_placement_assemble`: bob, stance and movement
  offsets, idle and look sway, gun kick). Output: one 3x4 skinning matrix per bone in view space (D3D axes,
  inches).
- **Runtime.** `codv-runtime/src/viewmodel.rs` loads models and clips on first use, keeps one rig for the held
  gun and ticks it after each `cv_mw2_step` from the weapon frame and the player. ABI 4: `cv_vm_frame` (mesh
  id, bones, ADS fraction, ADS zoom FOV), `cv_vm_mesh_info` / `cv_vm_mesh_data` (vertices, indices, draws with
  their image names and alpha-test / blend / two-sided flags).
- **Host.** `viewmodel_feed` (script thread) fetches the bones every frame and the mesh once per id with its
  DDS images, computes MW2's lens (`mw2input::LensFov`: cg_fov narrowed towards the weapon's ADS zoom FOV,
  also applied to the scripted camera) and a key / ambient light from GTA's clock and interiors.
  `viewmodel_render` (ScriptHookV's present callback, render thread) skins on the GPU and draws over the
  backbuffer with a private depth buffer, opaque then blended surfaces, saving and restoring GTA's pipeline
  state (`d3d11_saved_state.h`, after SK8V's x-ray pass).
- **Weapon switches.** The first-time pullout is scaled to the weapon's `first_raise_time_ms` (retail's
  ANIM_RATE_TABLE scales slot 0xE by firstRaiseTime), so it ends when the weapon is ready instead of playing on
  at its native length and being cut by the next action. Each weapon keeps its mesh id in the runtime and its
  GPU buffers in `viewmodel_render` (up to 8), so switching back to a gun neither fetches nor uploads it again.
- **Not yet.** GTA's lighting and shadows on the gun (a simple key + hemisphere light stands in), muzzle
  flash and shell ejection (F5 effects), notetracks (F5 sounds), attachments and camouflage.

## MW2's crosshair and scope (start of F5b)

- **Import.** Each weapon's row also carries its reticle images (`reticle_center_image`, `reticle_side_image`,
  resolved through the materials' 2D colour map) and its ADS overlay (`overlay_image`, the sniper scope), the
  reticle sizes (`reticle_center_size`, `reticle_side_size`, `reticle_min_ofs`, `hip_reticle_side_pos`), the
  ADS crosshair fractions and `ads_overlay_width` / `height`; the images go to `viewmodels/images/` with the
  view model's. Frags are imported too (their view model and centre reticle).
- **Runtime.** `codv-runtime/src/hud.rs` computes MW2's HUD with iw4L's `hud_iw4` (compiled from the overlay):
  `hip_crosshair_visible`, `hip_crosshair_trans_scale`, `reticle_draw_size`, `calc_reticle_spread` (the side
  images open with the weapon's spread cone and `aim_spread_scale`, which weapon_iw4 now raises with the
  usercmd's movement), `calc_reticle_alpha`, and the iris (`get_weap_reticle_zoom`, `draw_weap_reticle`,
  `ads_overlay_uses_four_quads`). ABI 7 `cv_mw2_hud` returns it for the camera's FOV; `cv_vm_frame` sets
  `CV_VM_SCOPED` while the overlay covers the view (iw4L hides the view model then).
- **Host.** `hud_render` (in `viewmodel_feed`'s present callback, after the view model) draws the four side
  images turned 0/90/180/270 degrees and the centre image on MW2's 480-unit virtual screen, and the overlay
  (one centred quad, or four mirrored quarters) with black around it. The host's rect crosshair (`hud.cpp`)
  stays off while MW2's draws.

## MW2's HUD (F5b part 1)

- **Import.** `codv-mw2data::import_hud` (`hud.rs`) reads, with `fastfile_iw4`'s menu, font and localize records,
  MW2's `weaponbar_hd` menu from `common_mp.ff` / `patch_mp.ff` (item rectangles with the menu's origin,
  alignments, colours, fonts, owner draws, and each item's visibility expression reduced to `variable op value`,
  e.g. `stockAmmo < 100`), the HUD fonts and strings from `localized_code_post_gfx_mp.ff` (glyph tables; the atlas
  `gamefonts_pc` is in `main/localized_<lang>_iw*.iwd`), and the HUD materials' images (ammo counter rounds,
  `damage_feedback`, `hit_direction`, grenade icon and pointer, each weapon's HUD icon). It writes
  `mw2/hud/hud.json` (`hudfile`) and the images beside the view model's.
- **Runtime.** `codv-runtime/src/mw2hud.rs` turns the weapon's last tick into quads with iw4L's HUD logic
  (`hud_iw4`): `ScreenPlacement` for MW2's 640 x 480 alignments, the clip rounds (`clip_pip_*`), the stock digits
  and weapon name laid out glyph by glyph as iw4L's draw2d does (drop shadows by text style, the name fading after
  a switch), the low ammo warning and its pulse; the hit marker as `_damagefeedback.gsc` places and fades it; the
  damage arrows as iw4L's `hud/src/damage_direction.rs` (cg_hudDamageIcon* defaults); the grenade indicator
  (cg_hudGrenade* defaults) for CODV's own frags and GTA's grenades; the compass from the menu's passes: each
  `compass_alpha` item is not drawn but masks the letter rings after it (MW2 writes it to the destination alpha;
  CODV hands it to the quad as `CV_HUD_QUAD_MASKED`), and the rings' texture corners turn by -(yaw - north) as
  iw4L's `rotate_st_corners` (north: GTA's +Y, IW4 yaw 90). Every hit plays `MP_hit_alert` (imported with the
  weapons' sounds). ABI 7: `cv_mw2_hud_draw`,
  `cv_mw2_hud_image`, `cv_mw2_hud_damage`.
- **Host.** `hud_events` finds who hurt the player (the nearby ped GTA recorded with
  `HAS_ENTITY_BEEN_DAMAGED_BY_ENTITY` when its health drops) and GTA's live grenades
  (`GET_COORDS_OF_PROJECTILE_TYPE_WITHIN_DISTANCE`); `viewmodel_feed` fetches the quads every frame for the last
  backbuffer size and `hud_render` draws them after the crosshair and scope, batched by image. `hud.cpp`'s ammo,
  hit marker and sprint meter stay off while MW2's HUD draws (`Mw2Hud=0` brings them back).

## Killstreaks (F5c)

- **Import.** `import_hud` now also reads `code_post_gfx_mp.ff`: the string tables (`hud.rs`'s
  `capture_string_table`, as iw4L's `menu_catalog`), MW2's `killstreak` splash menu (title, description, icon;
  its OnOpen / OnCloseRequest slide from x -640 and to +640 over 0.15 s), `dpad_hd`'s action slot 4 (owner draw
  174, the key label after it) and each material's texture atlas (the killstreak dpad icons are 4 x 8 / 2 x 8
  flipbooks). `killstreaks.rs` joins `mp/killstreakTable.csv` (kills, earn hint, sound, dialogs, icons) with
  `mp/splashTable.csv` (title, duration, glow) into `hud.json`'s `killstreaks`. `import_sounds` adds their
  aliases (the jingle, `us_1mc_achieve_*`, `us_1mc_use_*`, `emp_activate`: US Army Rangers' announcer) and
  writes streamed sounds from `main/*.iwd`'s `sound/` (`iwd::Images::open_prefixed`).
- **Runtime.** `killstreaks.rs` keeps `_killstreaks.gsc`'s rules: each kill adds to the streak, a picked reward
  comes at its exact kill count, rewards wait on a stack (last earned first) and survive death, death resets
  the streak. `mw2hud.rs` draws the splash ("&&1 Kill Streak!", the earn hint with the key, the icon; the title
  with the splash table's glow as a soft halo) and the held reward's flipbook in action slot 4. ABI 8:
  `cv_killstreak_step` (hostile kills, death, mode off and the activation in; the reward called in out), options
  `CV_OPT_KILLSTREAKS`, `CV_OPT_KS_SLOT1..3`, `CV_OPT_KS_EMP_KILLS`. Stage 1 implements the UAV and the EMP.
- **Host.** `gunplay` remembers the peds hit in the last 0.7 s and whether each was hostile while alive
  (`killstreak_rules.h`: police / SWAT / army ped types, `IS_PED_IN_COMBAT` with the player, a hate relationship,
  or damage to the player); a hostile one dying counts. `killstreak_feed` sends the kills, the death edge, the "4"
  key (`INPUT_SELECT_WEAPON_HEAVY`, taken from GTA under MW2) and the D-pad's right (raw XInput; GTA's
  `INPUT_CONTEXT` is taken only while a reward waits), and carries out the reward on GTA's game clock: the UAV
  adds red blips to hostile peds within 300 m every second for 30 s; the EMP draws `_emp.gsc`'s white flash
  (0.1 s, fading over 3 s), `SET_ARTIFICIAL_LIGHTS_STATE` for 60 s, stalls engines within 150 m and sends aircraft
  out of control. Nothing counts in a story mission (`GET_MISSION_FLAG`), and the effects stop there or when the
  mode is switched off.

### Stages 2-4

- **Import.** `import_hud` also keeps MW2's killstreak overlay menus (`KILLSTREAK_MENUS`: the Predator's
  `missilecam_hud_hd`, `ac130_hud_hd`, `remote_chopper_overlay_hd`, `targetmap_fullscreen`) with their images and
  "@KEY" strings (from `localized_common_mp.ff` too), the thermal FOF box (`hud_fofbox_hostile`), the crates' use
  hints (`MP_*_PICKUP`), and `import_sounds` the killstreaks' own aliases (`KILLSTREAK_SOUNDS`: the AC-130's guns
  and shells, the remote missile, sentry, airstrike and nuke sounds).
- **Runtime.** `mw2hud/streaks.rs` draws the view the host reports (`cv_killstreak_view`): each overlay from its
  menu items (the AC-130's frame and gun label for the gun in use, flickering as the menu's
  `1 - sin(ms / 90) * 0.65`, "Reloading", the time left as secondsAsCountdown; the Chopper Gunner without its
  owner draws), FOF boxes sized as cg_fofIcon* does, the Predator's static, the nuke's countdown and a crate's
  use bar. Quads gain `CV_HUD_QUAD_SOLID` and `CV_HUD_QUAD_ADDITIVE` (the FOF box is red on black: added).
  `cv_killstreak_step` takes a reward handed over (`give`: a crate's content, with MW2's pickup splash);
  `cv_play_alias` plays any imported alias; `cv_mw2_weapon_numbers` gives the killstreak guns' fire times, clips,
  reloads, blast radii, shell speeds and zoom; `cv_mw2_refill` is a crate's ammo. ABI 9.
- **Host.** `killstreak_remote` owns the views that take the camera (a scripted camera; MW2's eye steps aside with
  `mw2::Driver::Suspend`, the MW2 player stays frozen and vulnerable): the Predator steers with iw4L's numbers
  (pitch 15 / yaw 20 deg/s, 3000 units/s cruise, one 6000 boost) and blows up where its line meets the world;
  the AC-130 and Chopper Gunner circle the call point (70 s / 40 s per turn) with GTA's thermal
  (`SET_SEETHROUGH`), the guns' rates, clips and reloads, and shells that fly at the gun's speed; the target map
  pans a camera above the ground. `killstreak_units` flies a friendly Buzzard on a circle around the player
  (targets each 0.5 s, 40-round bursts), sets a minigun sentry on the ground (MW2's bursts, pauses, overheat),
  drops a crate on a flare (MW2's odds, hold E 0.5 s), passes two jets with 12-blast fans, and runs the nuke
  (ticks, slow motion, everyone near dies). Explosions and bullets have no owner, so the wanted level and the
  streak stay untouched.

### Reading ahead (no hitch at switch-on)

Decoding a gun's view model (its `.cvm` models and `.cva` animations) takes 100 to 450 ms, its sounds and DDS
images more disk time: done when the gun is first held, it stalled the game thread at switch-on. The runtime now
decodes the guns the player can hold next (the loadout and the frag: `Arms::ready_set`) on background threads
when it loads and whenever the loadout changes (`Viewmodels::prewarm`, `Audio::prewarm`; the caches are shared
and locked only to look up and insert). `cv_vm_ready_images` (ABI 10) lists those view models' images and every
HUD image, which `vmfeed::Prewarm` reads on a low-priority thread. With random guns the host rolls the next
switch-on's loadout at switch-off, so it is read while the mode is off; the first step after switch-on drops
from about 480 ms to about 1 ms. Only the GPU upload of the new textures stays on the render thread.

## MW2's body (F4b)

- **Import.** `codv-mw2data::import_viewmodels` also captures the rawfiles `animtrees/multiplayer.atr` and
  `mp/playeranim.script` from `common_mp.ff`, compiles them (`codv-mw2data::playeranim`, ported from iw4L's
  `asset_anim::animtree` / `atr_compile` / `playeranim_parse`), writes every animation the compiled tree
  uses and the skeleton of `mp_body_desert_tf141_assault_a` (from the first map zone that has it, `mp_rust.ff`
  first; surfaces left out) to `<data>/mw2/body/`.
- **Animation.** `codv-engine::body` ports iw4L's player animation path: after pmove (whose move type,
  stance event, jump and landing `Mw2Player::anim` now keeps) and after the weapon (fire, reload, raise,
  drop, melee events from the `GunFrame`), the player script (`sim::player_anim_script`) writes the legs
  and torso animations into the player state; the tree (`render_anim::remote_animation`) is rebound to
  them with MW2's goal weights, blend times and move-speed rates; the soldier is posed with
  `xmodel_runtime::apply_player_controller` (torso pitch from the view, prone).
- **Retarget.** `codv-engine::body::retarget` is SK8V's method (`skatev-runtime/src/ped.rs`) with MW2's
  joints as the source: MW2's axes onto the GTA ped's (from both binds), each mapped GTA bone wears its MW2
  joint's change from bind through a segment fit, the pelvis is placed between MW2's hips (lowered for a
  shorter ped), two-bone solves put the GTA ankles and wrists on MW2's (feet on the ground, hands on the
  gun), fingers follow, roll bones take their share of twist, shirt helpers under the legs follow the pelvis.
  The result is the ped's object space.
- **Runtime.** `codv-runtime/src/body.rs`; ABI 5: `cv_body_set_skeleton` (the GTA ped's skeleton as JSON and
  the ground's height in its object space), `cv_body_pose` (the bone object matrices of the last step);
  `CvMoveInput.third_person_range / angle` asks pmove's step for MW2's cg_thirdPerson camera
  (`playerstate_iw4::offset_third_person_view`, pulled in front of walls by the MW2 world) in
  `CvMoveOutput.camera`.
- **Host.** `body_feed` resolves the ped's live skeleton once a second (`probe::ResolvePedSkeleton`,
  `SkeletonJson`, from SK8V), hands it to the runtime when it changes, installs `pose_hook` (SK8V's: the
  player ped's animation pass at vtable slot 65 and two anim-job tasks write the pose after GTA's) and
  publishes the pose every frame; hand-backs fade over 250 ms. `mw2_driver` toggles the third-person camera
  on C (keyboard; GTA's look-behind) / Back (pad; GTA's camera button) and shows the ped there; shots still leave from the eye, as in MW2.
- **Not yet.** Legs turning separately from the torso (MW2's turning animations), leaning, death and pain
  animations, the GTA stand-in weapon's grip matching MW2's hands exactly.

## Guns, ammunition and frags (gameplay tuning)

- **Guns.** `codv-runtime/src/arms.rs` `MENU_GUNS` lists MW2's base guns (no attachments) in MW2's class order;
  those the player's MW2 has become the menu's list (`cv_mw2_gun_count` / `cv_mw2_gun_info`), and
  `CV_OPT_MW2_PRIMARY` / `SECONDARY` put one in a slot (a fresh loadout; the new gun is raised).
- **Keys.** `NumberKeys` (on): 1 / 2 (GTA's first weapon-category controls, taken under MW2) take out the
  primary / secondary (weapon_request loadout slots). G throws the frag, V is MW2's melee (the held gun's knife;
  GTA's camera key on the keyboard, R3 on a pad), C flips MW2's third-person camera. The runtime still accepts
  `CV_WEAPON_REQUEST_KNIFE` / `GRENADE` (the knife or the frag on the trigger, `Arms::hand`), unused by the host.
- **Random loadout** (`RandomLoadout`, on): each switch-on the host sends `CV_OPT_MW2_RANDOMIZE`; `Arms::randomize`
  picks a primary from MW2's assault rifles, SMGs, LMGs and snipers and a secondary from its shotguns, machine
  pistols and handguns (never the same pair twice running).
- **Ammunition.** `Gunner::with_reserve_scale`: spare rounds times `AmmoScale` (3; `data/mw2.json`
  `ammo_scale`), refills included.
- **Frags.** `Arsenal` keeps the thrown frag-class grenades as offhand weapons (ids after the guns').
  `Gunner::with_frags` fills weapon_iw4's `OffhandCmd` (as iw4L's sim/combat.rs) so MW2's frag button runs the
  retail offhand states: quick drop, pin pulled (`OffhandPrepare`), held with the fuse running (cooking), thrown
  on release (`OffhandUsed` with the fuse left) or gone off in the hand (`OffhandCookedOff`). While the offhand
  view flag is set the view model shows the grenade (`GunFrame::offhand_view`), and its notetracks play its
  sounds. `codv-engine::grenade` flies it: iw4L's launch velocity, MW2 gravity, `bounce_reflected_scale`
  against the MW2 world (one set of bounce coefficients for every surface), rest on the floor, the fuse. The
  runtime moves grenades with the player's frame and reports them (`cv_mw2_grenades`); the host's
  `grenade_feed` moves a GTA frag model along and calls `ADD_OWNED_EXPLOSION` (GTA's grenade explosion, owned by
  the player) where one goes off, so GTA deals the damage.

## MW2 weapon sounds (F5a)

- **Import.** `codv-mw2data::sounds` captures every SndAliasList row and LoadedSound of a zone (after iw4L's
  `asset_audio` `sound_catalog::capture_iw4`: alias rows at `asset_iw4::snd_alias` offsets, the SoundFile
  edges to loaded sounds, MSS PCM headers). `import_sounds` reads `localized_common_mp.ff` (where MW2 keeps
  its sounds), `common_mp.ff` and `patch_mp.ff`, keeps the aliases the imported weapons name (the
  `*_sound_player` fields and the `notetrack_sounds` map `import_viewmodels` now records, following
  secondary and chain aliases), and writes `<data>/mw2/sounds/aliases.json` (format `codv-mw2-sounds` v1:
  per alias its variants with file, volume and pitch ranges, probability, start delay, secondary, chain,
  flags) and `files/*.wav`.
- **Events.** The view model's scheduler now reports the notetracks its clips cross (forward only, after
  iw4L's `clip_scheduler` crossings; `Rig::take_notes`). `codv-runtime/src/audio.rs` maps a `GunFrame` to
  aliases as IW4's client does (fire / fire last, dry fire, raise / first raise, put-away, reload / reload
  empty, melee swipe) and notetracks through the weapon's `notetrack_sounds`.
- **Mixer.** A variant is picked by probability (0 counts as 1, as iw4L's catalog), with a random volume and
  pitch in its ranges and its start delay; the secondary alias starts with it and the chain alias after it.
  Up to 32 voices, linearly resampled to 48 kHz, mixed dry into one mono stream. The held weapon's sounds
  are read from disk when it is first held.
- **Runtime ABI 6.** SK8V's audio contract with one emitter (the player's weapon): `cv_audio_configure`,
  `cv_audio_set_paused`, `cv_audio_emitters`, `cv_audio_pull` (renders the requested frames on demand, from
  the host's feeder thread, under the runtime's audio lock), `cv_audio_get_status`. The emitter carries the
  player's eye position and `CV_AUDIO_EMITTER_LISTENER` in first person.
- **Host.** `gta_audio` is SK8V's GTA audio bridge cut down to one stream (`gta_audio_re` and
  `gta_audio_dsp` unchanged): the guarded GTA calls create a `BINK_MONO_SOUND` on the frontend audio entity
  fed by an audReferencedRingBuffer, non-positional (Pan 0) in first person, positional with its own
  naEnvironmentGroup in third person (recreated on the switch); a feeder thread tops the ring up every 3 ms
  through a limiter (`SoundGainDb`); the pause menu and a missing script heartbeat pause it; it never runs
  in a network session. While it plays, `gunplay` fires CODV's GTA bullets inaudibly.
- **Not yet.** Footsteps, MW2's NPC (`_npc`) and distant shot sounds, impact and melee hit sounds, the
  Rockstar Editor (SK8V's clip recording).

## codv-core (rust/codv-core): interim gunplay

Pure Rust, no FFI, no clock, no third-party crates.

- `defs.rs`: the weapon table (`data/weapons.json`, parsed with the small `json.rs`): player rules (sprint
  meter, walk speed for full move spread, ADS move scale, crouch scales, air spread, regeneration, quick jump,
  melee, trace range), the default loadout and every weapon (GTA weapon and its joaat hash, fire mode, RPM,
  burst, damage and ranges, head multiplier, pellets, magazine and reserve, reload times including per-round,
  ADS, sprint-out, raise and drop times, hip and ADS spread, bloom, move spread, kick ranges, ADS kick, kick
  recovery, movement scales). Validation names the weapon and field.
- `player.rs`: one script frame of handling. Inputs: the buttons, on foot / airborne / sprinting, speed, the GTA
  weapon in hand and its ammo, health. Outputs: the shots with their spread offsets, the camera kick, the move
  rate and sprint multiplier, whether to block GTA's sprint, a weapon to equip, reload and melee starts, rounds
  spent, quick jump, crouch changes, health to restore, and the HUD values. The weapon state machine is Ready,
  Reloading (timed or per round, cancelled by sprint), Raising, Dropping and Melee; fire timing carries each
  frame's overshoot so cadence is exact at any frame rate; a dry trigger and an empty magazine reload by
  themselves. Seeded: the same seed gives the same spread pattern.

## codv-runtime (rust/codv-runtime)

`CodVRuntime.dll`: `cv_api_version`, `cv_create`, `cv_destroy`, `cv_step`, `cv_report_hits`, `cv_report_kill`,
`cv_reset`, `cv_set_option`, `cv_weapon_count`, `cv_weapon_info`, `cv_rules`, `cv_status_text`. Every export
catches panics and fails closed. Structures are mirrored in `host/include/codv_runtime.h`; sizes and offsets
are asserted on both sides. The table loads from `<GTA>/CODV/weapons.json`; a missing or invalid file falls back
to the copy compiled into the DLL, and the log says why.

## Host (host/)

- `main.cpp`: settings, the mode toggle (F10 / Back + RB), the loadout, and each frame: control policy, read the
  buttons, gather GTA state, step the runtime, apply, draw the HUD.
- `input.h`: which GTA control action makes each button (pure, tested). They are read with
  `IS_DISABLED_CONTROL_PRESSED`, so they work while GTA's own use of them is disabled and the player's bindings
  carry over.
- `controls.h`: what GTA loses each frame (pure, tested). With a CODV weapon in hand: the trigger, reload,
  weapon switching and GTA's melee; on foot: crouch, look-behind (the quick melee) and cover; sprint when the
  meter is empty; jump when quick jump is on. GTA keeps aiming, walking, the camera, vehicles and pause.
- `gunplay.cpp`: for each shot, a direction from the gameplay camera's rotation plus the spread offsets
  (`aim.h`, tested), a synchronous line test from the camera, the hit's distance and headshot test, the damage
  from `cv_report_hits`, then GTA's own bullet along the same line with that damage (or, in GTA trigger mode,
  `SET_PED_SHOOTS_AT_COORD`). It also kicks the camera (relative pitch and heading), mirrors the magazine into
  GTA's clip and plays GTA's reload, swaps weapons, plays the melee clip and lunges, sets the crouch clipsets,
  the move rate and sprint multiplier, regenerates health, keeps first person and turns a hit ped's death into
  a kill marker.
- `hud.cpp`: crosshair sized from the spread and the camera FOV (when MW2's own is not drawn), hitmarker, ammo and weapon name, reload bar,
  low and no ammo, sprint meter, damage flash; GTA's reticle and weapon icon are hidden while CODV drives the
  weapon.
- `host_data.cpp`: `CodVHost.ini` (melee clip, crouch clipsets, sounds; pure parser, tested).
- `natives.h`: every native by hash, one per line, checked against the pinned nativedb by
  `tools/check-natives.py`.

## The MW2 item in GTA's weapon wheel

As SK8V's skateboard item: a weapon of CODV's own in GTA's weapon wheel turns the mode on.
- **Setup** (`prepare.py` stage `wheel`): `codv-gtapack` (rust/codv-gtapack, its own workspace on the pinned
  rpf-archive-rs) recovers GTA's keys from the player's GTA5.exe and extracts `weapons.meta` /
  `weaponanimations.meta` and `hud.gfx` from update.rpf (SK8V's gta-archives reader). `build-mw2-weapon.py`
  clones WEAPON_HAMMER into WEAPON_CODV_MW2 (slot right after fists, label `WT_CODV_MW2` = "MW2");
  `codv-gtapack pack` writes the OPEN DLC pack (setup2.xml, content.xml, the meta, dlctext and the label's
  gxt2: SK8V's live_clip pack without the clip); `build-mw2-icon.py` adds the INT<hash> icon frame (CODV's
  rifle + "MW2" art, `tools/mw2-wheel-icon.bin`) to the slot sprites of the player's hud.gfx (OpenIV's mods
  copy, else SK8V's pack copy so both icons show, else GTA's). Installed to `update/x64/dlcpacks/codv/`.
  The stage is optional: when it cannot run, F10 still turns the mode on.
- **Host**: `pack_loader.h` (SK8V's) mounts `dlcpacks:/codv/` at the end of GTA's DLC list, lets GTA read
  OPEN archives and points the `hud.gfx` registration at CODV's copy. It chains after another mod's hooks on
  the same six call sites, and when SkateVLegacy.asi is present it lets SK8V's loader patch first.
  `main.cpp` grants the item every 2 s, hides its hammer model and turns the mode on when it becomes the
  selected weapon; F10 / Back + RB / the menu still toggle. `ModeBanner` (off) shows the on / off banner.

## Setup (tools/)

`prepare.py` runs resumable stages (gta-check, weapons, verify) with receipts, then installs through
`setup_engine.py`: preflight (GTA5.exe present and Legacy, ScriptHookV present, GTA not running, folder
writable), a transactional copy with rollback, an ownership manifest (`CodVLegacy.installed.json`) and the INI
merge that keeps the player's menu settings. `build-and-install.ps1` (INSTALL.bat) chains the requirements
check, `BOOTSTRAP.bat -SkipCheck` and `prepare.py --install`. `check-data.py` verifies every GTA name CODV ships.

### CODV's movement on top of MW2's

All in `codv-engine::player` around iw4L's pmove, each one a runtime option (CV_OPT_*) and an INI key:
fall damage from CODV's heights (`FallDamage`, light: 6 m to 20 m; pmove's hard landing dropped for
drops those heights spare), the slide (`Slide`: crouch while sprinting), the dive (`Dive`: crouch held
through the slide), sprint speed (`SprintSpeed`), MW2's jump slowdown off (`JumpSlowdown=0`: a landing
keeps its speed), the auto vault (`AutoVault`: MW2's automantle perk while sprinting forward), the mantle
pace (`MantleSpeed`) and the high climb (`HighClimb`: a jump at a 62 to 96 unit ledge climbs it in
650 ms). The HUD's sprint meter (`StaminaBar`) sits under the weapon bar. The host lends the player to
GTA for ladders (walking into a tall wall head on, an edge with a 3 m drop, or the enter key) and for
skydives with a parachute; MW2's view and HUD stay meanwhile (`mw2::Driver::Borrowed`). View model
textures read ahead are created on the GPU one per frame (`vmrender::Warm`).
