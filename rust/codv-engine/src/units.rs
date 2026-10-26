//! IW4 <-> GTA conversions. Both are Z-up. IW4 measures inches and its yaw 0
//! looks along +X; GTA measures metres and its heading 0 looks along +Y. Both
//! turn counter-clockwise for a growing angle. IW4 pitch grows looking down,
//! GTA's camera pitch grows looking up.

pub const METRES_PER_UNIT: f32 = 0.0254;

pub fn to_metres(v: [f32; 3]) -> [f32; 3] {
    v.map(|c| c * METRES_PER_UNIT)
}

pub fn to_units(v: [f32; 3]) -> [f32; 3] {
    v.map(|c| c / METRES_PER_UNIT)
}

/// GTA heading (degrees, 0 = +Y) for an IW4 yaw (degrees, 0 = +X).
pub fn heading_from_yaw(yaw: f32) -> f32 {
    wrap360(yaw - 90.0)
}

pub fn yaw_from_heading(heading: f32) -> f32 {
    wrap180(heading + 90.0)
}

/// GTA camera pitch (up positive) for an IW4 view pitch (down positive).
pub fn gta_pitch(iw4_pitch: f32) -> f32 {
    -iw4_pitch
}

pub fn wrap360(a: f32) -> f32 {
    let r = a % 360.0;
    if r < 0.0 { r + 360.0 } else { r }
}

pub fn wrap180(a: f32) -> f32 {
    let r = wrap360(a);
    if r > 180.0 { r - 360.0 } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let v = [100.0, -40.0, 72.0];
        let back = to_units(to_metres(v));
        for i in 0..3 {
            assert!((back[i] - v[i]).abs() < 1e-3);
        }
        assert!((to_metres([39.37, 0.0, 0.0])[0] - 1.0).abs() < 1e-3, "39.37 inches to the metre");
        // IW4 yaw 90 looks along +Y, which is GTA heading 0; yaw 0 (+X) is heading 270.
        assert_eq!(heading_from_yaw(90.0), 0.0);
        assert_eq!(heading_from_yaw(0.0), 270.0);
        assert_eq!(yaw_from_heading(0.0), 90.0);
        assert_eq!(yaw_from_heading(270.0), 0.0);
        assert_eq!(gta_pitch(10.0), -10.0);
    }
}
