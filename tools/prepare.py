"""Prepares CODV's data and (with --install) installs it into the GTA
folder. One command for setup; each stage leaves a receipt, so a rerun
resumes after the last stage that finished (--redo STAGE reruns one and
everything after it).

    python tools/prepare.py --gta <GTA V folder> [--mw2 <MW2 folder>] [--install]

With --mw2 (your own Modern Warfare 2, PC), its multiplayer weapons are read
from zone/english/common_mp.ff by build/package/codv-mw2import(.exe) into
<data>/mw2/weapons.json, and MW2's weapons replace CODV's interim gunplay while
MW2 movement drives the player; their first-person view models (meshes,
animations, and images from main/iw_*.iwd) go to <data>/mw2/viewmodels/, and
MW2's character animation (and a soldier skeleton from a map zone) to
<data>/mw2/body/, and the weapons' sounds (aliases from
zone/english/localized_common_mp.ff, as WAV files) to <data>/mw2/sounds/. The folder is remembered (<data>/mw2/source.txt)
for later runs. Nothing from MW2 is part of CODV or leaves the machine.
From the GTA folder itself (any setup), build/package/codv-gtapack reads GTA's keys
and update.rpf to build CODV's DLC pack: WEAPON_CODV_MW2, the "MW2" item in GTA's
weapon wheel that turns the mode on, with its wheel icon (update/x64/dlcpacks/codv/).
Without it CODV uses its own table (data/weapons.json) and GTA's clips and
sounds (data/CodVHost.ini).
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

from _common import REPO, sha256_file, sha256_text
import setup_engine

TOOLS = REPO / 'tools'
DATA = REPO / 'data'
PACKAGE = REPO / 'build/package'
TABLE = 'weapons.json'
MW2_CONFIG = 'mw2.json'
IMPORTER = 'codv-mw2import.exe' if os.name == 'nt' else 'codv-mw2import'
GTAPACK = 'codv-gtapack.exe' if os.name == 'nt' else 'codv-gtapack'
# CODV's DLC pack (the MW2 item in GTA's weapon wheel) in the GTA folder; the ASI mounts it.
PACK_DIR = 'update/x64/dlcpacks/codv'
WORK = REPO / 'build/work'
# Per stage: what the setup window shows.
STEPS = {'gta-check': 'Checking the GTA V folder', 'weapons': 'Preparing the weapon table',
         'mw2': 'Importing your Modern Warfare 2 weapons and view models',
         'wheel': "Adding the MW2 item to GTA's weapon wheel",
         'verify': 'Checking the prepared data', 'install': 'Installing into the GTA folder'}
# Per stage: its share of the setup's time (the MW2 import takes most of it), for the setup window's
# progress bar ('@progress <percent>' as each stage ends).
WEIGHTS = {'gta-check': 2, 'weapons': 2, 'mw2': 80, 'wheel': 8, 'verify': 4, 'install': 4}


def check_table(path):
    """The few things the table needs before the runtime will take it (it validates fully and
    falls back to the copy compiled into the DLL, logging why)."""
    table = json.loads(path.read_text(encoding='utf-8'))
    weapons = table.get('weapons', [])
    names = [w.get('name') for w in weapons]
    if not weapons or 'player' not in table or None in names:
        raise SystemExit(f'{path.name}: needs "player" and a non-empty "weapons" list of named weapons')
    loadout = table.get('loadout', {})
    for slot in ('primary', 'secondary'):
        if loadout.get(slot) not in names:
            raise SystemExit(f'{path.name}: loadout.{slot} names no weapon in the table')
    return len(weapons)


class Prepare:
    def __init__(self, a):
        self.a = a
        self.gta = a.gta.resolve()
        self.out = (a.out or self.gta / setup_engine.DATA).resolve()
        self.receipts = self.out / '.stages'
        self.mw2 = self.find_mw2(a.mw2)

    def find_mw2(self, given):
        """The MW2 folder: --mw2, else the one a previous run imported from."""
        remembered = self.out / 'mw2' / 'source.txt'
        candidates = [given] if given else []
        if remembered.is_file():
            candidates.append(Path(remembered.read_text(encoding='utf-8').strip()))
        for c in candidates:
            if c and (Path(c) / 'zone' / 'english' / 'common_mp.ff').is_file():
                return Path(c).resolve()
        if given:
            print(f'warning: no zone/english/common_mp.ff under {given}: MW2 weapons not imported', flush=True)
        return None

    def stages(self):
        """(name, scripts whose change invalidates it, function)."""
        return [
            ('gta-check', ['prepare.py', 'setup_engine.py'], self.stage_gta_check),
            ('weapons', ['prepare.py'], self.stage_weapons),
            ('mw2', ['prepare.py'], self.stage_mw2),
            ('wheel', ['prepare.py', 'build-mw2-weapon.py', 'build-mw2-icon.py', 'mw2-wheel-icon.bin'], self.stage_wheel),
            ('verify', ['prepare.py'], self.verify),
        ]

    def stage_gta_check(self):
        errors, warnings = setup_engine.preflight(self.gta, require_scripthook=self.a.install)
        for w in warnings:
            print('warning:', w, flush=True)
        if errors:
            raise SystemExit('\n'.join(errors))

    def stage_weapons(self):
        self.out.mkdir(parents=True, exist_ok=True)
        n = check_table(DATA / TABLE)
        shutil.copyfile(DATA / TABLE, self.out / TABLE)
        print(f'{TABLE}: {n} weapons', flush=True)

    def stage_mw2(self):
        out = self.out / 'mw2'
        out.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(DATA / MW2_CONFIG, self.out / MW2_CONFIG)
        if not self.mw2:
            print('no Modern Warfare 2 folder given (--mw2): CODV keeps its interim gunplay', flush=True)
            return
        importer = PACKAGE / IMPORTER
        if not importer.is_file():
            raise SystemExit(f'{importer} is missing: build first (BUILD.bat)')
        r = subprocess.run([str(importer), str(self.mw2), str(out)], capture_output=True, text=True)
        print(r.stdout.strip() or r.stderr.strip(), flush=True)
        if r.returncode != 0:
            raise SystemExit(f'MW2 import failed: {r.stderr.strip()}')
        (out / 'source.txt').write_text(str(self.mw2), encoding='utf-8')

    @staticmethod
    def main_hud(out):
        """The title update's hud.gfx (x64/data/cdimages/scaleform_generic.rpf), not a dlc_patch copy."""
        main = out / 'x64/data/cdimages/scaleform_generic.rpf/hud.gfx'
        if main.is_file():
            return main
        found = sorted(out.rglob('hud.gfx'), key=lambda p: (len(p.parts), str(p)))
        return found[0] if found else None

    def hud_source(self, tool, keys, work):
        """The hud.gfx to add the MW2 icon to: OpenIV's copy of update.rpf when OpenIV.asi
        loads the mods folder (a HUD mod's movie is the one GTA runs), else SK8V's pack copy
        (it already carries the board icon, so both show), else GTA's own update.rpf."""
        mod = self.gta / 'mods/update/update.rpf'
        if mod.is_file() and (self.gta / 'OpenIV.asi').is_file():
            out = work / 'hud-mods'
            shutil.rmtree(out, ignore_errors=True)
            r = subprocess.run([str(tool), 'extract', '--keys', str(keys), str(mod), '*scaleform_generic.rpf/hud.gfx',
                                '-o', str(out)], capture_output=True, text=True)
            found = self.main_hud(out) if r.returncode == 0 else None
            if found:
                return found
            print("mods folder hud.gfx not read; using GTA's", flush=True)
        sk8v = self.gta / 'update/x64/dlcpacks/skatev/hud.gfx'
        if sk8v.is_file():
            return sk8v
        out = work / 'hud'
        shutil.rmtree(out, ignore_errors=True)
        subprocess.run([str(tool), 'extract', '--keys', str(keys), str(self.gta / 'update/update.rpf'),
                        '*scaleform_generic.rpf/hud.gfx', '-o', str(out)], check=True)
        found = self.main_hud(out)
        if not found:
            raise OSError('hud.gfx not found in update.rpf')
        return found

    def stage_wheel(self):
        """WEAPON_CODV_MW2 (a hammer clone named "MW2") and its wheel icon, from the player's
        own GTA files, packed as <data>/wheel/dlc.rpf + hud.gfx; install puts them in
        update/x64/dlcpacks/codv/. Optional: without them F10 still turns the mode on."""
        out = self.out / 'wheel'
        shutil.rmtree(out, ignore_errors=True)
        tool = PACKAGE / GTAPACK
        if not tool.is_file():
            print(f'warning: {tool} is missing (build first): no MW2 item in the weapon wheel; F10 still works', flush=True)
            return False
        work = WORK / 'gta'
        keys = work / 'keys'
        try:
            subprocess.run([str(tool), 'keys', str(self.gta / 'GTA5.exe'), str(keys)], check=True)
            meta = work / 'weapon-meta'
            shutil.rmtree(meta, ignore_errors=True)
            subprocess.run([str(tool), 'extract', '--keys', str(keys), str(self.gta / 'update/update.rpf'),
                            '*common/data/ai/weapons.meta', '*common/data/ai/weaponanimations.meta', '-o', str(meta)],
                           check=True)
            # update.rpf's own copies (its dlc_patch folders hold partial ones, as SK8V reads it).
            weapons = meta / 'common/data/ai/weapons.meta'
            anims = meta / 'common/data/ai/weaponanimations.meta'
            if not weapons.is_file() or not anims.is_file():
                raise OSError(f'weapons.meta / weaponanimations.meta not found in update.rpf (looked in {weapons.parent})')
            subprocess.run([sys.executable, str(TOOLS / 'build-mw2-weapon.py'), str(weapons), str(anims),
                            str(work / 'weapon')], check=True)
            out.mkdir(parents=True, exist_ok=True)
            subprocess.run([str(tool), 'pack', str(work / 'weapon'), str(out / 'dlc.rpf')], check=True)
            hud = self.hud_source(tool, keys, work)
            subprocess.run([sys.executable, str(TOOLS / 'build-mw2-icon.py'), '--source', str(hud),
                            '--out', str(out / 'hud.gfx')], check=True)
        except (subprocess.CalledProcessError, StopIteration, OSError) as e:
            shutil.rmtree(out, ignore_errors=True)
            print(f'warning: MW2 weapon wheel item not built ({e}); F10 still turns the mode on', flush=True)
            return False

    def verify(self):
        missing = [self.out / TABLE] if not (self.out / TABLE).is_file() else []
        if self.mw2 and not (self.out / 'mw2' / 'weapons.json').is_file():
            missing.append(self.out / 'mw2' / 'weapons.json')
        if self.a.install:
            missing += [PACKAGE / f for f in ('CodVLegacy.asi', 'CodVRuntime.dll') if not (PACKAGE / f).is_file()]
        if missing:
            raise SystemExit('missing: ' + ', '.join(map(str, missing)))

    def install(self):
        if not self.out.is_relative_to(self.gta):
            raise SystemExit('--install needs the data folder inside the GTA folder (omit --out)')
        rel_out = self.out.relative_to(self.gta).as_posix()
        files = {'CodVLegacy.asi': PACKAGE / 'CodVLegacy.asi', 'CodVRuntime.dll': PACKAGE / 'CodVRuntime.dll',
                 'CodVHost.ini': DATA / 'CodVHost.ini'}
        # The MW2 item in the weapon wheel (stage wheel), when it could be built.
        wheel = self.out / 'wheel'
        if (wheel / 'dlc.rpf').is_file():
            files[f'{PACK_DIR}/dlc.rpf'] = wheel / 'dlc.rpf'
            if (wheel / 'hud.gfx').is_file():
                files[f'{PACK_DIR}/hud.gfx'] = wheel / 'hud.gfx'
        plan = {'files': {k: str(v) for k, v in files.items()},
                'ini': {'GTA_VERSION': setup_engine.TESTED_GTA, 'DATA_ROOT': rel_out.replace('/', '\\')},
                'allow_existing': ['CodVHost.ini'], 'require_scripthook': True}
        errors, _ = setup_engine.preflight(self.gta, True)
        if errors:
            raise SystemExit('\n'.join(errors))
        try:
            written = setup_engine.install(self.gta, plan)
        except (setup_engine.SetupError, OSError) as e:
            raise SystemExit(f'install failed, nothing changed: {e}')
        for rel in written:
            print('installed', rel, flush=True)
        print(f'installed {rel_out}/{TABLE}', flush=True)

    def key(self, scripts):
        material = [sha256_file(TOOLS / s) for s in scripts]
        material += [str(self.gta), str(self.out), sha256_file(DATA / TABLE), sha256_file(DATA / 'CodVHost.ini')]
        material += [str(self.mw2 or ''), sha256_file(DATA / MW2_CONFIG)]
        for tool in (IMPORTER, GTAPACK):
            if (PACKAGE / tool).is_file():
                material.append(sha256_file(PACKAGE / tool))
        return sha256_text('\n'.join(material))

    def run(self):
        self.receipts.mkdir(parents=True, exist_ok=True)
        names = [s[0] for s in self.stages()]
        if self.a.redo and self.a.redo not in names:
            raise SystemExit(f'--redo: unknown stage {self.a.redo} (one of {", ".join(names)})')
        forcing = False
        total = sum(WEIGHTS[n] for n in names) + (WEIGHTS['install'] if self.a.install else 0)
        done = 0

        def progress(name):
            nonlocal done
            done += WEIGHTS[name]
            print(f'@progress {done * 100 // total}', flush=True)

        for name, scripts, fn in self.stages():
            forcing = forcing or name == self.a.redo
            receipt = self.receipts / f'{name}.json'
            key = self.key(scripts)
            if not forcing and receipt.is_file() and json.loads(receipt.read_text())['key'] == key:
                print(f'== {name}: up to date', flush=True)
                progress(name)
                continue
            print(f'== {name}: {STEPS[name]}', flush=True)
            if fn() is False:  # an optional stage that could not run: retried next time
                progress(name)
                continue
            receipt.write_text(json.dumps({'key': key}), encoding='utf-8')
            forcing = True  # later stages consume this one's output
            progress(name)
        if self.a.install:
            print(f'== install: {STEPS["install"]}', flush=True)
            self.install()
            progress('install')
        print('done', flush=True)


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--gta', type=Path, required=True, help='GTA V Legacy folder (holds GTA5.exe)')
    p.add_argument('--out', type=Path, help='data folder (default <GTA>/CODV)')
    p.add_argument('--mw2', type=Path, help='your Modern Warfare 2 (PC) folder, with zone/english: its weapons are imported')
    p.add_argument('--install', action='store_true', help='install into the GTA folder afterwards')
    p.add_argument('--redo', help='rerun this stage and every later one')
    Prepare(p.parse_args()).run()


if __name__ == '__main__':
    sys.exit(main())
