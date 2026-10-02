//! Jak 3's gameplay, rebuilt in Rust: Jak on foot, the JET-Board and the
//! Blaster, with their own collision against whatever world the host hands
//! over.
//!
//! The simulation keeps Jak's own units (4096 per meter, y up, 300 ticks a
//! second) and runs at the 60 Hz the game's movement was tuned at; the host
//! converts at the boundary and feeds one [`Jak::step`] per frame.
pub mod board;
pub mod board_anim;
pub mod collide;
pub mod control;
pub mod gun;
pub mod math;
pub mod pad;
pub mod projectile;
pub mod surface;
mod target;

pub use glam;
use glam::{Quat, Vec3};

pub use board::{BoardInfo, BoardTrick};
pub use board_anim::BoardAnim;
pub use collide::{
    CollideCache, CollideWorld, EmptyWorld, Pat, PatMaterial, PatMode, Tri, TriangleGrid,
};
pub use control::Control;
pub use gun::Gun;
pub use math::{Basis, FRAME_TICKS, METER, SECONDS_PER_FRAME, TICKS_PER_SECOND};
pub use pad::{Pad, PadInput, button};
pub use projectile::{ActorWorld, Projectile, ProjectileHit};

/// What Jak is doing. On-foot states first, board states after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Stance,
    Walk,
    Jump { min: f32, max: f32 },
    DoubleJump { min: f32, max: f32 },
    Falling,
    HitGround,
    BoardGetOn,
    BoardStance,
    BoardDuckStance,
    BoardJump { min: f32, max: f32, duck: bool },
    BoardFalling,
    BoardHitGround,
    BoardTurnTo { dir: Vec3, duration: i64 },
    BoardFlip,
    BoardTrick(BoardTrick),
    BoardGetOff,
}

impl State {
    pub fn is_board(&self) -> bool {
        !matches!(
            self,
            State::Stance
                | State::Walk
                | State::Jump { .. }
                | State::DoubleJump { .. }
                | State::Falling
                | State::HitGround
        )
    }

    pub(crate) fn kind(&self) -> board::StateKind {
        use board::StateKind as K;
        match self {
            State::BoardGetOn => K::BoardGetOn,
            State::BoardStance => K::BoardStance,
            State::BoardDuckStance => K::BoardDuckStance,
            State::BoardJump { .. } => K::BoardJump,
            State::BoardFalling => K::BoardFalling,
            State::BoardHitGround => K::BoardHitGround,
            State::BoardTurnTo { .. } => K::BoardTurnTo,
            State::BoardFlip => K::BoardFlip,
            State::BoardTrick(_) => K::BoardTrick,
            State::BoardGetOff => K::BoardGetOff,
            _ => K::OnFoot,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            State::Stance => "stance",
            State::Walk => "walk",
            State::Jump { .. } => "jump",
            State::DoubleJump { .. } => "double-jump",
            State::Falling => "falling",
            State::HitGround => "hit-ground",
            State::BoardGetOn => "board-get-on",
            State::BoardStance => "board-stance",
            State::BoardDuckStance => "board-duck-stance",
            State::BoardJump { .. } => "board-jump",
            State::BoardFalling => "board-falling",
            State::BoardHitGround => "board-hit-ground",
            State::BoardTurnTo { .. } => "board-turn-to",
            State::BoardFlip => "board-flip",
            State::BoardTrick(_) => "board-trick",
            State::BoardGetOff => "board-get-off",
        }
    }
}

/// Board tricks, as the combo counts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trick {
    Spin,
    Boost,
    Flip,
    Jump,
    DuckJump,
    QuickJump,
    Nosegrab,
    Noseflip,
    Kickspin,
    Kickflip,
    BoardSpin,
    Method,
}

/// Things the host presents: sounds, effects, attacks on the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    BoardOn,
    BoardOff,
    BoardJump,
    BoardLaunch,
    BoardFlip,
    BoardBoost,
    BoardGlance,
    BoardBounce {
        pitch: f32,
    },
    BoardZap {
        center: Vec3,
        radius: f32,
    },
    Trick {
        trick: Trick,
        points: f32,
    },
    Jump,
    Land,
    /// The Blaster fired: muzzle and direction.
    Fire {
        from: Vec3,
        dir: Vec3,
    },
    /// A shot stopped: where, the surface it struck and the damage it does.
    Impact(ProjectileHit),
}

/// Jak, his board and his gun.
pub struct Jak {
    pub control: Control,
    pub board: BoardInfo,
    pub gun: Gun,
    pub pad: Pad,
    pub state: State,
    pub state_time: i64,
    /// The game clock, in ticks.
    pub time: i64,
    /// The camera's frame: the stick is read through it.
    pub camera: Basis,
    pub projectiles: Vec<Projectile>,
    pub(crate) events: Vec<Event>,
    pub(crate) cache: CollideCache,
    pub(crate) probe_cache: CollideCache,
    pub(crate) pending: Option<State>,
}

impl Jak {
    /// Jak standing at `trans` facing `yaw` (rotation units, 0 along +z).
    pub fn new(trans: Vec3, yaw: f32) -> Self {
        let time = 0;
        Self {
            control: Control::new(trans, yaw, time),
            board: BoardInfo::default(),
            gun: Gun::default(),
            pad: Pad::default(),
            state: State::Falling,
            state_time: time,
            time,
            camera: Basis::IDENTITY,
            projectiles: Vec::new(),
            events: Vec::new(),
            cache: CollideCache::default(),
            probe_cache: CollideCache::default(),
            pending: None,
        }
    }

    pub fn trans(&self) -> Vec3 {
        self.control.trans
    }

    pub fn velocity(&self) -> Vec3 {
        self.control.transv
    }

    pub fn orientation(&self) -> Quat {
        self.control.quat
    }

    /// The events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Mount the board at once, without the button, as if R2 had been held
    /// with room above.
    pub fn request_board(&mut self) {
        self.board.latch = true;
    }

    /// One 60 Hz frame: input, the state's decisions, the movement, then the
    /// shots in flight.
    pub fn step(
        &mut self,
        input: &PadInput,
        world: &mut dyn CollideWorld,
        actors: Option<&mut dyn ActorWorld>,
    ) {
        self.time += FRAME_TICKS;
        self.pad.update(input);
        let mut guard = 0;
        while let Some(next) = self.decide(world) {
            self.go(next);
            guard += 1;
            if guard > 8 {
                break;
            }
        }
        if matches!(
            self.state,
            State::BoardStance | State::BoardDuckStance | State::BoardTurnTo { .. }
        ) {
            self.board_turn_anim();
        }
        self.post(world);
        if let Some(next) = self.pending.take() {
            self.go(next);
        }
        self.gun_frame(world);
        self.step_projectiles_with(world, actors);
    }

    /// Leaves the current state for `next`: the old state's exit runs with
    /// the new one already known, then the new one's entry.
    pub(crate) fn go(&mut self, next: State) {
        let old = self.state;
        if old.is_board() {
            self.board_exit_state(&old, &next);
        } else {
            self.foot_exit(&old, &next);
        }
        self.state = next;
        self.state_time = self.time;
        if next.is_board() {
            self.board_enter(&next);
        } else {
            self.foot_enter(&next);
        }
    }

    fn decide(&mut self, world: &mut dyn CollideWorld) -> Option<State> {
        if self.state.is_board() {
            self.board_trans()
        } else {
            self.foot_trans(world)
        }
    }

    fn post(&mut self, world: &mut dyn CollideWorld) {
        if self.state.is_board() && self.state != State::BoardGetOff {
            self.board_post(world);
        } else {
            self.target_post(world);
        }
    }
}
