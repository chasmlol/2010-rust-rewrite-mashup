use crate::frame::FrameWorld;
use entity_iw4::{TEAM_ALLIES, TEAM_AXIS, TEAM_FREE};
use gamemode_iw4::GameModeKind;
use gamemode_iw4::ffa::{SPAWN_CLASSNAME, START_SPAWN_CLASSNAME};
use movement_iw4::{CollisionBackend, GroundTraceInput};
use trace_iw4::Trace;

use crate::bullet_collision::{MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS};
use crate::identities::MatchRng;
use crate::match_state::ClassDef;

pub const SPAWN_IDEAL_DIST: f32 = 1600.0;

pub const SPAWN_BAD_DIST: f32 = 1200.0;

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredSpawnPoint {
    pub classname: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],

    pub script_linkto: String,

    pub script_destructable_area: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnDecision {
    pub classname: String,
    pub source_index: usize,
    pub raw_origin: [f32; 3],
    pub raw_angles: [f32; 3],
    pub traced_origin: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchBootstrap {
    pub spawns: Vec<AuthoredSpawnPoint>,
    pub classes: Vec<ClassDef>,
    pub bot_classes: Vec<ClassDef>,

    pub seed: u64,

    pub kind: gamemode_iw4::GameModeKind,

    pub allow_debug_actions: bool,

    pub score_limit: i32,

    pub time_limit_ms: u32,

    pub intermission_view: Option<AuthoredSpawnPoint>,

    pub airstrike_height: Option<f32>,
}

impl Default for MatchBootstrap {
    fn default() -> Self {
        let (score_limit, time_limit_ms) = crate::score::bootstrap_score_defaults();
        Self {
            spawns: Vec::new(),
            classes: Vec::new(),
            bot_classes: Vec::new(),
            seed: 0,
            kind: gamemode_iw4::GameModeKind::FreeForAll,
            allow_debug_actions: false,
            score_limit,
            time_limit_ms,
            intermission_view: None,
            airstrike_height: None,
        }
    }
}

pub fn host_game_mode_kind() -> GameModeKind {
    std::env::var("IW4L_GAMETYPE")
        .ok()
        .as_deref()
        .and_then(GameModeKind::parse_ascii_ignore_case)
        .unwrap_or(GameModeKind::FreeForAll)
}

#[derive(bevy_ecs::prelude::Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCheats(pub bool);

#[derive(bevy_ecs::prelude::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostGameModeSelection(GameModeKind);

impl HostGameModeSelection {
    pub const ALL: [Self; 8] = [
        Self(GameModeKind::FreeForAll),
        Self(GameModeKind::Domination),
        Self(GameModeKind::Demolition),
        Self(GameModeKind::TeamDeathmatch),
        Self(GameModeKind::SearchAndDestroy),
        Self(GameModeKind::CaptureTheFlag),
        Self(GameModeKind::Headquarters),
        Self(GameModeKind::Sabotage),
    ];

    pub fn from_token(token: &str) -> Option<Self> {
        GameModeKind::parse_ascii_ignore_case(token).map(Self)
    }

    pub fn from_env() -> Self {
        Self(host_game_mode_kind())
    }

    pub const fn kind(self) -> GameModeKind {
        self.0
    }

    pub fn token(self) -> &'static str {
        self.0.token()
    }

    pub fn display_name(self) -> &'static str {
        self.0.display_name()
    }
}

pub fn spawn_candidate_indices(spawns: &[AuthoredSpawnPoint]) -> Vec<usize> {
    spawn_candidate_indices_for(spawns, GameModeKind::FreeForAll, TEAM_FREE, true)
}

pub fn spawn_candidate_indices_for(
    spawns: &[AuthoredSpawnPoint],
    kind: GameModeKind,
    client_state_team: i32,
    use_start_spawns: bool,
) -> Vec<usize> {
    if kind.is_team() && (client_state_team == TEAM_AXIS || client_state_team == TEAM_ALLIES) {
        let axis = client_state_team == TEAM_AXIS;
        if use_start_spawns {
            if let Some(start) = kind.team_start_classname(axis) {
                let idx = filter_classname(spawns, |c| c == start);
                if !idx.is_empty() {
                    return idx;
                }
            }
        }
        let grid = kind.team_grid_classnames(axis);
        if !grid.is_empty() {
            let idx = filter_classname(spawns, |c| grid.iter().copied().any(|want| want == c));
            if !idx.is_empty() {
                return idx;
            }
        }
        return Vec::new();
    }
    ffa_candidate_indices(spawns)
}

