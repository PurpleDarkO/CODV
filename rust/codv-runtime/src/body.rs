//! MW2's third-person character behind the ABI (F4b): the imported
//! character animation (`<data>/mw2/body/`), one `codv_engine::body::Body`
//! ticked after each `cv_mw2_step`, and the retarget onto the GTA ped's
//! skeleton the host reports (`cv_body_set_skeleton`); the host reads the
//! GTA bone matrices every frame (`cv_body_pose`) and writes them into the
//! ped's skeleton (host/src/pose_hook).
use codv_core::json::{self, Value};
use codv_engine::Mw2Player;
use codv_engine::arms::GunFrame;
use codv_engine::body::retarget::{GtaBone, Retarget};
use codv_engine::body::{Body, GunAnim, Sources, event};
use glam::{Quat, Vec3};
use std::path::Path;
use std::sync::Arc;

pub struct Character {
    src: Arc<Sources>,
    body: Option<Body>,
    retarget: Option<Retarget>,
    /// GTA object matrices of the last tick, 16 floats each (RAGE rows:
    /// X axis, Y axis, Z axis, translation).
    pose: Vec<[f32; 16]>,
    melee_event: u8,
    error_logged: bool,
}

/// GTA ped skeleton JSON (host probe::SkeletonJson): bones with name,
/// parent, local t / r (xyzw) / s.
pub fn parse_skeleton(text: &str) -> Result<Vec<GtaBone>, String> {
    let v = json::parse(text)?;
    let bones = v.get("bones").and_then(Value::as_array).ok_or("skeleton JSON has no bones")?;
    let f = |b: &Value, key: &str, n: usize, default: f32| -> Vec<f32> {
        let a = b.get(key).and_then(Value::as_array);
        (0..n).map(|i| a.and_then(|a| a.get(i)).and_then(Value::as_f64).map_or(default, |x| x as f32)).collect()
    };
    bones
        .iter()
        .map(|b| {
            let (t, r, s) = (f(b, "t", 3, 0.0), f(b, "r", 4, 0.0), f(b, "s", 3, 1.0));
            let q = Quat::from_xyzw(r[0], r[1], r[2], if r.iter().all(|x| *x == 0.0) { 1.0 } else { r[3] });
            Ok(GtaBone {
                name: b.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                parent: b.get("parent").and_then(Value::as_f64).filter(|p| *p >= 0.0).map(|p| p as usize),
                t: Vec3::new(t[0], t[1], t[2]),
                r: if q.length_squared() > 1e-8 { q.normalize() } else { Quat::IDENTITY },
                s: Vec3::new(s[0], s[1], s[2]),
            })
        })
        .collect()
}

impl Character {
    /// None (logged) when the setup imported no character animation.
    pub fn load(root: Option<&Path>, log: &dyn Fn(&str)) -> Option<Character> {
        let dir = root?.join("mw2").join("body");
        if !dir.join("skeleton.cvm").is_file() {
            log("mw2: no imported character animation (re-run INSTALL.bat); the GTA ped keeps GTA's animation");
            return None;
        }
        match Sources::load(&dir) {
            Ok(src) => {
                log(&format!(
                    "mw2: character {}: {} animations, {} script slots, {} events",
                    src.skeleton.name,
                    src.anim_count(),
                    src.script.slots.len(),
                    src.script.events.len()
                ));
                let melee_event = if src.script.event(event::KNIFE_MELEE).is_some() {
                    event::KNIFE_MELEE
                } else {
                    event::MELEEATTACK
                };
                Some(Character {
                    src: Arc::new(src),
                    body: None,
                    retarget: None,
                    pose: Vec::new(),
                    melee_event,
                    error_logged: false,
                })
            }
            Err(e) => {
                log(&format!("mw2: character animation not loaded: {e}"));
                None
            }
        }
    }

    /// The host's GTA ped skeleton (`feet_z`: the ground in its object space).
    pub fn set_skeleton(&mut self, text: &str, feet_z: f32) -> Result<usize, String> {
        let bones = parse_skeleton(text)?;
        let bind = Body::new(Arc::clone(&self.src)).bind_pose();
        let rt = Retarget::new(&bones, &self.src.skeleton.bone_names, &bind, feet_z)?;
        let n = bones.len();
        self.retarget = Some(rt);
        self.pose.clear();
        Ok(n)
    }

    /// After pmove and the weapon's tick.
    pub fn tick(
        &mut self,
        player: &mut Mw2Player,
        gun: Option<GunAnim>,
        frame: Option<&GunFrame>,
        dt: f32,
        log: &dyn Fn(&str),
    ) {
        let src = Arc::clone(&self.src);
        let body = self.body.get_or_insert_with(|| Body::new(src));
        let mut events = Vec::new();
        if let Some(f) = frame {
            if f.putaway_started {
                events.push(event::DROPWEAPON);
            }
            if f.raise_started {
                events.push(event::RAISEWEAPON);
            }
            if f.fired {
                events.push(event::FIREWEAPON);
            }
            if f.reload_started {
                events.push(event::RELOAD);
            }
            if f.melee_started {
                events.push(self.melee_event);
            }
        }
        body.tick(player, gun, &events, dt);
        if let Some(e) = &body.error
            && !self.error_logged
        {
            log(&format!("mw2: character animation: {e}"));
            self.error_logged = true;
        }
        if let Some(rt) = &self.retarget {
            match rt.pose(&body.pose(player)) {
                Some(world) => self.pose = world.iter().map(|m| m.to_cols_array()).collect(),
                None => self.pose.clear(),
            }
        }
    }

    pub fn pose(&self) -> &[[f32; 16]] {
        &self.pose
    }

    /// Legs and torso animation names (status line).
    pub fn playing(&self) -> (String, String) {
        self.body.as_ref().map_or_else(Default::default, |b| {
            let (l, t) = b.playing();
            (l.to_owned(), t.to_owned())
        })
    }

    /// A new MW2 player (spawn): animation starts over.
    pub fn reset(&mut self) {
        self.body = None;
        self.pose.clear();
    }
}
