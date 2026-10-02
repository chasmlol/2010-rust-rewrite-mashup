//! Movement tuning tables. A movement mode (walking, a board jump, a spin…)
//! is a table of rates and limits; the ground under Jak is a second table of
//! multipliers; the two are multiplied field by field every frame and the
//! mode's hook then rewrites the few fields that depend on what Jak is doing.

pub mod flag {
    pub const LOOK_AROUND: u32 = 1 << 0;
    pub const XZ_LOCAL: u32 = 1 << 1;
    pub const NO_TURN_AROUND: u32 = 1 << 3;
    pub const TURN_TO_PAD: u32 = 1 << 4;
    pub const TURN_TO_VEL: u32 = 1 << 5;
    pub const NO_JUMP: u32 = 1 << 6;
    pub const NO_ATTACK: u32 = 1 << 7;
    pub const NO_FEET: u32 = 1 << 9;
    pub const CHECK_EDGE: u32 = 1 << 10;
    pub const AIR: u32 = 1 << 11;
    pub const DUCK: u32 = 1 << 13;
    pub const TURN_WHEN_CENTERED: u32 = 1 << 15;
    pub const TURN_TO_ALT: u32 = 1 << 16;
    pub const SPIN: u32 = 1 << 17;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Name {
    #[default]
    Current,
    Run,
    Duck,
    Air,
    Jump,
    JumpDouble,
    Spin,
    Stone,
    Edge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Ground,
    Air,
    Ride,
}

/// Which rewrite runs after the multiply, and so which of Jak's state the
/// table depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Hook {
    #[default]
    None,
    BoardWalk,
    BoardAir,
    BoardRideJump,
    BoardWallKick,
    BoardRide,
    GunWalk,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Surface {
    pub name: Name,
    pub turnv: f32,
    pub turnvf: f32,
    pub turnvv: f32,
    pub turnvvf: f32,
    pub tiltv: f32,
    pub tiltvf: f32,
    pub tiltvv: f32,
    pub tiltvvf: f32,
    pub vel_turn: f32,
    pub transv_max: f32,
    pub target_speed: f32,
    pub seek0: f32,
    pub seek90: f32,
    pub seek180: f32,
    pub fric: f32,
    pub nonlin_fric_dist: f32,
    pub slip_factor: f32,
    pub slide_factor: f32,
    pub slope_up_factor: f32,
    pub slope_down_factor: f32,
    pub slope_slip_angle: f32,
    pub impact_fric: f32,
    pub bend_factor: f32,
    pub bend_speed: f32,
    pub alignv: f32,
    pub slope_up_traction: f32,
    pub align_speed: f32,
    pub slope_change_preserve: f32,
    pub hook: Hook,
    pub mode: Mode,
    pub flags: u32,
}

const ZERO: Surface = Surface {
    name: Name::Current,
    turnv: 0.0,
    turnvf: 0.0,
    turnvv: 0.0,
    turnvvf: 0.0,
    tiltv: 0.0,
    tiltvf: 0.0,
    tiltvv: 0.0,
    tiltvvf: 0.0,
    vel_turn: 0.0,
    transv_max: 0.0,
    target_speed: 0.0,
    seek0: 0.0,
    seek90: 0.0,
    seek180: 0.0,
    fric: 0.0,
    nonlin_fric_dist: 0.0,
    slip_factor: 0.0,
    slide_factor: 0.0,
    slope_up_factor: 0.0,
    slope_down_factor: 0.0,
    slope_slip_angle: 0.0,
    impact_fric: 0.0,
    bend_factor: 0.0,
    bend_speed: 0.0,
    alignv: 0.0,
    slope_up_traction: 0.0,
    align_speed: 0.0,
    slope_change_preserve: 0.0,
    hook: Hook::None,
    mode: Mode::Ground,
    flags: 0,
};

impl Surface {
    /// `mods` scaled by the ground `surf`. The mode and the hook follow the
    /// ground when it has one of its own; the flags are both tables'.
    pub fn mult(mods: &Surface, surf: &Surface) -> Surface {
        Surface {
            name: mods.name,
            turnv: mods.turnv * surf.turnv,
            turnvf: mods.turnvf * surf.turnvf,
            turnvv: mods.turnvv * surf.turnvv,
            turnvvf: mods.turnvvf * surf.turnvvf,
            tiltv: mods.tiltv * surf.tiltv,
            tiltvf: mods.tiltvf * surf.tiltvf,
            tiltvv: mods.tiltvv * surf.tiltvv,
            tiltvvf: mods.tiltvvf * surf.tiltvvf,
            vel_turn: mods.vel_turn * surf.vel_turn,
            transv_max: mods.transv_max * surf.transv_max,
            target_speed: mods.target_speed * surf.target_speed,
            seek0: mods.seek0 * surf.seek0,
            seek90: mods.seek90 * surf.seek90,
            seek180: mods.seek180 * surf.seek180,
            fric: mods.fric * surf.fric,
            nonlin_fric_dist: mods.nonlin_fric_dist * surf.nonlin_fric_dist,
            slip_factor: mods.slip_factor * surf.slip_factor,
            slide_factor: mods.slide_factor * surf.slide_factor,
            slope_up_factor: mods.slope_up_factor * surf.slope_up_factor,
            slope_down_factor: mods.slope_down_factor * surf.slope_down_factor,
            slope_slip_angle: mods.slope_slip_angle * surf.slope_slip_angle,
            impact_fric: mods.impact_fric * surf.impact_fric,
            bend_factor: mods.bend_factor * surf.bend_factor,
            bend_speed: mods.bend_speed * surf.bend_speed,
            alignv: mods.alignv * surf.alignv,
            slope_up_traction: mods.slope_up_traction * surf.slope_up_traction,
            align_speed: mods.align_speed * surf.align_speed,
            slope_change_preserve: mods.slope_change_preserve * surf.slope_change_preserve,
            hook: if surf.hook != Hook::None {
                surf.hook
            } else {
                mods.hook
            },
            mode: surf.mode,
            flags: mods.flags | surf.flags,
        }
    }
}

/// The ground most of the world is: every rate passes through, the seek and
/// friction rates are set here.
pub const STONE: Surface = Surface {
    name: Name::Stone,
    turnv: 1.0,
    turnvf: 1.0,
    turnvv: 1.0,
    turnvvf: 1.0,
    tiltv: 1.0,
    tiltvf: 1.0,
    tiltvv: 1.0,
    tiltvvf: 1.0,
    vel_turn: 1.0,
    transv_max: 1.0,
    target_speed: 1.0,
    seek0: 153600.0,
    seek90: 153600.0,
    seek180: 256000.0,
    fric: 153600.0,
    nonlin_fric_dist: 5120.0,
    slip_factor: 1.0,
    slope_down_factor: 10240.0,
    slope_slip_angle: 10922.667,
    impact_fric: 1.0,
    bend_factor: 0.8,
    bend_speed: 4.0,
    alignv: 1.0,
    slope_up_traction: 1.0,
    align_speed: 1.0,
    slope_change_preserve: 1.0,
    ..ZERO
};

/// The ground at a ledge Jak's spheres only partly cover.
pub const EDGE: Surface = Surface {
    name: Name::Edge,
    turnv: 1.0,
    turnvf: 1.0,
    turnvv: 1.0,
    turnvvf: 1.0,
    tiltv: 1.0,
    tiltvf: 1.0,
    tiltvv: 1.0,
    tiltvvf: 1.0,
    vel_turn: 1.0,
    transv_max: 1.0,
    target_speed: 1.0,
    seek0: 153600.0,
    seek90: 153600.0,
    seek180: 256000.0,
    fric: 30720.0,
    nonlin_fric_dist: 5120.0,
    slip_factor: 1.0,
    slope_down_factor: 18432.0,
    slope_slip_angle: 10922.667,
    bend_factor: 0.8,
    bend_speed: 4.0,
    alignv: 1.0,
    align_speed: 1.0,
    ..ZERO
};

const UNIT_REST: Surface = Surface {
    nonlin_fric_dist: 1.0,
    slip_factor: 1.0,
    slide_factor: 1.0,
    slope_up_factor: 1.0,
    slope_down_factor: 1.0,
    slope_slip_angle: 1.0,
    impact_fric: 1.0,
    bend_factor: 1.0,
    bend_speed: 1.0,
    alignv: 1.0,
    slope_up_traction: 1.0,
    align_speed: 1.0,
    ..ZERO
};

pub const WALK: Surface = Surface {
    name: Name::Run,
    turnv: 131072.0,
    turnvf: 30.0,
    turnvv: 524288.0,
    turnvvf: 30.0,
    tiltv: 65536.0,
    tiltvf: 150.0,
    tiltvv: 262144.0,
    tiltvvf: 15.0,
    transv_max: 40960.0,
    target_speed: 40960.0,
    seek0: 1.0,
    seek90: 1.0,
    seek180: 1.0,
    fric: 1.0,
    flags: flag::LOOK_AROUND,
    ..UNIT_REST
};

pub const JUMP: Surface = Surface {
    name: Name::Jump,
    turnv: 131072.0,
    turnvf: 30.0,
    turnvv: 18204.445,
    turnvvf: 30.0,
    tiltv: 32768.0,
    tiltvf: 150.0,
    tiltvv: 262144.0,
    tiltvvf: 15.0,
    transv_max: 40960.0,
    target_speed: 40960.0,
    seek0: 0.3,
    seek90: 0.3,
    seek180: 0.3,
    fric: 0.2,
    mode: Mode::Air,
    flags: flag::CHECK_EDGE | flag::AIR,
    ..UNIT_REST
};

pub const DOUBLE_JUMP: Surface = Surface {
    name: Name::JumpDouble,
    transv_max: 32768.0,
    target_speed: 32768.0,
    ..JUMP
};

pub const GUN_WALK: Surface = Surface {
    name: Name::Run,
    turnv: 18204.445,
    turnvf: 60.0,
    turnvv: 72817.78,
    turnvvf: 300.0,
    tiltv: 65536.0,
    tiltvf: 150.0,
    tiltvv: 262144.0,
    tiltvvf: 15.0,
    transv_max: 40960.0,
    target_speed: 40960.0,
    seek0: 1.0,
    seek90: 1.0,
    seek180: 1.0,
    fric: 1.0,
    hook: Hook::GunWalk,
    flags: flag::LOOK_AROUND | flag::NO_TURN_AROUND,
    ..UNIT_REST
};

pub const TURN_AROUND: Surface = Surface {
    name: Name::Run,
    tiltv: 65536.0,
    tiltvf: 150.0,
    tiltvv: 262144.0,
    tiltvvf: 15.0,
    transv_max: 40960.0,
    target_speed: 40960.0,
    fric: 0.1,
    flags: flag::NO_TURN_AROUND,
    ..UNIT_REST
};

pub mod board {
    use super::{Hook, Mode, Name, Surface, UNIT_REST, flag};

