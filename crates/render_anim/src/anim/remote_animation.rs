use super::*;

#[derive(Clone, Debug)]
pub struct PlayerAnimationBinding {
    bodies: Arc<asset_model::BodyMeshCatalog>,
    body_name: String,
    tree: Arc<asset_anim::CompiledAnimTreeDefinition>,
    script: Arc<asset_anim::ParsedPlayerAnimScript>,
    catalog: Arc<asset_anim::XAnimCatalog>,
    rig: Arc<xmodel_runtime::DObj>,
}

impl PlayerAnimationBinding {
    pub fn body(&self) -> &asset_model::BodyMeshEntry {
        self.bodies
            .get(&self.body_name)
            .expect("bound character body")
    }

    fn matches(
        &self,
        bodies: &assets::PreparedBodies,
        body_name: &str,
        tree: &Arc<asset_anim::CompiledAnimTreeDefinition>,
        script: &Arc<asset_anim::ParsedPlayerAnimScript>,
        catalog: &assets::PreparedXAnims,
    ) -> bool {
        Arc::ptr_eq(&self.bodies, &bodies.0)
            && self.body_name == body_name
            && Arc::ptr_eq(&self.tree, tree)
            && Arc::ptr_eq(&self.script, script)
            && Arc::ptr_eq(&self.catalog, &catalog.0)
    }

    fn decode_leaf(&self, index: u16) -> Result<Arc<xmodel_runtime::AnimClip>, String> {
        let leaf = self
            .tree
            .node(index)
            .filter(|node| node.child_count == 0)
            .ok_or_else(|| format!("packed index {index} is not a character animation leaf"))?;
        let body = self.body();
        let clip = self
            .catalog
            .clip(body.namespace, &leaf.name)
            .ok_or_else(|| format!("decode failed for character clip `{}`", leaf.name))?;
        if !self.rig.tracks_for(&clip).iter().any(Option::is_some) {
            return Err(format!(
                "character clip `{}` has no tracks for body `{}`",
                leaf.name, self.body_name
            ));
        }
        Ok(clip)
    }
}

#[derive(Clone, Debug)]
pub struct PersistentRemoteTree {
    binding: Arc<PlayerAnimationBinding>,
    pub(crate) runtime: xmodel_runtime::XAnimTreeRuntime,
    pub(crate) namespace: asset_core::AssetNamespace,
    pub(crate) dobj: Option<std::sync::Arc<xmodel_runtime::DObj>>,
    pub(crate) reuse_key: Option<xmodel_runtime::DObjReuseKey>,
    pub(crate) legs: u16,
    pub(crate) torso: u16,
    pub(crate) legs_restart: bool,
    pub(crate) torso_restart: bool,

    pub(crate) occupation_tr_time: Option<i32>,

    pub(crate) legs_rate_sample: ClientAnimSample,

    pub(crate) torso_rate_sample: ClientAnimSample,
}

pub struct PoseClips {
    pub clip: Arc<xmodel_runtime::AnimClip>,
    pub torso_clip: Option<Arc<xmodel_runtime::AnimClip>>,
    pub legs_for_tree: Arc<xmodel_runtime::AnimClip>,
}

#[derive(Resource, Default)]
pub struct RemoteBodyTrees {
    by_ent: HashMap<u32, PersistentRemoteTree>,
}

