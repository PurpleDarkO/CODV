"""Install engine for the GTA folder: preflight, a transactional install with
rollback, an ownership manifest and the settings merge. Standard library only.

    python tools/setup_engine.py preflight --gta <GTA folder>
    python tools/setup_engine.py install --gta <GTA folder> --plan plan.json

plan.json: {"files": {"<path in GTA folder>": "<source file>"},
            "ini": {"<placeholder>": "<value>"},
            "allow_existing": ["<path>", ...],
            "require_scripthook": true}

Ownership: CodVLegacy.installed.json records the SHA-256 of every file
written. Install refuses to replace a file it does not own unless the plan
allows it, and removes a file a newer version dropped only while it still
matches. Uninstalling is deleting the files listed in the manifest (README).
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

TESTED_GTA = '1.0.3889.0'
MANIFEST = 'CodVLegacy.installed.json'
INI = 'CodVLegacy.ini'
TEMPLATE = Path(__file__).with_name('CodVLegacy.ini.template')
NEW, OLD = '.codv-new', '.codv-old'
DATA = 'CODV'
# INI keys the in-game menu saves: a reinstall keeps the player's value.
KEEP_KEYS = ['StartEnabled', 'ToggleKey', 'ToggleChord', 'Primary', 'Secondary', 'GiveLoadout', 'RecoilScale',
             'FireMode', 'RegenHealth', 'QuickJump', 'ForceFirstPerson', 'Crosshair', 'Hitmarker', 'HitSounds',
             'AmmoCounter', 'DamageFlash', 'InMissions', 'Debug', 'VerboseLog', 'MenuChord', 'StrictVersion', 'Runtime',
             'DataRoot', 'SoundGainDb', 'Mw2Primary', 'Mw2Secondary', 'AmmoScale', 'Frags', 'JumpHeight', 'SprintTime', 'SprintRecharge', 'NumberKeys', 'RandomLoadout', 'WeaponSets', 'Mw2Missiles', 'Mw2Hud',
             'Killstreaks', 'AllKillstreaks', 'SpacedKillstreaks', 'Scavenger', 'Killstreak1', 'Killstreak2', 'Killstreak3', 'EmpKills', 'KeepKillstreaksOnDeath', 'NukeKillsPlayer', 'FallDamage', 'Slide', 'SprintSpeed', 'StaminaBar', 'JumpSlowdown', 'ReloadSprint', 'PoliceJuggernauts', 'JuggernautCinematics', 'AutoVault', 'MantleSpeed', 'HighClimb', 'Dive', 'SmoothCollision', 'SlideSpeed', 'SlideTime', 'VaultMinHeight', 'ClimbMaxHeight', 'Mw2Footsteps', 'Lean', 'AutoLadders', 'KillCounter', 'Mw2Blood', 'KillFeedback', 'ThirdPerson', 'ThirdPersonKey']


class SetupError(Exception):
    pass


def sha256(path):
    with open(path, 'rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def file_version(exe):
    """FileVersion of a Windows executable as 'a.b.c.d', or None (also off Windows)."""
    if os.name != 'nt':
        return None
    import ctypes
    ver = ctypes.windll.version
    size = ver.GetFileVersionInfoSizeW(str(exe), None)
    if not size:
        return None
    buf = ctypes.create_string_buffer(size)
    if not ver.GetFileVersionInfoW(str(exe), 0, size, buf):
        return None
    ptr, n = ctypes.c_void_p(), ctypes.c_uint()
    if not ver.VerQueryValueW(buf, '\\', ctypes.byref(ptr), ctypes.byref(n)):
        return None
    ms, ls = ctypes.cast(ptr, ctypes.POINTER(ctypes.c_uint32))[2:4]
    return f'{ms >> 16}.{ms & 0xFFFF}.{ls >> 16}.{ls & 0xFFFF}'


def gta_running():
    if os.name != 'nt':
        return False
    out = subprocess.run(['tasklist', '/FI', 'IMAGENAME eq GTA5.exe', '/NH'], capture_output=True, text=True,
                         errors='replace').stdout
    return 'GTA5.exe' in out


def preflight(gta, require_scripthook=True):
    """Problems that stop an install, and warnings that do not."""
    gta = Path(gta)
    errors, warnings = [], []
    exe = gta / 'GTA5.exe'
    if not exe.is_file():
        if (gta / 'GTA5_Enhanced.exe').is_file():
            errors.append('this is GTA V Enhanced; CODV supports GTA V Legacy Story Mode only')
        else:
            errors.append(f'{gta} has no GTA5.exe')
        return errors, warnings
    version = file_version(exe)
    if version and version != TESTED_GTA:
        warnings.append(f'GTA5.exe is {version}; CODV was tested on {TESTED_GTA} (it uses natives only, so it may work)')
    if gta_running():
        errors.append('GTA V is running; close it first')
    if require_scripthook:
        for f in ('ScriptHookV.dll', 'dinput8.dll'):
            if not (gta / f).is_file():
                errors.append(f'{f} is missing: install ScriptHookV (https://www.dev-c.com/gtav/scripthookv/)')
    try:
        probe = gta / '.codv-write-test'
        probe.write_bytes(b'')
        probe.unlink()
    except OSError:
        errors.append(f'{gta} is not writable (run setup as administrator or change the folder permissions)')
    return errors, warnings


def read_keys(ini):
    keys = {}
    if Path(ini).is_file():
        for line in Path(ini).read_text(encoding='utf-8-sig', errors='replace').splitlines():
            s = line.strip()
            if s and s[0] not in ';#[' and '=' in s:
                k, v = s.split('=', 1)
                keys[k.strip()] = v.strip()
    return keys


def render_ini(placeholders, old_ini):
    """The template with placeholders filled and the player's kept values carried over."""
    text = TEMPLATE.read_text(encoding='utf-8')
    for k, v in placeholders.items():
        text = text.replace('{' + k + '}', str(v))
    old = read_keys(old_ini)
    lines = []
    for line in text.splitlines():
        s = line.strip()
        if s and s[0] not in ';#[' and '=' in s:
            k = s.split('=', 1)[0].strip()
            if k in KEEP_KEYS and k in old:
                line = f'{k}={old[k]}'
        lines.append(line)
    return '\n'.join(lines) + '\n'