    pub const WALK: Surface = Surface {
        name: Name::Run,
        turnv: 32768.0,
        turnvf: 30.0,
        turnvv: 131072.0,
        turnvvf: 30.0,
        tiltv: 131072.0,
        tiltvf: 15.0,
        tiltvv: 2621440.0,
        tiltvvf: 15.0,
        vel_turn: 65536.0,
        transv_max: 143360.0,
        target_speed: 102400.0,
        seek0: 0.5,
        seek90: 0.5,
        seek180: 0.5,
        fric: 0.2,
        slip_factor: -0.125,
        slope_up_factor: 8192.0,
        slope_down_factor: 24576.0,
        slope_change_preserve: 1.0,
        impact_fric: 0.0,
        hook: Hook::BoardWalk,
        flags: flag::TURN_TO_PAD,
        ..UNIT_REST
    };

    pub const DUCK: Surface = Surface {
        name: Name::Duck,
        turnv: 21845.334,
        transv_max: 151552.0,
        slip_factor: 0.5,
        slope_change_preserve: 0.75,
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_PAD | flag::DUCK,
        ..WALK
    };

    pub const AIR: Surface = Surface {
        name: Name::Air,
        turnv: 49152.0,
        turnvf: 30.0,
        turnvv: 131072.0,
        turnvvf: 30.0,
        tiltv: 16384.0,
        tiltvf: 150.0,
        tiltvv: 131072.0,
        tiltvvf: 60.0,
        vel_turn: 65536.0,
        transv_max: 143360.0,
        target_speed: 102400.0,
        seek0: 0.8,
        seek90: 0.8,
        seek180: 0.8,
        fric: 0.2,
        slip_factor: 1.0,
        slope_up_factor: 8192.0,
        slope_down_factor: 24576.0,
        slope_change_preserve: 1.0,
        impact_fric: 0.0,
        hook: Hook::BoardAir,
        mode: Mode::Air,
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_PAD | flag::CHECK_EDGE | flag::AIR,
        ..UNIT_REST
    };

