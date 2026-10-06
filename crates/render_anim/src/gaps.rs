use core::fmt;

use bevy::prelude::*;
use diag::gap::{self as ledger, Gap as _, GapLedger};

use render_material::SUPPORTED_OPCODE_SURFACE;

pub const UNSUPPORTED_COLOUR_PAIR_TAIL: usize = 74;

pub const SM3_OPCODE_SURFACE_SUPPORTED: usize = SUPPORTED_OPCODE_SURFACE.len();

const RENDER_GAP_COUNT: usize = <RenderGap as ledger::Gap>::ALL.len();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderGap {
    RemoteBodyAnimation,

    RemoteBodyWorldGun,

    FpvViewmodel,

    ThermalBodyMaterial,
}

impl ledger::Gap for RenderGap {
    const ALL: &'static [RenderGap] = &[
        RenderGap::RemoteBodyAnimation,
        RenderGap::RemoteBodyWorldGun,
        RenderGap::FpvViewmodel,
        RenderGap::ThermalBodyMaterial,
    ];

    fn name(self) -> &'static str {
        match self {
            RenderGap::RemoteBodyAnimation => "remote-body-animation",
            RenderGap::RemoteBodyWorldGun => "remote-body-world-gun",
            RenderGap::FpvViewmodel => "fpv-viewmodel",
            RenderGap::ThermalBodyMaterial => "thermal-body-material",
        }
    }

    fn is_standing(self) -> bool {
        false
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RenderGapCause {
    PlayerAnimSourcesNotPrepared,

    PlayerAnimSourceDecodeFailed {
        first: String,
    },

    MultiplayerAtrAbsent,

    PlayeranimScriptAbsent,

    AnimtreeCompilerMissing,

    AnimtreeCompileFailed {
        reason: String,
    },

    AnimScriptEvaluatorMissing,

    AnimScriptParseFailed {
        reason: String,
    },

    XAnimLeafBindMissing,

    XAnimCalcMissing,

    XAnimCalcFailed {
        reason: String,
    },

    RemoteBodySubmitMissing,

    RemoteBodyLightingAllocFailed,

    RemoteBodyMaterialMissing {
        name: String,
    },

    ThermalBodyMaterialMissing {
        name: String,
    },

    RemoteBodyWorldGunMissing {
        weapon: u32,
        world_model: String,
    },

    FpvCatalogMissing,

    FpvGunXModelUnresolved {
        weapon_id: u32,
    },

    FpvDependencyUnresolved {
        weapon_id: u32,
        role: &'static str,
        name: String,
    },

    FpvEyePoseFailed {
        gun_xmodel: String,
    },

    FpvNoCamera,

    FpvNoLightingAtlas,

    FpvNoActiveClips,
}

impl ledger::GapCause for RenderGapCause {
    type Gap = RenderGap;

    fn gap(&self) -> RenderGap {
        match self {
            RenderGapCause::PlayerAnimSourcesNotPrepared
            | RenderGapCause::PlayerAnimSourceDecodeFailed { .. }
            | RenderGapCause::MultiplayerAtrAbsent
            | RenderGapCause::PlayeranimScriptAbsent
            | RenderGapCause::AnimtreeCompilerMissing
            | RenderGapCause::AnimtreeCompileFailed { .. }
            | RenderGapCause::AnimScriptEvaluatorMissing
            | RenderGapCause::AnimScriptParseFailed { .. }
            | RenderGapCause::XAnimLeafBindMissing
            | RenderGapCause::XAnimCalcMissing
            | RenderGapCause::XAnimCalcFailed { .. }
            | RenderGapCause::RemoteBodySubmitMissing
            | RenderGapCause::RemoteBodyLightingAllocFailed
            | RenderGapCause::RemoteBodyMaterialMissing { .. } => RenderGap::RemoteBodyAnimation,
            RenderGapCause::ThermalBodyMaterialMissing { .. } => RenderGap::ThermalBodyMaterial,
            RenderGapCause::RemoteBodyWorldGunMissing { .. } => RenderGap::RemoteBodyWorldGun,
            RenderGapCause::FpvCatalogMissing
            | RenderGapCause::FpvGunXModelUnresolved { .. }
            | RenderGapCause::FpvDependencyUnresolved { .. }
            | RenderGapCause::FpvEyePoseFailed { .. }
            | RenderGapCause::FpvNoCamera
            | RenderGapCause::FpvNoLightingAtlas
            | RenderGapCause::FpvNoActiveClips => RenderGap::FpvViewmodel,
        }
    }
}

impl RenderGapCause {
    pub fn label(&self) -> &'static str {
        match self {
            RenderGapCause::PlayerAnimSourcesNotPrepared => "player animation sources not prepared",
            RenderGapCause::PlayerAnimSourceDecodeFailed { .. } => {
                "player animation source decode failed"
            }
            RenderGapCause::MultiplayerAtrAbsent => "animtrees/multiplayer.atr absent",
            RenderGapCause::PlayeranimScriptAbsent => "mp/playeranim.script absent",
            RenderGapCause::AnimtreeCompilerMissing => "ATR/script compiler missing",
            RenderGapCause::AnimtreeCompileFailed { .. } => "ATR compile failed",
            RenderGapCause::AnimScriptEvaluatorMissing => "playeranim evaluator missing",
            RenderGapCause::AnimScriptParseFailed { .. } => "playeranim.script parse failed",
            RenderGapCause::XAnimLeafBindMissing => "XAnim leaf bind missing",
            RenderGapCause::XAnimCalcMissing => "XAnimCalc missing",
            RenderGapCause::XAnimCalcFailed { .. } => "XAnimCalc failed",
            RenderGapCause::RemoteBodySubmitMissing => "remote body submit missing",
            RenderGapCause::RemoteBodyLightingAllocFailed => "remote body lighting alloc failed",
            RenderGapCause::RemoteBodyMaterialMissing { .. } => "remote body material missing",
            RenderGapCause::ThermalBodyMaterialMissing { .. } => "thermal body material missing",
            RenderGapCause::RemoteBodyWorldGunMissing { .. } => "remote body world gun missing",
            RenderGapCause::FpvCatalogMissing => "FPV catalog missing",
            RenderGapCause::FpvGunXModelUnresolved { .. } => "gunXModel[0] unresolved",
            RenderGapCause::FpvDependencyUnresolved { .. } => "FPV dependency unresolved",
            RenderGapCause::FpvEyePoseFailed { .. } => "FPV eye-pose failed",
            RenderGapCause::FpvNoCamera => "FPV waiting for camera",
            RenderGapCause::FpvNoLightingAtlas => "FPV hidden: ModelLightingCache atlas missing",
            RenderGapCause::FpvNoActiveClips => "FPV no active clips this frame",
        }
    }
}

