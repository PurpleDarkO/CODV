//! GTA's static map as the MW2 player's world, the way SK8V streams it for
//! its skater: the host finds GTA's physics level (Sol4ra's reader,
//! host/src/physics_level.cpp) and reports its address; a worker thread here
//! reads the triangles of a square area around the player out of it
//! (`codv_gtaworld::live`, SK8V's reader), turns them into MW2 collision
//! meshes (`codv_engine::MeshWorld`) and swaps the finished area in. The
//! player keeps the old area until the new one is ready.
//!
//! Precision: GTA coordinates reach about 8 km, 315,000 IW4 units, where an
//! f32 is only good to 1/32 unit. Each area is built around its own centre,
//! and the player moves in a frame of its own (`Frame`) that follows it, so
//! pmove never sees large coordinates.
use crate::Log;
use codv_engine::mesh::Triangle;
use codv_engine::units::METRES_PER_UNIT;
use codv_engine::world::trace::Trace;
use codv_engine::{MeshWorld, World};
use codv_gtaworld::live::{Level, Seen, changes_near};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Half the side of an area, metres.
pub const AREA_HALF: f32 = 64.0;
/// A new area is asked for once the player is this far from the centre.
pub const RECENTRE: f64 = 24.0;
/// The player moves in an area only this close to its centre (the area's
/// edge triangles are clipped, and a sweep reaches a few metres).
pub const COVER: f64 = 48.0;
/// How often the worker checks whether GTA streamed collision in or out
/// near the current area.
const RECHECK: Duration = Duration::from_secs(3);

/// One built area: MW2 meshes in IW4 units around `centre` (GTA XY metres;
/// mesh z is GTA z in units).
pub struct Area {
    pub centre: [f64; 2],
    pub mesh: MeshWorld,
    seen: Vec<Seen>,
    /// The table's slot words just before the walk: the incremental check
    /// compares against them (and, after a check that found nothing near,
    /// against the words it read).
    words: Mutex<Vec<u64>>,
    pub summary: String,
    /// Each triangle again with its GTA material and whether it came in as gap filler (GTA's
    /// bullet/camera copy), IW4 units around `centre`: the debug panel names what the player's hull
    /// was found inside of.
    diag: Vec<(Triangle, u8, bool)>,
}

impl Area {
    /// Whether GTA point `p` (metres) is inside the part of the area the
    /// player may move in.
    pub fn covers(&self, p: [f64; 2]) -> bool {
        (p[0] - self.centre[0]).abs() <= COVER && (p[1] - self.centre[1]).abs() <= COVER
    }
}

/// IW4 offset that takes a point from a frame at `from` (metres) into the
/// frame at `to`.
pub fn offset(from: [f64; 2], to: [f64; 2]) -> [f32; 3] {
    [((from[0] - to[0]) / METRES_PER_UNIT as f64) as f32, ((from[1] - to[1]) / METRES_PER_UNIT as f64) as f32, 0.0]
}

/// An area seen from another frame: queries arrive in the frame at `frame`
/// and are moved into the area's.
impl Area {
    /// The triangles a hull at `at` (IW4 units around the area's centre) overlaps, described for the
    /// debug panel: GTA material, gap filler or vegetation, and the triangle's span above the feet.
    pub fn overlaps(&self, at: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Vec<String> {
        let (lo, hi) = ([0, 1, 2].map(|k| at[k] + mins[k] - 2.0), [0, 1, 2].map(|k| at[k] + maxs[k] + 2.0));
        let mut out = Vec::new();
        for (t, material, gap) in &self.diag {
            let near = (0..3).all(|k| {
                t.iter().map(|p| p[k]).fold(f32::MAX, f32::min) <= hi[k]
                    && t.iter().map(|p| p[k]).fold(f32::MIN, f32::max) >= lo[k]
            });
            if !near || codv_engine::world::is_free(&MeshWorld::build(&[*t]), at, mins, maxs, mask) {
                continue;
            }
            let (z0, z1) = t.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[2] - at[2]), b.max(p[2] - at[2])));
            let e1 = [0, 1, 2].map(|k| t[1][k] - t[0][k]);
            let e2 = [0, 1, 2].map(|k| t[2][k] - t[0][k]);
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            let m = codv_engine::units::METRES_PER_UNIT;
            out.push(format!(
                "material {material}{}{} from {:+.2} to {:+.2} m above the feet, slope {:.0} deg",
                if *gap { " (gap filler: GTA's bullet copy)" } else { "" },
                if codv_gtaworld::clean::soft_vegetation(*material) { " (vegetation)" } else { "" },
                z0 * m,
                z1 * m,
                (n[2] / len).abs().acos().to_degrees()
            ));
            if out.len() >= 3 {
                break;
            }
        }
        out
    }
}