def install(gta, plan):
    """Copies plan['files'] into the GTA folder atomically per file, rolling
    every file back if any step fails. Returns the written relative paths."""
    gta = Path(gta)
    manifest_path = gta / MANIFEST
    owned = json.loads(manifest_path.read_text(encoding='utf-8')).get('files', {}) if manifest_path.is_file() else {}
    allow = set(plan.get('allow_existing', []))
    ini_text = render_ini(plan.get('ini', {}), gta / INI)
    staged = []  # (dest, new, old-or-None)
    written = {}
    try:
        for rel, src in sorted(plan['files'].items()):
            dest = gta / rel
            if dest.exists() and rel not in owned and rel not in allow and sha256(dest) != sha256(src):
                raise SetupError(f'{rel} exists and was not installed by CODV; refusing to replace it')
            dest.parent.mkdir(parents=True, exist_ok=True)
            new = dest.with_name(dest.name + NEW)
            new.write_bytes(Path(src).read_bytes())
            staged.append((dest, new, None))
            written[rel] = sha256(new)
        ini_new = gta / (INI + NEW)
        ini_new.write_text(ini_text, encoding='utf-8')
        staged.append((gta / INI, ini_new, None))
        # Swap: keep each old file until every new one is in place.
        for i, (dest, new, _) in enumerate(staged):
            old = None
            if dest.exists():
                old = dest.with_name(dest.name + OLD)
                if old.exists():
                    old.unlink()
                dest.replace(old)
            new.replace(dest)
            staged[i] = (dest, new, old)
    except Exception:
        for dest, new, old in reversed(staged):
            if old is not None and old.exists():
                old.replace(dest)
            elif new.exists():
                new.unlink()
        raise
    for _, _, old in staged:
        if old is not None and old.exists():
            old.unlink()
    # Files a previous version installed that this one no longer ships.
    for rel, digest in owned.items():
        p = gta / rel
        if rel not in written and p.is_file() and sha256(p) == digest:
            p.unlink()
    manifest_path.write_text(json.dumps({'files': written, 'ini': INI}, indent=2), encoding='utf-8')
    return sorted(written)


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('command', choices=['preflight', 'install'])
    p.add_argument('--gta', type=Path, required=True)
    p.add_argument('--plan', type=Path)
    a = p.parse_args()
    plan = json.loads(a.plan.read_text(encoding='utf-8')) if a.plan else {}
    errors, warnings = preflight(a.gta, plan.get('require_scripthook', True))
    for w in warnings:
        print('warning:', w)
    if errors:
        for e in errors:
            print('error:', e)
        return 1
    if a.command == 'install':
        if not a.plan:
            raise SystemExit('install needs --plan')
        for rel in install(a.gta, plan):
            print('installed', rel)
    return 0


if __name__ == '__main__':
    sys.exit(main())
