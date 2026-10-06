use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::{ConsoleLine, ConsoleSettings, ConsoleState};

#[derive(SystemParam)]
pub(crate) struct ConsoleEcho<'w> {
    console: ResMut<'w, ConsoleState>,
    settings: Res<'w, ConsoleSettings>,
    line: ResMut<'w, ConsoleLine>,
}

impl ConsoleEcho<'_> {
    pub(crate) fn write(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        diag::info!(Console, "{msg}");
        self.line.0 = msg.clone();
        self.console.echo(msg, self.settings.log_capacity);
    }
}
