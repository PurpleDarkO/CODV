//! The player's MW2 image archives (`main/iw_*.iwd`, plain zip files) and
//! their `.iwi` images, written out as DirectX `.dds` files. The IWI v8
//! layout (32-byte header; mips stored smallest first, the full size last,
//! ending at `file_size_for_picmip[0]`) follows iw4L's
//! `asset_iw4::iwi` / `asset_material::material_images::decode`
//! (Apache-2.0, NOTICE).
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

struct Entry {
    archive: usize,
    offset: u64,
    compressed: u64,
    size: u64,
    method: u16,
}

/// Every `images/*.iwi` across the archives; later archives win, as MW2's
/// search path does (iw_22 over iw_00).
pub struct Images {
    archives: Vec<PathBuf>,
    entries: HashMap<String, Entry>,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// One zip entry: name, local header offset, compressed and uncompressed size, method.
type DirEntry = (String, u64, u64, u64, u16);

fn central_directory(path: &Path) -> Result<Vec<DirEntry>, String> {
    let e = |w: &str| format!("{}: {w}", path.display());
    let mut f = File::open(path).map_err(|x| e(&x.to_string()))?;
    let len = f.metadata().map_err(|x| e(&x.to_string()))?.len();
    let tail_len = len.min(65_557) as usize;
    let mut tail = vec![0u8; tail_len];
    f.seek(SeekFrom::Start(len - tail_len as u64)).map_err(|x| e(&x.to_string()))?;
    f.read_exact(&mut tail).map_err(|x| e(&x.to_string()))?;
    let eocd = (0..=tail_len.saturating_sub(22))
        .rev()
        .find(|&i| u32le(&tail, i) == 0x0605_4b50)
        .ok_or_else(|| e("not a zip archive"))?;
    let count = u16le(&tail, eocd + 10) as usize;
    let cd_size = u32le(&tail, eocd + 12) as usize;
    let cd_offset = u64::from(u32le(&tail, eocd + 16));
    let mut cd = vec![0u8; cd_size];
    f.seek(SeekFrom::Start(cd_offset)).map_err(|x| e(&x.to_string()))?;
    f.read_exact(&mut cd).map_err(|x| e(&x.to_string()))?;
    let mut out = Vec::with_capacity(count);
    let mut at = 0usize;
    for _ in 0..count {
        if at + 46 > cd.len() || u32le(&cd, at) != 0x0201_4b50 {
            return Err(e("damaged central directory"));
        }
        let method = u16le(&cd, at + 10);
        let compressed = u64::from(u32le(&cd, at + 20));
        let size = u64::from(u32le(&cd, at + 24));
        let name_len = u16le(&cd, at + 28) as usize;
        let extra_len = u16le(&cd, at + 30) as usize;
        let comment_len = u16le(&cd, at + 32) as usize;
        let local = u64::from(u32le(&cd, at + 42));
        let name =
            String::from_utf8_lossy(cd.get(at + 46..at + 46 + name_len).ok_or_else(|| e("damaged name"))?).into_owned();
        out.push((name, local, compressed, size, method));
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

impl Images {
    /// `main/iw_NN.iwd` of the MW2 folder (sorted by name).
    pub fn open(mw2: &Path) -> Result<Images, String> {
        Self::open_prefixed(mw2, "images/", ".iwi")
    }

    /// The archives' files under `prefix` ending in `suffix` (e.g. "sound/", ""),
    /// named by the rest of their path, lower case. Streamed sounds (F5c) use it.
    pub fn open_prefixed(mw2: &Path, prefix: &str, suffix: &str) -> Result<Images, String> {
        let main = mw2.join("main");
        let mut archives: Vec<PathBuf> = std::fs::read_dir(&main)
            .map_err(|e| format!("{}: {e}", main.display()))?
            .filter_map(|d| d.ok().map(|d| d.path()))
            .filter(|p| {
                // iw_NN and the language's localized_<lang>_iwNN archives (the HUD font atlas lives there).
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    let n = n.to_ascii_lowercase();
                    (n.starts_with("iw_") || n.starts_with("localized_")) && n.ends_with(".iwd")
                })
            })
            .collect();
        archives.sort();
        Self::from_archives_prefixed(archives, prefix, suffix)
    }

    pub fn from_archives(archives: Vec<PathBuf>) -> Result<Images, String> {
        Self::from_archives_prefixed(archives, "images/", ".iwi")
    }

    fn from_archives_prefixed(archives: Vec<PathBuf>, prefix: &str, suffix: &str) -> Result<Images, String> {
        let mut entries = HashMap::new();
        for (index, path) in archives.iter().enumerate() {
            for (name, offset, compressed, size, method) in central_directory(path)? {
                let lower = name.to_ascii_lowercase().replace('\\', "/");
                let Some(stem) = lower.strip_prefix(prefix).and_then(|n| n.strip_suffix(suffix)) else { continue };
                if stem.is_empty() || stem.ends_with('/') {
                    continue;
                }
                entries.insert(stem.to_owned(), Entry { archive: index, offset, compressed, size, method });
            }
        }
        Ok(Images { archives, entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `.iwi` bytes of an image name (case-insensitive), if any archive has it.
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let Some(en) = self.entries.get(&name.to_ascii_lowercase()) else { return Ok(None) };
        let path = &self.archives[en.archive];
        let e = |w: &str| format!("{} ({name}): {w}", path.display());
        let mut f = File::open(path).map_err(|x| e(&x.to_string()))?;
        let mut local = [0u8; 30];
        f.seek(SeekFrom::Start(en.offset)).map_err(|x| e(&x.to_string()))?;
        f.read_exact(&mut local).map_err(|x| e(&x.to_string()))?;
        if u32le(&local, 0) != 0x0403_4b50 {
            return Err(e("damaged local header"));
        }
        let skip = u64::from(u16le(&local, 26)) + u64::from(u16le(&local, 28));
        f.seek(SeekFrom::Current(skip as i64)).map_err(|x| e(&x.to_string()))?;
        let mut data = vec![0u8; en.compressed as usize];
        f.read_exact(&mut data).map_err(|x| e(&x.to_string()))?;
        let bytes = match en.method {
            0 => data,
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&data, en.size as usize + 16)
                .map_err(|x| e(&format!("inflate {x:?}")))?,
            m => return Err(e(&format!("zip method {m}"))),
        };
        Ok(Some(bytes))
    }
}

/// Pixel layouts CODV writes (DXGI formats the host creates directly).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pixels {
    Bc1,
    Bc2,
    Bc3,
    Rgba8,
}

fn level_bytes(p: Pixels, w: u32, h: u32) -> usize {
    match p {
        Pixels::Bc1 => w.div_ceil(4) as usize * h.div_ceil(4) as usize * 8,
        Pixels::Bc2 | Pixels::Bc3 => w.div_ceil(4) as usize * h.div_ceil(4) as usize * 16,
        Pixels::Rgba8 => w as usize * h as usize * 4,
    }
}

/// A DDS file (legacy header for DXT1/3/5, A8B8G8R8 for RGBA) with `levels`
/// mips laid out largest first in `data`.
pub fn dds(p: Pixels, width: u32, height: u32, levels: u32, data: &[u8]) -> Vec<u8> {
    let mut h = vec![0u8; 128];
    let put = |h: &mut Vec<u8>, o: usize, v: u32| h[o..o + 4].copy_from_slice(&v.to_le_bytes());
    h[0..4].copy_from_slice(b"DDS ");
    put(&mut h, 4, 124);
    // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT | LINEARSIZE/PITCH
    let compressed = p != Pixels::Rgba8;
    put(&mut h, 8, 0x1 | 0x2 | 0x4 | 0x1000 | 0x20000 | if compressed { 0x80000 } else { 0x8 });
    put(&mut h, 12, height);
    put(&mut h, 16, width);
    put(&mut h, 20, if compressed { level_bytes(p, width, height) as u32 } else { width * 4 });
    put(&mut h, 28, levels);
    put(&mut h, 76, 32);
    match p {
        Pixels::Bc1 | Pixels::Bc2 | Pixels::Bc3 => {
            put(&mut h, 80, 0x4);
            h[84..88].copy_from_slice(match p {
                Pixels::Bc1 => b"DXT1",
                Pixels::Bc2 => b"DXT3",
                _ => b"DXT5",
            });
        }
        Pixels::Rgba8 => {
            put(&mut h, 80, 0x41);
            put(&mut h, 88, 32);
            put(&mut h, 92, 0x0000_00ff);
            put(&mut h, 96, 0x0000_ff00);
            put(&mut h, 100, 0x00ff_0000);
            put(&mut h, 104, 0xff00_0000);
        }
    }
    put(&mut h, 108, 0x1000 | if levels > 1 { 0x400008 } else { 0 });
    h.extend_from_slice(data);
    h
}

/// An IWI v8 image as a DDS file. Formats: 1 BGRA8, 2 RGB8, 3 LA8, 4 L8,
/// 5 A8 (expanded to RGBA8), 11/12/13 DXT1/3/5 (kept compressed). Wavelet
/// and BC5 images are refused (not used by MW2 PC's first-person materials).
pub fn iwi_to_dds(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 32 || &bytes[..3] != b"IWi" {
        return Err("not an IWI image".into());
    }
    if bytes[3] != 8 {
        return Err(format!("IWI version {} (MW2 PC uses 8)", bytes[3]));
    }
    let format = bytes[8];
    let width = u32::from(u16le(bytes, 10));
    let height = u32::from(u16le(bytes, 12));
    let mip0_end = (u32le(bytes, 16) as usize).min(bytes.len());
    if width == 0 || height == 0 {
        return Err("zero-sized image".into());
    }
    let (src_bpp, pixels) = match format {
        11 => (0, Pixels::Bc1),
        12 => (0, Pixels::Bc2),
        13 => (0, Pixels::Bc3),
        1 => (4, Pixels::Rgba8),
        2 => (3, Pixels::Rgba8),
        3 => (2, Pixels::Rgba8),
        4 | 5 => (1, Pixels::Rgba8),
        f => return Err(format!("IWI format {f} is not supported")),
    };
    let src_level = |w: u32, h: u32| if src_bpp == 0 { level_bytes(pixels, w, h) } else { (w * h) as usize * src_bpp };
    let no_mips = bytes[4] & 0x2 != 0;
    let full = 32 - width.max(height).leading_zeros();
    // Walk back from the end of mip 0; fall back to one level if the sizes do not add up.
    let mut ranges = Vec::new();
    let mut cursor = mip0_end;
    for level in 0..if no_mips { 1 } else { full } {
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        let size = src_level(w, h);
        let Some(start) = cursor.checked_sub(size) else { break };
        if start < 32 {
            break;
        }
        ranges.push((start, cursor, w, h));
        cursor = start;
    }
    if ranges.is_empty() {
        return Err("truncated IWI".into());
    }
    if cursor != 32 {
        ranges.truncate(1);
    }
    let mut data = Vec::new();
    for &(s, e, w, h) in &ranges {
        let src = &bytes[s..e];
        if src_bpp == 0 {
            data.extend_from_slice(src);
        } else {
            for px in src.chunks_exact(src_bpp).take((w * h) as usize) {
                let rgba = match format {
                    1 => [px[2], px[1], px[0], px[3]],
                    2 => [px[2], px[1], px[0], 255],
                    3 => [px[0], px[0], px[0], px[1]],
                    4 => [px[0], px[0], px[0], 255],
                    _ => [255, 255, 255, px[0]],
                };
                data.extend_from_slice(&rgba);
            }
        }
    }
    Ok(dds(pixels, width, height, ranges.len() as u32, &data))
}

/// A GfxImage whose pixels the zone carries (GfxImageLoadDef: D3D9 format,
/// all levels largest first) as a DDS file: DXT1/3/5 as they are, A8R8G8B8,
/// A8, L8 and A8L8 as RGBA8 (alpha-only images white with that alpha, as
/// MW2's fonts sample them).
pub fn d3d_to_dds(format: u32, width: u32, height: u32, levels: u32, data: &[u8]) -> Result<Vec<u8>, String> {
    const DXT1: u32 = u32::from_le_bytes(*b"DXT1");
    const DXT3: u32 = u32::from_le_bytes(*b"DXT3");
    const DXT5: u32 = u32::from_le_bytes(*b"DXT5");
    if width == 0 || height == 0 {
        return Err("zero-sized image".into());
    }
    let (pixels, src_bpp) = match format {
        DXT1 => (Pixels::Bc1, 0),
        DXT3 => (Pixels::Bc2, 0),
        DXT5 => (Pixels::Bc3, 0),
        21 => (Pixels::Rgba8, 4),
        28 | 50 => (Pixels::Rgba8, 1),
        51 => (Pixels::Rgba8, 2),
        f => return Err(format!("D3D format {f} is not supported")),
    };
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut kept = 0;
    for level in 0..levels.max(1) {
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        let size = if src_bpp == 0 { level_bytes(pixels, w, h) } else { (w * h) as usize * src_bpp };
        let Some(src) = data.get(at..at + size) else { break };
        at += size;
        kept += 1;
        if src_bpp == 0 {
            out.extend_from_slice(src);
        } else {
            for px in src.chunks_exact(src_bpp) {
                out.extend_from_slice(&match format {
                    21 => [px[2], px[1], px[0], px[3]],
                    28 => [255, 255, 255, px[0]],
                    50 => [px[0], px[0], px[0], 255],
                    _ => [px[0], px[0], px[0], px[1]],
                });
            }
        }
    }
    if kept == 0 {
        return Err("truncated image".into());
    }
    Ok(dds(pixels, width, height, kept, &out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iwi(format: u8, w: u16, h: u16, levels: &[Vec<u8>]) -> Vec<u8> {
        // levels given largest first; stored smallest first.
        let mut body = Vec::new();
        for l in levels.iter().rev() {
            body.extend_from_slice(l);
        }
        let mut b = vec![0u8; 32];
        b[..4].copy_from_slice(b"IWi\x08");
        b[8] = format;
        b[10..12].copy_from_slice(&w.to_le_bytes());
        b[12..14].copy_from_slice(&h.to_le_bytes());
        let end = (32 + body.len()) as u32;
        b[16..20].copy_from_slice(&end.to_le_bytes());
        b.extend_from_slice(&body);
        b
    }

    #[test]
    fn dxt_mip_chains_and_bitmaps_convert() {
        // 8x8 DXT1: 4 blocks, then 1, 1, 1.
        let d = iwi_to_dds(&iwi(11, 8, 8, &[vec![1; 32], vec![2; 8], vec![3; 8], vec![4; 8]])).unwrap();
        assert_eq!(&d[84..88], b"DXT1");
        assert_eq!(u32le(&d, 28), 4);
        assert_eq!(d.len(), 128 + 56);
        assert_eq!(d[128], 1);
        assert_eq!(d[128 + 32], 2);
        // 2x1 BGRA: two levels.
        let d = iwi_to_dds(&iwi(1, 2, 1, &[vec![10, 20, 30, 40, 1, 2, 3, 4], vec![5, 6, 7, 8]])).unwrap();
        assert_eq!(&d[128..132], &[30, 20, 10, 40]);
        assert_eq!(u32le(&d, 28), 2);
        assert!(iwi_to_dds(b"IWi\x06").is_err());
    }
}
