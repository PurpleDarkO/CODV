//! CODV's MW2 HUD file (`<GTA>/CODV/mw2/hud/hud.json`, F5b): what the import
//! keeps of MW2's HUD for the runtime to draw it. The fonts (glyph tables
//! and atlas image), the strings the HUD shows, the weapon bar's items
//! (MW2's `weaponbar_hd` menu: rectangles in its 640 x 480 virtual screen
//! with their alignment, colours, fonts and owner draws; the menu's own
//! origin already added) with their visibility tests reduced to
//! `variable op value`, and the HUD materials' images. Images are DDS files
//! under `<GTA>/CODV/mw2/viewmodels/images/`.
//!
//! F5c adds the killstreaks (`mp/killstreakTable.csv` with their
//! `mp/splashTable.csv` rows), the earn splash's items (MW2's `killstreak`
//! menu) and the reward slot's items (`dpad_hd`, action slot 4).
use codv_core::json::{self, Value};

pub const FORMAT: &str = "codv-mw2-hud";
pub const VERSION: f64 = 1.0;

/// One glyph: the letter, its offset and advance and size in font pixels,
/// and its atlas rectangle (s0, t0, s1, t1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glyph {
    pub letter: u16,
    pub x0: i8,
    pub y0: i8,
    pub dx: u8,
    pub width: u8,
    pub height: u8,
    pub st: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Font {
    pub name: String,
    pub pixel_height: i32,
    pub image: String,
    pub glyphs: Vec<Glyph>,
}

impl Font {
    pub fn glyph(&self, letter: u32) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| u32::from(g.letter) == letter)
    }
}

/// A visibility test of a menu item: `variable op value` (op: "<", "<=",
/// ">", ">=", "==", "!=", or "" for "variable is not zero").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Test {
    pub variable: String,
    pub op: String,
    pub value: f32,
}

