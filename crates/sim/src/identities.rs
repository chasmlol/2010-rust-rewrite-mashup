#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifeSequence(pub u32);

impl LifeSequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventSequence(pub u32);

impl EventSequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    pub fn is_newer_than(self, other: Self) -> bool {
        let distance = self.0.wrapping_sub(other.0);
        distance != 0 && distance < (1 << 31)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionSequence(pub u32);

impl ActionSequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FireCause {
    pub client: crate::ClientId,
    pub life: LifeSequence,
    pub command: crate::CommandSequence,
    pub ordinal: u16,
    pub hand: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingBrass {
    pub cause: Option<FireCause>,
    pub life: LifeSequence,
    pub shot: ShotId,
    pub weapon: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShotId(pub u32);

impl ShotId {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PelletId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScriptModelId(pub(crate) u32);

impl ScriptModelId {
    pub const fn from_wire(ordinal: u32) -> Self {
        Self(ordinal)
    }

    pub const fn to_wire(self) -> u32 {
        self.0
    }

    pub const fn from_authored_source_ordinal(ordinal: u32) -> Self {
        Self(ordinal)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectileId(pub u32);

impl ProjectileId {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DamageSource {
    Shot(ShotId),
    Projectile(ProjectileId),

    Radius(ScriptModelId),

    Melee,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchPhase {
    #[default]
    Warmup,
    Playing,

    Intermission,
    PostGame,
}

pub const RNG_DOMAIN_SCHEME: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum RngDomain {
    Spawn = 1,
    Combat = 2,
    Bot = 3,
}

fn mix_root_domain(root: u64, domain: RngDomain) -> u64 {
    let mut z = root
        .wrapping_add((domain as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0xA5A5_A5A5_5A5A_5A5A);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRng {
    seed: u64,

    draws: u64,
    state: u64,
}

impl MatchRng {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            draws: 0,

            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn from_root(root: u64, domain: RngDomain) -> Self {
        Self::new(mix_root_domain(root, domain))
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn draws(&self) -> u64 {
        self.draws
    }

    pub(crate) fn restore_draws(&mut self, draws: u64) {
        self.draws = draws;
    }

    pub fn next_u32(&mut self) -> u32 {
        self.draws = self.draws.wrapping_add(1);

        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D)) >> 32) as u32
    }

    pub fn next_index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        (self.next_u32() as usize) % len
    }
}
