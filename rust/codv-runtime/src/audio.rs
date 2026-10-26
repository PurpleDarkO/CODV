//! MW2's weapon sounds behind the ABI (F5a). The setup imported the player's
//! MW2 sound aliases and loaded sounds (`<data>/mw2/sounds/aliases.json` and
//! `files/*.wav`, codv-mw2data `import_sounds`); this module turns the held
//! weapon's events into aliases the way IW4's client does (`fire_sound_player`
//! on a shot, `fire_last_sound_player` on the clip's last round, the raise,
//! put-away and melee sounds, and the view model's notetracks through the
//! weapon's `notetrack_sounds` map), picks a variant per IW4's alias rules
//! (weighted by `probability`, random volume and pitch in their ranges, start
//! delay, secondary alias played with it, chain alias played after it), and
//! mixes the voices DRY into one mono 48 kHz stream.
//!
//! The host plays the stream inside GTA's audio engine (host/src/gta_audio,
//! ported from SK8V), which owns volume categories, reverb and pause.
//!
//! Threads: the game thread queues sounds (`Audio::gun_frame`, `Audio::play`)
//! and the host's feeder thread renders on demand (`Audio::pull`); both go
//! through the runtime's audio lock, held briefly.
use codv_core::json::{self, Value};
use codv_engine::arms::{Gun, GunFrame};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

pub const SAMPLE_RATE: u32 = 48_000;
/// Voices alive at once; the oldest stops first (IW4 MP keeps 32 channels).
pub const MAX_VOICES: usize = 32;
/// Secondary and chain aliases followed this deep (IW4 data never nests more).
const MAX_LINKS: u32 = 4;
/// IW4 SndAliasFlags (asset_iw4::snd_alias).
pub const FLAG_LOOPING: u32 = 0x1;

/// One variant of an alias as the import wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub file: String,
    pub vol: [f32; 2],
    pub pitch: [f32; 2],
    pub chance: f32,
    pub delay_ms: i32,
    pub secondary: String,
    pub chain: String,
    pub flags: u32,
}

/// `aliases.json` (format "codv-mw2-sounds" v1).
#[derive(Default)]
pub struct Aliases {
    map: HashMap<String, Vec<Variant>>,
}

impl Aliases {
    pub fn parse(text: &str) -> Result<Aliases, String> {
        let v = json::parse(text)?;
        if v.get("format").and_then(Value::as_str) != Some("codv-mw2-sounds") {
            return Err("not a codv-mw2-sounds file".into());
        }
        let aliases = v.get("aliases").and_then(Value::as_object).ok_or("no aliases")?;
        let pair = |v: &Value, key: &str, d: f32| -> [f32; 2] {
            let a = v.get(key).and_then(Value::as_array);
            let at = |i: usize| a.and_then(|a| a.get(i)).and_then(Value::as_f64).map_or(d, |x| x as f32);
            [at(0), at(1)]
        };
        let mut map = HashMap::new();
        for (name, rows) in aliases {
            let variants = rows
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(|r| Variant {
                    file: r.get("file").and_then(Value::as_str).unwrap_or("").to_owned(),
                    vol: pair(r, "vol", 1.0),
                    pitch: pair(r, "pitch", 1.0),
                    chance: r.get("chance").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                    delay_ms: r.get("delay_ms").and_then(Value::as_f64).unwrap_or(0.0) as i32,
                    secondary: r.get("secondary").and_then(Value::as_str).unwrap_or("").to_owned(),
                    chain: r.get("chain").and_then(Value::as_str).unwrap_or("").to_owned(),
                    flags: r.get("flags").and_then(Value::as_f64).unwrap_or(0.0) as u32,
                })
                .collect::<Vec<_>>();
            if !variants.is_empty() {
                map.insert(name.to_ascii_lowercase(), variants);
            }
        }
        Ok(Aliases { map })
    }

