//! MW2's killstreaks (F5c) from its string tables: `mp/killstreakTable.csv`
//! (code_post_gfx_mp.ff; maps/mp/killstreaks/_killstreaks.gsc reads it by
//! the streak's reference in column 1) joined with `mp/splashTable.csv`
//! (common_mp.ff; maps/mp/gametypes/_hud_message.gsc's killstreak splash).
//!
//! killstreakTable columns: 0 index, 1 reference, 2 name, 3 description,
//! 4 kills, 6 earn hint, 7 earn sound, 8 earn dialog, 9 / 10 allies / axis
//! use dialog, 12 weapon, 14 icon, 15 crate icon, 16 dpad icon.
//! splashTable columns: 0 reference, 1 title, 2 description, 3 icon,
//! 4 duration (s), 5..8 glow colour, 9 sound, 11 menu.
use crate::hudfile::Killstreak;

/// A string table: its column count and cells row by row.
pub type Table = (usize, Vec<String>);

/// The voice prefix of the announcer CODV uses: MW2's US Army Rangers
/// (factionTable "us_army" voice "US_", maps/mp/gametypes/_music_and_dialog.gsc adds "1mc_").
pub const VOICE: &str = "us_1mc_";

fn cell(t: &Table, row: usize, col: usize) -> &str {
    let (cols, cells) = t;
    if col >= *cols {
        return "";
    }
    cells.get(row * cols + col).map_or("", |s| s.trim())
}

fn rows(t: &Table) -> usize {
    t.1.len().checked_div(t.0).unwrap_or(0)
}

/// The killstreaks a player can pick (indexed rows with a kill count), in table order.
pub fn from_tables(killstreaks: &Table, splashes: &Table) -> Vec<Killstreak> {
    let mut out = Vec::new();
    for r in 0..rows(killstreaks) {
        // _killstreaks.gsc initKillstreakData walks the rows by index (column 0) from 1.
        if cell(killstreaks, r, 0).parse::<i32>().map_or(true, |i| i < 1) {
            continue;
        }
        let name = cell(killstreaks, r, 1).to_ascii_lowercase();
        let kills = cell(killstreaks, r, 4).parse::<i32>().unwrap_or(0);
        if name.is_empty() || !(1..1000).contains(&kills) {
            continue;
        }
        let splash = (0..rows(splashes)).find(|&s| cell(splashes, s, 0).eq_ignore_ascii_case(&name));
        let num = |s: Option<usize>, col: usize, fallback: f32| {
            s.and_then(|s| cell(splashes, s, col).parse::<f32>().ok()).unwrap_or(fallback)
        };
        let dialog = |col: usize, prefix: &str| {
            let d = cell(killstreaks, r, col).to_ascii_lowercase();
            if d.is_empty() || d == "null" { String::new() } else { format!("{VOICE}{prefix}{d}") }
        };
        out.push(Killstreak {
            kills,
            title: cell(killstreaks, r, 2).to_ascii_uppercase(),
            earned: cell(killstreaks, r, 6).to_ascii_uppercase(),
            splash_title: splash.map_or("MP_KILLSTREAK_N".into(), |s| cell(splashes, s, 1).to_ascii_uppercase()),
            splash_ms: (num(splash, 4, 2.5) * 1000.0) as i32,
            glow: [num(splash, 5, 0.25), num(splash, 6, 0.75), num(splash, 7, 0.25), num(splash, 8, 1.0)],
            sound: cell(killstreaks, r, 7).to_ascii_lowercase(),
            earn_dialog: dialog(8, ""),
            use_dialog: dialog(9, "use_"),
            icon: cell(killstreaks, r, 14).to_ascii_lowercase(),
            dpad_icon: cell(killstreaks, r, 16).to_ascii_lowercase(),
            name,
        });
    }
    out
}

/// The sound aliases the killstreaks play (jingles and announcer lines).
pub fn sound_aliases(list: &[Killstreak]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in list {
        for a in [&k.sound, &k.earn_dialog, &k.use_dialog] {
            if !a.is_empty() && !out.contains(a) {
                out.push(a.clone());
            }
        }
    }
    // _emp.gsc: every player hears "emp_activate" when the EMP goes off.
    if list.iter().any(|k| k.name == "emp") {
        out.push("emp_activate".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        let lines: Vec<Vec<String>> = text
            .lines()
            .map(|l| l.split(',').map(str::to_owned).collect())
            .filter(|r: &Vec<String>| r.len() > 1)
            .collect();
        let cols = lines.iter().map(Vec::len).max().unwrap_or(0);
        let mut cells = Vec::new();
        for mut r in lines {
            r.resize(cols, String::new());
            cells.extend(r);
        }
        (cols, cells)
    }

    #[test]
    fn rows_join_their_splash() {
        let ks = table(
            "a0,b1,c2,d3,e4,f5,g6,h7,i8,j9,k10,l11,m12,n13,o14,p15,q16\n\
             0,none,KILLSTREAKS_NONE,KILLSTREAKS_NONE_DESC,,,,,,,,,,,specialty_ks_null,specialty_uav_crate,specialty_null\n\
             1,uav,KILLSTREAKS_UAV,KILLSTREAKS_UAV_DESC,3,1,MP_EARNED_UAV,mp_killstreak_radar,achieve_uav,uav,uav,1,killstreak_uav_mp,100,specialty_uav,specialty_uav_crate,dpad_killstreak_uav\n\
             13,emp,KILLSTREAKS_EMP,KILLSTREAKS_EMP_DESC,15,10,MP_EARNED_EMP,mp_killstreak_emp,achieve_emp,emp,emp,1,killstreak_emp_mp,500,specialty_emp,specialty_emp_crate,dpad_killstreak_emp\n\
             airstrike,KILLSTREAKS_AIRSTRIKE,KILLSTREAKS_AIRSTRIKE_DESC,9999,9999,MP_EARNED_AIRSTRIKE,null,null,null,null,0,killstreak_airstrike_mp,300,compass_objpoint_airstrike,,,",
        );
        let splash =
            table("uav,MP_KILLSTREAK_N,MP_EARNED_UAV,,2.5,0.25,0.75,0.25,1,mp_killstreak_radar,uavrecon,killstreak");
        let list = from_tables(&ks, &splash);
        assert_eq!(list.iter().map(|k| (k.name.as_str(), k.kills)).collect::<Vec<_>>(), [("uav", 3), ("emp", 15)]);
        let uav = &list[0];
        assert_eq!(
            (uav.title.as_str(), uav.earned.as_str(), uav.splash_title.as_str()),
            ("KILLSTREAKS_UAV", "MP_EARNED_UAV", "MP_KILLSTREAK_N")
        );
        assert_eq!((uav.splash_ms, uav.glow), (2500, [0.25, 0.75, 0.25, 1.0]));
        assert_eq!((uav.earn_dialog.as_str(), uav.use_dialog.as_str()), ("us_1mc_achieve_uav", "us_1mc_use_uav"));
        assert_eq!((uav.icon.as_str(), uav.dpad_icon.as_str()), ("specialty_uav", "dpad_killstreak_uav"));
        // No splash row: MW2's killstreak splash defaults.
        assert_eq!(list[1].splash_ms, 2500);
        let sounds = sound_aliases(&list);
        assert!(sounds.contains(&"mp_killstreak_emp".to_owned()) && sounds.contains(&"emp_activate".to_owned()));
    }
}
