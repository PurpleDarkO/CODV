"""Checks CODV's GTA names against GTA's real lists (DurtyFree gta-v-data-dumps,
pinned in upstreams.lock.json; sparse-fetched here on first use):
every weapon in data/weapons.json, and the melee clip, crouch clipsets and
sounds in data/CodVHost.ini.

    python tools/check-data.py [--dumps <folder with the dump files>]
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

from _common import REPO, joaat, lock, read_ini

FILES = ['animDictsCompact.json', 'movementClipsetsCompact.json', 'soundNames.json', 'WeaponList.ini']


def fetch_dumps(dest):
    """Sparse, blobless checkout of the dump files at the pinned commit."""
    if all((dest / f).is_file() for f in FILES):
        return
    r = lock('gta-v-data-dumps')
    dest.mkdir(parents=True, exist_ok=True)
    git = ['git', '-C', str(dest)]
    if not (dest / '.git').exists():
        subprocess.run(git + ['init', '-q'], check=True)
        subprocess.run(git + ['remote', 'add', 'origin', r['url']], check=True)
    subprocess.run(git + ['sparse-checkout', 'set', '--no-cone', *(f'/{f}' for f in FILES)], check=True)
    subprocess.run(git + ['fetch', '-q', '--depth', '1', '--filter=blob:none', 'origin', r['commit']], check=True)
    subprocess.run(git + ['-c', 'advice.detachedHead=false', 'checkout', '-q', '--detach', 'FETCH_HEAD'], check=True)


def pair(value):
    parts = [s.strip() for s in value.split(',')]
    return parts if len(parts) == 2 and all(parts) else None


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--dumps', type=Path, default=REPO / 'upstream/gta-v-data-dumps')
    a = p.parse_args()
    fetch_dumps(a.dumps)
    dicts = {d['DictionaryName']: set(d['Animations']) for d in json.load(open(a.dumps / FILES[0], encoding='utf-8'))}
    clipsets = {c['Name'] for c in json.load(open(a.dumps / FILES[1], encoding='utf-8'))}
    sounds = {(s['AudioName'], s['AudioRef']) for s in json.load(open(a.dumps / FILES[2], encoding='utf-8'))}
    weapons = {}
    for line in (a.dumps / FILES[3]).read_text(encoding='utf-8', errors='replace').split():
        name, _, value = line.partition('=')
        weapons[name.lower()] = int(value) if value.isdigit() else None
    problems = []

    table = json.load(open(REPO / 'data/weapons.json', encoding='utf-8'))
    for w in table['weapons']:
        gta = w['gta'].lower()
        if gta not in weapons:
            problems.append(f'weapons.json {w["name"]}: {w["gta"]} is not a GTA weapon')
        elif weapons[gta] is not None and weapons[gta] != joaat(gta):
            problems.append(f'weapons.json {w["name"]}: joaat({gta}) does not match the dump')

    host = read_ini(REPO / 'data/CodVHost.ini')
    clip = pair(host.get('Melee', {}).get('clip', ''))
    if not clip:
        problems.append('CodVHost.ini [Melee] clip: expected "dict, clip"')
    elif clip[1] not in dicts.get(clip[0], set()):
        problems.append(f'CodVHost.ini [Melee] clip: {clip[0]}/{clip[1]} is not a GTA clip')
    for key in ('movement', 'strafe'):
        cs = host.get('Crouch', {}).get(key, '')
        if cs and cs not in clipsets:
            problems.append(f'CodVHost.ini [Crouch] {key}: {cs} is not a GTA movement clipset')
    for key, value in host.get('Sounds', {}).items():
        if not value:
            continue
        s = pair(value)
        if not s or tuple(s) not in sounds:
            problems.append(f'CodVHost.ini [Sounds] {key}: {value} is not a known GTA sound')

    for line in problems:
        print('PROBLEM', line)
    print(f'{len(table["weapons"])} weapons, 1 clip, 2 clipsets, {len(host.get("Sounds", {}))} sounds checked; '
          f'{len(problems)} problems')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
