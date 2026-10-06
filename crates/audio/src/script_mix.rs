use crate::backend::MatchEpoch;
use crate::media::LiveGain;
use asset_audio::ChannelKey;
use asset_core::AssetNamespace;
use bevy::prelude::*;
use std::{collections::HashMap, sync::Arc};

#[derive(Resource, Default)]
pub(crate) struct ScriptAudioMix {
    epoch: u64,
    pub(crate) gain: LiveGain,
}

impl ScriptAudioMix {
    pub(crate) fn fade(&mut self, frame: u64, to: f32, duration: i32) {
        self.gain.fade(
            frame,
            to,
            duration.max(0) as u64 * u64::from(crate::render_core::SAMPLE_RATE) / 1000,
        );
    }
    pub(crate) fn reset_epoch(&mut self, epoch: u64) {
        if self.epoch != epoch {
            self.epoch = epoch;
            self.gain.set(1.0);
        }
    }
}

struct ChannelGroup {
    active: bool,
    goal: [f32; 64],
}

impl Default for ChannelGroup {
    fn default() -> Self {
        Self {
            active: false,
            goal: [1.0; 64],
        }
    }
}

#[derive(Resource)]
pub(crate) struct ChannelAudioMix {
    epoch: u64,
    groups: HashMap<(AssetNamespace, u8), ChannelGroup>,
    selected: HashMap<AssetNamespace, u8>,
    gains: Arc<HashMap<ChannelKey, LiveGain>>,
    pending: std::collections::VecDeque<sim::ScriptAudioCommand>,
}

fn channel_groups() -> HashMap<(AssetNamespace, u8), ChannelGroup> {
    AssetNamespace::ALL
        .into_iter()
        .flat_map(|namespace| {
            (0..4).map(move |priority| {
                (
                    (namespace, priority),
                    ChannelGroup {
                        active: priority == 0,
                        ..Default::default()
                    },
                )
            })
        })
        .collect()
}

impl Default for ChannelAudioMix {
    fn default() -> Self {
        Self {
            epoch: 0,
            groups: channel_groups(),
            selected: AssetNamespace::ALL
                .into_iter()
                .map(|namespace| (namespace, 0))
                .collect(),
            gains: Arc::new(
                AssetNamespace::ALL
                    .into_iter()
                    .flat_map(|namespace| {
                        (0..64).map(move |id| (ChannelKey { namespace, id }, LiveGain::default()))
                    })
                    .collect(),
            ),
            pending: Default::default(),
        }
    }
}

impl ChannelAudioMix {
    pub(crate) fn bindings(&self) -> Arc<HashMap<ChannelKey, LiveGain>> {
        self.gains.clone()
    }
    pub(crate) fn reset_epoch(&mut self, epoch: u64) {
        if self.epoch != epoch {
            self.epoch = epoch;
            self.groups = channel_groups();
            self.selected = AssetNamespace::ALL
                .into_iter()
                .map(|namespace| (namespace, 0))
                .collect();
            self.pending.clear();
            for gain in self.gains.values() {
                gain.set(1.0);
            }
        }
    }
    fn apply(&self, namespace: AssetNamespace, frame: u64, fade_ms: i32) {
        let frames = fade_ms.max(0) as u64 * u64::from(crate::render_core::SAMPLE_RATE) / 1000;
        let selected = self.selected[&namespace];
        let group = &self.groups[&(namespace, selected)];
        for (id, goal) in group.goal.iter().enumerate() {
            self.gains[&ChannelKey {
                namespace,
                id: id as u32,
            }]
                .fade(frame, *goal, frames);
        }
    }
    fn set(
        &mut self,
        namespace: AssetNamespace,
        frame: u64,
        priority: u8,
        goals: &[f32],
        fade_ms: i32,
    ) {
        let group = self
            .groups
            .get_mut(&(namespace, priority))
            .expect("validated script mix priority");
        group.active = true;
        group.goal.fill(1.0);
        group.goal[..goals.len()].copy_from_slice(goals);
        let selected = (0..4)
            .rev()
            .find(|priority| self.groups[&(namespace, *priority)].active)
            .unwrap_or(0);
        self.selected.insert(namespace, selected);
        if selected == priority {
            self.apply(namespace, frame, fade_ms);
        }
    }
    fn deactivate(&mut self, namespace: AssetNamespace, frame: u64, priority: u8, fade_ms: i32) {
        self.groups
            .get_mut(&(namespace, priority))
            .expect("validated script mix priority")
            .active = false;
        if self.selected[&namespace] == priority {
            let selected = (0..4)
                .rev()
                .find(|priority| self.groups[&(namespace, *priority)].active)
                .unwrap_or(0);
            self.selected.insert(namespace, selected);
            self.apply(namespace, frame, fade_ms);
        }
    }
}

fn update_channel_mix(
    mut mix: ResMut<ChannelAudioMix>,
    mut events: MessageReader<net::SvcScriptAudio>,
    bank: Option<Res<crate::SoundBank>>,
    namespace: Option<Res<crate::ambient::SoundBankNamespace>>,
    local: Option<Res<net::LocalPresentClient>>,
    epoch: Res<MatchEpoch>,
    runtime: Res<crate::AudioRuntime>,
) {
    let now = runtime.audio_frame();
    mix.reset_epoch(epoch.0);
    for event in events.read() {
        if event.0.target().is_some() {
            mix.pending.push_back(event.0.clone());
        }
    }
    let Some(local) = local else {
        return;
    };
    let Some(namespace) = namespace else {
        return;
    };
    let namespace = namespace.namespace;
    while let Some(command) = mix.pending.front() {
        if command.target() != Some(local.0) || !command.valid() {
            mix.pending.pop_front();
            continue;
        }
        if let sim::ScriptAudioCommand::ChannelVolumes { .. } = command
            && bank.is_none()
        {
            break;
        }
        let command = mix.pending.pop_front().expect("front command");
        match command {
            sim::ScriptAudioCommand::ChannelVolumes {
                priority,
                volumes,
                fade_ms,
                ..
            } => {
                let Some(bank) = bank.as_ref() else {
                    continue;
                };
                let Some(channels) = bank.0.channels_in(namespace) else {
                    diag::warn!(
                        Audio,
                        "audio: no channel volume policy for namespace={namespace:?}"
                    );
                    continue;
                };
                let goals: Option<Vec<_>> = match volumes {
                    Some(volumes) => channels
                        .iter()
                        .map(|channel| volumes.get(&channel.name.to_ascii_lowercase()).copied())
                        .collect(),
                    None => Some(vec![0.0; channels.len()]),
                };
                let Some(goals) = goals.filter(|goals| !goals.is_empty() && goals.len() <= 64)
                else {
                    diag::warn!(
                        Audio,
                        "audio: channel volume profile is incomplete for the sound bank"
                    );
                    continue;
                };
                mix.set(namespace, now, priority, &goals, fade_ms);
            }
            sim::ScriptAudioCommand::DeactivateChannelVolumes {
                priority, fade_ms, ..
            } => mix.deactivate(namespace, now, priority, fade_ms),
            _ => {}
        }
    }
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<ScriptAudioMix>()
        .init_resource::<ChannelAudioMix>()
        .add_systems(Update, update_channel_mix.in_set(net::ClientSet::Effects));
}
