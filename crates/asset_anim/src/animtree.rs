use std::sync::Arc;

use anim_iw4::ANIMFLAG_ADDITIVE;
use bevy::prelude::{Component, Resource};

use crate::AnimClip;
use crate::dobj::{
    XAnimNodeDefinition, XAnimNodeId, XAnimNodeKind, XAnimTreeDefinition, XAnimTreeError,
};

pub const MULTIPLAYER_ANIMTREE_PATH: &str = "animtrees/multiplayer.atr";
pub const PLAYERANIM_SCRIPT_PATH: &str = "mp/playeranim.script";

pub const PLAYERANIM_TYPES_PATH: &str = "mp/playeranimtypes.txt";

#[derive(Clone, Debug, Default, Resource)]
pub struct PlayerAnimSources {
    family: Option<asset_core::FamilyId>,
    multiplayer_atr: Option<Vec<u8>>,
    playeranim_script: Option<Vec<u8>>,
    playeranim_types: Option<Vec<u8>>,
    decode_errors: Vec<&'static str>,
    compiled: Option<Result<Arc<CompiledAnimTreeDefinition>, AtrCompileError>>,
    parsed_script: Option<
        Result<
            Arc<crate::playeranim_parse::ParsedPlayerAnimScript>,
            crate::playeranim_parse::PlayerAnimParseError,
        >,
    >,
    leaf_binds: Option<PlayerAnimLeafBinds>,
}

impl PlayerAnimSources {
    pub fn family(&self) -> Option<asset_core::FamilyId> {
        self.family
    }
    pub fn capture(
        &mut self,
        family: asset_core::FamilyId,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) {
        if ![
            MULTIPLAYER_ANIMTREE_PATH,
            PLAYERANIM_SCRIPT_PATH,
            PLAYERANIM_TYPES_PATH,
        ]
        .contains(&name)
        {
            return;
        }
        if self.family.is_some_and(|owner| owner != family) {
            self.decode_errors
                .push("character animation source family mismatch");
            return;
        }
        self.family = Some(family);
        let (target, path) = match name {
            MULTIPLAYER_ANIMTREE_PATH => (&mut self.multiplayer_atr, MULTIPLAYER_ANIMTREE_PATH),
            PLAYERANIM_SCRIPT_PATH => (&mut self.playeranim_script, PLAYERANIM_SCRIPT_PATH),
            PLAYERANIM_TYPES_PATH => (&mut self.playeranim_types, PLAYERANIM_TYPES_PATH),
            _ => return,
        };
        let mut bytes = if zlib_compressed {
            match asset_transport::inflate_zlib(data) {
                Ok(bytes) => bytes,
                Err(_) => {
                    self.decode_errors.push(path);
                    return;
                }
            }
        } else {
            data.to_vec()
        };
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        *target = Some(bytes);
    }

    pub fn multiplayer_atr(&self) -> Option<&[u8]> {
        self.multiplayer_atr.as_deref()
    }

    pub fn playeranim_script(&self) -> Option<&[u8]> {
        self.playeranim_script.as_deref()
    }

    pub fn playeranim_types(&self) -> Option<&[u8]> {
        self.playeranim_types.as_deref()
    }

    pub fn decode_errors(&self) -> &[&'static str] {
        &self.decode_errors
    }

    pub fn compile(&mut self) {
        if self.compiled.is_some() {
            return;
        }
        if self.multiplayer_atr.is_none() || self.playeranim_script.is_none() {
            return;
        }
        self.compiled = Some(crate::atr_compile::compile_multiplayer(
            self.multiplayer_atr.as_deref(),
            self.playeranim_script.as_deref(),
        ));
        if let Some(Ok(tree)) = self.compiled.as_ref() {
            if let Some(script) = self.playeranim_script.as_deref() {
                self.parsed_script = Some(crate::playeranim_parse::parse_player_anim_script(
                    script, tree,
                ));
            }
        }
    }

    pub fn compiled(&self) -> Option<&Result<Arc<CompiledAnimTreeDefinition>, AtrCompileError>> {
        self.compiled.as_ref()
    }

    pub fn parsed_script(
        &self,
    ) -> Option<
        &Result<
            Arc<crate::playeranim_parse::ParsedPlayerAnimScript>,
            crate::playeranim_parse::PlayerAnimParseError,
        >,
    > {
        self.parsed_script.as_ref()
    }

