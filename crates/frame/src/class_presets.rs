use crate::HostClassSlot;

#[derive(Debug, Clone, Copy)]
pub struct ClassPreset {
    pub name: &'static str,
    pub primary: &'static str,
    pub primary_attachments: &'static [&'static str],
    pub secondary: &'static str,
    pub secondary_attachments: &'static [&'static str],
    pub lethal: &'static str,
    pub tactical: &'static str,
    pub perks: [&'static str; 3],
    pub deathstreak: &'static str,
}

impl From<&ClassPreset> for HostClassSlot {
    fn from(preset: &ClassPreset) -> Self {
        let owned = |list: &[&str]| list.iter().map(|value| (*value).to_owned()).collect();
        Self {
            name: preset.name.to_owned(),
            primary: preset.primary.to_owned(),
            primary_attachments: owned(preset.primary_attachments),
            secondary: preset.secondary.to_owned(),
            secondary_attachments: owned(preset.secondary_attachments),
            lethal: preset.lethal.to_owned(),
            tactical: preset.tactical.to_owned(),
            perks: preset.perks.map(str::to_owned),
            deathstreak: preset.deathstreak.to_owned(),
            camos: Default::default(),
        }
    }
}

pub fn showcase_classes() -> &'static [ClassPreset] {
    &SHOWCASE
}