    pub fn get(&self, name: &str) -> Option<&[Variant]> {
        self.map.get(&name.to_ascii_lowercase()).map(Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// A loaded sound, mono.
#[derive(Debug)]
pub struct Pcm {
    pub rate: u32,
    pub samples: Vec<i16>,
}

/// A RIFF WAVE file (8 or 16-bit PCM, mono or stereo) as mono PCM.
pub fn decode_wav(b: &[u8]) -> Result<Pcm, String> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not a WAVE file".into());
    }
    let (mut fmt, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= b.len() {
        let id = &b[at..at + 4];
        let len = u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &b[at + 8..(at + 8 + len).min(b.len())];
        match id {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + len + (len & 1);
    }
    let (fmt, data) = (fmt.ok_or("no fmt chunk")?, data.ok_or("no data chunk")?);
    if fmt.len() < 16 {
        return Err("short fmt chunk".into());
    }
    let u16_at = |o: usize| u16::from_le_bytes([fmt[o], fmt[o + 1]]);
    let (format, channels, bits) = (u16_at(0), u16_at(2) as usize, u16_at(14));
    let rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
    if format != 1 || !(1..=2).contains(&channels) || !(bits == 8 || bits == 16) || rate == 0 {
        return Err(format!("unsupported WAVE format {format}, {channels} ch, {bits} bits, {rate} Hz"));
    }
    let sample = |i: usize| -> i32 {
        if bits == 16 {
            i32::from(i16::from_le_bytes([data[2 * i], data[2 * i + 1]]))
        } else {
            (i32::from(data[i]) - 128) << 8
        }
    };
    let frames = data.len() / (channels * usize::from(bits / 8));
    let samples = (0..frames)
        .map(|f| {
            let sum: i32 = (0..channels).map(|c| sample(f * channels + c)).sum();
            (sum / channels as i32) as i16
        })
        .collect();
    Ok(Pcm { rate, samples })
}

/// One playing sound.
struct Voice {
    pcm: Arc<Pcm>,
    /// Position in source samples, and source samples per output frame.
    pos: f64,
    step: f64,
    gain: f32,
    /// Output frames still to wait before it starts.
    delay: u64,
    looping: bool,
}

/// The variant's PCM, by file name (loads it on first use).
pub trait Source {
    fn pcm(&mut self, file: &str) -> Option<Arc<Pcm>>;
}

/// xorshift32, as codv-engine's arms.
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }
    /// [0, 1)
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = if x == 0 { 1 } else { x };
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, r: [f32; 2]) -> f32 {
        let (lo, hi) = if r[0] <= r[1] { (r[0], r[1]) } else { (r[1], r[0]) };
        lo + (hi - lo) * self.next()
    }
}

/// IW4's variant choice: weighted by `probability` (0 counts as 1, as iw4L's
/// sound catalog).
pub fn pick<'a>(variants: &'a [Variant], rng: &mut Rng) -> Option<&'a Variant> {
    let weight = |v: &Variant| if v.chance > 0.0 { v.chance } else { 1.0 };
    let total: f32 = variants.iter().map(weight).sum();
    if variants.is_empty() || total <= 0.0 {
        return None;
    }
    let mut r = rng.next() * total;
    for v in variants {
        r -= weight(v);
        if r < 0.0 {
            return Some(v);
        }
    }
    variants.last()
}

/// The voices and their mono 48 kHz mix.
pub struct Mixer {
    voices: Vec<Voice>,
    rng: Rng,
    pub gain: f32,
    /// Frames rendered.
    pub frames: u64,
}

impl Mixer {
    pub fn new(seed: u32) -> Mixer {
        Mixer { voices: Vec::new(), rng: Rng::new(seed), gain: 1.0, frames: 0 }
    }

    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// Starts `name` (`delay` output frames from now). Returns the voices
    /// started (the alias, its secondary and its chain).
    pub fn play(&mut self, aliases: &Aliases, src: &mut dyn Source, name: &str, delay: u64) -> usize {
        self.play_linked(aliases, src, name, delay, 0, [1.0, 1.0])
    }

