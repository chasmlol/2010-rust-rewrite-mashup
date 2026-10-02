mod state;
mod validation;
use crate::{input::ControllerInput, physics::{GamePhysics, SkaterRuntime}};
use bevy::prelude::*;
use skate_core::physics::{board::BodyId, skeleton_animation_record::AnimationPartTransform};
#[derive(Clone, Copy, Debug)]
struct Marker {
    transform: AnimationPartTransform,
    on_board: bool,
    foot_forward: bool,
    generation: u64,
}

/// Audio owners consume these native GlobalFEPlaySound event IDs.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct SessionMarkerAudio(pub u64);

#[derive(Resource, Default)]
pub(crate) struct SessionMarker {
    marker: Option<Marker>,
    hold: state::Hold,
    generation: u64,
    pub visible: bool,
    pub can_place: bool,
    pub can_return: bool,
    pub progress: f32,
    blocked_until_release: bool,
    ui_time: f64,
    last_batch: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Status {
    pub placed: bool,
    pub can_return: bool,
    pub progress: f32,
}

pub(crate) struct Runtime { session: SessionMarker, validation: validation::Validation }
impl Runtime {
    pub fn load(root: &std::path::Path) -> Result<Self,String> { Ok(Self{session:SessionMarker::default(),validation:validation::Validation::load(root)?}) }
    pub fn status(&self) -> Status {
        Status {
            placed: self.session.marker.is_some(),
            can_return: self.session.can_return,
            progress: self.session.progress,
        }
    }
    pub fn suspend(&mut self) { self.session.hold.cancel(); self.session.blocked_until_release=true; self.session.ui_time=0.; }
    pub fn collect_time(&mut self,dt:f64) { self.session.ui_time+=dt; }
    pub fn advance(&mut self,input:&ControllerInput,physics:&GamePhysics,skater:&mut SkaterRuntime) {
        update(&mut self.session,input,physics,skater,&self.validation);
    }
}
fn update(session:&mut SessionMarker,input:&ControllerInput,physics:&GamePhysics,skater:&mut SkaterRuntime,validation:&validation::Validation) {
    let (modifier, set, held) = input.session_marker_actions();
    if session.blocked_until_release {
        if !modifier {
            session.blocked_until_release = false;
        }
        return;
    }
    let p = &skater.player_input.physical;
    let on_board = p.state.category_12 != 500;
    let deck = physics.board.part_transforms()[BodyId::Deck.index()];
    let mut transform = skater.animated_skeleton.roots.animation_to_world;
    if on_board {
        for i in 0..3 {
            transform[i][..3].copy_from_slice(&deck.basis.columns[i]);
        }
        transform[3] = [
            deck.translation.x,
            deck.translation.y + 0.2,
            deck.translation.z,
            0.,
        ];
        //82591E30: above .5m/s, project normalized velocity onto world Up.
        //The cross products are deliberately not normalized a second time.
        let velocity = Vec3::from_slice(&p.skateboard.vector_80.map(f32::from_bits)[..3]);
        if velocity.length_squared() > 0.25 {
            let right = Vec3::Y.cross(velocity.normalize());
            let forward = right.cross(Vec3::Y);
            if forward.length_squared() > 0.9 {
                transform[0] = right.extend(0.).to_array();
                transform[1] = Vec3::Y.extend(0.).to_array();
                transform[2] = forward.extend(0.).to_array();
            }
        }
    }
    let state = p.state.state_16;
    let state_allowed = (p.state.category_12 == 100
        && p.collision.wheel_count_0 >= 2
        && state != 104
        && deck.basis.columns[1][1] > 0.71)
        || (p.state.category_12 == 500 && state == 500);
    session.can_place = modifier
        && state_allowed
        && p.surface_default_mode != 8
        && validation.check(physics.world(), transform[3]);
    session.can_return = session
        .marker
        .is_some_and(|m| m.generation == 0)
        && !matches!(state, 104 | 502);
    session.visible = modifier;
    // A retained Pad publication must not turn one .pressed into repeated sets.
    if set && session.last_batch != input.consumed_batches {
        if session.can_place {
            session.marker = Some(Marker {
                transform,
                on_board,
                foot_forward: skater.animation.foot_forward(),
                generation: 0,
            });
            bevy::log::info!("Skate session marker placed");
        } else {
            bevy::log::info!("Skate session marker placement refused");
        }
    }
    session.last_batch = input.consumed_batches;
    let ready = p.state.flag_69 == 0
        && state != 702
        && !(physics.board_wiping_out && p.skeleton.teleport_pending_604 != 0)
        && skater.player_input.pending_teleport().is_none();
    let distance = session.marker.map_or(0., |m| {
        // 82DB6EC0 -> 82BE1AE8 publishes animation-to-world at output+368;
        // UpdateSessionMarker reads its translation at output+416.
        Vec3::from_slice(&skater.animated_skeleton.roots.animation_to_world[3][..3])
            .distance(Vec3::from_slice(&m.transform[3][..3]))
    });
    let usable = session.can_return;
    while session.ui_time >= 1. / 60. {
        session.ui_time -= 1. / 60.;
        let step = session.hold.update(held, usable, distance, ready);
        session.progress = step.progress;
        if step.relocate {
            if let Some(target) = session.marker {
                match skater.player_input.request_teleport(target.transform) {
                    Ok(()) => {
                        skater.animation.restore_foot_forward(target.foot_forward);
                        skater
                            .teleport_state
                            .request_manual(target.transform, target.on_board);
                        bevy::log::info!("Skate session marker returned");
                    }
                    Err(e) => {
                        warn!("Session marker return rejected: {e}");
                        session.hold.cancel();
                    }
                }
            }
        }
    }
}
