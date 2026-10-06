use std::collections::{BTreeMap, HashMap};

use crate::{AssetKey, AssetKind, AssetNamespace, CacAuthoredCategory, CapturedStringTable};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadoutRules {
    pub max_attachments: usize,
}

impl Default for LoadoutRules {
    fn default() -> Self {
        Self { max_attachments: 2 }
    }
}

impl LoadoutRules {
    pub fn for_class(perk1: &str) -> Self {
        Self {
            max_attachments: if perk1 == "specialty_bling" { 2 } else { 1 },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FamilyKey {
    pub namespace: AssetNamespace,
    pub base: String,
}

impl FamilyKey {
    pub fn new(namespace: AssetNamespace, base: &str) -> Self {
        let base = base.to_ascii_lowercase();
        let base = base.strip_suffix("_mp").unwrap_or(&base).to_owned();
        Self { namespace, base }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        if let Ok(key) = AssetKey::parse(raw) {
            return (key.kind == AssetKind::Weapon)
                .then(|| Self::new(key.namespace, key.logical_name()));
        }
        let (game, name) = raw.split_once(':')?;
        let namespace = AssetNamespace::parse(game)?;
        (!name.is_empty() && !name.contains('/')).then(|| Self::new(namespace, name))
    }

    pub fn short(&self) -> String {
        let game = self.namespace.as_str();
        let name = self
            .base
            .strip_prefix(game)
            .and_then(|rest| rest.strip_prefix('_'))
            .unwrap_or(&self.base);
        format!("{game}:{name}")
    }

    pub fn asset_key(&self) -> String {
        format!("{}:weapon/{}_mp", self.namespace.as_str(), self.base)
    }
}

impl core::fmt::Display for FamilyKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.asset_key())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilySlot {
    Primary,
    Secondary,
    Lethal,
    Tactical,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentChoice {
    pub name: String,
    pub point: String,
    pub caption_key: String,
    pub icon: String,
    pub desc_key: String,
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponFamily {
    pub key: FamilyKey,
    pub item_group: String,
    pub category: Option<CacAuthoredCategory>,
    pub slot: FamilySlot,
    pub display_key: String,
    pub image: String,
    pub base: Option<u32>,
    pub attachments: Vec<AttachmentChoice>,
}

impl WeaponFamily {
    pub fn name_key(&self) -> String {
        let key = self.display_key.trim_start_matches('@');
        match self.key.namespace {
            AssetNamespace::Iw4 => key.to_owned(),
            namespace => format!("{}:localize/{key}", namespace.as_str()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WeaponSelection {
    pub family: Option<FamilyKey>,
    pub attachments: Vec<String>,
}

impl WeaponSelection {
    pub fn bare(family: FamilyKey) -> Self {
        Self {
            family: Some(family),
            attachments: Vec::new(),
        }
    }

    pub fn with(family: FamilyKey, attachments: &[String]) -> Self {
        Self {
            family: Some(family),
            attachments: attachments.to_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigurationRefusal {
    UnknownFamily(String),
    UnknownAttachment(String),
    NotOffered(String),
    Incompatible {
        a: String,
        b: String,
    },
    RuleRestricted(String),
    MissingContent(String),
    MissingDependency {
        weapon: String,
        kind: &'static str,
        name: String,
    },
    Unsupported(String),
}

impl ConfigurationRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownFamily(_) => "weapon.unknown_family",
            Self::UnknownAttachment(_) => "weapon.unknown_attachment",
            Self::NotOffered(_) => "weapon.not_offered",
            Self::Incompatible { .. } => "weapon.incompatible",
            Self::RuleRestricted(_) => "weapon.rule_restricted",
            Self::MissingContent(_) => "weapon.missing_content",
            Self::MissingDependency { .. } => "weapon.missing_dependency",
            Self::Unsupported(_) => "weapon.unsupported",
        }
    }
}

impl core::fmt::Display for ConfigurationRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownFamily(name) => write!(f, "no weapon family `{name}`"),
            Self::UnknownAttachment(name) => write!(f, "no attachment `{name}` in this game"),
            Self::NotOffered(name) => write!(f, "`{name}` is not offered for this weapon"),
            Self::Incompatible { a, b } => write!(f, "`{a}` and `{b}` cannot be combined"),
            Self::RuleRestricted(why) => write!(f, "not allowed here: {why}"),
            Self::MissingContent(name) => write!(f, "`{name}` is not in the loaded content"),
            Self::MissingDependency { weapon, kind, name } => {
                write!(f, "`{weapon}` names {kind} `{name}`, which is not loaded")
            }
            Self::Unsupported(why) => write!(f, "not supported yet: {why}"),
        }
    }
}

impl std::error::Error for ConfigurationRefusal {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedConfiguration {
    handle: crate::WeaponHandle,
    selection: WeaponSelection,
}

impl ResolvedConfiguration {
    pub(crate) fn new(handle: crate::WeaponHandle, selection: WeaponSelection) -> Self {
        Self { handle, selection }
    }

    pub fn handle(&self) -> crate::WeaponHandle {
        self.handle
    }

    pub fn selection(&self) -> &WeaponSelection {
        &self.selection
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentOption {
    pub choice: AttachmentChoice,
    pub selected: bool,
    pub toggle: Result<ResolvedConfiguration, ConfigurationRefusal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Schema {
    Infinity,
    Treyarch,
}

#[derive(Clone, Debug, Default)]
struct NamespaceTables {
    attachments: BTreeMap<String, AttachmentChoice>,
    forbidden: std::collections::HashSet<(String, String)>,
    partners: HashMap<String, Vec<String>>,
    cosmetics: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponFamilies {
    families: Vec<WeaponFamily>,
    by_key: HashMap<FamilyKey, usize>,
    tables: HashMap<AssetNamespace, NamespaceTables>,
    described: HashMap<u32, WeaponSelection>,
    excluded: Vec<(String, String)>,
}

const IW_OFFERED_COLS: std::ops::RangeInclusive<i32> = 11..=21;

fn schema(namespace: AssetNamespace) -> Schema {
    match namespace {
        AssetNamespace::T5 | AssetNamespace::T6 => Schema::Treyarch,
        _ => Schema::Infinity,
    }
}

fn is_attachment_table(name: &str) -> bool {
    name.eq_ignore_ascii_case("mp/attachmentTable.csv")
}

fn is_combos_table(name: &str) -> bool {
    name.eq_ignore_ascii_case("mp/attachmentCombos.csv")
}

fn t5_point_rank(point: &str) -> u8 {
    match point {
        "top" => 1,
        "bottom" => 2,
        "trigger" => 3,
        "muzzle" => 4,
        _ => 0,
    }
}

fn read_attachments(namespace: AssetNamespace, table: &CapturedStringTable) -> NamespaceTables {
    let mut out = NamespaceTables::default();
    for row in 1..table.rows as i32 {
        let name = table.cell(row, 4).to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        let (point, service, cosmetic) = match schema(namespace) {
            Schema::Infinity => {
                let group = table.cell(row, 2);
                (group.to_owned(), group == "none", false)
            }
            Schema::Treyarch => {
                let kind = table.cell(row, 2);
                (
                    table.cell(row, 1).to_owned(),
                    kind != "attachment" || name == "none",
                    kind == "weaponoption",
                )
            }
        };
        if cosmetic {
            out.cosmetics.push(name);
            continue;
        }
        if service {
            continue;
        }
        if schema(namespace) == Schema::Treyarch {
            let partners = table
                .cell(row, 11)
                .split_ascii_whitespace()
                .map(str::to_ascii_lowercase)
                .collect();
            out.partners.insert(name.clone(), partners);
        }
        out.attachments
            .entry(name.clone())
            .or_insert_with(|| AttachmentChoice {
                name,
                point,
                caption_key: table.cell(row, 3).to_owned(),
                icon: table.cell(row, 6).to_owned(),
                desc_key: table.cell(row, 7).to_owned(),
                order: table.cell(row, 9).parse().unwrap_or(row as u32),
            });
    }
    if schema(namespace) == Schema::Infinity {
        for (order, choice) in out.attachments.values_mut().enumerate() {
            choice.order = order as u32;
        }
    }
    out
}

fn read_combos(table: &CapturedStringTable, into: &mut NamespaceTables) {
    for row in 1..table.rows as i32 {
        let a = table.cell(row, 0).to_ascii_lowercase();
        for col in 1..table.columns as i32 {
            if !table.cell(row, col).eq_ignore_ascii_case("no") {
                continue;
            }
            let b = table.cell(col, 0).to_ascii_lowercase();
            if a.is_empty() || b.is_empty() || a == b {
                continue;
            }
            into.forbidden.insert(ordered_pair(&a, &b));
        }
    }
}

fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

fn infinity_slot(
    namespace: AssetNamespace,
    category: Option<CacAuthoredCategory>,
    offhand: Option<crate::CacOffhandBucket>,
) -> FamilySlot {
    use CacAuthoredCategory as C;
    match category {
        Some(C::Pistol | C::MachinePistol | C::Projectile | C::Special) => FamilySlot::Secondary,
        Some(C::Shotgun) if namespace == AssetNamespace::Iw4 => FamilySlot::Secondary,
        Some(_) => FamilySlot::Primary,
        None => offhand_slot(offhand),
    }
}

fn offhand_slot(offhand: Option<crate::CacOffhandBucket>) -> FamilySlot {
    match offhand {
        Some(crate::CacOffhandBucket::Lethal) => FamilySlot::Lethal,
        Some(crate::CacOffhandBucket::Tactical) => FamilySlot::Tactical,
        None => FamilySlot::Other,
    }
}

pub(crate) trait FamilyContent:
    crate::weapon_catalog::configuration::WeaponConfigurationCompiler
{
    fn published_handle(&self, id: u32) -> Option<crate::WeaponHandle>;
    fn lookup(&self, namespace: AssetNamespace, name: &str) -> Option<u32>;
    fn offhand_class(&self, id: u32) -> i32;
    fn admission(&self, id: u32) -> Result<(), ConfigurationRefusal>;
    fn names_in(&self, namespace: AssetNamespace) -> Vec<(u32, String)>;
    fn prepared_all(&self) -> Vec<(u32, WeaponSelection)> {
        Vec::new()
    }
}

impl WeaponFamilies {
    pub(crate) fn build(
        tables: &[(AssetNamespace, CapturedStringTable)],
        content: &impl FamilyContent,
    ) -> Self {
        let mut out = Self::default();
        for (namespace, table) in tables {
            if is_attachment_table(&table.name) && !out.tables.contains_key(namespace) {
                out.tables
                    .insert(*namespace, read_attachments(*namespace, table));
            }
        }
        for (namespace, table) in tables {
            if is_combos_table(&table.name) {
                let entry = out.tables.entry(*namespace).or_default();
                if entry.forbidden.is_empty() {
                    read_combos(table, entry);
                }
            }
        }
        for (namespace, table) in tables {
            if crate::is_stats_table_name(&table.name) {
                out.read_stats(*namespace, table, content);
            }
        }
        out.describe_registry(content);
        out
    }

    fn read_stats(
        &mut self,
        namespace: AssetNamespace,
        table: &CapturedStringTable,
        content: &impl FamilyContent,
    ) {
        let known = self.tables.get(&namespace).cloned().unwrap_or_default();
        for row in 1..table.rows as i32 {
            let group = table.cell(row, 2);
            let base = table.cell(row, 4);
            if !group.starts_with("weapon_") || base.is_empty() || base == "weapon_null" {
                continue;
            }
            if namespace == AssetNamespace::T6 && table.cell(row, 12) == "-1" {
                continue;
            }
            let key = FamilyKey::new(namespace, base);
            if known.attachments.contains_key(&key.base) || self.by_key.contains_key(&key) {
                continue;
            }
            let base_id = content.lookup(namespace, &key.base);
            let category = crate::cac_category_from_item_group(group);
            let offhand = base_id
                .map(|id| content.offhand_class(id))
                .and_then(crate::cac_offhand_bucket);
            let (slot, offered): (FamilySlot, Vec<String>) = match schema(namespace) {
                Schema::Infinity => (
                    infinity_slot(namespace, category, offhand),
                    IW_OFFERED_COLS
                        .map(|col| table.cell(row, col).to_ascii_lowercase())
                        .filter(|name| !name.is_empty())
                        .collect(),
                ),
                Schema::Treyarch => (
                    match table.cell(row, 13) {
                        "primary" => FamilySlot::Primary,
                        "secondary" => FamilySlot::Secondary,
                        _ => offhand_slot(offhand),
                    },
                    table
                        .cell(row, 8)
                        .split_ascii_whitespace()
                        .map(str::to_ascii_lowercase)
                        .map(|name| match name.rsplit_once('_') {
                            Some((attachment, _))
                                if namespace == AssetNamespace::T6
                                    && !known.attachments.contains_key(&name) =>
                            {
                                attachment.to_owned()
                            }
                            _ => name,
                        })
                        .collect(),
                ),
            };
            let mut attachments: Vec<AttachmentChoice> = offered
                .iter()
                .filter_map(|name| known.attachments.get(name).cloned())
                .collect();
            attachments.sort_by_key(|choice| choice.order);
            attachments.dedup_by(|a, b| a.name == b.name);
            match base_id {
                None => self.excluded.push((
                    key.asset_key(),
                    format!("authored base `{}` is not in the loaded content", key.base),
                )),
                Some(id) if let Err(refusal) = content.admission(id) => {
                    self.excluded.push((key.asset_key(), refusal.to_string()))
                }
                Some(_) if slot == FamilySlot::Other => self
                    .excluded
                    .push((key.asset_key(), format!("`{group}` is not a class slot"))),
                Some(_) => {}
            }
            self.by_key.insert(key.clone(), self.families.len());
            self.families.push(WeaponFamily {
                key,
                item_group: group.to_owned(),
                category,
                slot,
                display_key: table.cell(row, 3).to_owned(),
                image: table.cell(row, 6).to_owned(),
                base: base_id,
                attachments,
            });
        }
    }

    fn describe_registry(&mut self, content: &impl FamilyContent) {
        let namespaces: Vec<AssetNamespace> = {
            let mut seen: Vec<AssetNamespace> =
                self.families.iter().map(|f| f.key.namespace).collect();
            seen.sort_by_key(|ns| ns.as_str());
            seen.dedup();
            seen
        };
        for namespace in namespaces {
            let mut bases: Vec<&WeaponFamily> = self
                .families
                .iter()
                .filter(|family| family.key.namespace == namespace)
                .collect();
            bases.sort_by_key(|family| std::cmp::Reverse(family.key.base.len()));
            let mut described = Vec::new();
            for (id, name) in content.names_in(namespace) {
                for family in &bases {
                    if let Some(attachments) = self.parse_configuration(family, &name) {
                        described
                            .push((id, WeaponSelection::with(family.key.clone(), &attachments)));
                        break;
                    }
                }
            }
            described.extend(content.prepared_all().into_iter().filter(|(_, selection)| {
                selection
                    .family
                    .as_ref()
                    .is_some_and(|key| key.namespace == namespace)
            }));
            for (id, selection) in described {
                if self
                    .resolve(&selection, LoadoutRules::default(), content)
                    .is_ok_and(|resolved| Some(resolved.handle()) == content.published_handle(id))
                {
                    self.described.insert(id, selection);
                }
            }
        }
    }

    fn parse_configuration(&self, family: &WeaponFamily, name: &str) -> Option<Vec<String>> {
        let parts = crate::weapon_catalog::configuration::authored_attachments(&family.key, name)?;
        if parts.is_empty() {
            return Some(parts);
        }
        let tables = self.tables.get(&family.key.namespace)?;
        if parts.is_empty()
            || !parts
                .iter()
                .all(|part| tables.attachments.contains_key(part))
        {
            return None;
        }
        let normalized = self.normalize(family.key.namespace, &parts);
        (normalized == parts).then_some(parts)
    }

    pub(crate) fn normalize(
        &self,
        namespace: AssetNamespace,
        attachments: &[String],
    ) -> Vec<String> {
        let mut out: Vec<String> = attachments
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .filter(|name| !name.is_empty() && name != "none")
            .collect();
        out.sort();
        out.dedup();
        if schema(namespace) == Schema::Treyarch {
            let tables = self.tables.get(&namespace);
            out.sort_by_key(|name| {
                tables
                    .and_then(|t| t.attachments.get(name))
                    .map(|choice| t5_point_rank(&choice.point))
                    .unwrap_or(0)
            });
        }
        out
    }

    fn compatible(&self, namespace: AssetNamespace, a: &str, b: &str) -> bool {
        let Some(tables) = self.tables.get(&namespace) else {
            return true;
        };
        match schema(namespace) {
            Schema::Infinity => !tables.forbidden.contains(&ordered_pair(a, b)),
            Schema::Treyarch => {
                let lists = |x: &str, y: &str| {
                    tables
                        .partners
                        .get(x)
                        .is_some_and(|partners| partners.iter().any(|p| p == y))
                };
                lists(a, b) && lists(b, a)
            }
        }
    }

    pub fn families(&self) -> &[WeaponFamily] {
        &self.families
    }

    pub fn family(&self, key: &FamilyKey) -> Option<&WeaponFamily> {
        self.by_key.get(key).map(|&i| &self.families[i])
    }

    pub fn find(&self, key: &FamilyKey) -> Option<&WeaponFamily> {
        self.family(key).or_else(|| {
            let prefixed = FamilyKey {
                namespace: key.namespace,
                base: format!("{}_{}", key.namespace.as_str(), key.base),
            };
            self.family(&prefixed)
        })
    }

    pub(crate) fn iw5_candidate_selections(&self) -> Vec<(u32, WeaponSelection)> {
        self.candidate_selections(AssetNamespace::Iw5)
    }

    pub(crate) fn candidate_selections(
        &self,
        namespace: AssetNamespace,
    ) -> Vec<(u32, WeaponSelection)> {
        let mut families: Vec<&WeaponFamily> = self
            .families
            .iter()
            .filter(|family| family.key.namespace == namespace && family.base.is_some())
            .collect();
        families.sort_by_key(|family| family.key.asset_key());
        let mut out = Vec::new();
        for family in families {
            let base = family.base.expect("filtered loaded base");
            out.push((base, WeaponSelection::bare(family.key.clone())));
            let mut names: Vec<String> = family
                .attachments
                .iter()
                .map(|choice| choice.name.clone())
                .collect();
            names.sort();
            names.dedup();
            for (i, name) in names.iter().enumerate() {
                out.push((
                    base,
                    WeaponSelection::with(family.key.clone(), std::slice::from_ref(name)),
                ));
                for other in &names[i + 1..] {
                    if self.compatible(namespace, name, other) {
                        out.push((
                            base,
                            WeaponSelection::with(
                                family.key.clone(),
                                &[name.clone(), other.clone()],
                            ),
                        ));
                    }
                }
            }
        }
        out
    }

    pub fn offered(&self) -> impl Iterator<Item = &WeaponFamily> {
        self.families.iter().filter(|family| {
            family.slot != FamilySlot::Other
                && !self
                    .excluded
                    .iter()
                    .any(|(key, _)| *key == family.key.asset_key())
        })
    }

    pub fn excluded(&self) -> &[(String, String)] {
        &self.excluded
    }

    pub fn cosmetic_choices(&self, namespace: AssetNamespace) -> &[String] {
        self.tables
            .get(&namespace)
            .map(|t| t.cosmetics.as_slice())
            .unwrap_or(&[])
    }

    pub fn describe(&self, id: u32) -> Option<&WeaponSelection> {
        self.described.get(&id)
    }

    pub(crate) fn resolve(
        &self,
        selection: &WeaponSelection,
        rules: LoadoutRules,
        content: &impl FamilyContent,
    ) -> Result<ResolvedConfiguration, ConfigurationRefusal> {
        let Some(key) = selection.family.as_ref() else {
            return Err(ConfigurationRefusal::UnknownFamily(String::new()));
        };
        let family = self
            .family(key)
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(key.asset_key()))?;
        let namespace = key.namespace;
        let attachments = self.normalize(namespace, &selection.attachments);
        let known = self.tables.get(&namespace);
        for name in &attachments {
            if !known.is_some_and(|t| t.attachments.contains_key(name)) {
                return Err(ConfigurationRefusal::UnknownAttachment(name.clone()));
            }
            if !family.attachments.iter().any(|choice| choice.name == *name) {
                return Err(ConfigurationRefusal::NotOffered(name.clone()));
            }
        }
        if attachments.len() > rules.max_attachments {
            return Err(ConfigurationRefusal::RuleRestricted(format!(
                "at most {} attachments per weapon",
                rules.max_attachments
            )));
        }
        for (i, a) in attachments.iter().enumerate() {
            for b in &attachments[i + 1..] {
                if !self.compatible(namespace, a, b) {
                    return Err(ConfigurationRefusal::Incompatible {
                        a: a.clone(),
                        b: b.clone(),
                    });
                }
            }
        }
        let selection = WeaponSelection::with(key.clone(), &attachments);
        let id = content.compile(family, &selection)?;
        Ok(ResolvedConfiguration::new(
            content
                .published_handle(id)
                .ok_or_else(|| ConfigurationRefusal::MissingContent(id.to_string()))?,
            selection,
        ))
    }

    pub(crate) fn attachment_options(
        &self,
        selection: &WeaponSelection,
        rules: LoadoutRules,
        content: &impl FamilyContent,
    ) -> Result<Vec<AttachmentOption>, ConfigurationRefusal> {
        let key = selection
            .family
            .as_ref()
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(String::new()))?;
        let family = self
            .family(key)
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(key.asset_key()))?;
        let current = self.normalize(key.namespace, &selection.attachments);
        Ok(family
            .attachments
            .iter()
            .map(|choice| {
                let selected = current.contains(&choice.name);
                let mut next = current.clone();
                if selected {
                    next.retain(|name| *name != choice.name);
                } else {
                    next.push(choice.name.clone());
                }
                let toggle =
                    self.resolve(&WeaponSelection::with(key.clone(), &next), rules, content);
                AttachmentOption {
                    choice: choice.clone(),
                    selected,
                    toggle,
                }
            })
            .collect())
    }
}