    /// As `play`, the alias's volume and pitch scaled by a random amount within `gain` and
    /// `pitch` (CODV's movement sounds: no two steps alike).
    pub fn play_varied(
        &mut self,
        aliases: &Aliases,
        src: &mut dyn Source,
        name: &str,
        gain: [f32; 2],
        pitch: [f32; 2],
    ) -> usize {
        let scale = [self.rng.range(gain), self.rng.range(pitch)];
        self.play_linked(aliases, src, name, 0, 0, scale)
    }

    fn play_linked(
        &mut self,
        aliases: &Aliases,
        src: &mut dyn Source,
        name: &str,
        delay: u64,
        depth: u32,
        scale: [f32; 2],
    ) -> usize {
        if name.is_empty() || depth > MAX_LINKS {
            return 0;
        }
        let Some(v) = aliases.get(name).and_then(|vs| pick(vs, &mut self.rng)).cloned() else { return 0 };
        let mut started = 0;
        let start = delay + u64::from(v.delay_ms.max(0) as u32) * u64::from(SAMPLE_RATE) / 1000;
        if let Some(pcm) = src.pcm(&v.file).filter(|p| !p.samples.is_empty()) {
            let pitch = (self.rng.range(v.pitch) * scale[1]).clamp(0.1, 4.0);
            let step = f64::from(pcm.rate) / f64::from(SAMPLE_RATE) * f64::from(pitch);
            let length = (pcm.samples.len() as f64 / step).ceil() as u64;
            let gain = (self.rng.range(v.vol) * scale[0]).clamp(0.0, 4.0);
            if self.voices.len() >= MAX_VOICES {
                self.voices.remove(0);
            }
            let looping = v.flags & FLAG_LOOPING != 0 && v.chain.is_empty();
            self.voices.push(Voice { pcm, pos: 0.0, step, gain, delay: start, looping });
            started += 1;
            if !looping {
                started += self.play_linked(aliases, src, &v.chain, start + length, depth + 1, scale);
            }
        }
        started + self.play_linked(aliases, src, &v.secondary, delay, depth + 1, scale)
    }

    /// Mixes the next `out.len()` frames into `out` (overwrites it).
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let scale = self.gain / 32768.0;
        for v in &mut self.voices {
            let n = v.pcm.samples.len();
            let skip = (v.delay.min(out.len() as u64)) as usize;
            v.delay -= skip as u64;
            for o in out.iter_mut().skip(skip) {
                if v.looping && v.pos >= n as f64 {
                    v.pos %= n as f64;
                }
                let i = v.pos as usize;
                if i >= n {
                    break;
                }
                let frac = (v.pos - i as f64) as f32;
                let a = f32::from(v.pcm.samples[i]);
                let b = f32::from(v.pcm.samples[(i + 1).min(n - 1)]);
                *o += (a + (b - a) * frac) * v.gain * scale;
                v.pos += v.step;
            }
        }
        self.voices.retain(|v| v.looping || v.delay > 0 || (v.pos as usize) < v.pcm.samples.len());
        self.frames += out.len() as u64;
    }
}

/// The aliases a weapon frame starts (IW4 client weapon events).
pub fn gun_sounds(gun: &Gun, f: &GunFrame, first_raise: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |key: &str| {
        let s = gun.string(key);
        if !s.is_empty() {
            out.push(s.to_ascii_lowercase());
        }
    };
    if f.putaway_started {
        push("putaway_sound_player");
    }
    if f.raise_started {
        if first_raise && !gun.string("first_raise_sound_player").is_empty() {
            push("first_raise_sound_player");
        } else {
            push("raise_sound_player");
        }
    }
    if f.fired {
        if f.clip == 0 && !gun.string("fire_last_sound_player").is_empty() {
            push("fire_last_sound_player");
        } else {
            push("fire_sound_player");
        }
    }
    if f.empty_click {
        push("empty_fire_sound_player");
    }
    if f.reload_started {
        if f.clip == 0 && !gun.string("reload_empty_sound_player").is_empty() {
            push("reload_empty_sound_player");
        } else {
            push("reload_sound_player");
        }
    }
    if f.melee_started {
        push("melee_swipe_sound_player");
    }
    out
}

