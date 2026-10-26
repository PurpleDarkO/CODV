//! The weapon table (data/weapons.json): player rules, the default loadout and
//! every weapon's handling. Parsed with the small json.rs and validated so a
//! bad edit names the weapon and field instead of misbehaving in game.
use crate::json::{self, Value};

pub const EMBEDDED_JSON: &str = include_str!("../../../data/weapons.json");

type R<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    Auto,
    Semi,
    Burst,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponDef {
    pub name: String,
    pub label: String,
    /// GTA weapon name ("weapon_carbinerifle") and its joaat hash.
    pub gta: String,
    pub gta_hash: u32,
    pub class: String,
    pub fire: FireMode,
    pub rpm: f32,
    pub burst: u32,
    pub burst_delay: f32,
    /// Damage at or inside range[0], falling linearly to damage[1] at range[1].
    pub damage: [f32; 2],
    pub range: [f32; 2],
    pub head: f32,
    pub pellets: u32,
    pub mag: u32,
    pub reserve: u32,
    pub reload: f32,
    pub reload_empty: f32,
    /// > 0: rounds go in one at a time (shotguns), this many seconds each.
    pub reload_round: f32,
    pub ads_time: f32,
    pub sprint_out: f32,
    pub raise: f32,
    pub drop: f32,
    pub hip_spread: [f32; 2],
    pub ads_spread: f32,
    pub bloom: f32,
    pub bloom_decay: f32,
    pub move_spread: f32,
    pub kick_pitch: [f32; 2],
    pub kick_yaw: [f32; 2],
    pub ads_kick: f32,
    pub kick_recover: f32,
    pub move_scale: f32,
    pub ads_move: f32,
}

impl WeaponDef {
    pub fn shot_interval(&self) -> f32 {
        60.0 / self.rpm
    }

    /// Damage of one bullet (or pellet) at `distance` metres.
    pub fn damage_at(&self, distance: f32, head: bool) -> f32 {
        let [near, far] = self.range;
        let t = if distance <= near {
            0.0
        } else if distance >= far {
            1.0
        } else {
            (distance - near) / (far - near)
        };
        let d = self.damage[0] + (self.damage[1] - self.damage[0]) * t;
        if head { d * self.head } else { d }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerRules {
    pub sprint_seconds: f32,
    pub sprint_min: f32,
    pub sprint_regen_delay: f32,
    pub sprint_regen_rate: f32,
    pub sprint_multiplier: f32,
    pub walk_speed: f32,
    pub ads_move_scale: f32,
    pub crouch_spread_scale: f32,
    pub crouch_kick_scale: f32,
    pub air_spread_add: f32,
    pub regen_delay: f32,
    pub regen_seconds: f32,
    pub quick_jump_speed: f32,
    pub melee_damage: f32,
    pub melee_range: f32,
    pub melee_seconds: f32,
    pub trace_range: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub player: PlayerRules,
    pub weapons: Vec<WeaponDef>,
    /// Indices into `weapons`.
    pub primary: usize,
    pub secondary: usize,
}

/// GTA's joaat (case-insensitive one-at-a-time hash).
pub fn joaat(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.to_ascii_lowercase().bytes() {
        h = h.wrapping_add(c as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

fn field<'a>(v: &'a Value, key: &str, ctx: &str) -> R<&'a Value> {
    v.get(key).ok_or_else(|| format!("{ctx}: missing \"{key}\""))
}

fn num(v: &Value, key: &str, ctx: &str) -> R<f32> {
    let n = field(v, key, ctx)?.as_f64().ok_or_else(|| format!("{ctx}: \"{key}\" must be a number"))?;
    if !n.is_finite() {
        return Err(format!("{ctx}: \"{key}\" must be finite"));
    }
    Ok(n as f32)
}

fn num_or(v: &Value, key: &str, def: f32, ctx: &str) -> R<f32> {
    if v.get(key).is_some() { num(v, key, ctx) } else { Ok(def) }
}

fn pair(v: &Value, key: &str, ctx: &str) -> R<[f32; 2]> {
    let a = field(v, key, ctx)?.as_array().ok_or_else(|| format!("{ctx}: \"{key}\" must be [a, b]"))?;
    match a {
        [x, y] => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok([x as f32, y as f32]),
            _ => Err(format!("{ctx}: \"{key}\" must hold two numbers")),
        },
        _ => Err(format!("{ctx}: \"{key}\" must be [a, b]")),
    }
}

fn text<'a>(v: &'a Value, key: &str, ctx: &str) -> R<&'a str> {
    field(v, key, ctx)?.as_str().ok_or_else(|| format!("{ctx}: \"{key}\" must be a string"))
}

fn positive(x: f32, key: &str, ctx: &str) -> R<f32> {
    if x > 0.0 { Ok(x) } else { Err(format!("{ctx}: \"{key}\" must be > 0")) }
}

fn non_negative(x: f32, key: &str, ctx: &str) -> R<f32> {
    if x >= 0.0 { Ok(x) } else { Err(format!("{ctx}: \"{key}\" must be >= 0")) }
}

