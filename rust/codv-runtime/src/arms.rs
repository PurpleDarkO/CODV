//! Modern Warfare 2's weapons behind the runtime's gunplay ABI: the table the
//! setup imported from the player's own MW2 (`<data>/mw2/weapons.json`) and
//! CODV's settings for it (`<data>/mw2.json`: loadout, the GTA weapon each
//! MW2 weapon borrows for its impacts and model until F4/F5, damage scale).
//! While MW2 movement drives the player, `cv_mw2_step` ticks the weapon
//! after pmove and `cv_step` / `cv_report_hits` answer from it instead of
//! the interim gunplay.
use crate::{CV_HIT_HEAD, CV_HIT_PED, CV_MAX_SHOTS, copy_str};
use crate::{
    CV_HUD_HEADSHOT, CV_HUD_KILL, CV_OUT_ACTIVE, CV_OUT_EQUIP, CV_OUT_MELEE, CV_OUT_RELOAD, CvHit, CvOutput, CvShot,
};
use codv_core::json::{self, Value};
use codv_engine::arms::{Arsenal, Gun, GunFrame, Gunner};
use codv_engine::units::METRES_PER_UNIT;
use std::path::Path;

/// CODV's own defaults for MW2 (data/mw2.json), used when the installed copy is missing.
const DEFAULT_CONFIG: &str = include_str!("../../../data/mw2.json");

/// codv_core::State values for CvOutput::state.
const STATE_READY: u32 = 1;
const STATE_FIRING: u32 = 2;
const STATE_RELOADING: u32 = 3;
const STATE_RAISING: u32 = 4;
const STATE_DROPPING: u32 = 5;
const STATE_MELEE: u32 = 6;
const STATE_SPRINTING: u32 = 7;
/// MW2 hit locations used when the host names none.
const HITLOC_HEAD: usize = 2;
const HITLOC_TORSO_UPPER: usize = 4;

/// The base guns CODV offers in the menu (MW2 multiplayer names without
/// attachments), in MW2's class order: assault rifles, SMGs, LMGs, snipers,
/// shotguns, machine pistols, handguns.
pub const MENU_GUNS: &[&str] = &[
    "m4",
    "famas",
    "scar",
    "tavor",
    "fal",
    "m16",
    "masada",
    "fn2000",
    "ak47",
    "mp5k",
    "ump45",
    "kriss",
    "p90",
    "uzi",
    "sa80",
    "rpd",
    "mg4",
    "aug",
    "m240",
    "cheytac",
    "barrett",
    "wa2000",
    "m21",
    "spas12",
    "aa12",
    "striker",
    "ranger",
    "m1014",
    "model1887",
    "pp2000",
    "glock",
    "beretta393",
    "tmp",
    "usp",
    "coltanaconda",
    "beretta",
    "deserteagle",
    "deserteaglegold",
];
/// How many of MENU_GUNS are primaries (assault rifles to snipers); the rest
/// (shotguns, machine pistols, handguns) are MW2's secondaries.
pub const MENU_PRIMARIES: usize = 23;
/// Where MW2's sniper rifles start in MENU_GUNS (cheytac .. m21): the third slot's pool; the
/// random primary comes from the guns before them (assault rifles, SMGs, LMGs).
pub const MENU_SNIPERS: usize = 19;
/// MW2's launchers (RPG-7, AT4, Stinger, Javelin, Thumper): the fourth slot's pool (key 5).
pub const MENU_LAUNCHERS: &[&str] = &["rpg", "at4", "stinger", "javelin", "m79"];
/// CODV's weapon sets (CV_OPT_WEAPON_SETS): up to this many guns behind each number key.
pub const SET_SIZE: usize = 3;
/// The number keys' groups (keys 1, 2, 3 and 5): primaries, secondaries, snipers, launchers.
pub const SETS: usize = 4;
/// The thrown grenade on MW2's frag button.
pub const FRAG: &str = "frag_grenade_mp";

