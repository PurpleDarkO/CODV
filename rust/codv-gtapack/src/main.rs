//! The setup's GTA archive tool (tools/prepare.py, stage `wheel`):
//!
//!     codv-gtapack keys <GTA5.exe> <keys dir>
//!         GTA's keys (rpf-archive-rs recovers them from the player's own GTA5.exe).
//!     codv-gtapack extract --keys <dir> <archive> [pattern]... -o <dir>
//!         `rage extract --recursive` for the few entries setup reads (SK8V's sk8v-rpf):
//!         same output paths, same "Extracted: N / M  Failed: F" line.
//!     codv-gtapack pack <weapon dir> <out dlc.rpf>
//!         CODV's DLC pack from tools/build-mw2-weapon.py's meta files.
//!
//! Nothing read from GTA is kept beyond the player's own machine.

mod archive;
mod pack;

use anyhow::{Context, Result, bail};
use archive::{Archive, GtaKeys, any_match, walk};
use std::path::PathBuf;
use std::sync::Arc;

const USAGE: &str = "usage: codv-gtapack keys <GTA5.exe> <keys dir>\n       \
                     codv-gtapack extract --keys <dir> <archive> [pattern]... -o <dir>\n       \
                     codv-gtapack pack <weapon dir> <out dlc.rpf>";

fn keys_command(args: &[String]) -> Result<()> {
    let [exe, out] = args else { bail!("{USAGE}") };
    let out = PathBuf::from(out);
    if GtaKeys::load_from_path(&out).is_ok() {
        println!("keys: {} (already there)", out.display());
        return Ok(());
    }
    GtaKeys::extract_from_exe(&PathBuf::from(exe), Some(&out)).context("GTA's keys")?;
    println!("keys: {}", out.display());
    Ok(())
}

fn extract_command(args: &[String]) -> Result<()> {
    let (mut keys_dir, mut out, mut archive, mut patterns) = (None, None, None, Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--no-update-check" | "--recursive" | "-r" => {}
            "--keys" => keys_dir = it.next().map(PathBuf::from),
            "-o" | "--output" => out = it.next().map(PathBuf::from),
            _ if archive.is_none() => archive = Some(PathBuf::from(a)),
            _ => patterns.push(a.to_lowercase()),
        }
    }
    let (Some(keys_dir), Some(archive), Some(out)) = (keys_dir, archive, out) else { bail!("{USAGE}") };
    let keys = GtaKeys::load_from_path(&keys_dir)?;
    let root = Arc::new(Archive::open(&archive, &keys).with_context(|| archive.display().to_string())?);
    let mut warnings = Vec::new();
    let entries = walk(root, &keys, &|p| any_match(&p.to_lowercase(), &patterns), &mut warnings);
    for w in &warnings {
        eprintln!("{w}");
    }
    let (mut ok, mut failed) = (0usize, 0usize);
    for e in &entries {
        let dest = out.join(&e.path);
        let written = e.read(&keys).and_then(|bytes| {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Ok(std::fs::write(&dest, bytes)?)
        });
        match written {
            Ok(()) => ok += 1,
            Err(err) => {
                eprintln!("Failed to extract {}: {err}", e.path);
                failed += 1;
            }
        }
    }
    println!("Extracted: {ok} / {}  Failed: {failed}", entries.len());
    if failed > 0 || entries.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

fn pack_command(args: &[String]) -> Result<()> {
    let [dir, out] = args else { bail!("{USAGE}") };
    let dir = PathBuf::from(dir);
    let weapon = pack::WEAPON_FILES
        .iter()
        .map(|(n, _)| Ok((*n, std::fs::read(dir.join(n)).with_context(|| dir.join(n).display().to_string())?)))
        .collect::<Result<Vec<_>>>()?;
    let bytes = pack::dlc(&weapon)?;
    std::fs::write(out, &bytes).with_context(|| out.clone())?;
    println!("{out}: CODV pack, {} bytes (WEAPON_CODV_MW2)", bytes.len());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keys") => keys_command(&args[1..]),
        Some("extract") => extract_command(&args[1..]),
        Some("pack") => pack_command(&args[1..]),
        _ => bail!("{USAGE}"),
    }
}