fn weapon(v: &Value) -> R<WeaponDef> {
    let name = text(v, "name", "weapon")?.to_owned();
    let c = format!("weapon {name}");
    let c = c.as_str();
    let fire = match text(v, "fire", c)? {
        "auto" => FireMode::Auto,
        "semi" => FireMode::Semi,
        "burst" => FireMode::Burst,
        f => return Err(format!("{c}: unknown fire mode \"{f}\" (auto, semi, burst)")),
    };
    let gta = text(v, "gta", c)?.to_owned();
    if !gta.to_ascii_lowercase().starts_with("weapon_") {
        return Err(format!("{c}: \"gta\" must be a GTA weapon name such as weapon_carbinerifle"));
    }
    let burst = num_or(v, "burst", 0.0, c)?;
    if fire == FireMode::Burst && !(2.0..=10.0).contains(&burst) {
        return Err(format!("{c}: burst weapons need \"burst\" 2..10"));
    }
    let range = pair(v, "range", c)?;
    if !(range[0] >= 0.0 && range[1] > range[0]) {
        return Err(format!("{c}: \"range\" must be [near, far] with far > near >= 0"));
    }
    let hip = pair(v, "hip_spread", c)?;
    if !(hip[0] >= 0.0 && hip[1] >= hip[0]) {
        return Err(format!("{c}: \"hip_spread\" must be [min, max] with max >= min >= 0"));
    }
    let pellets = num_or(v, "pellets", 1.0, c)?;
    if !(1.0..=20.0).contains(&pellets) {
        return Err(format!("{c}: \"pellets\" must be 1..20"));
    }
    let mag = num(v, "mag", c)?;
    if !(1.0..=500.0).contains(&mag) {
        return Err(format!("{c}: \"mag\" must be 1..500"));
    }
    let reload = positive(num(v, "reload", c)?, "reload", c)?;
    Ok(WeaponDef {
        label: v.get("label").and_then(Value::as_str).unwrap_or(&name).to_owned(),
        gta_hash: joaat(&gta),
        gta,
        class: v.get("class").and_then(Value::as_str).unwrap_or("rifle").to_owned(),
        fire,
        rpm: positive(num(v, "rpm", c)?, "rpm", c)?,
        burst: burst as u32,
        burst_delay: non_negative(num_or(v, "burst_delay", 0.2, c)?, "burst_delay", c)?,
        damage: pair(v, "damage", c)?,
        range,
        head: positive(num_or(v, "head", 1.0, c)?, "head", c)?,
        pellets: pellets as u32,
        mag: mag as u32,
        reserve: non_negative(num_or(v, "reserve", mag * 4.0, c)?, "reserve", c)? as u32,
        reload,
        reload_empty: positive(num_or(v, "reload_empty", reload, c)?, "reload_empty", c)?,
        reload_round: non_negative(num_or(v, "reload_round", 0.0, c)?, "reload_round", c)?,
        ads_time: positive(num(v, "ads_time", c)?, "ads_time", c)?,
        sprint_out: non_negative(num(v, "sprint_out", c)?, "sprint_out", c)?,
        raise: non_negative(num(v, "raise", c)?, "raise", c)?,
        drop: non_negative(num(v, "drop", c)?, "drop", c)?,
        hip_spread: hip,
        ads_spread: non_negative(num(v, "ads_spread", c)?, "ads_spread", c)?,
        bloom: non_negative(num_or(v, "bloom", 0.0, c)?, "bloom", c)?,
        bloom_decay: positive(num_or(v, "bloom_decay", 6.0, c)?, "bloom_decay", c)?,
        move_spread: non_negative(num_or(v, "move_spread", 0.0, c)?, "move_spread", c)?,
        kick_pitch: pair(v, "kick_pitch", c)?,
        kick_yaw: pair(v, "kick_yaw", c)?,
        ads_kick: non_negative(num_or(v, "ads_kick", 1.0, c)?, "ads_kick", c)?,
        kick_recover: num_or(v, "kick_recover", 0.0, c)?.clamp(0.0, 1.0),
        move_scale: positive(num_or(v, "move", 1.0, c)?, "move", c)?,
        ads_move: positive(num_or(v, "ads_move", 1.0, c)?, "ads_move", c)?,
        name,
    })
}