pub struct AreaView<'a> {
    pub area: &'a Area,
    shift: [f32; 3],
}

impl<'a> AreaView<'a> {
    pub fn new(area: &'a Area, frame: [f64; 2]) -> AreaView<'a> {
        AreaView { area, shift: offset(frame, area.centre) }
    }

    /// `Area::overlaps` for a hull at `at` in the player's frame.
    pub fn overlaps(&self, at: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Vec<String> {
        let s = self.shift;
        self.area.overlaps([at[0] + s[0], at[1] + s[1], at[2]], mins, maxs, mask)
    }
}

impl World for AreaView<'_> {
    fn trace(&self, start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Trace {
        let s = self.shift;
        let mv = |p: [f32; 3]| [p[0] + s[0], p[1] + s[1], p[2]];
        let mut t = self.area.mesh.trace(mv(start), mv(end), mins, maxs, mask);
        t.endpos = [t.endpos[0] - s[0], t.endpos[1] - s[1], t.endpos[2]];
        t
    }
}

#[derive(Default)]
struct Shared {
    level: Option<Level>,
    /// Where the player is (GTA metres), as the last step or query said.
    focus: Option<[f64; 2]>,
    /// A centre the worker should build an area around.
    want: Option<[f64; 2]>,
    area: Option<Arc<Area>>,
    building: bool,
    stop: bool,
    status: String,
    /// Movers' exact collision, per GTA model (model space, metres).
    templates: HashMap<u32, Template>,
    /// Instances to read a model's collision from.
    template_requests: Vec<TemplateRequest>,
}

/// A model's collision as the worker found it.
#[derive(Clone)]
pub enum Template {
    /// Asked for; the mover uses its model box until it arrives.
    Pending,
    /// GTA's own bound triangles, model space metres.
    Ready(Arc<Vec<codv_gtaworld::Tri>>),
    /// Unreadable or implausible: the model box for good.
    Box,
}

/// Read the collision of `model` from the live instance `instance`; its
/// triangles must fit the model's box (`lo`, `hi`, model space metres).
#[derive(Clone, Copy, Debug)]
pub struct TemplateRequest {
    pub model: u32,
    pub instance: usize,
    pub lo: [f32; 3],
    pub hi: [f32; 3],
}

/// The bound triangles of `r`'s instance, if they are plausible for the
/// model: inside its box grown by 0.5 m (a freed and reused instance reads
/// as garbage, or as another model).
pub fn read_template(r: &TemplateRequest) -> Result<Vec<codv_gtaworld::Tri>, String> {
    let tris = codv_gtaworld::live::instance_model(r.instance)?;
    let margin = 0.5;
    let inside =
        tris.iter().flat_map(|t| t.v).all(|p| (0..3).all(|k| p[k] >= r.lo[k] - margin && p[k] <= r.hi[k] + margin));
    if !inside {
        return Err(format!("{} triangles outside the model box", tris.len()));
    }
    if tris.len() > 20_000 {
        return Err(format!("{} triangles is too many for a mover", tris.len()));
    }
    Ok(tris)
}

/// The worker and what it shares with the game thread.
pub struct Streamer {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// GTA triangles (metres) in IW4 units around `centre`.
fn to_units(tris: &[codv_gtaworld::Tri], centre: [f64; 2]) -> Vec<codv_engine::mesh::Triangle> {
    let k = 1.0 / METRES_PER_UNIT as f64;
    tris.iter()
        .map(|t| {
            t.v.map(|p| {
                [
                    ((p[0] as f64 - centre[0]) * k) as f32,
                    ((p[1] as f64 - centre[1]) * k) as f32,
                    (p[2] as f64 * k) as f32,
                ]
            })
        })
        .collect()
}

/// Reads and builds one area (worker thread).
pub fn build_area(level: &Level, centre: [f64; 2]) -> Result<Area, String> {
    let t0 = Instant::now();
    let words = level.words()?;
    let (tris, census, seen) = level.walk([centre[0] as f32, centre[1] as f32], AREA_HALF)?;
    let read_ms = t0.elapsed().as_millis();
    let units = to_units(&tris, centre);
    let mesh = MeshWorld::build(&units);
    let first_gap = tris.len().saturating_sub(census.filled);
    let diag = units.iter().zip(&tris).enumerate().map(|(i, (u, t))| (*u, t.material, i >= first_gap)).collect();
    let summary = format!(
        "area ({:.0}, {:.0}): {} GTA triangles -> {} MW2 mesh sides in {} tables, read {} ms + built {} ms; {}",
        centre[0],
        centre[1],
        tris.len(),
        mesh.triangle_count(),
        mesh.chunk_count(),
        read_ms,
        t0.elapsed().as_millis() - read_ms,
        census
    );
    Ok(Area { centre, mesh, seen, words: Mutex::new(words), summary, diag })
}

/// Whether GTA streamed static collision in or out near `area` since it was
/// built: only the table slots whose words changed are read (a few dozen
/// reads instead of the whole table). `Some((added, removed))` when it did.
pub fn check_area(level: &Level, area: &Area) -> Result<Option<(usize, usize)>, String> {
    let now = level.words()?;
    let Ok(mut words) = area.words.lock() else { return Ok(None) };
    let changed: Vec<usize> = (0..now.len().max(words.len())).filter(|&i| now.get(i) != words.get(i)).collect();
    if changed.is_empty() {
        return Ok(None);
    }
    let centre = [area.centre[0] as f32, area.centre[1] as f32];
    let ids = level.near_in(&now, &changed, centre, AREA_HALF);
    let slots: HashSet<usize> = changed.iter().copied().collect();
    let (added, removed) = changes_near(&area.seen, &ids, centre, AREA_HALF, Some(&slots));
    if added + removed == 0 {
        // Nothing near changed (peds, cars, far streaming): compare with these next time.
        *words = now;
        return Ok(None);
    }
    Ok(Some((added, removed)))
}

fn work(shared: Arc<(Mutex<Shared>, Condvar)>, log: Arc<Log>) {
    let (lock, cv) = &*shared;
    let mut last_check = Instant::now();
    loop {
        let (level, centre, recheck) = {
            let Ok(mut s) = lock.lock() else { return };
            loop {
                if s.stop {
                    return;
                }
                if !s.template_requests.is_empty() {
                    break;
                }
                let due = s.level.is_some() && s.area.is_some() && last_check.elapsed() >= RECHECK;
                if s.level.is_some() && (s.want.is_some() || due) {
                    break;
                }
                s = match cv.wait_timeout(s, Duration::from_millis(500)) {
                    Ok((g, _)) => g,
                    Err(_) => return,
                };
            }
            // Movers' templates first: a few reads each, and a mover shows as
            // its box until its template is in.
            let requests = std::mem::take(&mut s.template_requests);
            if !requests.is_empty() {
                drop(s);
                let results: Vec<_> = requests.iter().map(|r| (r, read_template(r))).collect();
                let Ok(mut s) = lock.lock() else { return };
                for (r, result) in results {
                    let t = match result {
                        Ok(tris) => {
                            log(&format!("movers: model {:08x}: {} triangles from GTA's bound", r.model, tris.len()));
                            Template::Ready(Arc::new(tris))
                        }
                        Err(e) => {
                            log(&format!("movers: model {:08x}: model box ({e})", r.model));
                            Template::Box
                        }
                    };
                    s.templates.insert(r.model, t);
                }
                continue;
            }
            let level = s.level.unwrap();
            match s.want.take() {
                Some(c) => (level, c, None),
                None => {
                    last_check = Instant::now();
                    let area = s.area.clone().unwrap();
                    (level, s.focus.unwrap_or(area.centre), Some(area))
                }
            }
        };
        if let Some(area) = recheck {
            // Streaming near the current area makes it stale: rebuild where the player is.
            match check_area(&level, &area) {
                Ok(None) => continue,
                Ok(Some((added, removed))) => log(&format!(
                    "collision: GTA streamed collision near ({:.0}, {:.0}): +{added} -{removed} bounds; rebuilding",
                    area.centre[0], area.centre[1]
                )),
                Err(e) => {
                    log(&format!("collision: physics level check failed: {e}"));
                    continue;
                }
            }
        }
        if let Ok(mut s) = lock.lock() {
            s.building = true;
        }
        let built = build_area(&level, centre);
        let Ok(mut s) = lock.lock() else { return };
        s.building = false;
        match built {
            Ok(area) => {
                log(&format!("collision: {}", area.summary));
                s.status =
                    format!("{} MW2 tris around ({:.0}, {:.0})", area.mesh.triangle_count(), centre[0], centre[1]);
                s.area = Some(Arc::new(area));
                last_check = Instant::now();
            }
            Err(e) => {
                log(&format!("collision: area ({:.0}, {:.0}) failed: {e}", centre[0], centre[1]));
                s.status = "physics level unreadable; waiting for the host to find it again".into();
            }
        }
    }
}

impl Streamer {
    pub fn new(log: Arc<Log>) -> Streamer {
        let shared = Arc::new((
            Mutex::new(Shared { status: "waiting for GTA's physics level".into(), ..Default::default() }),
            Condvar::new(),
        ));
        let for_worker = shared.clone();
        let worker =
            std::thread::Builder::new().name("codv-collision".into()).spawn(move || work(for_worker, log)).ok();
        Streamer { shared, worker }
    }

    /// The host found GTA's physics level (`None`: lost it). The current area
    /// stays usable either way: its triangles are already copied.
    pub fn set_level(&self, level: Option<Level>) {
        let (lock, cv) = &*self.shared;
        if let Ok(mut s) = lock.lock() {
            if s.level == level {
                return;
            }
            s.level = level;
            if level.is_some() {
                s.want = s.focus.or(s.area.as_ref().map(|a| a.centre));
                s.status = "physics level found; waiting for the player's position".into();
            } else {
                s.status = "physics level lost; keeping the last area".into();
            }
            cv.notify_all();
        }
    }

    pub fn has_level(&self) -> bool {
        self.shared.0.lock().map(|s| s.level.is_some()).unwrap_or(false)
    }

    /// The player (or a query) is at GTA point `p` (metres): asks for a new
    /// area when there is none or `p` drifted from its centre.
    pub fn focus(&self, p: [f64; 2]) {
        let (lock, cv) = &*self.shared;
        if let Ok(mut s) = lock.lock() {
            s.focus = Some(p);
            if s.level.is_none() {
                return;
            }
            let far = s.area.as_ref().is_none_or(|a| dist(a.centre, p) > RECENTRE);
            let asked = s.want.is_some_and(|w| dist(w, p) <= RECENTRE);
            if far && !asked && !s.building {
                s.want = Some(p);
                cv.notify_all();
            }
        }
    }

    /// A model's template; asks the worker for it (from `request`'s
    /// instance) the first time.
    pub fn template(&self, request: TemplateRequest) -> Template {
        let (lock, cv) = &*self.shared;
        let Ok(mut s) = lock.lock() else { return Template::Box };
        if let Some(t) = s.templates.get(&request.model) {
            return t.clone();
        }
        if request.instance == 0 {
            return Template::Box;
        }
        s.templates.insert(request.model, Template::Pending);
        s.template_requests.push(request);
        cv.notify_all();
        Template::Pending
    }

    pub fn area(&self) -> Option<Arc<Area>> {
        self.shared.0.lock().ok().and_then(|s| s.area.clone())
    }

    pub fn status(&self) -> String {
        self.shared.0.lock().map(|s| s.status.clone()).unwrap_or_default()
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.0.lock() {
            s.stop = true;
        }
        self.shared.1.notify_all();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
