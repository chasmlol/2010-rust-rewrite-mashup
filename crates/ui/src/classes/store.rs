use bevy::prelude::*;

use crate::classes::setup::ClassSlotState;
use frame::{HostClassLoadouts, HostClassSlot};
use sim::match_state::PERSONAL_CLASS_SLOTS;

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionClassStore {
    pub slots: Vec<ClassSlotState>,

    pub equipped: Option<usize>,
    pub selected: usize,
}

impl SessionClassStore {
    pub fn from_showcase(seed: u64, registry: &asset_game::WeaponRegistry) -> Self {
        let combat = session::combat_table::from_registry(registry, None);
        let equipment = session::combat_table::equipment_from_registry(registry);
        let available = |slot: &HostClassSlot| {
            let row = session::ClassRow::from(slot);
            !session::project_class(0, &row, registry, &combat, &equipment)
                .def
                .locked
        };
        let mut slots: Vec<HostClassSlot> =
            frame::pick_showcase(seed, frame::showcase_classes().len())
                .into_iter()
                .map(HostClassSlot::from)
                .filter(&available)
                .take(PERSONAL_CLASS_SLOTS)
                .collect();
        if slots.len() < PERSONAL_CLASS_SLOTS {
            for primary in [
                "iw4:weapon/m4_mp",
                "iw4:weapon/mp5k_mp",
                "iw4:weapon/m16_mp",
                "iw4:weapon/ak47_mp",
                "iw4:weapon/rpd_mp",
            ] {
                let slot = HostClassSlot {
                    name: format!("custom_{}", slots.len() + 1),
                    primary: primary.into(),
                    primary_attachments: Vec::new(),
                    secondary: "iw4:weapon/usp_mp".into(),
                    secondary_attachments: Vec::new(),
                    lethal: "iw4:weapon/frag_grenade_mp".into(),
                    tactical: "iw4:weapon/flash_grenade_mp".into(),
                    perks: [
                        "specialty_fastreload".into(),
                        "specialty_bulletdamage".into(),
                        "specialty_bulletaccuracy".into(),
                    ],
                    deathstreak: "specialty_copycat".into(),
                    camos: Default::default(),
                };
                if available(&slot) {
                    slots.push(slot);
                }
                if slots.len() == PERSONAL_CLASS_SLOTS {
                    break;
                }
            }
        }
        if slots.len() < PERSONAL_CLASS_SLOTS {
            let families: Vec<_> = registry.weapon_families().offered().collect();
            for primary in families
                .iter()
                .filter(|family| family.slot == asset_game::FamilySlot::Primary)
            {
                let mut slot = HostClassSlot {
                    name: format!("custom_{}", slots.len() + 1),
                    primary: primary.key.asset_key(),
                    primary_attachments: Vec::new(),
                    secondary: String::new(),
                    secondary_attachments: Vec::new(),
                    lethal: String::new(),
                    tactical: String::new(),
                    perks: Default::default(),
                    deathstreak: String::new(),
                    camos: Default::default(),
                };
                if !available(&slot) {
                    continue;
                }
                for role in [
                    asset_game::FamilySlot::Secondary,
                    asset_game::FamilySlot::Lethal,
                    asset_game::FamilySlot::Tactical,
                ] {
                    for family in families.iter().filter(|family| {
                        family.slot == role && family.key.namespace == primary.key.namespace
                    }) {
                        let mut candidate = slot.clone();
                        let reference = family.key.asset_key();
                        match role {
                            asset_game::FamilySlot::Secondary => candidate.secondary = reference,
                            asset_game::FamilySlot::Lethal => candidate.lethal = reference,
                            asset_game::FamilySlot::Tactical => candidate.tactical = reference,
                            _ => unreachable!(),
                        }
                        if available(&candidate) {
                            slot = candidate;
                            break;
                        }
                    }
                }
                slots.push(slot);
                if slots.len() == PERSONAL_CLASS_SLOTS {
                    break;
                }
            }
        }
        if !slots.is_empty() {
            let available = slots.clone();
            while slots.len() < PERSONAL_CLASS_SLOTS {
                slots.push(available[slots.len() % available.len()].clone());
            }
        }
        Self {
            slots: slots.iter().map(ClassSlotState::from_host_slot).collect(),
            equipped: None,
            selected: 0,
        }
    }