const SHOWCASE: [ClassPreset; 20] = [
    ClassPreset {
        name: "ballistic_knife",
        primary: "t5:weapon/spectre_mp",
        primary_attachments: &["rf"],
        secondary: "t5:weapon/knife_ballistic_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/throwingknife_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_marathon",
            "specialty_lightweight",
            "specialty_extendedmelee",
        ],
        deathstreak: "specialty_finalstand",
    },
    ClassPreset {
        name: "ksg_breacher",
        primary: "iw5:weapon/iw5_ksg_mp",
        primary_attachments: &["grip"],
        secondary: "iw5:weapon/iw5_p99_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/semtex_mp",
        tactical: "iw4:weapon/concussion_grenade_mp",
        perks: [
            "specialty_marathon",
            "specialty_coldblooded",
            "specialty_extendedmelee",
        ],
        deathstreak: "specialty_combathigh",
    },
    ClassPreset {
        name: "intervention",
        primary: "iw4:weapon/cheytac_mp",
        primary_attachments: &["fmj", "heartbeat"],
        secondary: "iw4:weapon/javelin_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/claymore_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",
        perks: [
            "specialty_bling",
            "specialty_coldblooded",
            "specialty_localjammer",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "akimbo_uzi",
        primary: "iw4:weapon/uzi_mp",
        primary_attachments: &["akimbo", "rof"],
        secondary: "iw4:weapon/model1887_mp",
        secondary_attachments: &["akimbo"],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_bling",
            "specialty_lightweight",
            "specialty_extendedmelee",
        ],
        deathstreak: "specialty_combathigh",
    },
    ClassPreset {
        name: "shield_tacknife",
        primary: "iw4:weapon/riotshield_mp",
        primary_attachments: &[],
        secondary: "iw4:weapon/usp_mp",
        secondary_attachments: &["tactical", "fmj"],
        lethal: "iw4:weapon/throwingknife_mp",
        tactical: "iw4:weapon/concussion_grenade_mp",
        perks: [
            "specialty_bling",
            "specialty_lightweight",
            "specialty_extendedmelee",
        ],
        deathstreak: "specialty_finalstand",
    },
    ClassPreset {
        name: "crossbow",
        primary: "t5:weapon/galil_mp",
        primary_attachments: &["acog"],
        secondary: "t5:weapon/crossbow_explosive_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/claymore_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",
        perks: [
            "specialty_scavenger",
            "specialty_coldblooded",
            "specialty_heartbreaker",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "china_lake",
        primary: "t5:weapon/commando_mp",
        primary_attachments: &["silencer"],
        secondary: "t5:weapon/china_lake_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/c4_mp",
        tactical: "iw4:weapon/concussion_grenade_mp",
        perks: [
            "specialty_scavenger",
            "specialty_explosivedamage",
            "specialty_detectexplosive",
        ],
        deathstreak: "specialty_grenadepulldeath",
    },
    ClassPreset {
        name: "cz75_auto",
        primary: "t5:weapon/ak74u_mp",
        primary_attachments: &["grip"],
        secondary: "t5:weapon/cz75_mp",
        secondary_attachments: &["auto"],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_lightweight",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "strela_lmg",
        primary: "t5:weapon/stoner63_mp",
        primary_attachments: &["reflex"],
        secondary: "t5:weapon/strela_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/semtex_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",
        perks: [
            "specialty_scavenger",
            "specialty_hardline",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "xm25",
        primary: "iw5:weapon/iw5_type95_mp",
        primary_attachments: &["reflex"],
        secondary: "iw5:weapon/xm25_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/concussion_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_bulletdamage",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_combathigh",
    },
    ClassPreset {
        name: "fmg_akimbo",
        primary: "iw5:weapon/iw5_mp7_mp",
        primary_attachments: &["silencer"],
        secondary: "iw5:weapon/iw5_fmg9_mp",
        secondary_attachments: &["akimbo"],
        lethal: "iw4:weapon/throwingknife_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_marathon",
            "specialty_coldblooded",
            "specialty_heartbreaker",
        ],
        deathstreak: "specialty_finalstand",
    },
    ClassPreset {
        name: "hamr_magnum",
        primary: "iw5:weapon/iw5_pp90m1_mp",
        primary_attachments: &["hamrhybrid"],
        secondary: "iw5:weapon/iw5_44magnum_mp",
        secondary_attachments: &["tactical"],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_lightweight",
            "specialty_extendedmelee",
        ],
        deathstreak: "specialty_combathigh",
    },
    ClassPreset {
        name: "mg4_bling",
        primary: "iw4:weapon/mg4_mp",
        primary_attachments: &["grip", "heartbeat"],
        secondary: "iw4:weapon/at4_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",
        perks: [
            "specialty_bling",
            "specialty_bulletdamage",
            "specialty_detectexplosive",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "msr_p99",
        primary: "iw5:weapon/iw5_msr_mp",
        primary_attachments: &["silencer03"],
        secondary: "iw5:weapon/iw5_p99_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/claymore_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_marathon",
            "specialty_coldblooded",
            "specialty_heartbreaker",
        ],
        deathstreak: "specialty_finalstand",
    },
    ClassPreset {
        name: "m4_assault",
        primary: "iw4:weapon/m4_mp",
        primary_attachments: &["reflex"],
        secondary: "iw4:weapon/usp_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_coldblooded",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "t6_an94",
        primary: "t6:weapon/an94_mp",
        primary_attachments: &["reflex"],
        secondary: "t6:weapon/fnp45_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/frag_grenade_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_coldblooded",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "t6_mp7",
        primary: "t6:weapon/mp7_mp",
        primary_attachments: &["silencer"],
        secondary: "t6:weapon/fiveseven_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/semtex_mp",
        tactical: "iw4:weapon/concussion_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_coldblooded",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "t6_lsat",
        primary: "t6:weapon/lsat_mp",
        primary_attachments: &["grip"],
        secondary: "t6:weapon/beretta93r_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/c4_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_coldblooded",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "t6_ballista",
        primary: "t6:weapon/ballista_mp",
        primary_attachments: &[],
        secondary: "t6:weapon/fnp45_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/claymore_mp",
        tactical: "iw4:weapon/smoke_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_coldblooded",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
    ClassPreset {
        name: "t6_870mcs",
        primary: "t6:weapon/870mcs_mp",
        primary_attachments: &[],
        secondary: "t6:weapon/judge_mp",
        secondary_attachments: &[],
        lethal: "iw4:weapon/throwingknife_mp",
        tactical: "iw4:weapon/flash_grenade_mp",
        perks: [
            "specialty_fastreload",
            "specialty_coldblooded",
            "specialty_bulletaccuracy",
        ],
        deathstreak: "specialty_copycat",
    },
];

pub fn pick_showcase(seed: u64, count: usize) -> Vec<&'static ClassPreset> {
    let mut order: Vec<&'static ClassPreset> = SHOWCASE.iter().collect();
    let mut state = seed;
    for i in (1..order.len()).rev() {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        order.swap(i, (z % (i as u64 + 1)) as usize);
    }
    order.truncate(count);
    order
}
