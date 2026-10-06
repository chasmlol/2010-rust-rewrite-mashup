mod barracks_menu;
pub mod binds;
mod class_dispatch;
mod class_menu;
mod command;
mod community_servers;
mod debug_cg_gun;
mod debug_cl_yawspeed;
mod debug_distortion;
mod debug_dof;
mod debug_draw_method;
mod debug_fog;
mod debug_fx;
mod debug_fx_marks;
mod debug_glow;
mod debug_lod;
mod debug_move;
mod debug_scalar;
mod debug_script_mover;
mod debug_sm;
mod debug_smc;
mod debug_view_proj;
mod debug_vision;
mod diagnostics;
pub mod editor;
mod feature_dispatch;
mod frontend;
mod game_folders;
mod gamepad;
pub mod input;
mod local_account;
mod local_profile;
pub mod plugin;
pub mod registry;
mod saved_position;
mod startup;
pub mod suggest;
mod synthetic_input;
mod user_settings;
mod weapon_dispatch;

pub use binds::{
    BINDABLE_KEYS, BindButton, BindInputs, DEFAULT_CONTROLS, KeyBinds, PadButton, display_button,
    host_keynum, parse_button_name, parse_key_name,
};
pub use class_dispatch::class_completions;
pub use command::{ConsoleCommand, ConsoleQueue, SubmittedCommand};
pub use editor::ConsoleEditor;
pub use feature_dispatch::register_feature_commands;
pub use game_folders::stored_game_folders;
pub use plugin::{
    ConsoleCommandQueue, ConsoleDispatch, ConsoleDispatchSet, ConsoleFont, ConsolePlugin,
    ConsoleSettings, ConsoleState,
};
pub use registry::{ArgCompleter, CommandSpec, ConsoleRegistry, StaticCompleter};
pub use startup::{startup_commands, strip_cmds_flag};
pub use synthetic_input::{ConsoleInputState, PRESS_SECONDS, PRESS_TICK_DT_MAX};
pub use weapon_dispatch::{attach_completions, weapon_completions};

pub(crate) use synthetic_input::is_bind_command;

use bevy::prelude::Resource;

#[derive(Resource, Debug, Default, Clone)]
pub struct ConsoleLine(pub String);