/// Weapon requests beyond MW2's (CvMoveInput::weapon_request, number keys):
/// the knife (melee on the trigger) and the frag in hand (thrown on the trigger).
pub const REQUEST_KNIFE: u32 = 0x100;
pub const REQUEST_GRENADE: u32 = 0x101;
/// Retail button bits (playerstate_iw4::buttons).
const BUTTON_ATTACK: u32 = 0x1;
const BUTTON_MELEE: u32 = 0x4;
const BUTTON_ADS: u32 = 0x800;
const BUTTON_FRAG: u32 = 0x4000;

/// What the trigger does (CS-style number keys): the gun, the knife, the frag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hand {
    #[default]
    Gun,
    Knife,
    Grenade,
}

fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x.max(1);
    x
}

fn pick(rng: &mut u32, list: &[u32]) -> u32 {
    list[(xorshift(rng) as usize) % list.len()]
}

/// Up to `n` different picks from `list`, in random order.
fn pick_n(rng: &mut u32, list: &[u32], n: usize) -> Vec<u32> {
    let mut pool = list.to_vec();
    let mut out = Vec::new();
    while out.len() < n && !pool.is_empty() {
        let i = (xorshift(rng) as usize) % pool.len();
        out.push(pool.swap_remove(i));
    }
    out
}

