"""Checks every CV_NATIVE(NAME, 0xHASH) in host/src/natives.h against the
pinned alloc8or nativedb (upstream/gta5-nativedb-data, tools/fetch-upstreams.ps1).

    python tools/check-natives.py [--db <natives.json>]
"""
import argparse
import json
import re
import sys

from _common import REPO

NATIVE = re.compile(r'^CV_NATIVE\((\w+),\s*(0x[0-9A-Fa-f]{16})\)', re.M)


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--db', default=REPO / 'upstream/gta5-nativedb-data/natives.json')
    p.add_argument('--header', default=REPO / 'host/src/natives.h')
    a = p.parse_args()
    db = json.load(open(a.db, encoding='utf-8'))
    by_name = {f['name']: h.upper() for ns in db.values() for h, f in ns.items()}
    found = NATIVE.findall(open(a.header, encoding='utf-8').read())
    bad = [(n, h, by_name.get(n)) for n, h in found if by_name.get(n) != '0X' + h[2:].upper()]
    for name, have, want in bad:
        print(f'MISMATCH {name}: header {have}, nativedb {want or "unknown name"}')
    print(f'{len(found)} natives checked, {len(bad)} mismatched')
    return 1 if bad or not found else 0


if __name__ == '__main__':
    sys.exit(main())
