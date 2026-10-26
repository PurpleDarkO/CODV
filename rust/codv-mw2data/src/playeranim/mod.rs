//! MW2's multiplayer character animation sources: the animation tree
//! (`animtrees/multiplayer.atr`) compiled to the nodes the player script
//! names, and the player animation script (`mp/playeranim.script`): which
//! legs and torso animation each move type, stance, weapon and event plays.
//!
//! Ported from iw4L (Apache-2.0, NOTICE): `asset_anim::animtree`,
//! `asset_anim::atr_compile` and `asset_anim::playeranim_parse`, without
//! their Bevy resources and with the T5 / T6 branches left out (CODV reads
//! IW4 only). The text formats are Infinity Ward's; the files come from the
//! player's own MW2 at setup.
mod atr;
mod script;

pub use atr::{compile_multiplayer, compile_selected_multiplayer};
pub use script::{
    ParsedAnimCommand, ParsedAnimCondition, ParsedAnimItem, ParsedPlayerAnimScript, PlayerAnimParseError,
    PlayerAnimProperties, parse_player_anim_script,
};

pub const MULTIPLAYER_ANIMTREE_PATH: &str = "animtrees/multiplayer.atr";
pub const PLAYERANIM_SCRIPT_PATH: &str = "mp/playeranim.script";

/// The tree and the script, compiled and parsed as iw4L does for IW4.
pub fn compile(
    atr: &[u8],
    script: &[u8],
) -> Result<(std::sync::Arc<CompiledAnimTreeDefinition>, std::sync::Arc<ParsedPlayerAnimScript>), String> {
    let tree = compile_multiplayer(Some(atr), Some(script)).map_err(|e| format!("multiplayer.atr: {e}"))?;
    let parsed = parse_player_anim_script(script, &tree).map_err(|e| format!("playeranim.script: {e}"))?;
    Ok((tree, parsed))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledAnimNode {
    pub name: String,
    pub parent: Option<u16>,
    pub flags: u16,
    pub child_count: u16,
    pub first_child: u16,
}

impl CompiledAnimNode {
    pub(crate) fn blank() -> Self {
        Self { name: String::new(), parent: None, flags: 0, child_count: 0, first_child: 0 }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct CompiledAnimTreeDefinition {
    nodes: Vec<CompiledAnimNode>,
    ignored: usize,
    script_names: usize,
}

impl CompiledAnimTreeDefinition {
    pub(crate) fn from_nodes(nodes: Vec<CompiledAnimNode>, ignored: usize, script_names: usize) -> Self {
        Self { nodes, ignored, script_names }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn nodes(&self) -> &[CompiledAnimNode] {
        &self.nodes
    }

    pub fn leaf_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.child_count == 0).count()
    }

    pub fn ignored(&self) -> usize {
        self.ignored
    }

    pub fn script_names(&self) -> usize {
        self.script_names
    }

    pub fn index_of(&self, name: &str) -> Option<u16> {
        self.nodes.iter().position(|node| node.name == name).map(|index| index as u16)
    }

    pub fn node(&self, index: u16) -> Option<&CompiledAnimNode> {
        self.nodes.get(index as usize)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtrCompileError {
    MissingSource { path: &'static str },
    BadToken { offset: usize, message: String },
    DuplicateAnimation { name: String },
    EmptyTree,
}

impl core::fmt::Display for AtrCompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingSource { path } => write!(f, "missing {path}"),
            Self::BadToken { offset, message } => {
                write!(f, "{message} at byte {offset}")
            }
            Self::DuplicateAnimation { name } => {
                write!(f, "duplicate animation {name}")
            }
            Self::EmptyTree => write!(f, "anim tree is empty"),
        }
    }
}

impl std::error::Error for AtrCompileError {}