/// GTA's joaat (atStringHash).
pub fn joaat(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.bytes() {
        let c = c.to_ascii_lowercase();
        h = h.wrapping_add(u32::from(c));
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// "WEAPON_M4_CARBINE" -> "M4 CARBINE".
pub fn label(display: &str, name: &str) -> String {
    let base = if display.is_empty() { name.trim_end_matches("_mp") } else { display.trim_start_matches("WEAPON_") };
    base.replace('_', " ").to_uppercase()
}

/// The rounds carried: each weapon's (id, clip, stock), and the frags.
type KeptAmmo = (Vec<(u32, i32, i32)>, i32);

pub struct Arms {
    pub arsenal: Arsenal,
    /// The imported table (viewmodel names and placement facts per weapon).
    pub table: codv_mw2data::table::Table,
    pub gunner: Option<Gunner>,
    /// The rounds carried when the mode last stopped (restored for the same guns).
    kept: Option<KeptAmmo>,
    loadout: Vec<u32>,
    /// Per weapon id - 1: the GTA weapon it borrows.
    gta: Vec<u32>,
    labels: Vec<String>,
    pub damage_scale: f32,
    /// Spare rounds times this (AmmoScale; MW2 is 1) and frags carried.
    pub ammo_scale: f32,
    pub frags: i32,
    /// Weapon ids of MENU_GUNS the player's MW2 has (the menu's list).
    pub menu: Vec<u32>,
    /// The knife or the frag on the trigger (number keys 1 / 4).
    pub hand: Hand,
    /// xorshift state for random loadouts.
    rng: u32,
    /// CODV's weapon sets: the loadout is SETS groups back to back (`set_len` guns each), a number
    /// key takes its group's last gun and, pressed again, the group's next one.
    pub sets: bool,
    set_len: [usize; SETS],
    /// The loadout index asked for last, and each group's last one.
    selected: usize,
    set_last: [usize; SETS],
    /// Table row index by weapon name (the HUD's per-weapon facts).
    rows: std::collections::HashMap<String, usize>,
    /// The weapon the view shows after the last tick (the grenade mid-throw).
    pub shown: u32,
    /// The last tick, for the next `encode` (taken once).
    pub last: Option<GunFrame>,
    /// The last tick, kept for MW2's HUD (clip, stock, frags, weapon state).
    pub latest: GunFrame,
    /// Peds hit and killed so far (MW2's hit marker starts on each change).
    pub hits: u32,
    pub kills: u32,
    equipped: u32,
    reload_total: i32,
    hitmarker: f32,
    kill: bool,
    headshot: bool,
    /// One line about what was loaded (the menu's status).
    pub summary: String,
}

impl Arms {
    /// The imported table and CODV's settings under `root`; None (logged)
    /// when the setup imported no MW2.
    pub fn load(root: Option<&Path>, log: &dyn Fn(&str)) -> Option<Arms> {
        let root = root?;
        let path = root.join("mw2").join("weapons.json");
        let text = std::fs::read_to_string(&path).ok()?;
        let table = match codv_mw2data::table::from_json(&text) {
            Ok(t) => t,
            Err(e) => {
                log(&format!("mw2: {} unreadable ({e}); interim gunplay", path.display()));
                return None;
            }
        };
        let config_text = std::fs::read_to_string(root.join("mw2.json")).unwrap_or_else(|_| DEFAULT_CONFIG.to_owned());
        let config = json::parse(&config_text).or_else(|_| json::parse(DEFAULT_CONFIG)).ok()?;
        let (arsenal, refused) = Arsenal::from_table(&table);
        if arsenal.is_empty() {
            log("mw2: the imported table holds no usable guns; interim gunplay");
            return None;
        }
        let by_base = config.get("gta_by_base").and_then(Value::as_object).unwrap_or(&[]);
        let by_class = config.get("gta_by_class").and_then(Value::as_object).unwrap_or(&[]);
        let find = |list: &[(String, Value)], key: &str| {
            list.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()).map(joaat)
        };
        let gta = arsenal
            .guns()
            .iter()
            .map(|g| {
                let base = g.name.split('_').next().unwrap_or("");
                find(by_base, base)
                    .or_else(|| find(by_class, &g.facts.weap_class.to_string()))
                    .unwrap_or_else(|| joaat("weapon_carbinerifle"))
            })
            .collect();
        let labels = arsenal.guns().iter().map(|g| label(&g.display_name, &g.name)).collect();
        let mut loadout: Vec<u32> = config
            .get("loadout")
            .and_then(Value::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|v| v.as_str().and_then(|n| arsenal.id(n)))
            .collect();
        if loadout.is_empty() {
            loadout = ["m4_mp", "usp_mp"].iter().filter_map(|n| arsenal.id(n)).collect();
        }
        if loadout.is_empty() {
            loadout.push(1);
        }
        let mut rng = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9, |d| d.subsec_nanos() ^ d.as_secs() as u32)
            | 1;
        // Slot 3 (key 3): always one of MW2's sniper rifles, picked at random.
        let snipers: Vec<u32> =
            MENU_GUNS[MENU_SNIPERS..MENU_PRIMARIES].iter().filter_map(|b| arsenal.id(&format!("{b}_mp"))).collect();
        if loadout.len() < 3 && !snipers.is_empty() {
            while loadout.len() < 2 {
                loadout.push(loadout[0]);
            }
            loadout.push(snipers[(xorshift(&mut rng) as usize) % snipers.len()]);
        }
        // Slot 4 (key 5): always one of MW2's launchers, picked at random.
        let launchers: Vec<u32> = MENU_LAUNCHERS.iter().filter_map(|b| arsenal.id(&format!("{b}_mp"))).collect();
        if loadout.len() == 3 && !launchers.is_empty() {
            loadout.push(launchers[(xorshift(&mut rng) as usize) % launchers.len()]);
        }
        let damage_scale = config.get("damage_scale").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.1, 10.0) as f32;
        let ammo_scale = config.get("ammo_scale").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.0, 20.0) as f32;
        let frags = config.get("frags").and_then(Value::as_f64).unwrap_or(2.0).clamp(0.0, 20.0) as i32;
        let menu: Vec<u32> = MENU_GUNS.iter().filter_map(|b| arsenal.id(&format!("{b}_mp"))).collect();
        let names: Vec<&str> = loadout.iter().filter_map(|&id| arsenal.gun(id)).map(|g| g.name.as_str()).collect();
        let summary = format!(
            "mw2: {} guns from {} ({} refused); loadout {}; damage x{damage_scale}; spare rounds x{ammo_scale}; {frags} {}; {} guns in the menu",
            arsenal.len(),
            table.source,
            refused.len(),
            names.join(" / "),
            if arsenal.offhand_id(FRAG).is_some() { "frags" } else { "frags (none imported)" },
            menu.len()
        );
        log(&summary);
        let rows = table.weapons.iter().enumerate().map(|(i, r)| (r.name.clone(), i)).collect();
        Some(Arms {
            arsenal,
            table,
            gunner: None,
            kept: None,
            loadout,
            gta,
            labels,
            damage_scale,
            ammo_scale,
            frags,
            menu,
            hand: Hand::Gun,
            rows,
            shown: 0,
            latest: GunFrame::default(),
            hits: 0,
            kills: 0,
            rng,
            sets: false,
            set_len: [0; SETS],
            selected: 0,
            set_last: [0; SETS],
            last: None,
            equipped: 0,
            reload_total: 0,
            hitmarker: 0.0,
            kill: false,
            headshot: false,
            summary,
        })
    }

    /// The gunner, created with the loadout the first time it is needed.
    pub fn gunner(&mut self) -> &mut Gunner {
        if self.gunner.is_none() {
            let mut g = Gunner::new(&self.arsenal, &self.loadout)
                .with_reserve_scale(&self.arsenal, self.ammo_scale)
                .with_frags(&self.arsenal, FRAG, self.frags);
            // Back after a pause, a menu or a cutscene: the rounds the player had.
            if let Some((ammo, frags)) = self.kept.take() {
                g.restore_ammo(&ammo, frags);
            }
            self.gunner = Some(g);
            self.equipped = 0;
            self.selected = 0;
            self.set_last = [0; SETS];
        }
        self.gunner.as_mut().unwrap()
    }

    /// Before pmove: a weapon request (0 none, 1 next, 2 + n slot n) and the
    /// held weapon's facts into the player's context.
    pub fn before_step(&mut self, player: &mut codv_engine::Mw2Player, request: u32) {
        self.gunner();
        let pick = match request {
            n if self.sets && (2..REQUEST_KNIFE).contains(&n) => self.set_pick((n - 2) as usize),
            1 if self.sets => {
                let len = self.loadout.len().max(1);
                Some((self.selected + 1) % len)
            }
            _ => None,
        };
        if let Some(i) = pick {
            self.selected = i;
            if let Some(s) = self.set_of(i) {
                self.set_last[s] = i - self.set_start(s);
            }
        }
        let (Some(g), arsenal) = (self.gunner.as_mut(), &self.arsenal) else { return };
        match request {
            0 => {}
            REQUEST_KNIFE => self.hand = Hand::Knife,
            // The frag in hand only while there is one (and not mid-throw).
            REQUEST_GRENADE if g.frags() > 0 => self.hand = Hand::Grenade,
            REQUEST_GRENADE => {}
            _ if pick.is_some() => {
                self.hand = Hand::Gun;
                g.select(self.selected)
            }
            1 => {
                self.hand = Hand::Gun;
                g.cycle()
            }
            // A number key with an empty group: nothing changes.
            _ if self.sets => {}
            n => {
                self.hand = Hand::Gun;
                g.select((n - 2) as usize)
            }
        }
        g.arm(player, arsenal);
    }

    /// The gun last asked for by name (weapon sets' tests).
    #[cfg(test)]
    pub fn selected_name(&self) -> String {
        self.loadout
            .get(self.selected)
            .and_then(|&id| self.arsenal.gun(id))
            .map_or_else(String::new, |g| g.name.clone())
    }

    /// Where group `s` starts in the loadout (weapon sets).
    fn set_start(&self, s: usize) -> usize {
        self.set_len[..s.min(SETS)].iter().sum()
    }

    /// The group of loadout index `i` (weapon sets).
    fn set_of(&self, i: usize) -> Option<usize> {
        let mut start = 0;
        for (s, &len) in self.set_len.iter().enumerate() {
            if i < start + len {
                return Some(s);
            }
            start += len;
        }
        None
    }

    /// A number key with weapon sets: its group's last gun, or the group's next one when the
    /// last pick was already in it (None: an empty group).
    fn set_pick(&self, s: usize) -> Option<usize> {
        let len = *self.set_len.get(s)?;
        if len == 0 {
            return None;
        }
        let start = self.set_start(s);
        let k = if self.set_of(self.selected) == Some(s) {
            (self.selected - start + 1) % len
        } else {
            self.set_last[s] % len
        };
        Some(start + k)
    }

    /// Weapon sets on or off: a new random loadout in the layout asked for (names, as randomize).
    pub fn set_sets(&mut self, on: bool) -> Option<Vec<String>> {
        if self.sets == on {
            return None;
        }
        self.sets = on;
        self.randomize()
    }

    /// The player's buttons as the weapon takes them: with the knife the
    /// trigger is MW2's melee, with the frag in hand it is the frag button
    /// (hold to cook, release to throw); neither aims down sights.
    pub fn map_buttons(&self, buttons: u32) -> u32 {
        let attack = buttons & BUTTON_ATTACK != 0;
        let rest = buttons & !(BUTTON_ATTACK | BUTTON_ADS);
        match self.hand {
            Hand::Gun => buttons,
            Hand::Knife => rest | if attack { BUTTON_MELEE } else { 0 },
            Hand::Grenade => rest | if attack { BUTTON_FRAG } else { 0 },
        }
    }

    /// After pmove: one tick of the weapon; kept for the next `encode`.
    pub fn after_step(
        &mut self,
        player: &mut codv_engine::Mw2Player,
        msec: u32,
        buttons: u32,
        moves: (i32, i32),
    ) -> [f32; 2] {
        self.gunner();
        let (Some(g), arsenal) = (self.gunner.as_mut(), &self.arsenal) else { return [0.0; 2] };
        g.set_moves(moves.0, moves.1);
        let frame = g.step(player, arsenal, msec, buttons);
        // The frag thrown (or gone off, or none left): the gun is back on the trigger.
        if self.hand == Hand::Grenade
            && frame.offhand_view.is_none()
            && (frame.thrown.is_some() || frame.cooked_off.is_some() || frame.frags == 0)
        {
            self.hand = Hand::Gun;
        }
        let kick = frame.kick;
        self.shown = frame.offhand_view.unwrap_or(frame.weapon);
        self.latest = frame.clone();
        self.last = Some(frame);
        kick
    }

    /// Loadout slot `slot` (0 primary, 1 secondary) becomes the menu's gun
    /// `index`; the player gets a fresh loadout (the new gun is raised).
    pub fn set_slot(&mut self, slot: usize, index: usize) -> bool {
        let Some(&id) = self.menu.get(index) else { return false };
        if self.sets {
            // Weapon sets: the gun leads its group (swapped in when the group already has it).
            if self.set_len.get(slot).is_none_or(|&l| l == 0) {
                return false;
            }
            let at = self.set_start(slot);
            if self.loadout[at] == id {
                return true;
            }
            if let Some(j) = self.loadout.iter().position(|&x| x == id) {
                self.loadout.swap(at, j);
            } else {
                self.loadout[at] = id;
            }
            self.fresh();
            return true;
        }
        while self.loadout.len() <= slot {
            self.loadout.push(id);
        }
        if self.loadout[slot] == id {
            return true;
        }
        self.loadout[slot] = id;
        self.fresh();
        true
    }

    /// A random primary (assault rifle, SMG or LMG, never a sniper), a
    /// random secondary (shotgun, machine pistol, handgun), a random sniper
    /// rifle for slot 3 and a random launcher for slot 4 from the player's MW2,
    /// not the same set as now; a fresh loadout. Returns the names.
    pub fn randomize(&mut self) -> Option<Vec<String>> {
        let pool = |range: std::ops::Range<usize>| -> Vec<u32> {
            MENU_GUNS[range].iter().filter_map(|b| self.arsenal.id(&format!("{b}_mp"))).collect()
        };
        let (primaries, secondaries, snipers) =
            (pool(0..MENU_SNIPERS), pool(MENU_PRIMARIES..MENU_GUNS.len()), pool(MENU_SNIPERS..MENU_PRIMARIES));
        let launchers: Vec<u32> = MENU_LAUNCHERS.iter().filter_map(|b| self.arsenal.id(&format!("{b}_mp"))).collect();
        if primaries.is_empty() || secondaries.is_empty() {
            return None;
        }
        let before = self.loadout.clone();
        if self.sets {
            // Weapon sets: up to SET_SIZE different guns per group, a new roll each time.
            let pools = [&primaries, &secondaries, &snipers, &launchers];
            let mut loadout = Vec::new();
            for _ in 0..5 {
                loadout.clear();
                for (s, pool) in pools.iter().enumerate() {
                    let group = pick_n(&mut self.rng, pool, SET_SIZE);
                    self.set_len[s] = group.len();
                    loadout.extend(group);
                }
                if loadout != before {
                    break;
                }
            }
            self.loadout = loadout;
            self.fresh();
            return Some(self.all_names());
        }
        self.set_len = [0; SETS];
        let roll = |rng: &mut u32| {
            let mut l = vec![pick(rng, &primaries), pick(rng, &secondaries)];
            if !snipers.is_empty() {
                l.push(pick(rng, &snipers));
                if !launchers.is_empty() {
                    l.push(pick(rng, &launchers));
                }
            }
            l
        };
        let mut loadout = roll(&mut self.rng);
        for _ in 0..4 {
            if loadout != before {
                break;
            }
            loadout = roll(&mut self.rng);
        }
        self.loadout = loadout;
        self.fresh();
        Some(self.loadout_names().to_vec())
    }

    /// Every gun of the loadout by name, the weapon sets' groups split by " | ".
    pub fn all_names(&self) -> Vec<String> {
        let name = |id: &u32| self.arsenal.gun(*id).map_or_else(String::new, |g| g.name.clone());
        if !self.sets {
            return self.loadout.iter().map(name).collect();
        }
        let mut out = Vec::new();
        for s in 0..SETS {
            let start = self.set_start(s);
            let group: Vec<String> = self.loadout[start..start + self.set_len[s]].iter().map(name).collect();
            out.push(group.join(", "));
        }
        out
    }

    /// The imported row of a gun or grenade.
    pub fn row(&self, id: u32) -> Option<&codv_mw2data::table::Row> {
        let name = &self.arsenal.any(id)?.name;
        self.rows.get(name).and_then(|&i| self.table.weapons.get(i))
    }

    /// The loadout's gun names: primary, secondary, sniper, launcher ("" none); with weapon
    /// sets, each group's first gun.
    pub fn loadout_names(&self) -> [String; 4] {
        let name = |i: usize| {
            if self.sets && self.set_len[i] == 0 {
                return String::new();
            }
            let i = if self.sets { self.set_start(i) } else { i };
            self.loadout.get(i).and_then(|&id| self.arsenal.gun(id)).map_or_else(String::new, |g| g.name.clone())
        };
        [name(0), name(1), name(2), name(3)]
    }

    /// The GTA weapons standing in for the loadout's guns (no repeats, 0s left out): the host
    /// gives them and keeps their models loaded, so a switch never waits for GTA's streaming.
    pub fn ready_gta_hashes(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for &id in &self.loadout {
            let h = self.gta_hash(id);
            if h != 0 && !out.contains(&h) {
                out.push(h);
            }
        }
        out
    }

    /// The loadout's guns and the frag: what the player can hold next (their
    /// view models and sounds are read ahead of the mode).
    pub fn ready_set(&self) -> Vec<&Gun> {
        let frag = self.arsenal.offhand_id(FRAG).and_then(|id| self.arsenal.any(id));
        self.loadout.iter().filter_map(|&id| self.arsenal.gun(id)).chain(frag).collect()
    }

    /// New spare-round scale or frag count (from the next fresh loadout on).
    pub fn set_supplies(&mut self, ammo_scale: Option<f32>, frags: Option<i32>) {
        if let Some(a) = ammo_scale {
            self.ammo_scale = a.clamp(0.0, 20.0);
        }
        if let Some(f) = frags {
            self.frags = f.clamp(0, 20);
        }
        // Applied at once: the carried guns get the new reserve, the frags their count.
        if let Some(g) = self.gunner.take() {
            let held = g.loadout().to_vec();
            if held == self.loadout {
                self.gunner = Some(g.with_reserve_scale(&self.arsenal, self.ammo_scale).with_frags(
                    &self.arsenal,
                    FRAG,
                    self.frags,
                ));
            }
        }
    }

    /// A care package's ammo (_airdrop.gsc ammoCrateThink: refillAmmo): every carried gun full
    /// again and the frags back to their count. False before the first tick.
    pub fn refill(&mut self) -> bool {
        // Between ticks (a new life, before the mode drives again): the next gunner starts full.
        if self.gunner.is_none() && self.kept.take().is_some() {
            return true;
        }
        let Some(mut g) = self.gunner.take() else { return false };
        g.refill(&self.arsenal);
        if g.frags() < self.frags {
            g = g.with_frags(&self.arsenal, FRAG, self.frags);
        }
        self.gunner = Some(g);
        true
    }

    /// MW2's Scavenger on a hostile kill: a magazine more for every carried gun, a frag more.
    /// False before the first tick.
    pub fn scavenge(&mut self) -> bool {
        let Some(g) = self.gunner.as_mut() else { return false };
        g.scavenge(&self.arsenal, self.frags);
        true
    }

    /// An imported weapon row by name (the killstreaks' guns: "ac130_105mm_mp").
    pub fn find_row(&self, name: &str) -> Option<&codv_mw2data::table::Row> {
        self.table.weapons.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }

    /// Back to a fresh loadout next time (the mode stopped, a reset).
    /// The mode stopped for a while (a pause, a menu, a cutscene, the mode off): the hand and
    /// the weapon state start afresh, the rounds carried are kept for the same guns.
    pub fn reset(&mut self) {
        if let Some(g) = self.gunner.as_ref()
            && g.loadout() == self.loadout.as_slice()
        {
            self.kept = Some(g.ammo_state());
        }
        self.fresh();
    }

    /// A new loadout (or a new life): full ammo.
    pub fn fresh(&mut self) {
        self.kept = None;
        self.hand = Hand::Gun;
        self.gunner = None;
        self.last = None;
        self.equipped = 0;
    }

    pub fn gta_hash(&self, id: u32) -> u32 {
        id.checked_sub(1).and_then(|i| self.gta.get(i as usize)).copied().unwrap_or(0)
    }

    /// The last tick as the gunplay output. `heal` and `damage_flash` come
    /// from the interim core, which still owns health.
    pub fn encode(&mut self, dt: f32, heal: f32, damage_flash: f32) -> CvOutput {
        let f = self.last.take().unwrap_or_default();
        self.hitmarker = (self.hitmarker - dt.max(0.0) * 3.0).max(0.0);
        let mut out = CvOutput { heal, ..CvOutput::default() };
        out.state = match f.weaponstate {
            6 | 7 => STATE_FIRING,
            8..=12 => STATE_RELOADING,
            1 | 2 => STATE_RAISING,
            3..=5 => STATE_DROPPING,
            13..=15 => STATE_MELEE,
            23..=25 => STATE_SPRINTING,
            _ => STATE_READY,
        };
        out.flags = CV_OUT_ACTIVE;
        if f.reload_started {
            out.flags |= CV_OUT_RELOAD;
            self.reload_total = f.weapon_time.max(1);
        }
        if f.melee_started {
            out.flags |= CV_OUT_MELEE;
        }
        let gta = self.gta_hash(f.weapon);
        if gta != 0 && gta != self.equipped {
            out.flags |= CV_OUT_EQUIP;
            out.equip_hash = gta;
            self.equipped = gta;
        }
        for (slot, s) in out.shots.iter_mut().zip(&f.shots) {
            *slot = CvShot { weapon: s.weapon, yaw: s.yaw, pitch: s.pitch, gta_hash: gta };
        }
        out.shot_count = f.shots.len().min(CV_MAX_SHOTS) as u32;
        out.ammo_used = u32::from(f.fired);
        out.hud.mag = f.clip.max(0) as u32;
        out.hud.reserve = f.stock.max(0) as u32;
        out.hud.weapon = -1;
        out.hud.spread = f.spread;
        out.hud.ads = f.ads;
        out.hud.sprint = if out.state == STATE_SPRINTING { 1.0 } else { 0.0 };
        out.hud.reload = if out.state == STATE_RELOADING {
            1.0 - f.weapon_time as f32 / self.reload_total.max(1) as f32
        } else {
            0.0
        };
        out.hud.hitmarker = self.hitmarker;
        out.hud.flags = (if self.kill { CV_HUD_KILL } else { 0 }) | (if self.headshot { CV_HUD_HEADSHOT } else { 0 });
        out.hud.damage_flash = damage_flash;
        match self.hand {
            Hand::Knife => copy_str(&mut out.hud.label, "KNIFE"),
            Hand::Grenade => copy_str(&mut out.hud.label, &format!("FRAG x{}", f.frags)),
            Hand::Gun => {
                if let Some(l) = f.weapon.checked_sub(1).and_then(|i| self.labels.get(i as usize)) {
                    copy_str(&mut out.hud.label, l);
                }
            }
        }
        out.mag_size = self.arsenal.gun(f.weapon).map_or(0, |g| g.facts.clip_size.max(0) as u32);
        if self.hitmarker <= 0.0 {
            self.kill = false;
            self.headshot = false;
        }
        out
    }

    /// MW2's damage for one hit: range falloff (distance in metres), the hit
    /// location's multiplier (the host's MW2 hit location, else head or upper
    /// torso), CODV's scale.
    pub fn damage(&mut self, h: &CvHit) -> f32 {
        let Some(g) = self.arsenal.gun(h.weapon) else { return 0.0 };
        let distance = if h.distance.is_finite() { h.distance.max(0.0) / METRES_PER_UNIT } else { 1e9 };
        let ped = h.flags & CV_HIT_PED != 0;
        let head = h.flags & CV_HIT_HEAD != 0;
        let loc = match h.hitloc {
            n @ 1..=19 => n as usize,
            _ if head => HITLOC_HEAD,
            _ => HITLOC_TORSO_UPPER,
        };
        let scale = if ped { g.location_scale(loc) } else { 1.0 };
        if ped {
            self.hitmarker = 1.0;
            self.hits = self.hits.wrapping_add(1);
            // MW2's helmet and head locations.
            self.headshot |= matches!(loc, 1 | 2);
        }
        g.damage_at(distance) as f32 * scale * self.damage_scale
    }

    pub fn killed(&mut self) {
        self.kill = true;
        self.hitmarker = 1.0;
        self.kills = self.kills.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joaat_matches_gta() {
        assert_eq!(joaat("weapon_unarmed"), 0xA271_9263);
        assert_eq!(joaat("WEAPON_CARBINERIFLE"), 2_210_333_304);
    }

    #[test]
    fn labels_read_well() {
        assert_eq!(label("WEAPON_M4_CARBINE", "m4_mp"), "M4 CARBINE");
        assert_eq!(label("", "usp_silencer_mp"), "USP SILENCER");
    }

    #[test]
    fn the_default_settings_parse() {
        let c = json::parse(DEFAULT_CONFIG).unwrap();
        assert!(c.get("loadout").and_then(Value::as_array).is_some_and(|l| l.len() == 2));
        let _ = CV_HIT_HEAD;
    }
}