/// The alias a view model notetrack plays: the weapon's `notetrack_sounds`
/// map (`note=alias,...`), as IW4's notetrackSoundMapKeys / Values.
pub fn note_sound(gun: &Gun, note: &str) -> Option<String> {
    gun.string("notetrack_sounds").split(',').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k.eq_ignore_ascii_case(note) && !v.is_empty()).then(|| v.to_ascii_lowercase())
    })
}

/// WAV files under `<sounds>/files/`, decoded on first use (or ahead of it
/// on a background thread: `Audio::prewarm`) and kept.
pub struct Files {
    dir: PathBuf,
    cache: Arc<Mutex<HashMap<String, Option<Arc<Pcm>>>>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl Files {
    pub fn new(dir: PathBuf) -> Files {
        Files { dir, cache: Arc::default(), errors: Arc::default() }
    }
    pub fn loaded(&self) -> usize {
        self.cache.lock().map_or(0, |c| c.values().filter(|p| p.is_some()).count())
    }
    /// The files that could not be read (the first few).
    pub fn errors(&self) -> Vec<String> {
        self.errors.lock().map(|e| e.clone()).unwrap_or_default()
    }
    /// Another handle on the same files and cache (for the prewarm thread).
    fn share(&self) -> Files {
        Files { dir: self.dir.clone(), cache: Arc::clone(&self.cache), errors: Arc::clone(&self.errors) }
    }
}

impl Source for Files {
    fn pcm(&mut self, file: &str) -> Option<Arc<Pcm>> {
        // A variant the import could not write (a streamed sound) has no file.
        if file.is_empty() {
            return None;
        }
        // The lock is held only to look up and insert, never while reading or decoding.
        if let Some(p) = self.cache.lock().ok()?.get(file) {
            return p.clone();
        }
        let path = self.dir.join(file);
        let p = match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| decode_wav(&b)) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                if let Ok(mut errors) = self.errors.lock()
                    && errors.len() < 8
                {
                    errors.push(format!("{}: {e}", path.display()));
                }
                None
            }
        };
        self.cache.lock().ok()?.entry(file.to_owned()).or_insert(p).clone()
    }
}

/// The runtime's sound: the bank, the files and the mixer.
pub struct Audio {
    pub aliases: Aliases,
    pub files: Files,
    pub mixer: Mixer,
    pub paused: bool,
    /// The MW2 mode is on (the host keeps its stream while it is).
    pub active: bool,
    /// First person: the stream plays at the listener (non-positional);
    /// third person: at `position`.
    pub listener: bool,
    /// Emitter position (the player's eye, GTA world) and velocity.
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    raised: HashSet<u32>,
    preloaded: HashSet<String>,
    reported: usize,
}

