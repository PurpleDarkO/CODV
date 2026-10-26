use super::*;

fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2 * u32::from(channels)).to_le_bytes());
    b.extend_from_slice(&(2 * channels).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(&data);
    b
}

struct Mem(HashMap<String, Arc<Pcm>>);
impl Source for Mem {
    fn pcm(&mut self, file: &str) -> Option<Arc<Pcm>> {
        self.0.get(file).cloned()
    }
}

fn variant(file: &str) -> Variant {
    Variant {
        file: file.into(),
        vol: [1.0, 1.0],
        pitch: [1.0, 1.0],
        chance: 1.0,
        delay_ms: 0,
        secondary: String::new(),
        chain: String::new(),
        flags: 0,
    }
}

fn bank(rows: &[(&str, Variant)]) -> Aliases {
    let mut a = Aliases::default();
    for (n, v) in rows {
        a.map.entry((*n).to_owned()).or_default().push(v.clone());
    }
    a
}

fn constant(rate: u32, n: usize, value: i16) -> Arc<Pcm> {
    Arc::new(Pcm { rate, samples: vec![value; n] })
}

#[test]
fn wave_files_decode_to_mono() {
    let p = decode_wav(&wav(22050, 2, &[1000, 3000, -2000, -4000])).unwrap();
    assert_eq!(p.rate, 22050);
    assert_eq!(p.samples, vec![2000, -3000]);
    assert!(decode_wav(b"RIFFxxxxWAVE").is_err());
}

#[test]
fn aliases_parse_the_import_format() {
    let text = r#"{"format":"codv-mw2-sounds","version":1,"aliases":{"Weap_A":[
        {"file":"a.wav","vol":[0.5,0.6],"pitch":[0.9,1.1],"chance":0.3,"delay_ms":20,"secondary":"b","chain":"","flags":1}]}}"#;
    let a = Aliases::parse(text).unwrap();
    let v = &a.get("weap_a").unwrap()[0];
    assert_eq!(v.file, "a.wav");
    assert_eq!(v.vol, [0.5, 0.6]);
    assert_eq!((v.delay_ms, v.secondary.as_str(), v.flags), (20, "b", 1));
    assert!(Aliases::parse(r#"{"format":"other"}"#).is_err());
}

#[test]
fn variants_are_picked_by_probability() {
    let mut a = variant("a");
    a.chance = 0.9;
    let mut b = variant("b");
    b.chance = 0.1;
    let vs = [a, b];
    let mut rng = Rng::new(7);
    let picks_a = (0..10_000).filter(|_| pick(&vs, &mut rng).unwrap().file == "a").count();
    assert!((8700..9300).contains(&picks_a), "{picks_a}");
}

#[test]
fn the_mixer_resamples_to_48k_and_ends_voices() {
    // 24 kHz, 100 samples -> 200 output frames.
    let mut src = Mem(HashMap::from([("a.wav".to_owned(), constant(24_000, 100, 16384))]));
    let aliases = bank(&[("a", variant("a.wav"))]);
    let mut m = Mixer::new(1);
    assert_eq!(m.play(&aliases, &mut src, "a", 0), 1);
    let mut out = vec![0.0; 256];
    m.render(&mut out);
    assert!((out[0] - 0.5).abs() < 1e-3);
    assert!((out[198] - 0.5).abs() < 1e-3);
    assert_eq!(out[201], 0.0);
    assert_eq!(m.voices(), 0);
}

#[test]
fn delay_secondary_and_chain_follow_the_alias() {
    let mut src = Mem(HashMap::from([
        ("a.wav".to_owned(), constant(48_000, 480, 8192)),
        ("b.wav".to_owned(), constant(48_000, 480, 8192)),
        ("c.wav".to_owned(), constant(48_000, 480, 8192)),
    ]));
    let mut a = variant("a.wav");
    a.delay_ms = 10; // 480 frames
    a.chain = "c".into();
    a.secondary = "b".into();
    let aliases = bank(&[("a", a), ("b", variant("b.wav")), ("c", variant("c.wav"))]);
    let mut m = Mixer::new(1);
    assert_eq!(m.play(&aliases, &mut src, "a", 0), 3);
    let mut out = vec![0.0; 1600];
    m.render(&mut out);
    // b at once, a after its delay, c after a.
    assert!((out[10] - 0.25).abs() < 1e-3);
    assert!((out[500] - 0.25).abs() < 1e-3);
    assert!((out[1000] - 0.25).abs() < 1e-3);
    assert_eq!(out[1500], 0.0);
}

#[test]
fn the_oldest_voice_makes_room() {
    let mut src = Mem(HashMap::from([("a.wav".to_owned(), constant(48_000, 48_000, 1000))]));
    let aliases = bank(&[("a", variant("a.wav"))]);
    let mut m = Mixer::new(1);
    for _ in 0..MAX_VOICES + 5 {
        m.play(&aliases, &mut src, "a", 0);
    }
    assert_eq!(m.voices(), MAX_VOICES);
}

/// The player's own import (CODV_MW2_OUT: weapons.json + sounds/); skipped otherwise.
#[test]
fn the_m4_sounds_like_mw2() {
    let Ok(out) = std::env::var("CODV_MW2_OUT") else { return };
    let dir = PathBuf::from(out);
    let text = std::fs::read_to_string(dir.join("weapons.json")).unwrap();
    let table = codv_mw2data::table::from_json(&text).unwrap();
    let (arsenal, _) = codv_engine::arms::Arsenal::from_table(&table);
    let id = arsenal.id("m4_mp").unwrap();
    let gun = arsenal.gun(id).unwrap();
    let aliases = Aliases::parse(&std::fs::read_to_string(dir.join("sounds/aliases.json")).unwrap()).unwrap();
    let mut audio = Audio::new(aliases, Files::new(dir.join("sounds/files")));

    let fire = GunFrame { weapon: id, fired: true, clip: 29, ..Default::default() };
    assert_eq!(gun_sounds(gun, &fire, false), vec!["weap_m4carbine_fire_plr"]);
    let raise = GunFrame { weapon: id, raise_started: true, ..Default::default() };
    assert_eq!(gun_sounds(gun, &raise, true), vec!["weap_raise_plr"]);
    assert_eq!(note_sound(gun, "weap_m4carbine_clipout_plr").as_deref(), Some("weap_m4carbine_clipout_plr"));
    assert_eq!(note_sound(gun, "viewmodel_small"), None);

    audio.gun_frame(gun, &fire, &["weap_m4carbine_clipin_plr".to_owned()], &|l| panic!("{l}"));
    assert!(audio.mixer.voices() >= 2, "fire + clip in: {}", audio.mixer.voices());
    assert!(audio.files.loaded() >= 5, "the M4's sounds were preloaded: {}", audio.files.loaded());
    let mut buf = vec![0.0; 4800];
    audio.pull(&mut buf);
    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!("m4 fire: {} voices, peak {peak:.3}", audio.mixer.voices());
    assert!(peak > 0.05 && peak < 4.0, "{peak}");
    audio.paused = true;
    audio.pull(&mut buf);
    assert!(buf.iter().all(|s| *s == 0.0));
}
