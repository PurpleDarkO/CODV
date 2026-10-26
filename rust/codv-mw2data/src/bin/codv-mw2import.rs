//! `codv-mw2import <MW2 folder> <output folder>`: reads the player's own
//! Modern Warfare 2 multiplayer fastfiles and writes `weapons.json` (CODV's
//! weapon table) and `viewmodels/` (first-person meshes, animations and
//! images from the fastfiles and `main/iw_*.iwd`) into the output folder. Setup runs it; nothing it writes
//! leaves the player's machine.
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: codv-mw2import <MW2 folder (with zone\\english)> <output folder>");
        return ExitCode::from(2);
    }
    let t0 = std::time::Instant::now();
    let mw2 = std::path::Path::new(&args[1]);
    let mut table = match codv_mw2data::import(mw2) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("MW2 import failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let out = std::path::Path::new(&args[2]);
    // First person: meshes, animations, images (a failure keeps the weapons).
    match codv_mw2data::import_viewmodels(mw2, &mut table, out) {
        Ok(v) => {
            println!(
                "MW2 viewmodels: {} models, {} animations, {} images ({:.0} MB); {} camouflage models skipped",
                v.models,
                v.anims,
                v.images,
                v.bytes as f64 / 1e6,
                v.skipped_models
            );
            for m in v.missing_models.iter().take(10) {
                println!("  no first-person model {m}");
            }
            for m in v.missing_images.iter().take(10) {
                println!("  image: {m}");
            }
            match &v.body_error {
                None => println!(
                    "MW2 character: {} animations, skeleton {} (from {})",
                    v.body_anims, v.body_model, v.body_zone
                ),
                Some(e) => println!("MW2 character not imported (third person keeps GTA's animation): {e}"),
            }
        }
        Err(e) => eprintln!("MW2 viewmodels not imported (weapons still are): {e}"),
    }
    // F5a: the sounds the player hears from those guns.
    match codv_mw2data::import_sounds(mw2, &table, out) {
        Ok(v) => {
            println!("MW2 sounds: {} aliases, {} files ({:.0} MB)", v.aliases, v.files, v.bytes as f64 / 1e6);
            for m in v.missing.iter().take(10) {
                println!("  no sound alias {m}");
            }
        }
        Err(e) => eprintln!("MW2 sounds not imported (GTA's stand-in sounds stay): {e}"),
    }
    // F5b: MW2's HUD (weapon bar layout, fonts, strings, HUD images).
    match codv_mw2data::import_hud(mw2, &table, out) {
        Ok(h) => {
            println!(
                "MW2 HUD: {} fonts, {} weapon bar items, {} strings, {} images, {} killstreaks",
                h.fonts, h.items, h.strings, h.images, h.killstreaks
            );
            for m in h.missing.iter().take(10) {
                println!("  HUD: missing {m}");
            }
        }
        Err(e) => eprintln!("MW2 HUD not imported (the host's HUD stays): {e}"),
    }
    if let Err(e) = std::fs::create_dir_all(out)
        .and_then(|_| std::fs::write(out.join("weapons.json"), codv_mw2data::table::to_json(&table)))
    {
        eprintln!("writing {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let guns = table.weapons.iter().filter(|w| w.i("weap_type") == 0 && w.i("clip_size") > 0).count();
    println!(
        "MW2 import: {} weapons ({guns} guns) from {}; hit-location table {}; {:.1} s -> {}",
        table.weapons.len(),
        table.source,
        if table.location.is_some() { "found" } else { "missing (identity)" },
        t0.elapsed().as_secs_f32(),
        out.join("weapons.json").display()
    );
    ExitCode::SUCCESS
}