impl fmt::Display for RenderGapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderGapCause::PlayerAnimSourceDecodeFailed { first } => {
                write!(f, "{}: {first}", self.label())
            }
            RenderGapCause::AnimtreeCompileFailed { reason } => {
                write!(f, "{}: {reason}", self.label())
            }
            RenderGapCause::AnimScriptParseFailed { reason } => {
                write!(f, "{}: {reason}", self.label())
            }
            RenderGapCause::XAnimCalcFailed { reason } => {
                write!(f, "{}: {reason}", self.label())
            }
            RenderGapCause::RemoteBodyMaterialMissing { name } if name.is_empty() => {
                write!(f, "{}: authored name not retained at capture", self.label())
            }
            RenderGapCause::ThermalBodyMaterialMissing { name }
            | RenderGapCause::RemoteBodyMaterialMissing { name } => {
                write!(f, "{}: `{name}` not in global catalog", self.label())
            }
            RenderGapCause::RemoteBodyWorldGunMissing {
                weapon,
                world_model,
            } => {
                write!(f, "{}: weapon {weapon} `{world_model}`", self.label())
            }
            RenderGapCause::FpvGunXModelUnresolved { weapon_id } => {
                write!(f, "{} for weapon {weapon_id}", self.label())
            }
            RenderGapCause::FpvDependencyUnresolved {
                weapon_id,
                role,
                name,
            } => {
                write!(
                    f,
                    "{} for weapon {weapon_id}: {role} `{name}`",
                    self.label()
                )
            }
            RenderGapCause::FpvEyePoseFailed { gun_xmodel } => {
                write!(
                    f,
                    "{} for `{gun_xmodel}` (need viewhands+gun skels with tag_view)",
                    self.label()
                )
            }
            other => f.write_str(other.label()),
        }
    }
}

