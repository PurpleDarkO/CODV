//! GTA V's collision as triangles, read live from the game's physics level.
//!
//! Reused from SK8V (Apache-2.0, Copyright 2026 becaring) at the commit
//! pinned in `upstreams.lock.json`:
//! - `live.rs` is SK8V's `skatev-runtime/src/live.rs`, which SK8V adapted for
//!   Legacy from Sol4ra's LS-Skate-LiveCollision (credit: Sol4ra);
//! - `bounds.rs` and `clean.rs` are SK8V's `svwc` decoding and clean-up rules.
//!
//! CODV changes: no bevy (a small [`vec3::Vec3`] instead of `bevy_math`), no
//! `rustc-hash` (std `HashMap`), the cache file format left out, and the
//! memory reader also works on Linux (`/proc/self/mem`) so the tests run off
//! Windows. The flag rule keeps what a GTA ped collides with, which is what a
//! soldier on foot collides with too.

pub mod bounds;
pub mod clean;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
pub mod live;
pub mod vec3;

/// One triangle in GTA world space (metres, Z up) and its GTA material id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tri {
    pub v: [[f32; 3]; 3],
    pub material: u8,
}
