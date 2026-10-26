"""Assembles a player release from a built checkout (BOOTSTRAP.bat / BUILD.bat):

    python tools/build-release.py --version 0.1.0

Writes build/release/CODV-<version>/ and CODV-<version>.zip:
    CODV Setup.cmd, README.md, LICENSE, NOTICE, THIRD-PARTY-NOTICES.txt, SHA256SUMS.txt
    build/package/  CodVLegacy.asi, CodVRuntime.dll, codv-mw2import.exe, codv-gtapack.exe
    tools/          prepare.py and the setup modules, the wizard, get-python.ps1
    data/           weapons.json, CodVHost.ini, mw2.json
The layout mirrors the repository so tools/prepare.py runs unchanged.

rustc keeps source paths in binaries (panic locations): this writes a machine-local
.cargo/config.toml (gitignored) that maps this checkout and the user folder to neutral
names, and refuses binaries that still hold either: after the first run, rebuild
(BUILD.bat) and run it again. The ASI links with /PDBALTPATH (no folder in it).
No Call of Duty or GTA V data format may be in a release (a final scan refuses them).
"""
import argparse
import hashlib
import json
import shutil
import subprocess
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
# Call of Duty and GTA V data formats: none may be in a release.
RETAIL = {'.ff', '.iwd', '.iwi', '.rpf', '.ydr', '.ydd', '.yft', '.ytd', '.ytyp', '.ymap', '.ybn', '.ycd', '.gfx',
          '.meta', '.cvm', '.cva', '.wav', '.dds'}


def remap_config():
    """The repo's .cargo/config.toml (gitignored, this machine's paths): every cargo run inside the
    checkout gets the remaps, so builds never flip between flagged and unflagged (after SK8V's)."""
    pairs = [(Path.home(), '~'), (REPO, 'CODV')]  # the last match wins: the repo after home
    flags = [f'--remap-path-prefix={str(p).replace(chr(92), sep)}={to}' for p, to in pairs for sep in (chr(92), '/')]
    text = ('# Written by tools/build-release.py for this machine: hide local paths in built binaries.\n'
            '[build]\nrustflags = [' + ', '.join(f"'{f}'" for f in flags) + ']\n')
    config = REPO / '.cargo/config.toml'
    if not config.exists() or config.read_text(encoding='utf-8') != text:
        config.parent.mkdir(exist_ok=True)
        config.write_text(text, encoding='utf-8')
        return True
    return False


def local_paths(shipped):
    """Shipped binaries that still name this checkout or the user folder."""
    needles = [s.lower().encode() for p in (REPO, Path.home()) for s in (str(p), str(p).replace(chr(92), '/'))
               if len(str(p)) > 3]
    return [p for p in shipped if p.suffix.lower() in ('.exe', '.dll', '.asi')
            and any(n in p.read_bytes().lower() for n in needles)]


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--version', required=True)
    p.add_argument('--allow-paths', action='store_true', help='skip the build-path leak check (local test builds)')
    a = p.parse_args()
    if not all(c.isalnum() or c in '.-_' for c in a.version):
        raise SystemExit('--version: letters, digits, dot, dash and underscore only')
    if remap_config():
        print('wrote .cargo/config.toml (hides this machine\'s paths in the binaries): rebuild with BUILD.bat, '
              'then run this again', flush=True)
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
    for rel, src in copies.items():
        dest = out / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)
    (out / 'CODV Setup.cmd').write_bytes(SETUP_CMD.encode('ascii'))
    # The licenses of every crate linked into the shipped binaries (MIT / Apache-2.0 ask for them).
    subprocess.run([sys.executable, str(REPO / 'tools/third-party-notices.py'), '--out', str(out / 'THIRD-PARTY-NOTICES.txt')],
                   check=True)
    manifest = {'version': a.version, 'files': {rel: sha256_file(out / rel) for rel in sorted(copies)}}
    (out / 'release.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')

    shipped = [f for f in out.rglob('*') if f.is_file()]
    bad = [f for f in shipped if f.suffix.lower() in RETAIL]
    if bad:
        raise SystemExit('game data format in the release: ' + ', '.join(str(f.relative_to(out)) for f in bad[:10]))
    if not a.allow_paths:
        leaky = local_paths(shipped)
        if leaky:
            raise SystemExit('local paths in ' + ', '.join(str(f.relative_to(out)) for f in leaky) +
                             ': rebuild with BUILD.bat (the remaps in .cargo/config.toml apply), then run this again')
    sums = ''.join(f'{sha256_file(f)}  {f.relative_to(out).as_posix()}\n' for f in sorted(shipped))
    (out / 'SHA256SUMS.txt').write_text(sums, encoding='ascii')

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
