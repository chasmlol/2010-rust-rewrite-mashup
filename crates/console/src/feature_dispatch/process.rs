use std::io::Write;

use bevy::prelude::*;
use net::{MasterBridge, MasterBridgeState};

const LEAVE_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Resource)]
pub(crate) struct PendingExit(i32);

pub(crate) fn request_exit(
    mut exit: MessageReader<AppExit>,
    bridge: Option<Res<MasterBridge>>,
    mut commands: Commands,
) {
    let Some(code) = exit.read().last().map(|exit| match exit {
        AppExit::Success => 0,
        AppExit::Error(code) => i32::from(code.get()),
    }) else {
        return;
    };
    if let Some(bridge) = bridge {
        leave_master(&bridge);
    }
    commands.insert_resource(PendingExit(code));
}

const AUDIO_FADE: std::time::Duration = std::time::Duration::from_millis(60);

pub(crate) fn exit_process(world: &mut World) {
    let Some(PendingExit(code)) = world.remove_resource::<PendingExit>() else {
        return;
    };
    silence_audio(world);
    diag::lifecycle_boundary("process_exit", &format!(" code={code}"));
    diag::flush();
    let _ = std::io::stdout().flush();
    diag::exit::exit_now(code);
}

fn silence_audio(world: &mut World) {
    let Some(audio) = world.get_resource::<audio::AudioRuntime>() else {
        return;
    };
    audio.set_master_volume(0.0);
    std::thread::sleep(AUDIO_FADE);
}

fn leave_master(bridge: &MasterBridge) {
    if !matches!(
        bridge.state(),
        MasterBridgeState::Hosting { .. }
            | MasterBridgeState::Joining { .. }
            | MasterBridgeState::Joined { .. }
    ) {
        return;
    }
    bridge.leave();
    let until = std::time::Instant::now() + LEAVE_BUDGET;
    while std::time::Instant::now() < until {
        if bridge.state().is_terminal() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    diag::warn!(
        Console,
        "quit: master had not confirmed the leave after {}ms",
        LEAVE_BUDGET.as_millis()
    );
}
