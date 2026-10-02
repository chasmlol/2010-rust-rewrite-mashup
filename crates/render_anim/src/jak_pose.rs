//! Jak's own model, board and gun, posed from the gameplay: each state plays
//! the animation Jak 3 plays for it, sampled from the clips the player
//! exported, and the board stance mixes its turn, lean and duck animations
//! by the amounts the board simulation drives.
use std::sync::Arc;

use assets::bot_model::BotModel;
use assets::jak_model::{JakAssets, Rig, Trs};
use bevy::prelude::*;
use frame::JakSkins;
use jak_mode::board_anim::TURN_FRAMES;
use jak_mode::{BoardTrick, Jak, State, TICKS_PER_SECOND};

use crate::anim::remote_body::{CpuBodyGeom, CpuSurfMeta};
use crate::jak::MAP_PER_METER;

/// Above this speed on foot Jak runs rather than walks, meters a second.
const RUN_SPEED: f32 = 5.0;

/// Jak's meters (x left, y up, z forward) in the soldier body's frame (x
/// forward, y left, z up), in map units.
fn body_frame() -> Mat4 {
    let s = MAP_PER_METER;
    Mat4::from_cols(
        Vec4::new(0.0, s, 0.0, 0.0),
        Vec4::new(0.0, 0.0, s, 0.0),
        Vec4::new(s, 0.0, 0.0, 0.0),
        Vec4::W,
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Hold {
    /// Play through once, then go on to the next step.
    Once,
    Loop,
}

#[derive(Clone, Copy, PartialEq)]
struct Step {
    clip: &'static str,
    rate: f32,
    hold: Hold,
}

const fn once(clip: &'static str) -> Step {
    Step {
        clip,
        rate: 1.0,
        hold: Hold::Once,
    }
}

const fn looped(clip: &'static str) -> Step {
    Step {
        clip,
        rate: 1.0,
        hold: Hold::Loop,
    }
}

/// What a state shows: a run of clips, or the board stance mix.
#[derive(Clone, PartialEq)]
enum Program {
    Clips { steps: Vec<Step>, blend: f32 },
    Stance { lead_in: Option<Step> },
}

/// Which program is showing and since when, in game ticks.
#[derive(Clone, PartialEq)]
struct Showing {
    program: Program,
    start: i64,
}

pub(crate) struct Animator {
    showing: Option<Showing>,
    /// The pose to blend from, and when the blend began.
    from: Option<(Vec<Trs>, i64, f32)>,
    last: Vec<Trs>,
}

impl Animator {
    pub(crate) fn new() -> Self {
        Self {
            showing: None,
            from: None,
            last: Vec::new(),
        }
    }

    /// Skins for Jak, his board and his gun this frame, in the body frame.
    pub(crate) fn pose(&mut self, assets: &JakAssets, jak: &Jak) -> Arc<JakSkins> {
        let body = &assets.body;
        let (program, start) = program_for(jak);
        let changed = self
            .showing
            .as_ref()
            .is_none_or(|s| s.program != program || s.start != start);
        if changed {
            let blend = match &program {
                Program::Clips { blend, .. } => *blend,
                Program::Stance { .. } => 0.1,
            };
            if !self.last.is_empty() {
                self.from = Some((self.last.clone(), jak.time, blend));
            }
            self.showing = Some(Showing { program, start });
        }
        let showing = self.showing.as_ref().expect("set above");
        let elapsed = (jak.time - showing.start) as f32 / TICKS_PER_SECOND as f32;
        let mut pose = match &showing.program {
            Program::Clips { steps, .. } => play(body, steps, elapsed),
            Program::Stance { lead_in } => {
                let rest = lead_in.and_then(|step| {
                    let clip = body.clips.get(step.clip)?;
                    let t = elapsed * step.rate;
                    (t < clip.duration).then(|| sampled(body, step.clip, t))
                });
                rest.unwrap_or_else(|| stance(body, jak))
            }
        };
        if let Some((from, began, length)) = &self.from {
            let t = (jak.time - began) as f32 / TICKS_PER_SECOND as f32 / length.max(1e-3);
            if t < 1.0 && from.len() == pose.len() {
                for (p, f) in pose.iter_mut().zip(from) {
                    *p = f.lerp(p, t);
                }
            } else {
                self.from = None;
            }
        }
        self.last.clone_from(&pose);

        let frame = body_frame();
        let globals = body.globals(&pose);
        let attached = |rig: &Option<Rig>, joint: &str, clip: &str, shown: bool| {
            let rig = rig.as_ref().filter(|_| shown)?;
            let at = body.joint(joint)?;
            let mut own = rig.rest_pose();
            if let Some(c) = rig.clips.get(clip) {
                let t = if c.duration > 0.0 {
                    (jak.time as f32 / TICKS_PER_SECOND as f32) % c.duration
                } else {
                    0.0
                };
                c.sample(t, &mut own);
            }
            Some(rig.skin_matrices(&rig.globals(&own), frame * globals[at]))
        };
        Arc::new(JakSkins {
            body: body.skin_matrices(&globals, frame),
            board: attached(
                &assets.board,
                "board",
                "board-board-idle",
                jak.state.is_board(),
            )
            .unwrap_or_else(Vec::new),
            gun: attached(&assets.gun, "gun", "gun-idle-yellow", true).unwrap_or_else(Vec::new),
        })
    }
}

/// The program for Jak's state, and the tick it counts from.
fn program_for(jak: &Jak) -> (Program, i64) {
    let clips = |steps: Vec<Step>, blend: f32| Program::Clips { steps, blend };
    let gun = jak.gun.out;
    let since = jak.state_time;
    match jak.state {
        State::Stance => {
            let fire = jak.time - jak.gun.fire_time;
            if gun && fire >= 0 && fire < jak_mode::math::seconds(0.3) {
                return (
                    clips(
                        vec![
                            once("jakb-gun-yellow-fire"),
                            looped("jakb-gun-stance-yellow"),
                        ],
                        0.05,
                    ),
                    jak.gun.fire_time,
                );
            }
            let clip = if gun {
                "jakb-gun-stance-yellow"
            } else {
                "jakb-stance-loop"
            };
            (clips(vec![looped(clip)], 0.2), since)
        }
        State::Walk => {
            let speed = jak_mode::math::xz_length(jak.velocity()) / jak_mode::METER;
            let clip = match (gun, speed > RUN_SPEED) {
                (false, false) => "jakb-walk",
                (false, true) => "jakb-run",
                (true, false) => "jakb-gun-front-walk",
                (true, true) => "jakb-gun-front-run",
            };
            (clips(vec![looped(clip)], 0.2), since)
        }
        State::Jump { .. } | State::DoubleJump { .. } => {
            let first = if gun {
                "jakb-gun-front-jump"
            } else {
                "jakb-jump"
            };
            (
                clips(vec![once(first), looped("jakb-jump-loop")], 0.05),
                since,
            )
        }
        State::Falling => (clips(vec![looped("jakb-jump-loop")], 0.2), since),
        State::HitGround => {
            let clip = if gun {
                "jakb-gun-front-jump-land"
            } else {
                "jakb-jump-land"
            };
            (clips(vec![once(clip)], 0.05), since)
        }
        State::BoardGetOn => (clips(vec![once("jakb-board-get-on")], 0.05), since),
        State::BoardHitGround => (
            Program::Stance {
                lead_in: Some(Step {
                    clip: "jakb-board-get-on-land",
                    rate: 1.8,
                    hold: Hold::Once,
                }),
            },
            since,
        ),
        State::BoardStance | State::BoardDuckStance | State::BoardTurnTo { .. } => {
            (Program::Stance { lead_in: None }, 0)
        }
        State::BoardJump { duck, .. } => {
            let first = if duck {
                "jakb-board-jump-high"
            } else {
                "jakb-board-jump"
            };
            (
                clips(vec![once(first), looped("jakb-board-jump-loop")], 0.05),
                since,
            )
        }
        State::BoardFalling => (clips(vec![looped("jakb-board-jump-loop")], 0.5), since),
        State::BoardFlip => (
            clips(
                vec![
                    once("jakb-board-flip-forward"),
                    looped("jakb-board-flip-forward-loop"),
                ],
                0.1,
            ),
            since,
        ),
        State::BoardTrick(trick) => {
            let steps = match trick {
                BoardTrick::Grab => vec![
                    once("jakb-board-nosegrab"),
                    looped("jakb-board-nosegrab-loop"),
                ],
                BoardTrick::Kick => vec![
                    once("jakb-board-kickflip-a"),
                    looped("jakb-board-jump-loop"),
                ],
                BoardTrick::Hold => {
                    vec![once("jakb-board-method"), looped("jakb-board-method-loop")]
                }
            };
            (clips(steps, 0.08), since)
        }
        State::BoardGetOff => (clips(vec![once("jakb-board-get-off")], 0.05), since),
    }
}

/// A run of clips `elapsed` seconds in: each one-shot plays through before
/// the next, a loop holds.
fn play(rig: &Rig, steps: &[Step], elapsed: f32) -> Vec<Trs> {
    let mut t = elapsed;
    let mut shown = None;
    for step in steps {
        let Some(clip) = rig.clips.get(step.clip) else {
            continue;
        };
        let local = t * step.rate;
        shown = Some((step.clip, local));
        match step.hold {
            Hold::Loop => {
                let local = if clip.duration > 0.0 {
                    local % clip.duration
                } else {
                    0.0
                };
                shown = Some((step.clip, local));
                break;
            }
            Hold::Once if local < clip.duration => break,
            Hold::Once => t -= clip.duration / step.rate.max(1e-3),
        }
    }
    match shown {
        Some((clip, local)) => sampled(rig, clip, local),
        None => sampled(rig, "jakb-stance-loop", elapsed),
    }
}

fn sampled(rig: &Rig, clip: &str, time: f32) -> Vec<Trs> {
    let mut pose = rig.rest_pose();
    if let Some(clip) = rig.clips.get(clip) {
        clip.sample(time, &mut pose);
    }
    pose
}

/// The board stance: the turn animation at the sprung frame, leaned toward
/// the slope, crouched or lifted by the duck amount.
fn stance(rig: &Rig, jak: &Jak) -> Vec<Trs> {
    let a = &jak.board.anim;
    let frame = a.shown_frame(jak.time - jak.state_time);
    let at = |clip: &str| -> Vec<Trs> {
        let duration = rig.clips.get(clip).map_or(0.0, |c| c.duration);
        sampled(
            rig,
            clip,
            (frame + TURN_FRAMES) / (2.0 * TURN_FRAMES) * duration,
        )
    };
    let mut pose = at("jakb-board-turn");
    let mut toward = |clip: &str, weight: f32| {
        if weight > 0.0 && rig.clips.contains_key(clip) {
            let other = at(clip);
            for (p, o) in pose.iter_mut().zip(&other) {
                *p = p.lerp(o, weight.min(1.0));
            }
        }
    };
    let [ahead, side] = a.tween;
    toward(
        if ahead >= 0.0 {
            "jakb-board-turn-up"
        } else {
            "jakb-board-turn-down"
        },
        ahead.abs(),
    );
    toward(
        if side >= 0.0 {
            "jakb-board-turn-left"
        } else {
            "jakb-board-turn-right"
        },
        side.abs(),
    );
    toward(
        if a.duck >= 0.0 {
            "jakb-board-duck-turn"
        } else {
            "jakb-board-air-turn"
        },
        a.duck.abs(),
    );
    pose
}

/// Jak, the board under him and the gun on him, skinned into `geom`.
pub(crate) fn skin(skins: &JakSkins, geom: &mut CpuBodyGeom) {
    let Some(assets) = assets::jak_model::local_jak() else {
        return;
    };
    skin_model(&assets.body.mesh, &skins.body, geom);
    if let Some(board) = &assets.board {
        skin_model(&board.mesh, &skins.board, geom);
    }
    if let Some(gun) = &assets.gun {
        skin_model(&gun.mesh, &skins.gun, geom);
    }
    geom.decoded_n = geom.packed.len();
}

fn skin_model(model: &BotModel, matrices: &[Mat4], geom: &mut CpuBodyGeom) {
    if matrices.len() < model.joints.len() {
        return;
    }
    for surface in &model.surfaces {
        let vertex_base = geom.packed.len() as u32;
        let index_start = geom.indices.len() as u32;
        for v in &surface.vertices {
            let mut position = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            let sum: f32 = v.weights.iter().sum();
            for i in 0..4 {
                let weight = v.weights[i] / sum.max(1e-8);
                if weight == 0.0 {
                    continue;
                }
                let matrix = matrices[v.joints[i]];
                position += matrix.transform_point3(Vec3::from_array(v.position)) * weight;
                normal += matrix.transform_vector3(Vec3::from_array(v.normal)) * weight;
            }
            let normal = normal.normalize_or(Vec3::Z);
            let mut packed = [0u8; asset_iw4::size::GFX_PACKED_VERTEX];
            for (i, f) in position.to_array().iter().enumerate() {
                packed[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
            }
            packed[12..16].copy_from_slice(&1.0f32.to_le_bytes());
            packed[16..20].fill(255);
            packed[20..24].copy_from_slice(&v.uv.to_le_bytes());
            packed[24..28].copy_from_slice(&pack_normal(normal));
            packed[28..32].copy_from_slice(&pack_normal(normal.any_orthonormal_vector()));
            geom.packed.push(packed);
        }
        geom.indices
            .extend(surface.indices.iter().map(|i| vertex_base + i));
        geom.surfaces.push(CpuSurfMeta {
            index_start,
            index_count: surface.indices.len() as u32,
            name: Some(surface.material.clone()),
        });
    }
}

fn pack_normal(n: Vec3) -> [u8; 4] {
    [
        (n.x * 127.0 + 127.5) as u8,
        (n.y * 127.0 + 127.5) as u8,
        (n.z * 127.0 + 127.5) as u8,
        63,
    ]
}