fn ffa_candidate_indices(spawns: &[AuthoredSpawnPoint]) -> Vec<usize> {
    let dm: Vec<_> = spawns
        .iter()
        .enumerate()
        .filter(|(_, p)| p.classname == SPAWN_CLASSNAME)
        .map(|(i, _)| i)
        .collect();
    if !dm.is_empty() {
        return dm;
    }
    let start: Vec<_> = spawns
        .iter()
        .enumerate()
        .filter(|(_, p)| p.classname == START_SPAWN_CLASSNAME)
        .map(|(i, _)| i)
        .collect();
    if !start.is_empty() {
        return start;
    }
    (0..spawns.len()).collect()
}

fn filter_classname(spawns: &[AuthoredSpawnPoint], pred: impl Fn(&str) -> bool) -> Vec<usize> {
    spawns
        .iter()
        .enumerate()
        .filter(|(_, p)| pred(&p.classname))
        .map(|(i, _)| i)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnReject {
    NoAuthoredCandidates,
    NoGroundHit,
    StartSolid,
    Occupied,
    UnsupportedCoverage,
    AllRejected,
}

impl SpawnReject {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoAuthoredCandidates => "no_authored_candidates",
            Self::NoGroundHit => "no_ground_hit",
            Self::StartSolid => "start_solid",
            Self::Occupied => "occupied",
            Self::UnsupportedCoverage => "unsupported_coverage",
            Self::AllRejected => "all_rejected",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpawnAttemptReport {
    pub tried: u32,
    pub rejected: Vec<(usize, SpawnReject)>,
    pub accepted: Option<SpawnDecision>,
}

const PLACE_SPAWN_UP: f32 = 128.0;

const PLACE_SPAWN_DOWN: f32 = 262_144.0;

fn lerp3(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

struct SpawnClip<'a>(&'a FrameWorld<'a>);

impl CollisionBackend for SpawnClip<'_> {
    fn penetrations(
        &self,
        input: GroundTraceInput,
        contacts: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        self.0.penetrations(input, contacts)
    }

    fn trace(&self, input: GroundTraceInput) -> Trace {
        self.0.trace_clip(
            input.start,
            input.end,
            input.mins,
            input.maxs,
            input.tracemask,
        )
    }
}

pub(crate) fn ground_spawn(world: &FrameWorld, feet: [f32; 3]) -> Result<[f32; 3], SpawnReject> {
    if !world.has_world_clip() {
        return Err(SpawnReject::UnsupportedCoverage);
    }
    // A Minecraft world's spawn is at map origin; the stand-in map's own
    // spawn points may be buried in its terrain.
    if crate::voxel::active() {
        return Ok([0.0, 0.0, 0.0]);
    }
    let at_feet = world.trace_clip(feet, feet, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
    if at_feet.startsolid != 0 || at_feet.allsolid != 0 {
        return recover_spawn(world, feet);
    }
    let up_end = [feet[0], feet[1], feet[2] + PLACE_SPAWN_UP];
    let up = world.trace_clip(feet, up_end, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
    let raised = lerp3(feet, up_end, up.fraction);
    let down_end = [raised[0], raised[1], raised[2] - PLACE_SPAWN_DOWN];
    let down = world.trace_clip(
        raised,
        down_end,
        PLAYER_MINS,
        PLAYER_MAXS,
        MASK_PLAYER_SOLID,
    );
    if down.fraction >= 1.0 {
        return Err(if down.startsolid != 0 || down.allsolid != 0 {
            SpawnReject::StartSolid
        } else {
            SpawnReject::NoGroundHit
        });
    }
    let landed = lerp3(raised, down_end, down.fraction);
    let stuck = world.trace_clip(landed, landed, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
    if stuck.startsolid != 0 || stuck.allsolid != 0 {
        return recover_spawn(world, landed);
    }
    Ok(landed)
}

fn recover_spawn(world: &FrameWorld, origin: [f32; 3]) -> Result<[f32; 3], SpawnReject> {
    let fixed = SpawnClip(world)
        .correct_solid(origin, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID)
        .ok_or(SpawnReject::StartSolid)?;
    if fixed.trace.startsolid != 0 || fixed.trace.allsolid != 0 {
        return Err(SpawnReject::StartSolid);
    }
    if !fixed.trace.fraction.is_finite() || fixed.trace.fraction >= 1.0 {
        return Err(SpawnReject::NoGroundHit);
    }
    Ok(fixed.origin)
}

fn try_spawn_order(
    world: &FrameWorld,
    spawns: &[AuthoredSpawnPoint],
    order: &[usize],
    avoid: &[[f32; 3]],
) -> SpawnAttemptReport {
    let mut report = SpawnAttemptReport::default();
    for &source_index in order {
        report.tried += 1;
        let point = &spawns[source_index];
        match ground_spawn(world, point.origin) {
            Ok(traced_origin) => {
                if occupied(avoid, traced_origin) {
                    report.rejected.push((source_index, SpawnReject::Occupied));
                    continue;
                }
                report.accepted = Some(SpawnDecision {
                    classname: point.classname.clone(),
                    source_index,
                    raw_origin: point.origin,
                    raw_angles: point.angles,
                    traced_origin,
                });
                return report;
            }
            Err(reason) => report.rejected.push((source_index, reason)),
        }
    }
    if report.accepted.is_none() && !report.rejected.is_empty() {
        report.rejected.push((usize::MAX, SpawnReject::AllRejected));
    }
    report
}

fn occupied(avoid: &[[f32; 3]], origin: [f32; 3]) -> bool {
    avoid.iter().any(|other| {
        (other[0] - origin[0]).abs() < PLAYER_MAXS[0] - PLAYER_MINS[0]
            && (other[1] - origin[1]).abs() < PLAYER_MAXS[1] - PLAYER_MINS[1]
            && (other[2] - origin[2]).abs() < PLAYER_MAXS[2] - PLAYER_MINS[2]
    })
}

pub const FORCED_SPAWN_SOURCE: usize = usize::MAX;

pub(crate) fn decide_forced_spawn(
    world: &FrameWorld,
    pick: crate::SpawnPick,
    avoid: &[[f32; 3]],
    client_state_team: i32,
) -> SpawnAttemptReport {
    let mut refused_at = None;
    let seed = match pick {
        crate::SpawnPick::Seeded(seed) => seed,
        crate::SpawnPick::At { origin, yaw } => {
            match ground_spawn(world, origin) {
                Ok(traced) if !occupied(avoid, traced) => {
                    return SpawnAttemptReport {
                        tried: 1,
                        rejected: Vec::new(),
                        accepted: Some(SpawnDecision {
                            classname: String::from("forced"),
                            source_index: FORCED_SPAWN_SOURCE,
                            raw_origin: origin,
                            raw_angles: [0.0, yaw, 0.0],
                            traced_origin: traced,
                        }),
                    };
                }
                Ok(_) => refused_at = Some(SpawnReject::Occupied),
                Err(reason) => refused_at = Some(reason),
            }
            0
        }
    };
    let spawns = world.bootstrap_ref().spawns.clone();
    let kind = world.bootstrap_ref().kind;
    let mut order = spawn_candidate_indices_for(&spawns, kind, client_state_team, false);
    let mut rng = MatchRng::new(seed);
    for j in (1..order.len()).rev() {
        let k = rng.next_index(j + 1);
        order.swap(j, k);
    }
    let mut report = if order.is_empty() {
        SpawnAttemptReport {
            tried: 0,
            rejected: vec![(0, SpawnReject::NoAuthoredCandidates)],
            accepted: None,
        }
    } else {
        try_spawn_order(world, &spawns, &order, avoid)
    };
    if let Some(reason) = refused_at {
        report.tried += 1;
        report.rejected.insert(0, (FORCED_SPAWN_SOURCE, reason));
    }
    report
}

pub fn pick_ffa_spawn(spawns: &[AuthoredSpawnPoint]) -> Option<(usize, &AuthoredSpawnPoint)> {
    spawns
        .iter()
        .enumerate()
        .find(|(_, p)| p.classname == START_SPAWN_CLASSNAME)
        .or_else(|| {
            spawns
                .iter()
                .enumerate()
                .find(|(_, p)| p.classname == SPAWN_CLASSNAME)
        })
        .or_else(|| spawns.first().map(|p| (0, p)))
}