fn player(v: &Value) -> R<PlayerRules> {
    let c = "player";
    Ok(PlayerRules {
        sprint_seconds: positive(num(v, "sprint_seconds", c)?, "sprint_seconds", c)?,
        sprint_min: non_negative(num_or(v, "sprint_min", 1.0, c)?, "sprint_min", c)?,
        sprint_regen_delay: non_negative(num_or(v, "sprint_regen_delay", 1.0, c)?, "sprint_regen_delay", c)?,
        sprint_regen_rate: positive(num_or(v, "sprint_regen_rate", 1.0, c)?, "sprint_regen_rate", c)?,
        sprint_multiplier: num_or(v, "sprint_multiplier", 1.2, c)?.clamp(1.0, 1.49),
        walk_speed: positive(num_or(v, "walk_speed", 2.0, c)?, "walk_speed", c)?,
        ads_move_scale: num_or(v, "ads_move_scale", 0.6, c)?.clamp(0.1, 1.0),
        crouch_spread_scale: num_or(v, "crouch_spread_scale", 0.7, c)?.clamp(0.1, 1.0),
        crouch_kick_scale: num_or(v, "crouch_kick_scale", 0.85, c)?.clamp(0.1, 1.0),
        air_spread_add: non_negative(num_or(v, "air_spread_add", 4.0, c)?, "air_spread_add", c)?,
        regen_delay: non_negative(num_or(v, "regen_delay", 5.0, c)?, "regen_delay", c)?,
        regen_seconds: positive(num_or(v, "regen_seconds", 2.5, c)?, "regen_seconds", c)?,
        quick_jump_speed: positive(num_or(v, "quick_jump_speed", 4.8, c)?, "quick_jump_speed", c)?,
        melee_damage: non_negative(num_or(v, "melee_damage", 150.0, c)?, "melee_damage", c)?,
        melee_range: positive(num_or(v, "melee_range", 2.0, c)?, "melee_range", c)?,
        melee_seconds: positive(num_or(v, "melee_seconds", 0.65, c)?, "melee_seconds", c)?,
        trace_range: positive(num_or(v, "trace_range", 350.0, c)?, "trace_range", c)?,
    })
}

impl Table {
    pub fn parse(text_json: &str) -> R<Table> {
        let v = json::parse(text_json)?;
        let player = player(field(&v, "player", "table")?)?;
        let list = field(&v, "weapons", "table")?.as_array().ok_or("\"weapons\" must be an array")?;
        let mut weapons = Vec::with_capacity(list.len());
        for w in list {
            let w = weapon(w)?;
            if weapons.iter().any(|o: &WeaponDef| o.name == w.name) {
                return Err(format!("weapon {}: defined twice", w.name));
            }
            if let Some(o) = weapons.iter().find(|o: &&WeaponDef| o.gta_hash == w.gta_hash) {
                return Err(format!("weapon {}: {} is already used by {}", w.name, w.gta, o.name));
            }
            weapons.push(w);
        }
        if weapons.is_empty() {
            return Err("\"weapons\" is empty".into());
        }
        let loadout = field(&v, "loadout", "table")?;
        let slot = |key: &str| -> R<usize> {
            let n = text(loadout, key, "loadout")?;
            weapons.iter().position(|w| w.name == n).ok_or_else(|| format!("loadout: no weapon named \"{n}\""))
        };
        let (primary, secondary) = (slot("primary")?, slot("secondary")?);
        Ok(Table { player, weapons, primary, secondary })
    }

    pub fn embedded() -> Table {
        Table::parse(EMBEDDED_JSON).expect("shipped data/weapons.json is valid")
    }

    pub fn by_hash(&self, hash: u32) -> Option<usize> {
        self.weapons.iter().position(|w| w.gta_hash == hash)
    }

    pub fn by_name(&self, name: &str) -> Option<usize> {
        self.weapons.iter().position(|w| w.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_table_is_valid() {
        let t = Table::embedded();
        assert_eq!(t.weapons[t.primary].name, "carbine");
        assert_eq!(t.weapons[t.secondary].name, "pistol");
        assert!(t.weapons.len() >= 8);
        // joaat matches GTA (the host's static_assert uses the same value).
        assert_eq!(joaat("weapon_unarmed"), 0xA2719263);
        assert_eq!(t.weapons[t.primary].gta_hash, joaat("WEAPON_CARBINERIFLE"));
    }

    #[test]
    fn damage_falls_off_linearly_with_range() {
        let t = Table::embedded();
        let c = &t.weapons[t.by_name("carbine").unwrap()];
        assert_eq!(c.damage_at(5.0, false), 30.0);
        assert_eq!(c.damage_at(35.0, false), 25.0);
        assert_eq!(c.damage_at(100.0, false), 20.0);
        assert!((c.damage_at(5.0, true) - 42.0).abs() < 1e-4);
        assert!((c.shot_interval() - 0.08).abs() < 1e-6);
    }

    #[test]
    fn bad_edits_name_the_weapon_and_field() {
        let bad = EMBEDDED_JSON.replacen("\"rpm\": 750", "\"rpm\": -1", 1);
        assert_eq!(Table::parse(&bad).unwrap_err(), "weapon carbine: \"rpm\" must be > 0");
        let bad = EMBEDDED_JSON.replacen("\"fire\": \"auto\"", "\"fire\": \"laser\"", 1);
        assert!(Table::parse(&bad).unwrap_err().contains("unknown fire mode"));
        let bad = EMBEDDED_JSON.replacen("\"primary\": \"carbine\"", "\"primary\": \"railgun\"", 1);
        assert_eq!(Table::parse(&bad).unwrap_err(), "loadout: no weapon named \"railgun\"");
        let bad = EMBEDDED_JSON.replacen("weapon_assaultrifle", "weapon_carbinerifle", 1);
        assert!(Table::parse(&bad).unwrap_err().contains("already used by carbine"));
    }
}
