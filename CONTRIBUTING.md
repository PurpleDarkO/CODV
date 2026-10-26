# Contributing to CODV

Thanks for helping. CODV is a fan project: issues, play test reports and pull requests are all welcome.

## Ground rules

- **No game files, ever.** Nothing from Call of Duty or GTA V goes into this repository: no `.ff`, `.iwd`,
  `.rpf`, models, textures, animations, sounds, extracted tables or numbers copied out of them. CODV reads
  MW2 from the player's own copy at setup time; tests that need MW2 data read it from a local import (see
  below) and skip when it is absent.
- **English** for code, comments, docs, commit messages and log lines.
- **No personal data** in code, docs or logs (names, user folders, machine paths). Paths come from detection
  (`tools/gta_locate.py`) or from the player's settings.

## Building and testing

Windows x64. `CHECK.bat` checks the requirements, `BOOTSTRAP.bat` fetches the pinned upstreams and builds,
`BUILD.bat` rebuilds; BUILDING.md has the details.

Before a pull request:

- Rust: `cargo fmt --all` (the workspace's `rustfmt.toml`), `cargo clippy --release --workspace --all-targets`
  with no warnings, and `cargo test --release --workspace` (all from `rust/`).
- Host: the host tests build and pass off Windows too
  (`g++ -std=c++20 -Ihost/include -Ihost/src host/tests/host_tests.cpp host/src/host_data.cpp ...`, see
  BUILDING.md), and `py tools/check-natives.py` reports no mismatched native.
- Tests against your own MW2: run the setup's importer on your copy
  (`codv-mw2import "<MW2 folder>" "<output folder>"`), then set `CODV_MW2_OUT` to the output folder and
  `CODV_MW2_TABLE` to its `weapons.json` before `cargo test`. Never commit that folder.

## The runtime ABI

The host (`CodVLegacy.asi`) and the runtime (`CodVRuntime.dll`) talk through the C ABI in
`host/include/codv_runtime.h` (`CODV_ABI_VERSION`). Within a released ABI version, only add (new options,
new flags in unused bits); anything that changes or removes a field, an option or a function needs a new ABI
version, bumped on both sides together.

## Pull requests

- One change per pull request, with a short description of what it changes in game and how it was tested
  (in game, with the logs, or by tests).
- New settings get a default in `tools/CodVLegacy.ini.template` (with a comment), a menu item when players
  should reach it in game, a line in `tools/setup_engine.py`'s kept keys, and a mention in README.md.
- Note player-facing changes under `[Unreleased]` in CHANGELOG.md.

## Reporting bugs

Use the bug report template and attach both logs (`CodVLegacy.log` and `CodVRuntime.log` from
`%LOCALAPPDATA%\CodV\`). For movement problems, the debug overlay (`Debug=1`) shows what the movement meets.