impl RemoteBodyTrees {
    pub(crate) fn bind_player(
        &self,
        sources: &asset_anim::PlayerAnimSources,
        bodies: &assets::PreparedBodies,
        catalog: &assets::PreparedXAnims,
        axis: bool,
        persist_key: u32,
    ) -> Result<Arc<PlayerAnimationBinding>, String> {
        let tree = sources
            .compiled()
            .and_then(|t| t.as_ref().ok())
            .ok_or("character tree is not compiled")?;
        let script = sources
            .parsed_script()
            .and_then(|s| s.as_ref().ok())
            .ok_or("character script is not parsed")?;
        let kit = bodies
            .0
            .kits()
            .kit(axis)
            .ok_or("no third-person character kit")?;
        if let Some(slot) = self.by_ent.get(&persist_key)
            && slot
                .binding
                .matches(bodies, &kit.body, tree, script, catalog)
        {
            return Ok(slot.binding.clone());
        }
        let body = bodies.0.get(&kit.body).ok_or("character body missing")?;
        if sources.family() != Some(body.namespace) {
            return Err(format!(
                "{} character requires its own animation tree and script; supplied family {:?}",
                body.namespace.as_str(),
                sources.family()
            ));
        }
        let pose = body
            .skel
            .pose
            .as_ref()
            .ok_or("character body pose missing")?;
        let rig = xmodel_runtime::DObj::build(&[(pose, None)]).map_err(|e| e.to_string())?;
        for name in ["legs", "torso"] {
            let index = tree
                .index_of(name)
                .ok_or_else(|| format!("player tree {name} branch missing"))?;
            if tree.node(index).is_none_or(|n| n.child_count == 0) {
                return Err(format!("player tree {name} is not a branch"));
            }
        }
        Ok(Arc::new(PlayerAnimationBinding {
            bodies: bodies.0.clone(),
            body_name: kit.body.clone(),
            tree: tree.clone(),
            script: script.clone(),
            catalog: catalog.0.clone(),
            rig: Arc::new(rig),
        }))
    }

    pub fn get(&self, persist_key: u32) -> Option<&PersistentRemoteTree> {
        self.by_ent.get(&persist_key)
    }

    pub fn get_mut(&mut self, persist_key: u32) -> Option<&mut PersistentRemoteTree> {
        self.by_ent.get_mut(&persist_key)
    }

    pub fn remove(&mut self, persist_key: u32) -> Option<PersistentRemoteTree> {
        self.by_ent.remove(&persist_key)
    }

    pub fn retain_live(&mut self, live: &HashSet<u32>) {
        self.by_ent.retain(|ent, _| live.contains(ent));
    }
}

