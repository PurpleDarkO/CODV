"""Helpers shared by the tools/*.py scripts. Standard library only."""
import hashlib
import json
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]


def sha256_file(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def sha256_text(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def joaat(text):
    """GTA's case-insensitive Jenkins one-at-a-time hash."""
    h = 0
    for c in text.lower().encode():
        h = (h + c) & 0xFFFFFFFF
        h = (h + (h << 10)) & 0xFFFFFFFF
        h ^= h >> 6
    h = (h + (h << 3)) & 0xFFFFFFFF
    h ^= h >> 11
    return (h + (h << 15)) & 0xFFFFFFFF


def lock(name):
    """One repository entry of upstreams.lock.json."""
    data = json.loads((REPO / 'upstreams.lock.json').read_text(encoding='utf-8'))
    return next(r for r in data['repositories'] if r['name'] == name)


def read_ini(path):
    """{section: {key: value}} with lower-case keys; ';' comments. Tolerant like GetPrivateProfileString."""
    out, section = {}, None
    for raw in Path(path).read_text(encoding='utf-8-sig').splitlines():
        line = raw.strip()
        if not line or line[0] in ';#':
            continue
        if line.startswith('[') and line.endswith(']'):
            section = line[1:-1].strip()
            out.setdefault(section, {})
        elif '=' in line and section is not None:
            k, v = line.split('=', 1)
            out[section][k.strip().lower()] = v.strip()
    return out
