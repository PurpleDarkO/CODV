//! The held MW2 weapon's first-person view model behind the ABI: the
//! imported meshes and animations under `<data>/mw2/viewmodels/` (read on
//! first use), one `codv_engine::viewmodel::Rig` for the held weapon, ticked
//! after each `cv_mw2_step`; the host fetches the rig's mesh once per
//! `mesh_id` and its bone matrices every frame (`cv_vm_*`).
use codv_engine::Mw2Player;
use codv_engine::arms::GunFrame;
use codv_engine::viewmodel::{self as v, Mesh, PlacementFacts, Rig, Timers};
use codv_mw2data::table::{Row, Table};
use codv_mw2data::vm;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use xmodel_runtime::AnimClip;

/// Meshes kept for the host (the loadout, the frag, the knife and a few past guns).
const MESHES: usize = 8;

/// The imported files, decoded once and shared with the prewarm thread
/// (decoding a weapon's models and animations takes a few hundred ms: done
/// on the game thread it is a visible hitch).
struct Disk {
    dir: PathBuf,
    rows: HashMap<String, Row>,
    models: Mutex<HashMap<String, Arc<vm::Model>>>,
    clips: Mutex<HashMap<String, Option<Arc<AnimClip>>>>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Disk {
    // The locks are held only to look up and insert, never while reading or decoding.
    fn model(&self, name: &str) -> Result<Arc<vm::Model>, String> {
        let key = name.to_ascii_lowercase();
        if let Some(m) = locked(&self.models).get(&key) {
            return Ok(Arc::clone(m));
        }
        let path = self.dir.join("models").join(format!("{}.cvm", vm::file_stem(&key)));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let m = Arc::new(vm::read_model(&bytes).map_err(|e| format!("{}: {e}", path.display()))?);
        Ok(Arc::clone(locked(&self.models).entry(key).or_insert(m)))
    }

    fn clip(&self, name: &str) -> Option<Arc<AnimClip>> {
        let key = name.to_ascii_lowercase();
        if let Some(c) = locked(&self.clips).get(&key) {
            return c.clone();
        }
        let path = self.dir.join("anims").join(format!("{}.cva", vm::file_stem(&key)));
        let c =
            std::fs::read(&path).ok().and_then(|b| vm::read_anim(&b).ok()).and_then(|a| v::clip(&a).ok()).map(Arc::new);
        locked(&self.clips).entry(key).or_insert(c).clone()
    }

    /// Decodes everything a weapon's rig needs (errors show when it is built).
    fn warm(&self, weapon: &str) {
        let Some(row) = self.rows.get(weapon) else { return };
        let _ = self.model(row.s("hand_model"));
        let _ = self.model(row.s("gun_model"));
        if !row.s("rocket_model").is_empty() {
            let _ = self.model(row.s("rocket_model"));
        }
        for s in 0..v::SLOTS {
            let n = row.s(&format!("xanim.{s}"));
            if !n.is_empty() {
                self.clip(n);
            }
        }
    }
}

/// The decoded files, for another thread (the host's image prewarm).
#[derive(Clone)]
pub struct Shared(Arc<Disk>);

impl Shared {
    /// The image file stems (as `CvVmDraw` names them) these weapons' view
    /// models draw with, as "c:stem" (colour map) or "n:stem" (normal map);
    /// decodes the models it has not yet.
    pub fn images(&self, weapons: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |kind: &str, name: &str| {
            let stem = vm::file_stem(&name.to_ascii_lowercase());
            let entry = format!("{kind}:{stem}");
            if !stem.is_empty() && !out.contains(&entry) {
                out.push(entry);
            }
        };
        for w in weapons {
            let Some(row) = self.0.rows.get(w) else { continue };
            for model in [row.s("hand_model"), row.s("gun_model"), row.s("rocket_model")] {
                if model.is_empty() {
                    continue;
                }
                let Ok(m) = self.0.model(model) else { continue };
                for s in &m.surfaces {
                    add("c", &s.material.color_map);
                    if !s.material.normal_map.starts_with('$') {
                        add("n", &s.material.normal_map);
                    }
                }
            }
        }
        out
    }
}

pub struct Viewmodels {
    disk: Arc<Disk>,
    rig: Option<Rig>,
    /// Weapon ids whose rig could not be built (logged once).
    failed: HashSet<u32>,
    mesh_id: u32,
    /// The meshes handed out, at most `MESHES` (the host fetches each id once).
    meshes: Vec<(u32, Arc<Mesh>)>,
    /// Each weapon keeps its mesh id: switching back to a gun reuses the mesh
    /// the host already holds instead of fetching and uploading it again.
    weapon_mesh: HashMap<u32, u32>,
    next_mesh_id: u32,
    bones: Vec<[f32; 12]>,
}

impl Viewmodels {
    /// None when the setup imported no viewmodels.
    pub fn load(root: Option<&Path>, table: &Table, log: &dyn Fn(&str)) -> Option<Viewmodels> {
        let dir = root?.join("mw2").join("viewmodels");
        if !dir.join("models").is_dir() {
            log("mw2: no imported viewmodels (re-run INSTALL.bat); GTA's weapon models stay");
            return None;
        }
        let rows = table
            .weapons
            .iter()
            .filter(|r| !r.s("gun_model").is_empty())
            .map(|r| (r.name.clone(), r.clone()))
            .collect::<HashMap<_, _>>();
        log(&format!("mw2: viewmodels for {} weapons in {}", rows.len(), dir.display()));
        Some(Viewmodels {
            disk: Arc::new(Disk { dir, rows, models: Mutex::default(), clips: Mutex::default() }),
            rig: None,
            failed: HashSet::new(),
            mesh_id: 0,
            meshes: Vec::new(),
            weapon_mesh: HashMap::new(),
            next_mesh_id: 0,
            bones: Vec::new(),
        })
    }

