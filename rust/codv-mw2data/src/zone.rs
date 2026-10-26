//! A Modern Warfare 2 (IW4, PC) fastfile opened into its zone image: the file
//! header, the signed format's 8 KiB hash chunks dropped (one in every 257),
//! zlib inflated. The same steps as iw4L's `asset_transport::zone`
//! (`parse_iw4_zone_image`, `dechunk_authed`), without its filesystem layer.
use fastfile_iw4::{FILE_PREAMBLE_LEN, MAGIC_AUTH_HEADER, MAX_XFILE_COUNT, Signing, ZoneHeader, ZoneStream};

const AUTHED_CHUNK_SIZE: usize = 0x2000;
const AUTHED_CHUNKS_PER_GROUP: usize = 256;

/// The inflated zone (XFile header, asset table, streams).
pub fn open(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let header = fastfile_iw4::parse_file_header(bytes).map_err(|e| format!("not a MW2 PC fastfile: {e:?}"))?;
    let body = &bytes[FILE_PREAMBLE_LEN..];
    let compressed = match header.signing {
        Signing::Unsigned => body.to_vec(),
        Signing::Signed => dechunk(body)?,
    };
    let image = miniz_oxide::inflate::decompress_to_vec_zlib(&compressed).map_err(|e| format!("inflate: {e:?}"))?;
    if image.len() < fastfile_iw4::XFILE_HEADER_LEN {
        return Err("inflated zone is shorter than its header".into());
    }
    Ok(image)
}

fn dechunk(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < 8 || &body[0..8] != MAGIC_AUTH_HEADER {
        return Err("signed fastfile without its auth header".into());
    }
    let mut pos = AUTHED_CHUNK_SIZE;
    let mut out = Vec::with_capacity(body.len());
    let mut chunk = 0usize;
    while pos < body.len() {
        let end = (pos + AUTHED_CHUNK_SIZE).min(body.len());
        // The first chunk of each group of 257 holds the hashes of the next 256.
        if chunk != 0 {
            out.extend_from_slice(&body[pos..end]);
        }
        pos = end;
        chunk = (chunk + 1) % (AUTHED_CHUNKS_PER_GROUP + 1);
    }
    Ok(out)
}

/// The zone's load-time memory blocks (iw4L's `ZoneMemory`).
pub struct Memory {
    blocks: [Vec<u8>; MAX_XFILE_COUNT],
    insert_map: Vec<u8>,
}

impl Memory {
    pub fn for_header(header: &ZoneHeader) -> Memory {
        Memory {
            blocks: std::array::from_fn(|i| vec![0u8; header.block_size[i] as usize]),
            insert_map: vec![0u8; ZoneStream::insert_map_len(header)],
        }
    }

    pub fn stream<'a>(&'a mut self, image: &'a [u8]) -> Result<ZoneStream<'a>, String> {
        let [b0, b1, b2, b3, b4, b5, b6, b7] = &mut self.blocks;
        let blocks: [&mut [u8]; MAX_XFILE_COUNT] = [b0, b1, b2, b3, b4, b5, b6, b7].map(|b| b.as_mut_slice());
        ZoneStream::new(image, blocks, &mut self.insert_map).map_err(|e| format!("zone stream: {e:?}"))
    }
}