    pub const JUMP: Surface = Surface {
        name: Name::Jump,
        turnv: 49152.0,
        turnvf: 30.0,
        turnvv: 524288.0,
        turnvvf: 30.0,
        tiltv: 32768.0,
        tiltvf: 30.0,
        tiltvv: 131072.0,
        tiltvvf: 18.0,
        vel_turn: 10922.667,
        transv_max: 143360.0,
        target_speed: 102400.0,
        fric: 0.2,
        slope_up_factor: 0.0,
        slope_down_factor: 0.0,
        impact_fric: 0.0,
        hook: Hook::BoardAir,
        mode: Mode::Air,
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_PAD | flag::CHECK_EDGE | flag::AIR,
        ..UNIT_REST
    };

    pub const DUCK_JUMP: Surface = JUMP;

    pub const RIDE_JUMP: Surface = Surface {
        name: Name::Jump,
        turnv: 49152.0,
        turnvf: 30.0,
        turnvv: 32768.0,
        turnvvf: 300.0,
        tiltv: 32768.0,
        tiltvf: 30.0,
        tiltvv: 131072.0,
        tiltvvf: 18.0,
        transv_max: 143360.0,
        target_speed: 102400.0,
        slope_up_factor: 0.0,
        slope_down_factor: 0.0,
        impact_fric: 0.0,
        hook: Hook::BoardRideJump,
        mode: Mode::Air,
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_PAD | flag::CHECK_EDGE | flag::AIR,
        ..UNIT_REST
    };

