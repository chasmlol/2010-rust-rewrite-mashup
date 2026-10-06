//! Minecraft's sounds on the Minecraft map: sound files the world resolves
//! from the resource pack's `sounds.json`, decoded once each and played as
//! world one-shots with vanilla's linear falloff (sixteen blocks, further for
//! louder sounds) and MW2's panning.
use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::media::{LivePan, PcmBuffer, RenderMedia};
use crate::playback::{AmbientListener, world_oneshot_channel_gains};

/// One sound to play.
pub struct McSoundRequest {
    /// The sound file's id, which its decode is cached under.
    pub key: String,
    /// The file's bytes (Ogg Vorbis).
    pub bytes: Arc<[u8]>,
    /// Where it plays in map units, or at the listener for none.
    pub position: Option<Vec3>,
    pub volume: f32,
    pub pitch: f32,
    /// Map units a block spans, for the falloff.
    pub block: f32,
}

/// Minecraft's sounds against MW2's much hotter mix.
const MIX_GAIN: f32 = 2.5;

#[derive(Resource, Default)]
pub struct McSoundQueue(pub Vec<McSoundRequest>);

#[derive(Resource, Default)]
struct McSoundCache(HashMap<String, Option<PcmBuffer>>);

pub(crate) fn register(app: &mut App) {
    app.init_resource::<McSoundQueue>()
        .init_resource::<McSoundCache>()
        .add_systems(Update, play_minecraft_sounds);
}

fn play_minecraft_sounds(
    mut queue: ResMut<McSoundQueue>,
    mut cache: ResMut<McSoundCache>,
    runtime: Res<crate::AudioRuntime>,
    epoch: Res<crate::backend::MatchEpoch>,
    silent: Option<Res<crate::AudioSilent>>,
    listeners: Query<&Transform, With<AmbientListener>>,
) {
    if silent.is_some() {
        queue.0.clear();
        return;
    }
    if queue.0.is_empty() {
        return;
    }
    let pose = listeners
        .iter()
        .next()
        .map(|t| (t.translation, t.rotation * Vec3::X));
    for request in std::mem::take(&mut queue.0) {
        let pcm = cache.0.entry(request.key.clone()).or_insert_with(|| {
            let decoded = crate::pcm::decode_audio_bytes(&request.bytes);
            if decoded.is_err() {
                diag::warn!(Audio, "minecraft sound `{}` did not decode", request.key);
            }
            decoded.ok()
        });
        let Some(pcm) = pcm.as_ref() else {
            continue;
        };
        // `SoundEngine`: linear falloff to sixteen blocks, times the volume
        // when it is above one.
        let (gain, pan) = match (request.position, pose) {
            (Some(at), Some((ear, right))) => {
                let reach = 16.0 * request.volume.max(1.0) * request.block;
                let falloff = (1.0 - (at - ear).length() / reach).clamp(0.0, 1.0);
                (
                    request.volume.min(1.0) * falloff,
                    world_oneshot_channel_gains(ear, right, at, 1.0),
                )
            }
            _ => (
                request.volume.min(1.0),
                (
                    std::f32::consts::FRAC_1_SQRT_2,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
            ),
        };
        if gain <= 0.001 {
            continue;
        }
        let mut media = RenderMedia::from_buffer(pcm.clone());
        media.pan = Some(LivePan::new(pan.0, pan.1));
        runtime.play_pcm(
            media,
            (gain * MIX_GAIN).max(0.0),
            request.pitch.clamp(0.5, 2.0),
            epoch.0,
        );
    }
}
