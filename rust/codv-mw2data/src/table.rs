//! CODV's MW2 weapon table: what the importer writes (`<GTA>/CODV/mw2/weapons.json`)
//! and the runtime reads. Each weapon keeps the retail definition's numbers
//! under their iw4L `WeaponGeometry` field names (`n`), its alias and model
//! names (`s`), and its own hit-location multipliers (`location`, or null);
//! the table keeps MW2's global hit-location table (`info/mp_lochit_dmgtable`).
//!
//! Plain JSON through codv-core's own reader and writer: the runtime needs
//! no third-party crates for it.
use codv_core::json::{self, Value};

pub const FORMAT: &str = "codv-mw2-weapons";
pub const VERSION: f64 = 1.0;
/// MW2's 20 hit locations (weapon_iw4::HITLOC_NAMES order).
pub const HITLOCS: usize = 20;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub name: String,
    pub display_name: String,
    pub numbers: Vec<(String, f64)>,
    pub strings: Vec<(String, String)>,
    pub location: Option<[f32; HITLOCS]>,
}

impl Row {
    /// A number by its `WeaponGeometry` name (`kick.` prefix for the kick).
    pub fn num(&self, key: &str) -> Option<f64> {
        self.numbers.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }
    pub fn i(&self, key: &str) -> i32 {
        self.num(key).map_or(0, |v| v as i32)
    }
    pub fn f(&self, key: &str) -> f32 {
        self.num(key).map_or(0.0, |v| v as f32)
    }
    pub fn b(&self, key: &str) -> bool {
        self.num(key).is_some_and(|v| v != 0.0)
    }
    /// An optional flag (-1 in the table: unset).
    pub fn opt_b(&self, key: &str) -> Option<bool> {
        self.num(key).filter(|v| *v >= 0.0).map(|v| v != 0.0)
    }
    pub fn s(&self, key: &str) -> &str {
        self.strings.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// Which files it was read from (log line).
    pub source: String,
    pub location: Option<[f32; HITLOCS]>,
    pub weapons: Vec<Row>,
}

impl Table {
    pub fn find(&self, name: &str) -> Option<&Row> {
        self.weapons.iter().find(|w| w.name == name)
    }
}

fn floats(v: &[f32]) -> Value {
    Value::Arr(v.iter().map(|x| Value::Num(*x as f64)).collect())
}

fn read_floats(v: Option<&Value>) -> Option<[f32; HITLOCS]> {
    let a = v?.as_array()?;
    if a.len() != HITLOCS {
        return None;
    }
    let mut out = [0.0f32; HITLOCS];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_f64()? as f32;
    }
    Some(out)
}

pub fn to_json(t: &Table) -> String {
    let rows = t
        .weapons
        .iter()
        .map(|w| {
            json::obj([
                ("name", Value::Str(w.name.clone())),
                ("display_name", Value::Str(w.display_name.clone())),
                ("n", Value::Obj(w.numbers.iter().map(|(k, v)| (k.clone(), Value::Num(*v))).collect())),
                ("s", Value::Obj(w.strings.iter().map(|(k, v)| (k.clone(), Value::Str(v.clone()))).collect())),
                ("location", w.location.map_or(Value::Null, |l| floats(&l))),
            ])
        })
        .collect();
    json::to_string_pretty(&json::obj([
        ("format", Value::Str(FORMAT.into())),
        ("version", Value::Num(VERSION)),
        ("source", Value::Str(t.source.clone())),
        ("location", t.location.map_or(Value::Null, |l| floats(&l))),
        ("weapons", Value::Arr(rows)),
    ]))
}

pub fn from_json(text: &str) -> Result<Table, String> {
    let v = json::parse(text)?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!("not a {FORMAT} file"));
    }
    if v.get("version").and_then(Value::as_f64) != Some(VERSION) {
        return Err("table version differs: run the MW2 import again".into());
    }
    let mut t = Table {
        source: v.get("source").and_then(Value::as_str).unwrap_or("").to_owned(),
        location: read_floats(v.get("location")),
        weapons: Vec::new(),
    };
    for w in v.get("weapons").and_then(Value::as_array).ok_or("no weapons")? {
        let pairs = |key: &str| w.get(key).and_then(Value::as_object).unwrap_or(&[]);
        t.weapons.push(Row {
            name: w.get("name").and_then(Value::as_str).ok_or("weapon without a name")?.to_owned(),
            display_name: w.get("display_name").and_then(Value::as_str).unwrap_or("").to_owned(),
            numbers: pairs("n").iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect(),
            strings: pairs("s").iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned()))).collect(),
            location: read_floats(w.get("location")),
        });
    }
    Ok(t)
}