pub struct AdvancedRemoteTree {
    pub runtime: xmodel_runtime::XAnimTreeRuntime,
    pub clips: Option<PoseClips>,
    pub reused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinAfterPose {
    Culled,
    ReuseCache,
    Blend,
}

pub fn skin_after_pose(culled: bool, pose_same: bool, has_cache: bool) -> SkinAfterPose {
    if culled {
        SkinAfterPose::Culled
    } else if pose_same && has_cache {
        SkinAfterPose::ReuseCache
    } else {
        SkinAfterPose::Blend
    }
}

pub fn skip_frozen_corpse_dobj(
    is_corpse: bool,
    tree_reused: bool,
    last_was_cache_hit: bool,
    has_cache: bool,
) -> bool {
    is_corpse && tree_reused && last_was_cache_hit && has_cache
}

pub fn clone_corpse_tree_from_victim(
    trees: &mut RemoteBodyTrees,
    corpse_ent: u32,
    victim: u32,
    tr_time: i32,
) {
    if trees
        .by_ent
        .get(&corpse_ent)
        .is_some_and(|slot| slot.occupation_tr_time == Some(tr_time))
    {
        return;
    }
    let Some(src) = trees.by_ent.get(&victim).cloned() else {
        trees.by_ent.remove(&corpse_ent);
        return;
    };
    let mut dst = src;
    dst.dobj = None;
    dst.reuse_key = None;
    dst.occupation_tr_time = Some(tr_time);
    trees.by_ent.insert(corpse_ent, dst);
}

pub fn packed_anim(raw: i32) -> Option<PlayerAnimValue> {
    let value = PlayerAnimValue::from_raw((raw as u16) & PLAYER_ANIM_RAW_MASK)?;
    (value.effective_index() != 0).then_some(value)
}

pub fn zero_anim() -> PlayerAnimValue {
    PlayerAnimValue::from_raw(0).expect("zero fits PLAYER_ANIM_RAW_MASK")
}

pub fn advance_remote_tree(
    binding: &Arc<PlayerAnimationBinding>,
    legs: PlayerAnimValue,
    torso: PlayerAnimValue,
    persist_key: u32,
    dt: f32,
    origin: [f32; 3],
    pose_time_ms: i32,
    trees: &mut RemoteBodyTrees,
) -> Result<AdvancedRemoteTree, String> {
    let tree = &binding.tree;
    let script = &binding.script;
    let body = binding.body();
    let legs_index = legs.effective_index();
    let torso_index = torso.effective_index();
    let legs_restart = legs.restart_toggle();
    let torso_restart = torso.restart_toggle();
    let previous = trees.by_ent.remove(&persist_key);
    let occupation_tr_time = previous.as_ref().and_then(|slot| slot.occupation_tr_time);
    let prev = previous.filter(|slot| Arc::ptr_eq(&slot.binding, binding));
    let prev_restart_changed_legs = prev
        .as_ref()
        .is_some_and(|slot| slot.legs_restart != legs_restart);
    let prev_restart_changed_torso = prev
        .as_ref()
        .is_some_and(|slot| slot.torso_restart != torso_restart);
    let reused = prev.as_ref().is_some_and(|slot| {
        slot.namespace == body.namespace
            && tree_clips_unchanged(
                slot.legs,
                slot.torso,
                slot.legs_restart,
                slot.torso_restart,
                legs_index,
                torso_index,
                legs_restart,
                torso_restart,
            )
    });

    let mut clips = None;
    let mut slot = if reused {
        let mut slot = prev.expect("reuse checked");
        slot.legs = legs_index;
        slot.torso = torso_index;
        slot.legs_restart = legs_restart;
        slot.torso_restart = torso_restart;
        slot
    } else {
        let clip = binding.decode_leaf(legs_index)?;
        let (legs_for_tree, torso_clip) = if torso_index == 0 {
            (Arc::clone(&clip), None)
        } else {
            let torso_clip = binding.decode_leaf(torso_index)?;
            let overlayed = overlay_legs_clip(&clip, &torso_clip);
            (Arc::new(overlayed), Some(torso_clip))
        };
        let old_legs = prev.as_ref().map(|slot| slot.legs).unwrap_or(0);
        let old_torso = prev.as_ref().map(|slot| slot.torso).unwrap_or(0);
        let old_legs_moving = prev
            .as_ref()
            .is_some_and(|slot| slot.legs_rate_sample.move_speed > 0.0);
        let old_torso_moving = prev
            .as_ref()
            .is_some_and(|slot| slot.torso_rate_sample.move_speed > 0.0);
        let mut bound: HashMap<u16, Arc<xmodel_runtime::AnimClip>> = HashMap::new();
        if let Some(slot) = &prev
            && slot.namespace == body.namespace
        {
            for (index, state) in slot.runtime.states().iter().enumerate() {
                if state.weight > 0.0 || state.goal_weight > 0.0 {
                    if let Some(clip) = slot
                        .runtime
                        .leaf_clip(xmodel_runtime::XAnimNodeId(index as u16))
                    {
                        bound.insert(index as u16, clip);
                    }
                }
            }
        }
        bound.insert(legs_index, Arc::clone(&legs_for_tree));
        if let Some(torso_for_tree) = torso_clip.clone() {
            bound.insert(torso_index, torso_for_tree);
        }
        let definition = tree
            .to_runtime_definition(|index, _name| bound.get(&index).cloned())
            .map_err(|error| error.to_string())?;
        let mut slot = match prev {
            Some(mut slot) if slot.runtime.states().len() == definition.nodes().len() => {
                slot.runtime
                    .rebind(definition)
                    .map_err(|error| error.to_string())?;
                slot
            }
            Some(_) | None => PersistentRemoteTree {
                binding: binding.clone(),
                runtime: xmodel_runtime::XAnimTreeRuntime::new(definition),
                namespace: body.namespace,
                dobj: None,
                reuse_key: None,
                legs: 0,
                torso: 0,
                legs_restart: false,
                torso_restart: false,
                occupation_tr_time,
                legs_rate_sample: ClientAnimSample::default(),
                torso_rate_sample: ClientAnimSample::default(),
            },
        };
        slot.namespace = body.namespace;
        let legs_properties = script.animation_properties(legs_index);
        let torso_properties = script.animation_properties(torso_index);
        slot.legs_rate_sample.move_speed = if legs_properties.stationary {
            0.0
        } else {
            clip.move_speed()
        };
        slot.legs_rate_sample.ladder = legs_properties.ladder;
        slot.torso_rate_sample.move_speed = if torso_properties.stationary {
            0.0
        } else {
            torso_clip.as_ref().map_or(0.0, |clip| clip.move_speed())
        };
        slot.torso_rate_sample.ladder = torso_properties.ladder;

        let locomotion_phase = if old_legs != legs_index
            && old_legs_moving
            && slot.legs_rate_sample.move_speed > 0.0
            && clip.looping
            && slot
                .runtime
                .leaf_clip(xmodel_runtime::XAnimNodeId(old_legs))
                .is_some_and(|clip| clip.looping)
        {
            Some(slot.runtime.states()[old_legs as usize].time)
        } else {
            None
        };
        apply_player_anim_goals(
            &mut slot.runtime,
            xmodel_runtime::PlayerBodyBranches {
                legs: xmodel_runtime::XAnimNodeId(
                    tree.index_of("legs")
                        .ok_or("player tree legs branch missing")?,
                ),
                torso: xmodel_runtime::XAnimNodeId(
                    tree.index_of("torso")
                        .ok_or("player tree torso branch missing")?,
                ),
            },
            old_legs,
            old_torso,
            legs_index,
            torso_index,
            prev_restart_changed_legs,
            prev_restart_changed_torso,
            old_legs_moving,
            old_torso_moving,
            slot.legs_rate_sample.move_speed > 0.0,
            slot.torso_rate_sample.move_speed > 0.0,
            [legs_properties.blend_ms, torso_properties.blend_ms],
        )?;
        if let Some(time) = locomotion_phase {
            let id = xmodel_runtime::XAnimNodeId(legs_index);
            let mut state = slot.runtime.states()[legs_index as usize];
            state.time = time;
            state.old_time = time;
            slot.runtime
                .set_state(id, state)
                .map_err(|error| error.to_string())?;
        }
        clips = Some(PoseClips {
            clip,
            torso_clip,
            legs_for_tree,
        });
        slot.legs = legs_index;
        slot.torso = torso_index;
        slot.legs_restart = legs_restart;
        slot.torso_restart = torso_restart;
        slot
    };
    apply_player_anim_rates(
        &mut slot.runtime,
        &mut slot.legs_rate_sample,
        &mut slot.torso_rate_sample,
        legs_index,
        torso_index,
        origin,
        pose_time_ms,
    )?;
    slot.runtime.update(dt).map_err(|error| error.to_string())?;
    if !leaf_enrolled(&slot.runtime, legs_index) {
        return Err("complete goal weight left the legs leaf at 0".into());
    }
    if torso_index != 0 && !leaf_enrolled(&slot.runtime, torso_index) {
        return Err("complete goal weight left the torso leaf at 0".into());
    }
    trees.by_ent.insert(persist_key, slot);
    let slot = trees.by_ent.get_mut(&persist_key).expect("tree inserted");
    Ok(AdvancedRemoteTree {
        runtime: slot.runtime.clone(),
        clips,
        reused,
    })
}

fn leaf_enrolled(runtime: &xmodel_runtime::XAnimTreeRuntime, index: u16) -> bool {
    runtime
        .states()
        .get(index as usize)
        .is_some_and(|state| state.weight > 0.0 || state.goal_weight > 0.0)
}

pub fn tree_clips_unchanged(
    last_legs: u16,
    last_torso: u16,
    last_legs_restart: bool,
    last_torso_restart: bool,
    legs: u16,
    torso: u16,
    legs_restart: bool,
    torso_restart: bool,
) -> bool {
    last_legs == legs
        && last_torso == torso
        && last_legs_restart == legs_restart
        && last_torso_restart == torso_restart
}
