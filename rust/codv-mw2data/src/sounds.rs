//! The sound aliases and loaded sounds of a zone (F5a): which file each
//! alias plays and with what volume, pitch, delay and chance, and the loaded
//! sounds' PCM. Follows iw4L's `asset_audio::sound_catalog` capture
//! (`capture_iw4.rs`: the SndAliasList rows, the LoadedSound records and the
//! SoundFile edges between them), without its catalog and curve layers.
//!
//! iw4L is Apache-2.0 (NOTICE).
use asset_iw4::size as sz;
use asset_iw4::snd_alias::*;
use fastfile_iw4::{AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZonePtr, ZoneStream};
use std::collections::HashMap;

/// MSS format of uncompressed PCM (asset_audio MSS_PCM).
pub const MSS_PCM: i32 = 1;

/// One loaded sound as the zone holds it.
#[derive(Clone, Debug, Default)]
pub struct Loaded {
    pub name: String,
    pub format: i32,
    pub rate: u32,
    pub bits: i32,
    pub channels: i32,
    pub data: Vec<u8>,
}

/// One row of an alias list: a variant the alias may play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Alias {
    pub name: String,
    /// The loaded sound it plays (empty: streamed or none).
    pub loaded: String,
    /// A streamed file (dir/name in main/iw_*.iwd), when not loaded.
    pub streamed: String,
    pub secondary: String,
    pub chain: String,
    pub vol_min: f32,
    pub vol_max: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub dist_min: f32,
    pub dist_max: f32,
    pub probability: f32,
    pub start_delay_ms: i32,
    pub flags: u32,
}

#[derive(Default)]
pub struct Found {
    pub loaded: HashMap<String, Loaded>,
    /// Alias name -> its variants.
    pub aliases: HashMap<String, Vec<Alias>>,
}

type Key = (u8, u32);
fn key(p: Ptr) -> Key {
    (p.block, p.offset)
}

#[derive(Default)]
struct Sink {
    found: Found,
    last_loaded: Option<String>,
    loaded_by_ptr: HashMap<Key, String>,
    file_to_loaded: HashMap<Key, String>,
    file_to_streamed: HashMap<Key, String>,
}

fn name_at(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> String {
    match s.ptr_at(parent, field) {
        Ok(ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).map(str::to_owned).unwrap_or_default(),
        _ => String::new(),
    }
}

impl Sink {
    /// The loaded sound a SoundFile points at (`capture_iw4` inspect_sound_file).
    fn file_loaded(&self, s: &ZoneStream<'_>, file: Ptr) -> String {
        if let Some(n) = self.file_to_loaded.get(&key(file)) {
            return n.clone();
        }
        if s.u8_at(file, 0).ok() != Some(1) {
            return String::new();
        }
        match s.ptr_at(file, 4) {
            Ok(ZonePtr::Offset(q)) => self
                .loaded_by_ptr
                .get(&key(q))
                .or_else(|| self.loaded_by_ptr.get(&key(s.resolve_alias(q))))
                .cloned()
                .unwrap_or_else(|| {
                    let target = s.resolve_alias(q);
                    match s.u32_at(target, 0).ok().map(ZonePtr::decode) {
                        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => String::new(),
                        _ => name_at(s, target, 0),
                    }
                }),
            _ => String::new(),
        }
    }

    fn file_streamed(&self, s: &ZoneStream<'_>, file: Ptr) -> String {
        if let Some(n) = self.file_to_streamed.get(&key(file)) {
            return n.clone();
        }
        if !matches!(s.u8_at(file, 0).ok(), Some(2) | Some(3)) {
            return String::new();
        }
        let (dir, name) = (name_at(s, file, 4), name_at(s, file, 8));
        if name.is_empty() {
            String::new()
        } else if dir.is_empty() {
            name
        } else {
            format!("{dir}/{name}")
        }
    }
}

impl AssetSink for Sink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        fastfile_iw4::load_asset_at_observed(s, ty, slot, self)?;
        Ok(())
    }
}

