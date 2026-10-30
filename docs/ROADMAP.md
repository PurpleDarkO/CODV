# Roadmap

CODV brings Modern Warfare 2's gameplay into GTA V Legacy the way SK8V brings Skate 3: MW2's own engine code
(iw4L's crates, pinned and patched by `tools/prepare_overlay.py` and `patches/iw4l/`) runs inside the mod, and
GTA is the world. Every MW2 asset is read from the player's own copy at setup time.

## In 0.1.0

| Area | What works |
|---|---|
| Movement | MW2's pmove drives the player in Story Mode (run, sprint, jump, crouch, prone, mantle) against GTA's live collision, vehicles, props and peds; CODV's slide, dive, auto vault, high climb and lean; GTA's ladders and parachute keep MW2's view and HUD |
| Weapons | MW2's multiplayer weapons with their real fire rates, spread, kick, reloads, swaps and melee; weapon sets of three random guns per number key; MW2's frag; the Stinger and the Javelin lock on, with long-range missiles and a proximity fuse; shooting from a vehicle's seat |
| View and body | MW2's first-person view models with MW2's animation, bob and sway; MW2's character animation on the GTA ped in third person |
| Sound | MW2's weapon, footstep and killstreak sounds inside GTA's audio engine |
| HUD | MW2's compass, crosshair, scopes, weapon bar, hit marker, damage direction, grenade indicator, kill feedback, blood and score bar |
| Killstreaks | All ten of MW2's rewards, from the UAV to the Tactical Nuke, with their overlays, on spaced kill counts by default |
| Setup | Finds GTA V and MW2, imports MW2 from the player's machine and installs next to `GTA5.exe` |

## In 0.2.0

| Area | What came |
|---|---|
| Movement | Reload while sprinting; sprint and aim, the last pressed wins; no invisible walls from loose debris or broken fences |
| Weapons | A weapon switch cancels a reload at once (MW2's rule) |
| Police | Juggernauts join the police at four and five stars (up to three), with an entrance and a death cinematic |
| Killstreaks | The nuke ends the run (the streak and the held rewards start over), a round counter, an option for the nuke to kill the player; kills count in killstreak views; animals never count |
| Menu | A new look for the CODV menu; CODV stays on while it is open |

## Next

- Movement tuning: the remaining small hitches at car corners and low fences.
- More of MW2's arsenal: the Riot Shield, attachments (sights, suppressors, grips), equipment (Claymore, C4,
  Semtex, throwing knife) and special grenades (flash, stun, smoke).
- An in-flight sound for CODV's own missiles.

## Later

- A Zombies mode.
- Multiplayer.
