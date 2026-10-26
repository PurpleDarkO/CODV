"""Finds the player's GTA V Legacy install (Steam libraries from the registry +
libraryfolders.vdf, the Rockstar launcher's registry key, then common folders)
and Modern Warfare 2 (2009, PC; Steam libraries). Standard library only.

    python tools/gta_locate.py        prints {"gta": ..., "mw2": ...}
"""
import json
import os
import re
from pathlib import Path

GTA_STEAM_APP = 271590
# Modern Warfare 2 (2009): the single-player and multiplayer apps share one folder.
MW2_STEAM_APPS = (10180, 10190)


def _registry(path, value, hive='HKLM'):
    if os.name != 'nt':
        return None
    import winreg
    root = winreg.HKEY_LOCAL_MACHINE if hive == 'HKLM' else winreg.HKEY_CURRENT_USER
    for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
        try:
            with winreg.OpenKey(root, path, 0, winreg.KEY_READ | view) as k:
                return winreg.QueryValueEx(k, value)[0]
        except OSError:
            continue
    return None


def steam_libraries():
    roots = []
    for hive, key in (('HKCU', r'Software\Valve\Steam'), ('HKLM', r'SOFTWARE\Valve\Steam')):
        p = _registry(key, 'SteamPath', hive) or _registry(key, 'InstallPath', hive)
        if p:
            roots.append(Path(p))
    roots.append(Path(r'C:\Program Files (x86)\Steam'))
    libs = []
    for root in roots:
        vdf = root / 'steamapps' / 'libraryfolders.vdf'
        if vdf.is_file():
            for m in re.finditer(r'"path"\s+"([^"]+)"', vdf.read_text(encoding='utf-8', errors='replace')):
                libs.append(Path(m.group(1).replace('\\\\', '\\')))
        libs.append(root)
    seen, out = set(), []
    for lib in libs:
        key = str(lib).lower()
        if key not in seen and (lib / 'steamapps').is_dir():
            seen.add(key)
            out.append(lib)
    return out


def steam_app(app_id):
    for lib in steam_libraries():
        acf = lib / 'steamapps' / f'appmanifest_{app_id}.acf'
        if acf.is_file():
            m = re.search(r'"installdir"\s+"([^"]+)"', acf.read_text(encoding='utf-8', errors='replace'))
            if m and (lib / 'steamapps' / 'common' / m.group(1)).is_dir():
                return lib / 'steamapps' / 'common' / m.group(1)
    return None


def is_gta(path):
    return path is not None and (Path(path) / 'GTA5.exe').is_file()


def find_gta():
    candidates = [steam_app(GTA_STEAM_APP),
                  _registry(r'SOFTWARE\Rockstar Games\Grand Theft Auto V', 'InstallFolder'),
                  Path(r'C:\Program Files\Rockstar Games\Grand Theft Auto V'),
                  Path(r'C:\Program Files\Epic Games\GTAV')]
    return next((Path(c) for c in candidates if c and is_gta(c)), None)


def is_mw2(path):
    return path is not None and (Path(path) / 'zone' / 'english' / 'common_mp.ff').is_file()


def find_mw2():
    candidates = [steam_app(app) for app in MW2_STEAM_APPS]
    return next((Path(c) for c in candidates if c and is_mw2(c)), None)


if __name__ == '__main__':
    print(json.dumps({'gta': str(find_gta() or ''), 'mw2': str(find_mw2() or '')}, indent=2))