impl Audio {
    /// None (logged) when the setup imported no sounds.
    pub fn load(root: Option<&Path>, log: &dyn Fn(&str)) -> Option<Audio> {
        let dir = root?.join("mw2").join("sounds");
        let path = dir.join("aliases.json");
        if !path.is_file() {
            log("mw2: no imported weapon sounds (re-run INSTALL.bat); MW2 weapons stay silent");
            return None;
        }
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Aliases::parse(&t)) {
            Ok(aliases) => {
                log(&format!("mw2: {} weapon sound aliases in {}", aliases.len(), dir.display()));
                Some(Audio::new(aliases, Files::new(dir.join("files"))))
            }
            Err(e) => {
                log(&format!("mw2: weapon sounds not loaded: {}: {e}", path.display()));
                None
            }
        }
    }

    pub fn new(aliases: Aliases, files: Files) -> Audio {
        Audio {
            aliases,
            files,
            mixer: Mixer::new(0x5EED_CAFE),
            paused: false,
            active: false,
            listener: true,
            position: [0.0; 3],
            velocity: [0.0; 3],
            raised: HashSet::new(),
            preloaded: HashSet::new(),
            reported: 0,
        }
    }

    pub fn play(&mut self, name: &str) -> usize {
        self.mixer.play(&self.aliases, &mut self.files, name, 0)
    }

    /// A movement sound at `gain` of its alias's volume, varied: up to 20 % quieter and 5 % off in
    /// pitch each time.
    pub fn play_foley(&mut self, name: &str, gain: f32) -> usize {
        self.mixer.play_varied(&self.aliases, &mut self.files, name, [gain * 0.8, gain], [0.95, 1.05])
    }

    /// The files of every sound the weapon can play.
    fn gun_files(&self, gun: &Gun) -> Vec<String> {
        let mut names: Vec<String> = gun
            .strings
            .iter()
            .filter(|(k, _)| k.ends_with("_sound_player") || k == "melee_hit_sound")
            .map(|(_, v)| v.to_ascii_lowercase())
            .collect();
        names.extend(gun.string("notetrack_sounds").split(',').filter_map(|p| Some(p.split_once('=')?.1.to_owned())));
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        while let Some(n) = names.pop() {
            if n.is_empty() || !seen.insert(n.clone()) {
                continue;
            }
            for v in self.aliases.get(&n).unwrap_or(&[]) {
                files.push(v.file.clone());
                names.push(v.secondary.clone());
                names.push(v.chain.clone());
            }
        }
        files
    }

    /// Reads every sound the weapon can play (once per weapon), so its first
    /// shot does not wait on the disk.
    fn preload(&mut self, gun: &Gun) {
        if !self.preloaded.insert(gun.name.clone()) {
            return;
        }
        for f in self.gun_files(gun) {
            self.files.pcm(&f);
        }
    }

    /// Decodes these weapons' sounds on a background thread (before the mode
    /// is on), so their first frame does not wait on the disk either.
    pub fn prewarm(&mut self, guns: &[&Gun]) {
        let mut files = Vec::new();
        for g in guns {
            if self.preloaded.insert(g.name.clone()) {
                files.extend(self.gun_files(g));
            }
        }
        if files.is_empty() {
            return;
        }
        let mut shared = self.files.share();
        let _ = std::thread::Builder::new().name("codv-sound-prewarm".into()).spawn(move || {
            for f in files {
                shared.pcm(&f);
            }
        });
    }

    /// One weapon frame and the view model's notetracks crossed in it.
    pub fn gun_frame(&mut self, gun: &Gun, f: &GunFrame, notes: &[String], log: &dyn Fn(&str)) {
        self.preload(gun);
        let first = f.raise_started && self.raised.insert(f.weapon);
        for name in gun_sounds(gun, f, first) {
            self.play(&name);
        }
        for note in notes {
            if let Some(name) = note_sound(gun, note) {
                self.play(&name);
            }
        }
        let errors = self.files.errors();
        while self.reported < errors.len() {
            log(&format!("mw2: weapon sound: {}", errors[self.reported]));
            self.reported += 1;
        }
    }

    /// The viewmodel's notetracks, through the shown weapon's map (the
    /// grenade's during a throw).
    pub fn notes(&mut self, shown: &Gun, notes: &[String]) {
        if notes.is_empty() {
            return;
        }
        self.preload(shown);
        for note in notes {
            if let Some(name) = note_sound(shown, note) {
                self.play(&name);
            }
        }
    }

    /// The alias a weapon field names (e.g. a grenade's throw sound).
    pub fn play_field(&mut self, gun: &Gun, field: &str) {
        let name = gun.string(field).to_ascii_lowercase();
        if !name.is_empty() {
            self.preload(gun);
            self.play(&name);
        }
    }

    /// The next `out.len()` frames (silence while paused).
    pub fn pull(&mut self, out: &mut [f32]) {
        if self.paused {
            out.fill(0.0);
            return;
        }
        self.mixer.render(out);
    }

    /// A new MW2 player: first raises sound again.
    pub fn reset(&mut self) {
        self.raised.clear();
    }
}
