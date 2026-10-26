//! Against the player's own common_mp.ff (CODV_MW2_FF); skipped otherwise.
#[test]
fn scout() {
    let Ok(path) = std::env::var("CODV_MW2_FF") else { return };
    let bytes = std::fs::read(path).unwrap();
    let image = codv_mw2data::zone::open(&bytes).unwrap();
    let t = std::time::Instant::now();
    let f = codv_mw2data::viewmodels::capture(&image).unwrap();
    println!("walk {:?}", t.elapsed());
    println!(
        "models {} anims {} images {} weapons {} skipped {}",
        f.models.len(),
        f.anims.len(),
        f.images.len(),
        f.weapon_anims.len(),
        f.skipped.len()
    );
    for s in f.skipped.iter().take(10) {
        println!("skip {s}");
    }
    let in_zone = f.images.values().filter(|i| !i.payload.is_empty()).count();
    println!("images with pixels in zone: {in_zone}");
    for m in
        f.models.iter().filter(|m| m.name.starts_with("viewhands") || m.name == "viewmodel_base_viewhands").take(12)
    {
        println!(
            "{} bones {} surfs {} verts {} tris {}",
            m.name,
            m.bone_names.len(),
            m.surfaces.len(),
            m.vertices.len(),
            m.indices.len() / 3
        );
        println!("  bones {:?}", &m.bone_names[..m.bone_names.len().min(60)]);
        for s in m.surfaces.iter().take(4) {
            println!(
                "   {} col={} nrm={} bits={:08x} {:08x} sort {}",
                s.material.name,
                s.material.color_map,
                s.material.normal_map,
                s.material.state_bits[0],
                s.material.state_bits[1],
                s.material.sort_key
            );
        }
    }
    if let Some(a) = f.weapon_anims.get("m4_mp") {
        for (i, n) in a.iter().enumerate() {
            if !n.is_empty() {
                println!("m4 anim {i}: {n} {}", f.anims.contains_key(n));
            }
        }
    }
    let total: usize = f.anims.values().map(|a| codv_mw2data::vm::write_anim(a).len()).sum();
    let mt: usize = f.models.iter().map(|m| codv_mw2data::vm::write_model(m).len()).sum();
    println!("anim bytes {total} model bytes {mt}");
}

#[test]
fn scout_images() {
    let Ok(dir) = std::env::var("CODV_MW2_MAIN") else { return };
    let mut archives: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|d| d.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "iwd"))
        .collect();
    archives.sort();
    let t = std::time::Instant::now();
    let imgs = codv_mw2data::iwd::Images::from_archives(archives).unwrap();
    println!("{} iwi in {:?}", imgs.len(), t.elapsed());
    let mut formats = std::collections::BTreeMap::new();
    for name in
        ["weapon_m4_short_col", "us_army_glove_col", "weapon_m4_short_nml", "us_army_sleeve_gps_col", "weapon_usp_col"]
    {
        match imgs.read(name).unwrap() {
            Some(b) => {
                *formats.entry(b[8]).or_insert(0) += 1;
                let d = codv_mw2data::iwd::iwi_to_dds(&b);
                println!(
                    "{name}: iwi {} bytes fmt {} {}x{} -> {:?}",
                    b.len(),
                    b[8],
                    u16::from_le_bytes([b[10], b[11]]),
                    u16::from_le_bytes([b[12], b[13]]),
                    d.map(|d| d.len())
                );
            }
            None => println!("{name}: not in these archives"),
        }
    }
}

#[test]
fn scout_weapons() {
    let Ok(path) = std::env::var("CODV_MW2_FF") else { return };
    let image = codv_mw2data::zone::open(&std::fs::read(path).unwrap()).unwrap();
    let f = codv_mw2data::viewmodels::capture(&image).unwrap();
    for w in ["m4_mp", "m4_acog_mp", "usp_mp", "cheytac_mp", "ak47_mp"] {
        println!("{w}: {:?}", f.weapon_models.get(w));
    }
    let models: std::collections::HashSet<_> = f.models.iter().map(|m| m.name.as_str()).collect();
    let missing: Vec<_> = f
        .weapon_models
        .values()
        .map(|m| m.gun.as_str())
        .filter(|g| !models.contains(g))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    println!("guns without a captured model: {}: {:?}", missing.len(), &missing[..missing.len().min(30)]);
}