#[derive(Resource, Debug)]
pub struct RenderPresentationGaps {
    inner: std::sync::Mutex<RenderPresentationGapsInner>,
}

#[derive(Debug, Default)]
struct RenderPresentationGapsInner {
    ledger: GapLedger<RenderGapCause, RENDER_GAP_COUNT>,
    reported: String,

    report_dirty: bool,
}

impl Default for RenderPresentationGaps {
    fn default() -> Self {
        Self {
            inner: std::sync::Mutex::new(RenderPresentationGapsInner::default()),
        }
    }
}

impl RenderPresentationGaps {
    fn lock(&self) -> std::sync::MutexGuard<'_, RenderPresentationGapsInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn raise(&self, cause: RenderGapCause) {
        if matches!(cause, RenderGapCause::RemoteBodyLightingAllocFailed) {
            perf::lighting_fail();
        }
        let mut inner = self.lock();
        inner.report_dirty |= inner.ledger.cause(ledger::GapCause::gap(&cause)) != Some(&cause);
        inner.ledger.raise(cause);
    }

    pub fn clear(&self, gap: RenderGap) {
        let mut inner = self.lock();
        inner.report_dirty |= inner.ledger.is_live(gap);
        inner.ledger.clear(gap);
    }

    pub fn is_live(&self, gap: RenderGap) -> bool {
        self.lock().ledger.is_live(gap)
    }

    pub fn cause(&self, gap: RenderGap) -> Option<RenderGapCause> {
        self.lock().ledger.cause(gap).cloned()
    }

    pub fn hits(&self, gap: RenderGap) -> u64 {
        self.lock().ledger.hits(gap)
    }

    pub fn live(&self) -> Vec<RenderGap> {
        self.lock().ledger.live().collect()
    }

    pub fn count(&self) -> usize {
        self.lock().ledger.count()
    }
}

pub(crate) fn report_render_gaps(gaps: Res<RenderPresentationGaps>) {
    let mut inner = gaps.lock();
    if !std::mem::take(&mut inner.report_dirty) {
        return;
    }
    let mut signature = String::new();
    inner
        .ledger
        .write_signature(&mut signature)
        .expect("writing into a String cannot fail");
    if signature == inner.reported {
        return;
    }
    inner.reported = signature;
    let live: Vec<RenderGap> = inner.ledger.live().collect();
    let causes: Vec<(RenderGap, Option<RenderGapCause>)> = live
        .iter()
        .map(|gap| (*gap, inner.ledger.cause(*gap).cloned()))
        .collect();
    drop(inner);
    if causes.is_empty() {
        diag::info!(World, "render: no presentation gaps live");
        return;
    }
    for (gap, cause) in causes {
        if let Some(cause) = cause {
            diag::info!(World, "render gap {}: {}", gap.name(), cause);
        }
    }
}

pub fn register_render_gaps(app: &mut App) {
    app.init_resource::<RenderPresentationGaps>()
        .add_systems(Update, report_render_gaps.in_set(net::ClientSet::Diag));
}
