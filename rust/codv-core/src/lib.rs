//! CODV core: Call of Duty-style weapon handling and movement rules for the
//! GTA V player. Pure logic over plain data: no GTA, no FFI, no clock, no
//! third-party crates. The runtime crate (CodVRuntime.dll) wraps it in a C ABI.
//!
//! Units: metres, seconds, degrees. Original code: vladtrc/iw4L was read as a
//! behaviour reference only (NOTICE).

pub mod defs;
pub mod json;
pub mod math;
pub mod player;

pub use defs::{FireMode, Table, WeaponDef};
pub use player::{FrameIn, FrameOut, Hit, Player, Shot, State};