/// The sound fields an attachment variant often leaves empty in MW2's zones: a variant that sounds as its base
/// gun points its sounds at the aliases the base already loaded, which the import does not follow.
pub const VARIANT_SOUND_FIELDS: [&str; 6] = [
    "fire_sound",
    "fire_sound_player",
    "fire_last_sound_player",
    "empty_fire_sound_player",
    "raise_sound_player",
    "melee_swipe_sound_player",
];

/// Fills an attachment variant's empty sound fields from the gun it sounds as: `<base>_silencer_mp` for a
/// silenced variant (when that one has the sound), else `<base>_mp` (m4_reflex_mp fires as m4_mp,
/// m4_reflex_silencer_mp as m4_silencer_mp). Fields the variant has are kept. How many fields were filled.
pub fn fill_variant_sounds(table: &mut Table) -> usize {
    let index: std::collections::HashMap<String, usize> =
        table.weapons.iter().enumerate().map(|(i, r)| (r.name.clone(), i)).collect();
    let mut fills: Vec<(usize, &'static str, String)> = Vec::new();
    for (i, row) in table.weapons.iter().enumerate() {
        let Some(stem) = row.name.strip_suffix("_mp") else { continue };
        let Some((base, parts)) = stem.split_once('_') else { continue };
        let mut sources = Vec::new();
        if parts.split('_').any(|p| p == "silencer") {
            sources.push(format!("{base}_silencer_mp"));
        }
        sources.push(format!("{base}_mp"));
        for field in VARIANT_SOUND_FIELDS {
            if !row.s(field).is_empty() {
                continue;
            }
            let found = sources
                .iter()
                .filter(|s| **s != row.name)
                .filter_map(|s| index.get(s).map(|&j| table.weapons[j].s(field)))
                .find(|v| !v.is_empty());
            if let Some(v) = found {
                fills.push((i, field, v.to_owned()));
            }
        }
    }
    let n = fills.len();
    for (i, field, v) in fills {
        let row = &mut table.weapons[i];
        row.strings.retain(|(k, _)| k != field);
        row.strings.push((field.to_owned(), v));
    }
    n
}

#[cfg(test)]
mod tests {
    #[test]
    fn variants_sound_as_their_gun() {
        use super::*;
        let row = |name: &str, fire: &str| Row {
            name: name.into(),
            strings: if fire.is_empty() { Vec::new() } else { vec![("fire_sound_player".into(), fire.into())] },
            ..Default::default()
        };
        let mut t = Table {
            weapons: vec![
                row("m4_mp", "weap_m4carbine_fire_plr"),
                row("m4_silencer_mp", "weap_m4silenced_fire_plr"),
                row("m4_reflex_mp", ""),
                row("m4_reflex_silencer_mp", ""),
                row("m4_shotgun_mp", "own_sound"),
                row("frag_grenade_mp", ""),
            ],
            ..Default::default()
        };
        assert_eq!(fill_variant_sounds(&mut t), 2);
        assert_eq!(t.weapons[2].s("fire_sound_player"), "weap_m4carbine_fire_plr");
        assert_eq!(t.weapons[3].s("fire_sound_player"), "weap_m4silenced_fire_plr");
        assert_eq!(t.weapons[4].s("fire_sound_player"), "own_sound");
        assert_eq!(t.weapons[5].s("fire_sound_player"), "");
    }

    use super::*;

    #[test]
    fn round_trips() {
        let mut loc = [1.0f32; HITLOCS];
        loc[2] = 1.4;
        let t = Table {
            source: "common_mp.ff".into(),
            location: Some(loc),
            weapons: vec![Row {
                name: "m4_mp".into(),
                display_name: "WEAPON_M4_CARBINE".into(),
                numbers: vec![
                    ("fire_time_ms".into(), 70.0),
                    ("kick.hip_view_kick_pitch_max".into(), 1.5),
                    ("select_requires_ammo".into(), -1.0),
                ],
                strings: vec![("fire_sound".into(), "weap_m4_fire".into())],
                location: None,
            }],
        };
        let back = from_json(&to_json(&t)).unwrap();
        assert_eq!(back, t);
        let m4 = back.find("m4_mp").unwrap();
        assert_eq!((m4.i("fire_time_ms"), m4.f("kick.hip_view_kick_pitch_max")), (70, 1.5));
        assert_eq!(m4.opt_b("select_requires_ammo"), None);
        assert_eq!(m4.i("missing"), 0);
        assert!(from_json("{\"format\":\"x\"}").is_err());
    }
}
