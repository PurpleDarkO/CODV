//! CODV's bridge to iw4L's Modern Warfare 2 engine code.
//!
//! The movement here is not CODV's: `player::Mw2Player::step` builds a
//! retail `usercmd_s` and calls iw4L's `movement_iw4::pmove`, the same
//! function iw4L's own simulation runs for every player. CODV supplies only
//! what pmove asks of a host: the world to collide with (`world::World`, in
//! IW4 units), the per-frame command, and the spawn state iw4L's simulation
//! gives a player.
//!
//! GTA's streets reach pmove as MW2 collision meshes (`mesh::MeshWorld`,
//! traced with the retail capsule-vs-mesh walk of `clipmap_iw4`).
//!
//! Units: IW4 inches inside; `units` converts to and from GTA's metres and
//! headings at the host's edge.

pub mod arms;
pub mod body;
pub mod grenade;
pub mod mesh;
pub mod player;
pub mod units;
pub mod viewmodel;
pub mod world;

pub use mesh::MeshWorld;
pub use player::{MoveInput, MoveOutput, Mw2Player, Stance};
pub use world::{BoxWorld, World};
