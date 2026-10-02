use bevy::prelude::*;

pub struct RenderAnimPlugin;

impl Plugin for RenderAnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::anim::scene_submission::AnimDObjSceneSkels>()
            .init_resource::<crate::anim::remote_body::RemoteBodyTrees>()
            .init_resource::<crate::anim::remote_body::RemoteSkinPoseHashes>()
            .init_resource::<crate::anim::remote_body::RemoteBodySkinnedQueue>()
            .init_resource::<crate::anim::fpv_host::FpvPresentCursor>()
            .init_resource::<crate::anim::fpv_host::PendingFpvSpawn>()
            .init_resource::<crate::anim::fpv_host::PendingFpvNotetracks>()
            .init_resource::<crate::anim::fpv_host::FpvHeldSettled>()
            .init_resource::<crate::anim::fpv_host::FpvHeldLife>()
            .init_resource::<crate::anim::fpv_host::FpvBoltTargets>()
            .init_resource::<crate::anim::fpv_host::FpvPoseProduct>()
            .init_resource::<crate::draw::FpvDrawPlan>()
            .init_resource::<crate::draw::RemoteBodyDrawPlan>()
            .init_resource::<crate::draw::ScriptModelDrawPlan>()
            .init_resource::<crate::draw::MissileDrawPlan>()
            .init_resource::<crate::draw::ItemDrawPlan>()
            .init_resource::<crate::draw::DynEntDrawPlan>()
            .add_message::<crate::anim::scene_submission::AnimDObjSceneSubmission>();
        crate::occupancy::fpv_present::register_fpv_present_systems(app);
        crate::occupancy::held_sync::register_held_sync_systems(app);
        crate::occupancy::match_reset::register_match_reset_systems(app);
        crate::occupancy::remote_body::register_remote_body_systems(app);
        crate::occupancy::script_model::register_script_model_systems(app);
        crate::occupancy::missile::register_missile_systems(app);
        crate::occupancy::item::register_item_systems(app);
        crate::occupancy::dyn_ent::register_dyn_ent_systems(app);
        crate::gaps::register_render_gaps(app);
        crate::skate::register(app);
        crate::jak::register(app);
        crate::minecraft_world::register(app);
    }
}