impl AssetLinkSink for Sink {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        if ty == AssetType::LoadedSound
            && let Some(name) = self.last_loaded.clone()
        {
            self.loaded_by_ptr.insert(key(slot), name.clone());
            if let Some(p) = insert {
                self.loaded_by_ptr.insert(key(p), name);
            }
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        if ty == AssetType::LoadedSound
            && let Some(name) = self.loaded_by_ptr.get(&key(target)).cloned()
        {
            self.loaded_by_ptr.insert(key(slot), name);
        }
        Ok(())
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> fastfile_iw4::Result<()> {
        let name = name_at(s, header, 0);
        if name.is_empty() {
            self.last_loaded = None;
            return Ok(());
        }
        // IW4 x86 LoadedSound: name, then the MssSound wave format.
        let format = s.i32_at(header, 4).unwrap_or(0);
        let rate = s.u32_at(header, 16).unwrap_or(0);
        let bits = s.i32_at(header, 20).unwrap_or(16);
        let channels = s.i32_at(header, 24).unwrap_or(1);
        let data = s.slice_at(pcm, 0, data_len).map(<[u8]>::to_vec).unwrap_or_default();
        self.last_loaded = Some(name.clone());
        if !data.is_empty() {
            self.found.loaded.insert(name.clone(), Loaded { name, format, rate, bits, channels, data });
        }
        Ok(())
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> fastfile_iw4::Result<()> {
        if let Some(n) = self.last_loaded.clone() {
            self.file_to_loaded.insert(key(file), n);
        }
        Ok(())
    }

    fn bind_streamed_sound_file(&mut self, file: Ptr, dir: &str, name: &str) -> fastfile_iw4::Result<()> {
        let path = if dir.is_empty() { name.to_owned() } else { format!("{dir}/{name}") };
        self.file_to_streamed.insert(key(file), path);
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        let list_name = name_at(s, list, 0).to_ascii_lowercase();
        let Some(arr) = head else { return Ok(()) };
        let mut rows = Vec::with_capacity(count);
        for i in 0..count {
            let row = arr.at(i * sz::SND_ALIAS);
            let (loaded, streamed) = match s.ptr_at(row, SND_ALIAS_SOUND_FILE) {
                Ok(ZonePtr::Offset(p)) => {
                    let file = s.resolve_alias(p);
                    (self.file_loaded(s, file), self.file_streamed(s, file))
                }
                _ => (String::new(), String::new()),
            };
            let f = |o: usize, d: f32| s.f32_at(row, o).unwrap_or(d);
            rows.push(Alias {
                name: name_at(s, row, SND_ALIAS_ALIAS_NAME).to_ascii_lowercase(),
                loaded,
                streamed,
                secondary: name_at(s, row, SND_ALIAS_SECONDARY).to_ascii_lowercase(),
                chain: name_at(s, row, SND_ALIAS_CHAIN).to_ascii_lowercase(),
                vol_min: f(SND_ALIAS_VOL_MIN, 1.0),
                vol_max: f(SND_ALIAS_VOL_MAX, 1.0),
                pitch_min: f(SND_ALIAS_PITCH_MIN, 1.0),
                pitch_max: f(SND_ALIAS_PITCH_MAX, 1.0),
                dist_min: f(SND_ALIAS_DIST_MIN, 0.0),
                dist_max: f(SND_ALIAS_DIST_MAX, 0.0),
                probability: f(SND_ALIAS_PROBABILITY, 1.0),
                start_delay_ms: s.i32_at(row, SND_ALIAS_START_DELAY).unwrap_or(0),
                flags: s.u32_at(row, SND_ALIAS_FLAGS).unwrap_or(0),
            });
        }
        if !list_name.is_empty() {
            self.found.aliases.insert(list_name, rows);
        }
        Ok(())
    }
}

/// Every alias and loaded sound of one zone image (`zone::open`).
pub fn capture(image: &[u8]) -> Result<Found, String> {
    let header = fastfile_iw4::parse_zone_header(image).map_err(|e| format!("zone header: {e:?}"))?;
    let mut memory = crate::zone::Memory::for_header(&header);
    let mut stream = memory.stream(image)?;
    let mut sink = Sink::default();
    fastfile_iw4::load_zone(&mut stream, &mut sink).map_err(|e| format!("zone walk stopped: {e:?}"))?;
    Ok(sink.found)
}

/// A loaded PCM sound as a RIFF WAVE file (what the runtime reads back).
pub fn wav(l: &Loaded) -> Option<Vec<u8>> {
    if l.format != MSS_PCM || !(l.bits == 8 || l.bits == 16) || !(1..=2).contains(&l.channels) || l.rate == 0 {
        return None;
    }
    let ch = l.channels as u16;
    let bits = l.bits as u16;
    let block = ch * bits / 8;
    let mut out = Vec::with_capacity(44 + l.data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + l.data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&l.rate.to_le_bytes());
    out.extend_from_slice(&(l.rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(l.data.len() as u32).to_le_bytes());
    out.extend_from_slice(&l.data);
    Some(out)
}