    pub fn equipped_slot(&self) -> Option<&ClassSlotState> {
        self.equipped.and_then(|i| self.slots.get(i))
    }

    pub fn apply_coverage_locks(&mut self, reasons: impl IntoIterator<Item = Option<String>>) {
        for (slot, reason) in self.slots.iter_mut().zip(reasons) {
            slot.lock_reason = reason;
        }
    }
}

impl From<&ClassSlotState> for HostClassSlot {
    fn from(slot: &ClassSlotState) -> Self {
        Self {
            name: slot.name.clone(),
            primary: slot.primary.clone(),
            primary_attachments: slot.primary_attachments.clone(),
            secondary: slot.secondary.clone(),
            secondary_attachments: slot.secondary_attachments.clone(),
            lethal: slot.lethal.clone(),
            tactical: slot.tactical.clone(),
            perks: [slot.perk1.clone(), slot.perk2.clone(), slot.perk3.clone()],
            deathstreak: slot.deathstreak.clone(),
            camos: slot.camos.clone(),
        }
    }
}

pub(crate) fn sync_host_class_loadouts(
    store: Res<SessionClassStore>,
    mut host: ResMut<HostClassLoadouts>,
) {
    if !store.is_changed() {
        return;
    }
    host.slots = store.slots.iter().map(HostClassSlot::from).collect();
}

const CLASS_FILE_HEADER: &str = "iw4l-classes 1";

#[derive(Resource, Default)]
pub(crate) struct ClassStoreFile {
    path: Option<std::path::PathBuf>,
    loaded: bool,
    written: Option<String>,
    retry_at: Option<std::time::Instant>,
}

fn clean_field(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}

fn encode_slots(slots: &[ClassSlotState]) -> String {
    let mut out = String::from(CLASS_FILE_HEADER);
    out.push('\n');
    for slot in slots {
        let attachments = |list: &[String]| {
            list.iter()
                .map(|value| clean_field(value).replace(',', " "))
                .collect::<Vec<_>>()
                .join(",")
        };
        let fields = [
            clean_field(&slot.name),
            clean_field(&slot.primary),
            attachments(&slot.primary_attachments),
            clean_field(&slot.secondary),
            attachments(&slot.secondary_attachments),
            clean_field(&slot.lethal),
            clean_field(&slot.tactical),
            clean_field(&slot.perk1),
            clean_field(&slot.perk2),
            clean_field(&slot.perk3),
            clean_field(&slot.deathstreak),
        ];
        out.push_str(&fields.join("\t"));
        if slot.camos.iter().any(|camo| !camo.is_empty()) {
            for camo in &slot.camos {
                out.push('\t');
                out.push_str(&clean_field(camo));
            }
        }
        out.push('\n');
    }
    out
}

fn decode_slots(text: &str) -> Option<Vec<ClassSlotState>> {
    let mut lines = text.lines();
    if lines.next()? != CLASS_FILE_HEADER {
        return None;
    }
    let attachments = |field: &str| -> Vec<String> {
        field
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let mut slots = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let mut fields: Vec<&str> = line.split('\t').collect();
        let camos = match fields.as_slice() {
            [.., primary, secondary] if fields.len() == 13 => {
                let camos = [(*primary).to_owned(), (*secondary).to_owned()];
                fields.truncate(11);
                camos
            }
            _ => Default::default(),
        };
        let [
            name,
            primary,
            primary_attachments,
            secondary,
            secondary_attachments,
            lethal,
            tactical,
            perk1,
            perk2,
            perk3,
            deathstreak,
        ] = fields.as_slice()
        else {
            return None;
        };
        if slots.len() == PERSONAL_CLASS_SLOTS {
            return None;
        }
        slots.push(ClassSlotState {
            name: (*name).to_owned(),
            primary: (*primary).to_owned(),
            primary_attachments: attachments(*primary_attachments),
            secondary: (*secondary).to_owned(),
            secondary_attachments: attachments(*secondary_attachments),
            lethal: (*lethal).to_owned(),
            tactical: (*tactical).to_owned(),
            perk1: (*perk1).to_owned(),
            perk2: (*perk2).to_owned(),
            perk3: (*perk3).to_owned(),
            deathstreak: (*deathstreak).to_owned(),
            camos,
            lock_reason: None,
        });
    }
    (!slots.is_empty()).then_some(slots)
}

