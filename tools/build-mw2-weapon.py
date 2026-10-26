"""WEAPON_CODV_MW2 for CODV's DLC pack: the MW2 item in GTA's weapon wheel.
Picking it turns CODV's Modern Warfare 2 mode on (host/src/main.cpp), as SK8V's
board item takes the skateboard out.

Clones the retail WEAPON_HAMMER (a one-hand melee weapon: no ammo, no pickup
needed) from the player's own update.rpf meta into weapons_codv.meta and
weaponanimations_codv.meta under its own slot, named WT_CODV_MW2 ("MW2").
The hammer model stays the weapon model; the host hides it and switches to the
MW2 weapons. Port of SK8V tools/build-board-weapon.py (Apache-2.0, NOTICE).

usage: build-mw2-weapon.py <weapons.meta> <weaponanimations.meta> <out dir>
(extracted by codv-gtapack from <GTA>/update/update.rpf, common/data/ai/)
"""
import copy
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

SRC = 'WEAPON_HAMMER'
NAME = 'WEAPON_CODV_MW2'
SLOT = 'SLOT_CODV_MW2'
LABEL = 'WT_CODV_MW2'


def weapon_info(src):
    blob = ET.parse(src).getroot()
    infos = [i for i in blob.iter('Item') if i.get('type') == 'CWeaponInfo' and i.findtext('Name') == SRC]
    assert len(infos) == 1, f'{SRC} not found once in {src}'
    w = copy.deepcopy(infos[0])
    w.find('Name').text = NAME
    w.find('Slot').text = SLOT
    w.find('HumanNameHash').text = LABEL
    w.find('PickupHash').text = None  # no hammer pickup dropped on death
    # Slot orders: the wheel steps fists -> MW2 (right after unarmed, as SK8V's
    # board; +6 so both items fit when SK8V is installed too); the best-weapon
    # list keeps it after the hammer so GTA never auto-equips it.
    order = {}
    navs = blob.find('SlotNavigateOrder').findall('Item')
    for lst in navs + [blob.find('SlotBestOrder')]:
        after = 'SLOT_UNARMED' if lst in navs else 'SLOT_HAMMER'
        for it in lst.find('WeaponSlots').findall('Item'):
            if it.findtext('Entry') == after:
                order.setdefault(id(lst), int(it.find('OrderNumber').get('value')) + 6)
    nav = [order[id(lst)] for lst in navs]
    best = order[id(blob.find('SlotBestOrder'))]

    def slots(parent, n):
        ws = ET.SubElement(parent, 'WeaponSlots')
        it = ET.SubElement(ws, 'Item')
        ET.SubElement(it, 'OrderNumber', value=str(n))
        ET.SubElement(it, 'Entry').text = SLOT

    out = ET.Element('CWeaponInfoBlob')
    sno = ET.SubElement(out, 'SlotNavigateOrder')
    for n in nav:
        slots(ET.SubElement(sno, 'Item'), n)
    slots(ET.SubElement(out, 'SlotBestOrder'), best)
    for tag in ('TintSpecValues', 'FiringPatternAliases', 'UpperBodyFixupExpressionData', 'AimingInfos'):
        ET.SubElement(out, tag)
    item = ET.SubElement(ET.SubElement(out, 'Infos'), 'Item')
    ET.SubElement(item, 'Infos').append(w)
    ET.SubElement(out, 'VehicleWeaponInfos')
    ET.SubElement(out, 'Name').text = 'CODV - MW2'
    return out, nav, best


def animations(src):
    sets = ET.parse(src).getroot().find('WeaponAnimationsSets')
    out = ET.Element('CWeaponAnimationsSets')
    oset = ET.SubElement(out, 'WeaponAnimationsSets')
    n = 0
    for s in sets.findall('Item'):
        hit = [a for a in s.find('WeaponAnimations').findall('Item') if a.get('key') == SRC]
        if not hit:
            continue
        o = ET.SubElement(oset, 'Item', key=s.get('key'))
        ET.SubElement(o, 'Fallback').text = s.findtext('Fallback') or None
        a = copy.deepcopy(hit[0])
        a.set('key', NAME)
        ET.SubElement(o, 'WeaponAnimations').append(a)
        n += 1
    assert n, f'{SRC} has no animation sets in {src}'
    return out, n


def write(tree, path):
    ET.indent(tree)
    path.write_bytes(b'<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(tree, encoding='utf-8') + b'\n')


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    weapons, anims, out = map(Path, sys.argv[1:])
    out.mkdir(parents=True, exist_ok=True)
    w, nav, best = weapon_info(weapons)
    an, sets = animations(anims)
    write(w, out / 'weapons_codv.meta')
    write(an, out / 'weaponanimations_codv.meta')
    # Self-check: the written files parse back and name the weapon once.
    back = ET.parse(out / 'weapons_codv.meta').getroot()
    assert [i.findtext('Name') for i in back.iter('Item') if i.get('type') == 'CWeaponInfo'] == [NAME]
    print(f'{NAME} from {SRC}: slot orders {nav} / best {best}, {sets} animation sets -> {out}')


if __name__ == '__main__':
    main()
