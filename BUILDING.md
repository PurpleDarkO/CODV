# Building CODV

Windows x64 only. The result is the same release folder and zip a published CODV download contains.

## Requirements

- Git
- Rust via rustup (the pinned 1.98.1 toolchain installs itself from `rust-toolchain.toml`)
- Visual Studio 2022 with "Desktop development with C++" (MSVC)
- CMake 3.24 or newer
- Python 3.11 or newer with the `py` launcher (standard library only)

## Steps

1. Clone this repository.
2. Run `CHECK.bat`. It checks (read-only) Windows x64, Git, Rust, Visual Studio 2022 with the C++ tools,
   the Windows SDK, CMake/CTest on PATH, Python, free disk space, the checkout path and the download hosts,
   and prints how to fix anything missing. `CHECK.bat -Offline` skips the network checks;
   `CHECK.bat -Gta "<GTA V folder>"` also checks the GTA folder for install (GTA5.exe build, ScriptHookV,
   write access).
3. Run `BOOTSTRAP.bat`. It runs the same check first and stops if something is missing
   (`BOOTSTRAP.bat -SkipCheck` to try anyway), then:
   - fetches the pinned nativedb, iw4L and rpf-archive-rs into `upstream/` (`upstreams.lock.json`; `-All` also fetches the
     reference-only repositories);
   - fetches the ScriptHookV SDK into `third_party/` (`external.lock.json`);
   - checks `host/src/natives.h` against the nativedb (`tools/check-natives.py`);
   - builds the iw4L overlay (`tools/prepare_overlay.py`: the pinned crates plus `patches/iw4l`, in
     `build/overlay/iw4l`), then builds and tests the Rust workspace, then the ASI and its host tests (later
     rebuilds: `BUILD.bat`).

   Shortcut for steps 2-3 plus the install: `INSTALL.bat` (GTA V found automatically on Steam, the Rockstar
   Games Launcher or Epic, or `INSTALL.bat "<GTA V folder>"`) checks the requirements and the GTA folder, runs `.\BOOTSTRAP.bat -SkipCheck`
   (fetches what is missing, then builds), then `py .\tools\prepare.py --gta "<folder>" --install`, asking for
   administrator rights only if the GTA folder needs them. `INSTALL.bat -SkipBuild` reinstalls the last build.
4. Optional: `py tools\check-data.py` checks every GTA weapon in `data/weapons.json` and the melee clip, crouch
   clipsets and sounds in `data/CodVHost.ini` against GTA's real lists.
5. Set `RUSTFLAGS=--remap-path-prefix=<checkout>=.` so binaries do not carry your paths, rebuild, then
   run `py tools\build-release.py --version <version>`. The release is written to
   `build\release\CODV-<version>\` and its `.zip`.

## Layout

- `host/` is the ScriptHookV ASI (C++, `CodVLegacy.asi`): firing, camera kick, ammo, reloads, swaps, movement
  rates, crouch, melee, health, the HUD and the settings menu, with natives only; `physics_level.*` finds GTA's
  physics level for the runtime's collision reader. `host/include/codv_runtime.h`
  is the C ABI; `host/tests/` checks the input map, control policy, aim math, host data, menu and HUD helpers
  without GTA (`g++ -std=c++20 -Ihost/include -Ihost/src host/tests/host_tests.cpp host/src/host_data.cpp`
  works off Windows too).
- `rust/` holds the workspace:
  - `codv-engine`: the bridge to iw4L's Modern Warfare 2 crates (the player's pmove, GTA triangles as MW2
    collision meshes, the weapons and the first-person view model rig);
  - `codv-gtaworld`: GTA's loaded collision read live from its physics level (SK8V's reader, credit Sol4ra);
  - `codv-mw2data`: the MW2 weapon table, the view model files and `codv-mw2import.exe`, which the setup runs on the player's own
    MW2 (`INSTALL.bat -Mw2 "<MW2 folder>"`, found automatically on Steam);
  - `codv-core`: the interim gunplay rules as pure, FFI-free, unit-tested logic;
  - `codv-runtime`: `CodVRuntime.dll`, the core behind the versioned C ABI (`cv_*` exports);
  - `codv-gtapack` (its own workspace on `upstream/rpf-archive-rs`): `codv-gtapack.exe`, which the setup runs on
    the player's GTA to build CODV's DLC pack (the "MW2" item in the weapon wheel).
- `data/` holds `weapons.json` (interim player rules, loadout, weapons), `mw2.json` (MW2 loadout, the GTA
  weapon each MW2 gun borrows, damage scale) and `CodVHost.ini` (GTA clip, clipset and sound names).
- `patches/iw4l/` holds CODV's changes to the pinned iw4L crates (empty today).
- `tools/` holds the build scripts, the iw4L overlay (`prepare_overlay.py`), the requirements check (`check-requirements.ps1`), the one-step build and
  install (`build-and-install.ps1`), the player setup (`prepare.py`, `setup_engine.py`, `gta_locate.py`,
  `setup-wizard.ps1`) and the data checks.
- `docs/` explains the architecture, the weapons and the roadmap.

No GTA or Call of Duty data is in this repository.
