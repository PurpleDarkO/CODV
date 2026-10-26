"""Assembles a player release from a built checkout (BOOTSTRAP.bat / BUILD.bat):

    python tools/build-release.py --version 0.1.0

Writes build/release/CODV-<version>/ and CODV-<version>.zip:
    CODV Setup.cmd, README.md, LICENSE, NOTICE
    build/package/  CodVLegacy.asi, CodVRuntime.dll
    tools/          prepare.py and the setup modules, the wizard, get-python.ps1
    data/           weapons.json, CodVHost.ini
The layout mirrors the repository so tools/prepare.py runs unchanged.
"""
import argparse
import hashlib
import json
import shutil
import sys
import zipfile
from pathlib import Path

from _common import REPO, sha256_file

PACKAGE_FILES = ['CodVLegacy.asi', 'CodVRuntime.dll', 'codv-mw2import.exe', 'codv-gtapack.exe']
TOOL_FILES = ['_common.py', 'prepare.py', 'setup_engine.py', 'gta_locate.py', 'setup-wizard.ps1', 'get-python.ps1',
              'CodVLegacy.ini.template', 'build-mw2-weapon.py', 'build-mw2-icon.py', 'mw2-wheel-icon.bin']
DATA_FILES = ['weapons.json', 'CodVHost.ini', 'mw2.json']
DOCS = ['README.md', 'LICENSE', 'NOTICE']
SETUP_CMD = ('@echo off\r\n'
             'powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\\setup-wizard.ps1"\r\n')


def leaks_build_paths(path):
    """True when a binary still holds this checkout's absolute path (rustc/MSVC debug info)."""
    needle = str(REPO).encode('utf-8', errors='ignore')
    return len(needle) > 3 and needle in Path(path).read_bytes()


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--version', required=True)
    p.add_argument('--allow-paths', action='store_true', help='skip the build-path leak check (local test builds)')
    a = p.parse_args()
    if not all(c.isalnum() or c in '.-_' for c in a.version):
        raise SystemExit('--version: letters, digits, dot, dash and underscore only')
    name = f'CODV-{a.version}'
    out = REPO / 'build/release' / name
    if out.exists():
        shutil.rmtree(out)
    missing = [f for f in PACKAGE_FILES if not (REPO / 'build/package' / f).is_file()]
    if missing:
        raise SystemExit(f'build/package lacks {", ".join(missing)}: run BUILD.bat first')
    copies = {}
    for f in PACKAGE_FILES:
        copies[f'build/package/{f}'] = REPO / 'build/package' / f
    for f in TOOL_FILES:
        copies[f'tools/{f}'] = REPO / 'tools' / f
    for f in DATA_FILES:
        copies[f'data/{f}'] = REPO / 'data' / f
    for f in DOCS:
        copies[f] = REPO / f
    if not a.allow_paths:
        leaky = [rel for rel, src in copies.items() if src.suffix in ('.asi', '.dll', '.exe') and leaks_build_paths(src)]
        if leaky:
            raise SystemExit(f'{", ".join(leaky)} contain this checkout\'s path (set RUSTFLAGS=--remap-path-prefix '
                             f'and rebuild, or pass --allow-paths for a local build)')
    for rel, src in copies.items():
        dest = out / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)
    (out / 'CODV Setup.cmd').write_bytes(SETUP_CMD.encode('ascii'))
    manifest = {'version': a.version, 'files': {rel: sha256_file(out / rel) for rel in sorted(copies)}}
    (out / 'release.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')

    zip_path = out.parent / f'{name}.zip'  # with_suffix would cut the version at its dots
    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(out.rglob('*')):
            if f.is_file():
                z.write(f, Path(name) / f.relative_to(out))
    digest = hashlib.sha256(zip_path.read_bytes()).hexdigest()
    print(f'{out}\n{zip_path}  sha256 {digest}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