    pub fn shared(&self) -> Shared {
        Shared(Arc::clone(&self.disk))
    }

    /// Decodes these weapons' models and animations on a background thread,
    /// so the rig is built from memory when one is first held.
    pub fn prewarm(&self, weapons: Vec<String>) {
        let disk = Arc::clone(&self.disk);
        let _ = std::thread::Builder::new().name("codv-vm-prewarm".into()).spawn(move || {
            for w in weapons {
                disk.warm(&w);
            }
        });
    }

    fn build(&self, id: u32, name: &str) -> Result<Rig, String> {
        let disk = &self.disk;
        let row = disk.rows.get(name).ok_or_else(|| format!("{name}: no viewmodel was imported"))?;
        let hands = disk.model(row.s("hand_model"))?;
        let gun = disk.model(row.s("gun_model"))?;
        let clips = (0..v::SLOTS)
            .map(|s| {
                let n = row.s(&format!("xanim.{s}")).to_owned();
                if n.is_empty() { None } else { disk.clip(&n) }
            })
            .collect();
        let hide: Vec<String> = row.s("hide_tags").split(',').filter(|t| !t.is_empty()).map(str::to_owned).collect();
        // The rocket on a loaded launcher (the RPG-7's), when the setup imported it.
        let rocket = match row.s("rocket_model") {
            "" => None,
            n => disk.model(n).ok(),
        };
        Rig::build_with_rocket(
            id,
            &hands,
            &gun,
            rocket.as_deref(),
            &hide,
            clips,
            Timers::from_row(row),
            PlacementFacts::from_row(row),
        )
    }

    /// After the weapon's tick: (re)build the rig for the held weapon, then
    /// animate and place it.
    pub fn tick(&mut self, id: u32, name: &str, player: &Mw2Player, frame: &GunFrame, dt: f32, log: &dyn Fn(&str)) {
        if id == 0 {
            self.rig = None;
            return;
        }
        if self.rig.as_ref().is_none_or(|r| r.weapon != id) {
            self.rig = None;
            if self.failed.contains(&id) {
                return;
            }
            match self.build(id, name) {
                Ok(rig) => {
                    let known = self.weapon_mesh.get(&id).copied().filter(|m| self.meshes.iter().any(|(k, _)| k == m));
                    if let Some(m) = known {
                        self.mesh_id = m;
                        self.rig = Some(rig);
                    } else {
                        self.next_mesh_id = self.next_mesh_id.wrapping_add(1).max(1);
                        self.mesh_id = self.next_mesh_id;
                        // The host may still be drawing an older mesh: keep a few, never the current one.
                        if self.meshes.len() >= MESHES {
                            self.meshes.remove(0);
                        }
                        self.meshes.push((self.mesh_id, Arc::clone(rig.mesh())));
                        self.weapon_mesh.insert(id, self.mesh_id);
                        log(&format!(
                            "mw2: viewmodel {name}: {} bones, {} triangles, {} draws",
                            rig.bone_count(),
                            rig.mesh().indices.len() / 3,
                            rig.mesh().draws.len()
                        ));
                        self.rig = Some(rig);
                    }
                }
                Err(e) => {
                    log(&format!("mw2: no viewmodel for {name}: {e}"));
                    self.failed.insert(id);
                    return;
                }
            }
        }
        if let Some(rig) = self.rig.as_mut() {
            rig.tick(player, frame, dt);
            self.bones = rig.skin();
        }
    }

    /// The notetracks the view model crossed since the last call (F5a sounds).
    pub fn take_notes(&mut self) -> Vec<String> {
        self.rig.as_mut().map(Rig::take_notes).unwrap_or_default()
    }

    /// Hide until the next tick (the mode stopped).
    pub fn reset(&mut self) {
        self.rig = None;
        self.bones.clear();
    }

    /// (mesh id, bones, ADS fraction, ADS zoom FOV) of the last tick.
    pub fn frame(&self) -> Option<(u32, &[[f32; 12]], f32, f32)> {
        let rig = self.rig.as_ref()?;
        let (fov, ads) = rig.zoom();
        (!self.bones.is_empty()).then_some((self.mesh_id, self.bones.as_slice(), ads, fov))
    }

    pub fn mesh(&self, id: u32) -> Option<&Arc<Mesh>> {
        self.meshes.iter().find(|(m, _)| *m == id).map(|(_, mesh)| mesh)
    }
}
