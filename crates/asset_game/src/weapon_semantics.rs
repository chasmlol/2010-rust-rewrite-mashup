use asset_core::AssetNamespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThermalScopePolicy {
    Native { enabled: bool },
    T5OverlayNameCompatibility { enabled: bool },
}

impl ThermalScopePolicy {
    pub fn enabled(self) -> bool {
        match self {
            Self::Native { enabled } | Self::T5OverlayNameCompatibility { enabled } => enabled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeImpact {
    Hit,
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeCuePrecedence {
    T5AuthoredFirstCompatibility,
    KnifeFirstCompatibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeCuePolicy {
    pub precedence: MeleeCuePrecedence,
    pub knife_hit: &'static str,
    pub knife_miss: &'static str,
    pub generic_hit: &'static str,
    pub generic_miss: &'static str,
}

impl MeleeCuePolicy {
    pub fn generic(self, knife: bool, impact: MeleeImpact) -> &'static str {
        match (knife, impact) {
            (true, MeleeImpact::Hit) => self.knife_hit,
            (true, MeleeImpact::Miss) => self.knife_miss,
            (false, MeleeImpact::Hit) => self.generic_hit,
            (false, MeleeImpact::Miss) => self.generic_miss,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponCueNamespacePolicy {
    SourceNamespace(AssetNamespace),
    T6HostNamespaceCompatibility,
}

impl WeaponCueNamespacePolicy {
    pub fn namespace(self) -> AssetNamespace {
        match self {
            Self::SourceNamespace(namespace) => namespace,
            Self::T6HostNamespaceCompatibility => AssetNamespace::Iw4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeWeaponPolicy {
    Own,
    T5KnifeCompatibility { weapon: u32 },
}

impl MeleeWeaponPolicy {
    pub fn weapon(self, current: u32) -> u32 {
        match self {
            Self::Own => current,
            Self::T5KnifeCompatibility { weapon } => weapon,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BreathCueSource {
    T5ConventionCompatibility,
    IwConventionCompatibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BreathCuePolicy {
    pub source: BreathCueSource,
    pub inhale: &'static str,
    pub exhale: &'static str,
    pub gasp: &'static str,
    pub heartbeat: &'static str,
}

impl BreathCuePolicy {
    pub fn aliases(self) -> [&'static str; 4] {
        [self.inhale, self.exhale, self.gasp, self.heartbeat]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponSemanticPolicy {
    pub thermal_scope: ThermalScopePolicy,
    pub cue_namespace: WeaponCueNamespacePolicy,
    pub melee_cues: MeleeCuePolicy,
    pub melee_weapon: MeleeWeaponPolicy,
    pub breath_cues: BreathCuePolicy,
}

impl WeaponSemanticPolicy {
    pub(crate) fn compile(
        namespace: AssetNamespace,
        native_thermal: bool,
        overlay: Option<&str>,
        melee_weapon: MeleeWeaponPolicy,
    ) -> Self {
        let cue_namespace = match namespace {
            AssetNamespace::T6 => WeaponCueNamespacePolicy::T6HostNamespaceCompatibility,
            namespace => WeaponCueNamespacePolicy::SourceNamespace(namespace),
        };
        if namespace == AssetNamespace::T5 {
            Self {
                cue_namespace,
                melee_weapon,
                breath_cues: BreathCuePolicy {
                    source: BreathCueSource::T5ConventionCompatibility,
                    inhale: "wpn_sniper_breathin",
                    exhale: "wpn_sniper_breathout",
                    gasp: "wpn_sniper_breathgasp",
                    heartbeat: "wpn_sniper_heartbeat",
                },
                thermal_scope: ThermalScopePolicy::T5OverlayNameCompatibility {
                    enabled: overlay.is_some_and(|name| name.contains("_ir")),
                },
                melee_cues: MeleeCuePolicy {
                    precedence: MeleeCuePrecedence::T5AuthoredFirstCompatibility,
                    knife_hit: "wpn_melee_knife_hit_body",
                    knife_miss: "wpn_melee_knife_hit_other",
                    generic_hit: "wpn_melee_hit",
                    generic_miss: "wpn_melee_hit_other",
                },
            }
        } else {
            Self {
                cue_namespace,
                melee_weapon,
                breath_cues: BreathCuePolicy {
                    source: BreathCueSource::IwConventionCompatibility,
                    inhale: "weap_sniper_breathin",
                    exhale: "weap_sniper_breathout",
                    gasp: "weap_sniper_breathgasp",
                    heartbeat: "weap_sniper_heartbeat",
                },
                thermal_scope: ThermalScopePolicy::Native {
                    enabled: native_thermal,
                },
                melee_cues: MeleeCuePolicy {
                    precedence: MeleeCuePrecedence::KnifeFirstCompatibility,
                    knife_hit: "melee_knife_hit_body",
                    knife_miss: "melee_knife_hit_other",
                    generic_hit: "melee_hit",
                    generic_miss: "melee_hit_other",
                },
            }
        }
    }
}