pub(crate) fn load_class_store(
    identity: Option<Res<frame::LaunchIdentity>>,
    mut file: ResMut<ClassStoreFile>,
    mut store: ResMut<SessionClassStore>,
    catalog: Res<crate::ClassLoadoutCatalog>,
) {
    if file.loaded {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if identity.artifacts.as_os_str().is_empty() {
        return;
    }
    let Some(registry) = catalog.resolver.0.as_deref() else {
        return;
    };
    let path = identity.artifacts.join("profile").join("classes.txt");
    file.loaded = true;
    let generate = || {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as u64);
        SessionClassStore::from_showcase(seed, registry)
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match decode_slots(&text) {
            Some(slots) => {
                diag::info!(Ui, "classes: {} read from {}", slots.len(), path.display());
                file.written = Some(encode_slots(&slots));
                store.slots = slots;
                for slot in &mut store.slots {
                    slot.lock_reason = catalog.validate_class(slot).err();
                }
                store.selected = store.selected.min(store.slots.len().saturating_sub(1));
            }
            None => {
                *store = generate();
                diag::warn!(
                    Ui,
                    "classes: {} is not a class file this build reads; file preserved",
                    path.display()
                );
                return;
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            *store = generate();
        }
        Err(error) => {
            *store = generate();
            diag::warn!(
                Ui,
                "classes: cannot read {}: {error}; file preserved",
                path.display()
            );
            return;
        }
    }
    file.path = Some(path);
}

fn write_class_file(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pending = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut output = std::fs::File::create(&pending)?;
        output.write_all(contents.as_bytes())?;
        output.sync_all()?;
        drop(output);
        std::fs::rename(&pending, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&pending);
    }
    result
}

pub(crate) fn save_class_store(
    store: Res<SessionClassStore>,
    catalog: Res<crate::ClassLoadoutCatalog>,
    mut file: ResMut<ClassStoreFile>,
) {
    if !file.loaded
        || store.slots.is_empty()
        || file
            .retry_at
            .is_some_and(|at| std::time::Instant::now() < at)
    {
        return;
    }
    if !store.is_changed() && file.retry_at.is_none() && file.written.is_some() {
        return;
    }
    let Some(path) = file.path.clone() else {
        return;
    };
    let contents = encode_slots(&store.slots);
    if file.written.as_ref() == Some(&contents) {
        return;
    }
    let saved = file.written.as_deref().and_then(decode_slots);
    for (index, slot) in store.slots.iter().enumerate() {
        if let Err(reason) = catalog.validate_class(slot) {
            let unchanged = saved
                .as_ref()
                .and_then(|slots| slots.get(index))
                .is_some_and(|old| HostClassSlot::from(old) == HostClassSlot::from(slot));
            if !unchanged {
                file.retry_at = None;
                diag::warn!(Ui, "classes: not saved: {reason}");
                return;
            }
        }
    }
    match write_class_file(&path, &contents) {
        Ok(()) => {
            file.written = Some(contents);
            file.retry_at = None;
        }
        Err(error) => {
            diag::warn!(Ui, "classes: cannot write {}: {error}", path.display());
            file.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
        }
    }
}