    pub const SPIN: Surface = Surface {
        name: Name::Spin,
        seek0: 0.0,
        seek90: 0.0,
        seek180: 0.0,
        vel_turn: 0.0,
        turnv: 0.0,
        turnvv: 0.0,
        tiltv: 0.0,
        tiltvf: 0.0,
        flags: flag::NO_TURN_AROUND | flag::CHECK_EDGE | flag::AIR,
        ..JUMP
    };

    pub const SPIN_POST: Surface = Surface {
        seek0: 0.0,
        seek90: 0.0,
        seek180: 0.0,
        vel_turn: 0.0,
        ..JUMP
    };

    pub const FLIP: Surface = Surface {
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_VEL | flag::AIR,
        seek0: 0.0,
        seek90: 0.0,
        seek180: 0.0,
        vel_turn: 0.0,
        turnv: 49152.0,
        turnvf: 30.0,
        turnvv: 0.0,
        turnvvf: 0.0,
        ..DUCK_JUMP
    };

    pub const WALL_KICK: Surface = Surface {
        name: Name::Jump,
        tiltv: 65536.0,
        tiltvf: 150.0,
        tiltvv: 262144.0,
        tiltvvf: 60.0,
        transv_max: 143360.0,
        target_speed: 102400.0,
        seek180: 0.8,
        fric: 0.2,
        slope_up_factor: 0.0,
        slope_down_factor: 0.0,
        impact_fric: 0.8,
        hook: Hook::BoardWallKick,
        mode: Mode::Air,
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_VEL | flag::AIR,
        ..UNIT_REST
    };

    pub const TURN_TO: Surface = Surface {
        name: Name::Run,
        turnv: 524288.0,
        turnvf: 30.0,
        turnvv: 0.0,
        turnvvf: 0.0,
        tiltv: 131072.0,
        tiltvf: 15.0,
        tiltvv: 2621440.0,
        tiltvvf: 15.0,
        vel_turn: 65536.0,
        transv_max: 143360.0,
        target_speed: 102400.0,
        seek0: 0.5,
        seek90: 0.5,
        seek180: 0.5,
        fric: 0.2,
        slip_factor: 0.5,
        slope_up_factor: 8192.0,
        slope_down_factor: 24576.0,
        slope_change_preserve: 1.0,
        impact_fric: 0.0,
        hook: Hook::BoardWalk,
        flags: flag::NO_TURN_AROUND | flag::TURN_TO_VEL | flag::TURN_WHEN_CENTERED,
        ..UNIT_REST
    };

    pub const RIDE: Surface = Surface {
        name: Name::Run,
        turnv: 218453.33,
        turnvf: 30.0,
        turnvv: 131072.0,
        turnvvf: 30.0,
        tiltv: 131072.0,
        tiltvf: 60.0,
        tiltvv: 262144.0,
        tiltvvf: 30.0,
        transv_max: 143360.0,
        target_speed: 40960.0,
        seek0: 0.5,
        seek90: 0.5,
        seek180: 0.5,
        fric: 0.1,
        slip_factor: 0.6,
        slope_up_factor: 24576.0,
        slope_down_factor: 49152.0,
        bend_factor: 0.0,
        hook: Hook::BoardRide,
        mode: Mode::Ride,
        flags: flag::NO_TURN_AROUND
            | flag::TURN_TO_PAD
            | flag::CHECK_EDGE
            | flag::TURN_WHEN_CENTERED,
        ..UNIT_REST
    };

    /// The hop off the board.
    pub const GET_OFF: Surface = Surface {
        name: Name::Jump,
        turnv: 131072.0,
        turnvf: 30.0,
        turnvv: 18204.445,
        turnvvf: 30.0,
        tiltv: 131072.0,
        tiltvf: 30.0,
        tiltvv: 262144.0,
        tiltvvf: 15.0,
        transv_max: 65536.0,
        target_speed: 65536.0,
        seek0: 0.3,
        seek90: 0.3,
        seek180: 0.3,
        fric: 0.05,
        mode: Mode::Air,
        flags: flag::CHECK_EDGE | flag::AIR,
        ..UNIT_REST
    };
}
