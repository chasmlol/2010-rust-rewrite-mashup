use std::collections::VecDeque;
use std::fmt;

use bevy::prelude::*;

use asset_core::AssetNamespace;

const START_DECISION_CAP: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartOutcome {
    Submitted,
    Pending,
    Suppressed(SuppressReason),
    Failed(StartFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuppressReason {
    Inaudible,
    VoiceLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundClass {
    Weapon,
    World,
    Ui,
    Music,
    Ambience,
}

impl SoundClass {
    pub fn start_wait(self) -> std::time::Duration {
        match self {
            Self::Weapon => std::time::Duration::from_millis(200),
            Self::World => std::time::Duration::from_millis(350),
            Self::Ui => std::time::Duration::from_millis(500),
            Self::Music | Self::Ambience => std::time::Duration::from_secs(30),
        }
    }

    pub fn scope(self) -> crate::backend::AudioScope {
        match self {
            Self::Ui => crate::backend::AudioScope::Menu,
            Self::Weapon | Self::World | Self::Music | Self::Ambience => {
                crate::backend::AudioScope::Match
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartFailure {
    AdmissionRefused(crate::AdmissionFailure),
    CueRefused(crate::CueFailure),
    BankChanged,
    BankMissing,
    MissingAlias,
    NoPcm,
    NoListener,
    NoFalloffCurve,
    UnsupportedSpatialPolicy(asset_core::AssetNamespace),
    MissingChannelPolicy(asset_core::AssetNamespace),
    FalloffEval,
    DecodeFailed,
    UnsupportedCodec(asset_audio::SabCodec),
    UnsupportedCueFeatures(std::sync::Arc<[asset_audio::UnsupportedCueFeature]>),
    MediaMetadataMismatch,
    MediaReadFailed,
    MediaRequestLimit,
    InvalidPcm(crate::media::PcmError),
    Expired,
    OutputUnavailable,
}

impl From<crate::clip_store::ClipError> for StartFailure {
    fn from(error: crate::clip_store::ClipError) -> Self {
        match error {
            crate::clip_store::ClipError::InvalidPcm(reason) => Self::InvalidPcm(reason),
            crate::clip_store::ClipError::RequestLimit => Self::MediaRequestLimit,
            crate::clip_store::ClipError::UnsupportedCodec(codec) => Self::UnsupportedCodec(codec),
            crate::clip_store::ClipError::MetadataMismatch
            | crate::clip_store::ClipError::ForeignOwner => Self::MediaMetadataMismatch,
            crate::clip_store::ClipError::Read => Self::MediaReadFailed,
            _ => Self::DecodeFailed,
        }
    }
}

impl StartOutcome {
    pub fn is_terminal_success(&self) -> bool {
        matches!(self, Self::Submitted)
    }

    pub fn allows_binding_fallback(&self) -> bool {
        matches!(
            self,
            Self::Failed(StartFailure::MissingAlias | StartFailure::NoPcm)
        )
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Self::Submitted | Self::Pending | Self::Suppressed(_))
    }
}

impl fmt::Display for StartOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Submitted => f.write_str("Submitted"),
            Self::Pending => f.write_str("Pending"),
            Self::Suppressed(SuppressReason::Inaudible) => f.write_str("SuppressedInaudible"),
            Self::Suppressed(SuppressReason::VoiceLimit) => f.write_str("SuppressedVoiceLimit"),
            Self::Failed(StartFailure::CueRefused(reason)) => write!(f, "CueRefused{reason:?}"),
            Self::Failed(StartFailure::BankChanged) => f.write_str("FailedBankChanged"),
            Self::Failed(StartFailure::BankMissing) => f.write_str("FailedBankMissing"),
            Self::Failed(StartFailure::AdmissionRefused(reason)) => {
                write!(f, "AdmissionRefused{reason:?}")
            }
            Self::Failed(StartFailure::MissingAlias) => f.write_str("FailedMissingAlias"),
            Self::Failed(StartFailure::NoPcm) => f.write_str("FailedNoPcm"),
            Self::Failed(StartFailure::NoListener) => f.write_str("FailedNoListener"),
            Self::Failed(StartFailure::NoFalloffCurve) => f.write_str("FailedNoFalloffCurve"),
            Self::Failed(StartFailure::UnsupportedSpatialPolicy(namespace)) => {
                write!(f, "UnsupportedSpatialPolicy{namespace:?}")
            }
            Self::Failed(StartFailure::MissingChannelPolicy(namespace)) => {
                write!(f, "MissingChannelPolicy{namespace:?}")
            }
            Self::Failed(StartFailure::FalloffEval) => f.write_str("FailedFalloffEval"),
            Self::Failed(StartFailure::MediaRequestLimit) => f.write_str("FailedMediaRequestLimit"),
            Self::Failed(StartFailure::DecodeFailed) => f.write_str("FailedDecode"),
            Self::Failed(StartFailure::UnsupportedCodec(codec)) => {
                write!(f, "UnsupportedCodec{codec:?}")
            }
            Self::Failed(StartFailure::UnsupportedCueFeatures(features)) => {
                write!(f, "UnsupportedCueFeatures{features:?}")
            }
            Self::Failed(StartFailure::MediaReadFailed) => f.write_str("FailedMediaRead"),
            Self::Failed(StartFailure::MediaMetadataMismatch) => {
                f.write_str("FailedMediaMetadataMismatch")
            }
            Self::Failed(StartFailure::InvalidPcm(reason)) => write!(f, "FailedPcm{reason:?}"),
            Self::Failed(StartFailure::Expired) => f.write_str("ExpiredStartDeadline"),
            Self::Failed(StartFailure::OutputUnavailable) => f.write_str("OutputUnavailable"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StartDecision {
    pub event: Option<crate::AudioEvent>,
    pub namespace: AssetNamespace,
    pub alias: String,
    pub variant: Option<usize>,
    pub loaded_binding_origin: Option<asset_audio::LoadedBindingOrigin>,
    pub outcome: StartOutcome,
    pub secondary: Option<(String, StartOutcome)>,

    pub detail: Option<String>,
}

impl StartDecision {
    pub fn line(&self) -> String {
        let variant = self
            .variant
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into());
        let mut line = format!(
            "audio: start alias=`{}:{}` variant={variant} result={}",
            self.namespace.as_str(),
            self.alias,
            self.outcome
        );
        if let Some(event) = self.event {
            line.push_str(&format!(" event={:?}", event.id));
        }
        if let Some(origin) = self.loaded_binding_origin {
            line.push_str(&format!(" loaded_binding={origin:?}"));
        }
        if let Some((sec, outcome)) = &self.secondary {
            line.push_str(&format!(" secondary=`{sec}` result={outcome}"));
        }
        if let Some(detail) = &self.detail {
            line.push(' ');
            line.push_str(detail);
        }
        line
    }
}

#[derive(Resource, Default, Debug)]
pub struct StartDecisions {
    entries: VecDeque<StartDecision>,
}

impl StartDecisions {
    pub fn record(&mut self, decision: StartDecision) {
        if self.entries.len() == START_DECISION_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(decision);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn lines(&self) -> impl Iterator<Item = String> + '_ {
        self.entries.iter().map(StartDecision::line)
    }
}