#[test]
fn scout_body() {
    let Ok(path) = std::env::var("CODV_MW2_FF") else { return };
    let image = codv_mw2data::zone::open(&std::fs::read(path).unwrap()).unwrap();
    let f = codv_mw2data::viewmodels::capture(&image).unwrap();
    let bodies: Vec<_> = f.models.iter().filter(|m| codv_mw2data::viewmodels::is_body_model(&m.name)).collect();
    println!("bodies {}", bodies.len());
    for b in bodies.iter().take(40) {
        println!("  {} bones {} surfaces {}", b.name, b.bone_names.len(), b.surfaces.len());
    }
    if let Some(b) = bodies.first() {
        println!("  bones: {:?}", b.bone_names);
    }
    for (k, v) in &f.raw {
        println!("raw {k}: {} bytes", v.len());
    }
    if f.raw.is_empty() {
        return;
    }
    let atr = &f.raw[codv_mw2data::playeranim::MULTIPLAYER_ANIMTREE_PATH];
    let script = &f.raw[codv_mw2data::playeranim::PLAYERANIM_SCRIPT_PATH];
    let (tree, parsed) = codv_mw2data::playeranim::compile(atr, script).unwrap();
    println!("{}", parsed.report_line());
    let leaves: Vec<_> = tree.nodes().iter().filter(|n| n.child_count == 0).map(|n| n.name.clone()).collect();
    let have = leaves.iter().filter(|n| f.anims.contains_key(n.as_str())).count();
    println!(
        "tree nodes {} leaves {} in zone {} legs {:?} torso {:?}",
        tree.node_count(),
        leaves.len(),
        have,
        tree.index_of("legs"),
        tree.index_of("torso")
    );
    let missing: Vec<_> = leaves.iter().filter(|n| !f.anims.contains_key(n.as_str())).take(10).collect();
    println!("missing {missing:?}");
    let bytes: usize = leaves
        .iter()
        .filter_map(|n| f.anims.get(n.as_str()))
        .map(|a| {
            a.data_byte.len()
                + a.data_short.len() * 2
                + a.data_int.len() * 4
                + a.random_data_byte.len()
                + a.random_data_short.len() * 2
                + a.random_data_int.len() * 4
                + a.indices.len() * 2
        })
        .sum();
    println!("anim payload ~{:.1} MB", bytes as f64 / 1e6);
}

#[test]
fn scout_sounds() {
    let Ok(path) = std::env::var("CODV_MW2_FF") else { return };
    let image = codv_mw2data::zone::open(&std::fs::read(path).unwrap()).unwrap();
    let t = std::time::Instant::now();
    let f = codv_mw2data::sounds::capture(&image).unwrap();
    println!("walk {:?}: {} aliases, {} loaded", t.elapsed(), f.aliases.len(), f.loaded.len());
    let mut formats = std::collections::BTreeMap::new();
    for l in f.loaded.values() {
        *formats.entry((l.format, l.bits, l.channels, l.rate)).or_insert(0) += 1;
    }
    println!("formats {formats:?}");
    for a in [
        "weap_m4_fire_plr",
        "weap_m4_lastshot_plr",
        "weap_dryfire_rifle_plr",
        "weap_raise_plr",
        "weap_m4_clipout_plr",
        "melee_swing_plr_small",
    ] {
        println!("{a}: {:?}", f.aliases.get(a));
    }
    let mut names: Vec<_> = f.aliases.keys().filter(|k| k.contains("m4")).cloned().collect();
    names.sort();
    println!("m4 aliases: {names:?}");
}
