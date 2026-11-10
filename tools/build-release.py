"""Assembles a player release from a built checkout (BOOTSTRAP.bat / BUILD.bat):

    python tools/build-release.py --version 0.1.0

Writes build/release/CODV-<version>/ and CODV-<version>.zip:
    CODV Setup.cmd, README.md, LICENSE, NOTICE, THIRD-PARTY-NOTICES.txt, SHA256SUMS.txt
    build/package/  CodVLegacy.asi, CodVRuntime.dll, codv-mw2import.exe, codv-gtapack.exe
    tools/          prepare.py and the setup modules, the wizard
    data/           weapons.json, CodVHost.ini, mw2.json
    python/         python.org's embeddable Python (the pin in tools/get-python.ps1, its hash checked)
The layout mirrors the repository so tools/prepare.py runs unchanged.

The Python ships inside the release instead of being downloaded at setup: a zip holding a setup script that
downloads and runs something next to executables reads as a trojan dropper to Windows Defender's machine-learning
heuristics (Trojan:Script/Sabsik.FL.A!ml on 0.3.0), even though every file scans clean on its own. On Windows the
finished zip is scanned with Defender as a downloaded file, and a detection is reported.

rustc keeps source paths in binaries (panic locations): this writes a machine-local
.cargo/config.toml (gitignored) that maps this checkout and the user folder to neutral
names, and refuses binaries that still hold either: after the first run, rebuild
(BUILD.bat) and run it again. The ASI links with /PDBALTPATH (no folder in it).
No Call of Duty or GTA V data format may be in a release (a final scan refuses them).
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

from _common import REPO, sha256_file

PACKAGE_FILES = ['CodVLegacy.asi', 'CodVRuntime.dll', 'codv-mw2import.exe', 'codv-gtapack.exe']
TOOL_FILES = ['_common.py', 'prepare.py', 'setup_engine.py', 'gta_locate.py', 'setup-wizard.ps1',
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


def python_pin():
    """The embeddable Python's URL and SHA-256, from tools/get-python.ps1 (one pin for both)."""
    text = (REPO / 'tools/get-python.ps1').read_text(encoding='utf-8')
    url = re.search(r"Url\s*=\s*'([^']+)'", text)
    sha = re.search(r"Sha\s*=\s*'([0-9a-fA-F]{64})'", text)
    if not url or not sha:
        raise SystemExit('tools/get-python.ps1: no Url / Sha pin found')
    return url.group(1), sha.group(1).lower()


def bundle_python(out):
    """python.org's embeddable Python into <release>/python, as tools/get-python.ps1 lays it out (without its
    ._pth, so the tools import each other). Downloaded once into build/python-embed/, its hash checked."""
    url, sha = python_pin()
    cache = REPO / 'build/python-embed' / url.rsplit('/', 1)[-1]
    if not cache.is_file() or sha256_file(cache) != sha:
        cache.parent.mkdir(parents=True, exist_ok=True)
        print(f'downloading {url}', flush=True)
        with urllib.request.urlopen(url, timeout=120) as r:
            data = r.read()
        if hashlib.sha256(data).hexdigest() != sha:
            raise SystemExit(f'{url} does not match its pinned SHA-256: nothing bundled')
        cache.write_bytes(data)
    target = out / 'python'
    with zipfile.ZipFile(cache) as z:
        z.extractall(target)
    for pth in target.glob('python*._pth'):
        pth.unlink()
    if not (target / 'python.exe').is_file() or not (target / 'LICENSE.txt').is_file():
        raise SystemExit('the embeddable Python lacks python.exe or LICENSE.txt')


def defender_scan(zip_path):
    """Windows only: Defender's verdict on the zip as a downloaded file (the mark of the web on a copy), the
    way a player's PC first sees it. Prints the result; a detection is reported, never fatal."""
    if os.name != 'nt':
        return
    tool = Path(os.environ.get('ProgramFiles', r'C:\Program Files')) / 'Windows Defender/MpCmdRun.exe'
    if not tool.is_file():
        print('Defender scan skipped (MpCmdRun.exe not found)')
        return
    with tempfile.TemporaryDirectory() as tmp:
        copy = Path(tmp) / zip_path.name
        shutil.copy2(zip_path, copy)
        with open(str(copy) + ':Zone.Identifier', 'w', encoding='ascii') as zone:
            zone.write('[ZoneTransfer]\r\nZoneId=3\r\n')
        r = subprocess.run([str(tool), '-Scan', '-ScanType', '3', '-File', str(copy), '-DisableRemediation'],
                           capture_output=True, text=True)
        clean = 'found no threats' in r.stdout
        print('Defender: no threats' if clean else 'Defender FLAGGED the zip, do not publish it:\n' + r.stdout)


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
    # experimental/ (local only) is built into the ASI whenever the folder is there: never in a release.
    if b'experimental build (not for release)' in (REPO / 'build/package/CodVLegacy.asi').read_bytes():
        raise SystemExit('CodVLegacy.asi was built with experimental/: move that folder out of the checkout, '
                         'rebuild (BUILD.bat) and run this again')
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
    bundle_python(out)
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
    defender_scan(zip_path)
    return 0


if __name__ == '__main__':
    sys.exit(main())