    pub fn compile_report_line(&self) -> String {
        match self.compiled() {
            Some(Ok(tree)) => format!(
                "multiplayer.atr compiled: nodes={} leaves={} ignored={} script_names={} legs={} torso={} turning={}",
                tree.node_count(),
                tree.leaf_count(),
                tree.ignored(),
                tree.script_names(),
                tree.index_of("legs")
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "NULL".into()),
                tree.index_of("torso")
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "NULL".into()),
                tree.index_of("turning")
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "NULL".into()),
            ),
            Some(Err(error)) => format!("multiplayer.atr compile failed: {error}"),
            None => "multiplayer.atr not compiled (ATR or script absent)".into(),
        }
    }

    pub fn parse_report_line(&self) -> String {
        match self.parsed_script() {
            Some(Ok(script)) => script.report_line(),
            Some(Err(error)) => format!("playeranim.script parse failed: {error}"),
            None => "playeranim.script not parsed".into(),
        }
    }

    pub fn bind_leaves(&mut self, catalog: &crate::XAnimCatalog) {
        if self.leaf_binds.is_some() {
            return;
        }
        let Some(Ok(tree)) = self.compiled.as_ref() else {
            return;
        };
        let Some(family) = self.family else {
            return;
        };
        let leaves: Vec<(usize, String)> = tree
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| node.child_count == 0)
            .map(|(index, node)| (index, node.name.clone()))
            .collect();
        let node_count = tree.node_count();
        let mut bound = vec![false; node_count];
        let mut bound_leaves = 0usize;
        let mut missing_leaves = 0usize;
        let mut first_missing = None;
        for (index, name) in leaves {
            if catalog.get(family, &name).is_some() {
                bound[index] = true;
                bound_leaves += 1;
            } else {
                missing_leaves += 1;
                if first_missing.is_none() {
                    first_missing = Some(name);
                }
            }
        }
        self.leaf_binds = Some(PlayerAnimLeafBinds {
            bound,
            bound_leaves,
            missing_leaves,
            first_missing,
        });
    }

    pub fn leaf_binds(&self) -> Option<&PlayerAnimLeafBinds> {
        self.leaf_binds.as_ref()
    }

    pub fn bind_report_line(&self) -> String {
        match self.leaf_binds() {
            Some(binds) => binds.report_line(),
            None => "XAnim leaf bind not run".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerAnimLeafBinds {
    bound: Vec<bool>,
    pub bound_leaves: usize,
    pub missing_leaves: usize,
    pub first_missing: Option<String>,
}

impl PlayerAnimLeafBinds {
    pub fn is_bound(&self, index: u16) -> bool {
        self.bound.get(index as usize).copied().unwrap_or(false)
    }

    pub fn report_line(&self) -> String {
        match self.first_missing.as_deref() {
            Some(name) => format!(
                "XAnimCreate leaf bind: bound={} missing={} first_missing={name}",
                self.bound_leaves, self.missing_leaves
            ),
            None => format!(
                "XAnimCreate leaf bind: bound={} missing=0",
                self.bound_leaves
            ),
        }
    }
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
        Self {
            name: String::new(),
            parent: None,
            flags: 0,
            child_count: 0,
            first_child: 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct CompiledAnimTreeDefinition {
    nodes: Vec<CompiledAnimNode>,
    ignored: usize,
    script_names: usize,
}

impl CompiledAnimTreeDefinition {
    pub(crate) fn from_nodes(
        nodes: Vec<CompiledAnimNode>,
        ignored: usize,
        script_names: usize,
    ) -> Self {
        Self {
            nodes,
            ignored,
            script_names,
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn nodes(&self) -> &[CompiledAnimNode] {
        &self.nodes
    }

    pub fn leaf_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.child_count == 0)
            .count()
    }

    pub fn ignored(&self) -> usize {
        self.ignored
    }

    pub fn script_names(&self) -> usize {
        self.script_names
    }

    pub fn index_of(&self, name: &str) -> Option<u16> {
        self.nodes
            .iter()
            .position(|node| node.name == name)
            .map(|index| index as u16)
    }

    pub fn node(&self, index: u16) -> Option<&CompiledAnimNode> {
        self.nodes.get(index as usize)
    }

    pub fn to_runtime_definition(
        &self,
        mut leaf_clip: impl FnMut(u16, &str) -> Option<Arc<AnimClip>>,
    ) -> Result<Arc<XAnimTreeDefinition>, XAnimTreeError> {
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            let parent = node.parent.map(XAnimNodeId);
            let kind = if node.child_count == 0 {
                match leaf_clip(index as u16, &node.name) {
                    Some(clip) => XAnimNodeKind::Leaf { clip, parts: None },
                    None => XAnimNodeKind::Blend,
                }
            } else if node.flags & ANIMFLAG_ADDITIVE != 0 {
                XAnimNodeKind::Additive
            } else {
                XAnimNodeKind::Blend
            };
            nodes.push(XAnimNodeDefinition { parent, kind });
        }
        Ok(Arc::new(XAnimTreeDefinition::new(nodes)?))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimTreeDefinitionError {
    MissingMultiplayerAtr { path: &'static str },
}

impl core::fmt::Display for AnimTreeDefinitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingMultiplayerAtr { path } => {
                write!(f, "required multiplayer animtree is missing: {path}")
            }
        }
    }
}

impl std::error::Error for AnimTreeDefinitionError {}

#[derive(Component, Debug)]
pub struct DObjAnimTreeRuntime {
    definition: Arc<CompiledAnimTreeDefinition>,
    nodes: Vec<AnimTreeNodeRuntime>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimTreeNodeRuntime {
    pub time: f32,
    pub rate: f32,
    pub weight: f32,
    pub goal_weight: f32,
}

impl DObjAnimTreeRuntime {
    pub fn definition(&self) -> &Arc<CompiledAnimTreeDefinition> {
        &self.definition
    }

    pub fn nodes(&self) -> &[AnimTreeNodeRuntime] {
        &self.nodes
    }
}
