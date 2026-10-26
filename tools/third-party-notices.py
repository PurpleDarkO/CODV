"""Writes THIRD-PARTY-NOTICES.txt for a release: the project-level credits below,
then every crate linked into the shipped Rust binaries (CodVRuntime.dll,
codv-mw2import.exe, codv-gtapack.exe) with its own license files, taken from
the resolved dependency graphs (cargo metadata). Adapted from SK8V's (Apache-2.0).

    python tools/third-party-notices.py [--out build/package/THIRD-PARTY-NOTICES.txt]

Fails when a shipped crate has neither license files nor a license field.
"""
import argparse
import json
import shlex
import subprocess
from pathlib import Path

from _common import REPO

# The shipped binaries' workspaces and their root packages. The setup's importer is codv-mw2data's binary
# with the "import" feature, so the graphs are resolved with every feature on (a superset at worst).
WORKSPACES = [(REPO / 'rust/Cargo.toml', ['codv-runtime', 'codv-mw2data']),
              (REPO / 'rust/codv-gtapack/Cargo.toml', ['codv-gtapack'])]
# CODV's own crates (Apache-2.0, LICENSE and NOTICE at the release's root).
OURS = {'codv-core', 'codv-engine', 'codv-gtaworld', 'codv-mw2data', 'codv-runtime', 'codv-gtapack'}
TARGET = 'x86_64-pc-windows-msvc'
LICENSE_FILES = ('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE')

HEADER = """CODV third-party notices
========================

CODV is licensed under the Apache License, Version 2.0 (LICENSE); NOTICE lists
what it reuses and from where. It contains or is built from the third-party work
below. CODV ships no Call of Duty or GTA V code or data: players import their
own copy of Modern Warfare 2 on their own machine. ScriptHookV is not included;
CODV's ASI links against its SDK import library and needs the player's own
ScriptHookV.

Modern Warfare 2 engine code (Apache-2.0)
-----------------------------------------
iw4L, Copyright 2026 vladtrc. https://github.com/vladtrc/iw4L
  Its crates (movement_iw4, weapon_iw4, playerstate_iw4, hud_iw4, trace_iw4,
  clipmap_iw4, math_iw4, asset_iw4, d3d9_decl, d3d9_state, anim_iw4,
  xmodel_runtime; fastfile_iw4 in codv-mw2import.exe) are compiled into the
  shipped binaries. Their license and iw4L's NOTICE are reproduced below.

Code from SK8V (Apache-2.0)
---------------------------
SK8V, Copyright 2026 becaring. https://github.com/becaring/SK8V
  CODV reuses SK8V code in its ASI and runtime (NOTICE lists each file).

Real-time collision detection: credit to Sol4ra. The live GTA collision reader
comes from SK8V, which adapted it for Legacy from Sol4ra's
LS-Skate-LiveCollision (GTA V Enhanced), shared with SK8V by its author.

RAGE archives (Unlicense)
-------------------------
rpf-archive-rs, by VIRUXE, released into the public domain.
  https://github.com/VIRUXE/rpf-archive-rs  (in codv-gtapack.exe)

Rust crates
-----------
Each crate below is linked into a shipped binary and is distributed under its
own license, reproduced from the crate's sources.
"""


def graph(cargo, manifest, roots):
    meta = json.loads(subprocess.run(
        cargo + ['metadata', '--format-version', '1', '--filter-platform', TARGET, '--all-features',
                 '--manifest-path', str(manifest)],
        check=True, capture_output=True, text=True).stdout)
    packages = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    todo = [p['id'] for p in meta['packages'] if p['name'] in roots and p['id'] in meta['workspace_members']]
    missing = set(roots) - {packages[i]['name'] for i in todo}
    if missing:
        raise SystemExit(f'{manifest}: no workspace package {", ".join(sorted(missing))}')
    seen = set()
    while todo:
        pid = todo.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]['deps']:
            if any(k['kind'] is None for k in dep['dep_kinds']):  # normal deps only: what gets linked
                todo.append(dep['pkg'])
    return [packages[i] for i in seen]


def license_files(manifest):
    """The crate's own license files, else its workspace's (the nearest folder above that has some:
    iw4L keeps one LICENSE and NOTICE at its root)."""
    folder = Path(manifest).parent
    for d in [folder, *list(folder.parents)[:3]]:
        files = sorted(f for f in d.iterdir() if f.is_file() and f.name.upper().startswith(LICENSE_FILES))
        if files:
            return files
    return []


def section(p, printed):
    """`printed`: license text -> the crate that first reproduced it (identical texts appear once)."""
    name = f"{p['name']} {p['version']}"
    lines = [f"{name}  ({p.get('license') or 'see files'})"]
    if p.get('repository'):
        lines.append(f"  {p['repository']}")
    files = license_files(p['manifest_path'])
    if not files and not p.get('license'):
        raise SystemExit(f"{name}: no license files and no license field")
    for f in files:
        text = f.read_text(encoding='utf-8', errors='replace').strip()
        if text in printed:
            lines.append(f'--- {f.name}: same text as {printed[text]} ---')
        else:
            printed[text] = name
            lines += ['', f'--- {f.name} ---', text]
    return '\n'.join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', type=Path, default=REPO / 'build/package/THIRD-PARTY-NOTICES.txt')
    ap.add_argument('--cargo', default='cargo', help='the cargo command (default: cargo on PATH)')
    args = ap.parse_args()
    cargo = shlex.split(args.cargo)
    crates = {}
    for manifest, roots in WORKSPACES:
        for p in graph(cargo, manifest, roots):
            if p['name'] not in OURS:
                crates[(p['name'], p['version'])] = p
    printed = {}
    body = ('\n\n' + '=' * 78 + '\n\n').join(section(crates[k], printed) for k in sorted(crates))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(HEADER + '\n' + body + '\n', encoding='utf-8', newline='\n')
    print(f'{args.out}: {len(crates)} crates')


if __name__ == '__main__':
    main()