impl Test {
    pub fn passes(&self, v: f32) -> bool {
        match self.op.as_str() {
            "<" => v < self.value,
            "<=" => v <= self.value,
            ">" => v > self.value,
            ">=" => v >= self.value,
            "==" => v == self.value,
            "!=" => v != self.value,
            _ => v != 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub name: String,
    pub owner_draw: i32,
    /// x, y, w, h in the 640 x 480 virtual screen.
    pub rect: [f32; 4],
    /// Horizontal and vertical alignment (hud_iw4 ScreenPlacement ALIGN_*).
    pub align: [i32; 2],
    pub color: [f32; 4],
    pub text_scale: f32,
    pub font: i32,
    pub text_align: i32,
    pub text_align_xy: [f32; 2],
    pub text_style: i32,
    pub style: i32,
    /// The background material's image ("" none).
    pub background: String,
    pub visible: Option<Test>,
    /// The item's own text (a localize reference "@KEY", or plain).
    pub text: String,
}

/// One killstreak reward (a `mp/killstreakTable.csv` row and its splash).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Killstreak {
    /// The streak's reference ("uav", "emp", ...).
    pub name: String,
    /// Kills in a row that earn it.
    pub kills: i32,
    /// Localize keys: its name ("KILLSTREAKS_UAV") and the earn hint ("MP_EARNED_UAV").
    pub title: String,
    pub earned: String,
    /// The splash: its title (MP_KILLSTREAK_N, "&&1" the streak), how long it shows and the
    /// title's glow (splashTable columns 1, 4 and 5..8).
    pub splash_title: String,
    pub splash_ms: i32,
    pub glow: [f32; 4],
    /// Sound aliases: the earn jingle, the announcer's "earned" and "inbound" lines.
    pub sound: String,
    pub earn_dialog: String,
    pub use_dialog: String,
    /// HUD materials: the splash icon and the reward slot icon.
    pub icon: String,
    pub dpad_icon: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hud {
    pub fonts: Vec<Font>,
    /// Localized strings by key (upper case).
    pub strings: Vec<(String, String)>,
    pub items: Vec<Item>,
    /// HUD material -> image.
    pub images: Vec<(String, String)>,
    /// F5c: the killstreaks, the earn splash's items and the reward slot's items.
    pub killstreaks: Vec<Killstreak>,
    pub splash_items: Vec<Item>,
    pub dpad_items: Vec<Item>,
    /// Flipbook materials: (rows, columns) of their image (the animated dpad icons).
    pub atlas: Vec<(String, [u8; 2])>,
    /// F5c: MW2's killstreak overlay menus by name (missilecam_hud_hd, ac130_hud_hd,
    /// remote_chopper_overlay_hd, targetmap_fullscreen), their items with the menu's origin added.
    pub menus: Vec<(String, Vec<Item>)>,
}

impl Hud {
    pub fn font(&self, name: &str) -> Option<&Font> {
        self.fonts.iter().find(|f| f.name.eq_ignore_ascii_case(name))
    }
    pub fn string(&self, key: &str) -> Option<&str> {
        self.strings.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
    pub fn image(&self, material: &str) -> Option<&str> {
        self.images.iter().find(|(k, _)| k.eq_ignore_ascii_case(material)).map(|(_, v)| v.as_str())
    }
    pub fn menu(&self, name: &str) -> &[Item] {
        self.menus.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map_or(&[], |(_, v)| v.as_slice())
    }
    pub fn atlas(&self, material: &str) -> Option<[u8; 2]> {
        self.atlas.iter().find(|(k, _)| k.eq_ignore_ascii_case(material)).map(|(_, v)| *v)
    }
    pub fn killstreak(&self, name: &str) -> Option<&Killstreak> {
        self.killstreaks.iter().find(|k| k.name.eq_ignore_ascii_case(name))
    }
}

/// Reduces a menu expression dump (fastfile_iw4's) to a test, when it is
/// `( ( dvarInt/localVar "name" ) ) op value` or just the variable.
/// Operators by IW4 operation code: 8 <, 9 <=, 10 >, 11 >=, 12 ==, 13 !=.
pub fn parse_test(dump: &str) -> Option<Test> {
    let tokens: Vec<&str> = dump.split_whitespace().collect();
    let name_at = tokens.iter().position(|t| t.starts_with("s:"))?;
    let hex = &tokens[name_at][2..];
    let bytes: Option<Vec<u8>> =
        (0..hex.len() / 2).map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
    let variable = String::from_utf8(bytes?).ok()?;
    // After the closing brace of the variable's group: "op N value".
    let close = tokens.iter().skip(name_at).position(|t| *t == "}").map(|i| i + name_at)?;
    let rest = &tokens[close + 1..];
    let (op, value) = match rest {
        [] => (String::new(), 0.0),
        ["op", code, value, ..] => {
            let op = match *code {
                "8" => "<",
                "9" => "<=",
                "10" => ">",
                "11" => ">=",
                "12" => "==",
                "13" => "!=",
                _ => return None,
            };
            (op.to_owned(), value.parse::<f32>().ok()?)
        }
        _ => return None,
    };
    Some(Test { variable, op, value })
}

fn nums<const N: usize>(v: Option<&Value>) -> [f32; N] {
    let mut out = [0.0; N];
    if let Some(a) = v.and_then(Value::as_array) {
        for (o, x) in out.iter_mut().zip(a) {
            *o = x.as_f64().unwrap_or(0.0) as f32;
        }
    }
    out
}

fn arr(values: &[f32]) -> Value {
    Value::Arr(values.iter().map(|&x| Value::Num(f64::from(x))).collect())
}

pub fn to_json(h: &Hud) -> String {
    let fonts = h
        .fonts
        .iter()
        .map(|f| {
            let glyphs = f
                .glyphs
                .iter()
                .map(|g| {
                    arr(&[
                        f32::from(g.letter),
                        f32::from(g.x0),
                        f32::from(g.y0),
                        f32::from(g.dx),
                        f32::from(g.width),
                        f32::from(g.height),
                        g.st[0],
                        g.st[1],
                        g.st[2],
                        g.st[3],
                    ])
                })
                .collect();
            json::obj([
                ("name", f.name.as_str().into()),
                ("pixel_height", Value::Num(f64::from(f.pixel_height))),
                ("image", f.image.as_str().into()),
                ("glyphs", Value::Arr(glyphs)),
            ])
        })
        .collect();
    let item_list = |list: &[Item]| -> Value { Value::Arr(list.iter().map(item_json).collect()) };
    let killstreaks = h
        .killstreaks
        .iter()
        .map(|k| {
            json::obj([
                ("name", k.name.as_str().into()),
                ("kills", Value::Num(f64::from(k.kills))),
                ("title", k.title.as_str().into()),
                ("earned", k.earned.as_str().into()),
                ("splash_title", k.splash_title.as_str().into()),
                ("splash_ms", Value::Num(f64::from(k.splash_ms))),
                ("glow", arr(&k.glow)),
                ("sound", k.sound.as_str().into()),
                ("earn_dialog", k.earn_dialog.as_str().into()),
                ("use_dialog", k.use_dialog.as_str().into()),
                ("icon", k.icon.as_str().into()),
                ("dpad_icon", k.dpad_icon.as_str().into()),
            ])
        })
        .collect();
    let pairs =
        |p: &[(String, String)]| Value::Obj(p.iter().map(|(k, v)| (k.clone(), Value::Str(v.clone()))).collect());
    json::to_string_pretty(&json::obj([
        ("format", FORMAT.into()),
        ("version", Value::Num(VERSION)),
        ("fonts", Value::Arr(fonts)),
        ("strings", pairs(&h.strings)),
        ("items", item_list(&h.items)),
        ("images", pairs(&h.images)),
        ("killstreaks", Value::Arr(killstreaks)),
        ("splash_items", item_list(&h.splash_items)),
        ("dpad_items", item_list(&h.dpad_items)),
        ("menus", Value::Obj(h.menus.iter().map(|(n, items)| (n.clone(), item_list(items))).collect())),
        (
            "atlas",
            Value::Obj(h.atlas.iter().map(|(k, v)| (k.clone(), arr(&[f32::from(v[0]), f32::from(v[1])]))).collect()),
        ),
    ]))
}

fn item_json(i: &Item) -> Value {
    let visible = match &i.visible {
        Some(t) => json::obj([
            ("variable", t.variable.as_str().into()),
            ("op", t.op.as_str().into()),
            ("value", Value::Num(f64::from(t.value))),
        ]),
        None => Value::Null,
    };
    json::obj([
        ("name", i.name.as_str().into()),
        ("owner_draw", Value::Num(f64::from(i.owner_draw))),
        ("rect", arr(&i.rect)),
        ("align", arr(&[i.align[0] as f32, i.align[1] as f32])),
        ("color", arr(&i.color)),
        ("text_scale", Value::Num(f64::from(i.text_scale))),
        ("font", Value::Num(f64::from(i.font))),
        ("text_align", Value::Num(f64::from(i.text_align))),
        ("text_align_xy", arr(&i.text_align_xy)),
        ("text_style", Value::Num(f64::from(i.text_style))),
        ("style", Value::Num(f64::from(i.style))),
        ("background", i.background.as_str().into()),
        ("text", i.text.as_str().into()),
        ("visible", visible),
    ])
}

pub fn from_json(text: &str) -> Result<Hud, String> {
    let v = json::parse(text)?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!("not a {FORMAT} file"));
    }
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let mut h = Hud::default();
    for f in v.get("fonts").and_then(Value::as_array).unwrap_or(&[]) {
        let glyphs = f
            .get("glyphs")
            .and_then(Value::as_array)
            .unwrap_or(&[])
            .iter()
            .map(|g| {
                let x: [f32; 10] = nums(Some(g));
                Glyph {
                    letter: x[0] as u16,
                    x0: x[1] as i8,
                    y0: x[2] as i8,
                    dx: x[3] as u8,
                    width: x[4] as u8,
                    height: x[5] as u8,
                    st: [x[6], x[7], x[8], x[9]],
                }
            })
            .collect();
        h.fonts.push(Font {
            name: s(f, "name"),
            pixel_height: n(f, "pixel_height") as i32,
            image: s(f, "image"),
            glyphs,
        });
    }
    let pairs = |key: &str| -> Vec<(String, String)> {
        v.get(key)
            .and_then(Value::as_object)
            .unwrap_or(&[])
            .iter()
            .map(|(k, x)| (k.clone(), x.as_str().unwrap_or("").to_owned()))
            .collect()
    };
    h.strings = pairs("strings");
    h.images = pairs("images");
    let items = |key: &str| -> Vec<Item> {
        v.get(key).and_then(Value::as_array).unwrap_or(&[]).iter().map(item_from_json).collect()
    };
    h.items = items("items");
    h.splash_items = items("splash_items");
    h.dpad_items = items("dpad_items");
    for (k, x) in v.get("menus").and_then(Value::as_object).unwrap_or(&[]) {
        h.menus.push((k.clone(), x.as_array().unwrap_or(&[]).iter().map(item_from_json).collect()));
    }
    for (k, x) in v.get("atlas").and_then(Value::as_object).unwrap_or(&[]) {
        let a: [f32; 2] = nums(Some(x));
        h.atlas.push((k.clone(), [a[0] as u8, a[1] as u8]));
    }
    for k in v.get("killstreaks").and_then(Value::as_array).unwrap_or(&[]) {
        h.killstreaks.push(Killstreak {
            name: s(k, "name"),
            kills: n(k, "kills") as i32,
            title: s(k, "title"),
            earned: s(k, "earned"),
            splash_title: s(k, "splash_title"),
            splash_ms: n(k, "splash_ms") as i32,
            glow: nums(k.get("glow")),
            sound: s(k, "sound"),
            earn_dialog: s(k, "earn_dialog"),
            use_dialog: s(k, "use_dialog"),
            icon: s(k, "icon"),
            dpad_icon: s(k, "dpad_icon"),
        });
    }
    Ok(h)
}

fn item_from_json(i: &Value) -> Item {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let align: [f32; 2] = nums(i.get("align"));
    let visible = i.get("visible").filter(|t| t.as_object().is_some()).map(|t| Test {
        variable: s(t, "variable"),
        op: s(t, "op"),
        value: n(t, "value") as f32,
    });
    Item {
        name: s(i, "name"),
        owner_draw: n(i, "owner_draw") as i32,
        rect: nums(i.get("rect")),
        align: [align[0] as i32, align[1] as i32],
        color: nums(i.get("color")),
        text_scale: n(i, "text_scale") as f32,
        font: n(i, "font") as i32,
        text_align: n(i, "text_align") as i32,
        text_align_xy: nums(i.get("text_align_xy")),
        text_style: n(i, "text_style") as i32,
        style: n(i, "style") as i32,
        background: s(i, "background"),
        text: s(i, "text"),
        visible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_tests_reduce_to_variable_op_value() {
        let t = parse_test("op 16 { op 16 op 49 s:73746f636b416d6d6f op 1 } op 8 100").unwrap();
        assert_eq!((t.variable.as_str(), t.op.as_str(), t.value), ("stockAmmo", "<", 100.0));
        assert!(t.passes(99.0) && !t.passes(100.0));
        let t = parse_test("op 16 { op 16 op 49 s:66726167416d6d6f op 1 }").unwrap();
        assert_eq!((t.variable.as_str(), t.op.as_str()), ("fragAmmo", ""));
        assert!(t.passes(1.0) && !t.passes(0.0));
        assert!(parse_test("op 16 op 47 op 1 op 15 op 90 op 1").is_none());
    }

    #[test]
    fn the_file_round_trips() {
        let h = Hud {
            fonts: vec![Font {
                name: "fonts/hudbigfont".into(),
                pixel_height: 42,
                image: "gamefonts_pc".into(),
                glyphs: vec![Glyph {
                    letter: 65,
                    x0: -1,
                    y0: -30,
                    dx: 20,
                    width: 22,
                    height: 31,
                    st: [0.1, 0.2, 0.3, 0.4],
                }],
            }],
            strings: vec![("PLATFORM_RELOAD".into(), "Reload".into())],
            items: vec![Item {
                owner_draw: 119,
                rect: [-136.0, -52.0, 37.0, 0.5],
                align: [10, 10],
                color: [1.0; 4],
                visible: Some(Test { variable: "stockAmmo".into(), op: "<".into(), value: 100.0 }),
                ..Default::default()
            }],
            images: vec![("hud_weaponbar".into(), "hud_weaponbar".into())],
            killstreaks: vec![Killstreak {
                name: "uav".into(),
                kills: 3,
                title: "KILLSTREAKS_UAV".into(),
                earned: "MP_EARNED_UAV".into(),
                splash_title: "MP_KILLSTREAK_N".into(),
                splash_ms: 2500,
                glow: [0.25, 0.75, 0.25, 1.0],
                sound: "mp_killstreak_radar".into(),
                earn_dialog: "us_1mc_achieve_uav".into(),
                use_dialog: "us_1mc_use_uav".into(),
                icon: "specialty_uav".into(),
                dpad_icon: "dpad_killstreak_uav".into(),
            }],
            splash_items: vec![Item { name: "killstreak_title".into(), font: 9, ..Default::default() }],
            dpad_items: vec![Item { owner_draw: 174, ..Default::default() }],
            atlas: vec![("dpad_killstreak_uav".into(), [4, 8])],
            menus: vec![(
                "ac130_hud_hd".into(),
                vec![Item {
                    text: "@AC130_HUD_WEAPON_105MM".into(),
                    background: "ac130_overlay_105mm".into(),
                    ..Default::default()
                }],
            )],
        };
        let back = from_json(&to_json(&h)).unwrap();
        assert_eq!(back, h);
        assert_eq!(back.font("FONTS/HUDBIGFONT").unwrap().glyph(65).unwrap().dx, 20);
        assert_eq!(back.string("platform_reload"), Some("Reload"));
        assert_eq!(back.killstreak("UAV").unwrap().kills, 3);
        assert_eq!(back.atlas("DPAD_KILLSTREAK_UAV"), Some([4, 8]));
        assert_eq!(back.menu("AC130_HUD_HD")[0].text, "@AC130_HUD_WEAPON_105MM");
        assert!(back.menu("nonesuch").is_empty());
    }

    #[test]
    fn files_from_before_the_killstreaks_still_read() {
        let back = from_json(r#"{"format":"codv-mw2-hud","version":1,"fonts":[],"strings":{},"items":[],"images":{}}"#)
            .unwrap();
        assert!(back.killstreaks.is_empty() && back.splash_items.is_empty() && back.dpad_items.is_empty());
    }
}
