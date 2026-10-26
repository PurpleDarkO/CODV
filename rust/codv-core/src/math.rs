//! Minimal vector maths in GTA's conventions.
use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
    pub fn flat(self) -> Vec3 {
        Vec3::new(self.x, self.y, 0.0)
    }
    pub fn flat_length(self) -> f32 {
        self.flat().length()
    }
    /// Unit vector, or zero for a (near) zero vector.
    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 1e-5 { self * (1.0 / l) } else { Vec3::ZERO }
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

/// GTA heading (degrees) -> unit forward vector on the ground plane.
pub fn forward(heading_degrees: f32) -> Vec3 {
    let h = heading_degrees.to_radians();
    Vec3::new(-h.sin(), h.cos(), 0.0)
}

/// Direction on the ground plane -> GTA heading in [0, 360).
pub fn heading_of(dir: Vec3) -> f32 {
    let h = (-dir.x).atan2(dir.y).to_degrees();
    if h < 0.0 { h + 360.0 } else { h }
}

/// Smallest signed difference b - a in degrees, in (-180, 180].
pub fn angle_delta(a: f32, b: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// Camera-relative stick (x right, y forward, -1..1) -> world direction on the
/// ground plane, zero inside the dead zone.
pub fn stick_world(stick: [f32; 2], camera_heading: f32, dead_zone: f32) -> Vec3 {
    let mag = (stick[0] * stick[0] + stick[1] * stick[1]).sqrt();
    if mag < dead_zone {
        return Vec3::ZERO;
    }
    let fwd = forward(camera_heading);
    let right = Vec3::new(fwd.y, -fwd.x, 0.0);
    (fwd * stick[1] + right * stick[0]).normalized()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_round_trip() {
        for h in [0.0f32, 45.0, 90.0, 180.0, 270.0, 359.0] {
            let back = heading_of(forward(h));
            assert!(angle_delta(h, back).abs() < 1e-3, "{h} -> {back}");
        }
        // GTA: heading 90 faces -X (west).
        let w = forward(90.0);
        assert!((w.x + 1.0).abs() < 1e-5 && w.y.abs() < 1e-5);
    }

    #[test]
    fn deltas_wrap() {
        assert_eq!(angle_delta(350.0, 10.0), 20.0);
        assert_eq!(angle_delta(10.0, 350.0), -20.0);
    }

    #[test]
    fn stick_is_camera_relative() {
        // Camera faces west (90): pushing right faces north (+Y).
        let d = stick_world([1.0, 0.0], 90.0, 0.2);
        assert!((d.y - 1.0).abs() < 1e-4, "{d:?}");
        assert_eq!(stick_world([0.05, 0.05], 0.0, 0.2), Vec3::ZERO);
    }
}
