use asset_game::{LocalizeCatalog, MapTeamSettings, MenuCatalog, MenuDef};
use hud_iw4::{ExprError, ExprHost, Operand, PartyFlag, WeaponLockView};

use crate::playercard::UiLocalVars;

pub(crate) struct MenuWorld<'a> {
    pub ms: i32,
    pub in_game: bool,
    pub party: &'a frame::UiPartyState,
    pub dvars: Option<sim::ScriptDvars<'a>>,
    pub sv_running: bool,
    pub catalog: Option<&'a MenuCatalog>,
    pub localize: Option<&'a LocalizeCatalog>,
    pub kind: Option<gamemode_iw4::GameModeKind>,
    pub team: i32,
    pub team_scores: [i32; 3],
    pub player_score: i32,
    pub time_left_s: i32,
    pub teams: Option<&'a MapTeamSettings>,
    pub scores_open: bool,
    pub classes: Option<&'a frame::HostClassLoadouts>,
    pub weapons: Option<&'a asset_game::WeaponRegistry>,
    pub emp_jammed: bool,
    pub radar_blocked: bool,
}

const GAMETYPES_TABLE: &str = "mp/gametypestable.csv";
const CUSTOM_CLASS_SLOTS: usize = 10;

pub(crate) struct MenuHost<'a> {
    pub world: &'a MenuWorld<'a>,
    pub menu: &'a MenuDef,
    pub dvars: &'a frame::UiMenuDvars,
    pub locals: &'a UiLocalVars,
    pub open: &'a [String],
    pub focus_rect: Option<[f32; 4]>,
}

impl MenuHost<'_> {
    fn dvar(&self, name: &str) -> String {
        if let Some(value) = self.dvars.get(name) {
            return value.to_owned();
        }
        if let Some(value) = self.world.dvars.as_ref().and_then(|d| d.string(name)) {
            return value.to_owned();
        }
        let lower = name.to_ascii_lowercase();
        let flag = |on: bool| String::from(if on { "1" } else { "0" });
        match lower.as_str() {
            "cl_ingame" => flag(self.world.in_game),
            "gamemode" => "mp".to_owned(),
            "ui_multiplayer" | "widescreen" | "hidef" => flag(true),
            "sv_running" => flag(self.world.sv_running),
            "g_gametype" | "ui_gametype" => self
                .world
                .kind
                .map(|kind| kind.token().to_owned())
                .unwrap_or_default(),
            "g_teamicon_allies" => self.team_key(|t| t.allies.as_ref()),
            "g_teamicon_axis" => self.team_key(|t| t.axis.as_ref()),
            "g_teamname_allies" => self.team_key(|t| t.allies_name.as_ref()),
            "g_teamname_axis" => self.team_key(|t| t.axis_name.as_ref()),
            _ => String::new(),
        }
    }

    fn team_key(&self, pick: impl Fn(&MapTeamSettings) -> Option<&asset_core::AssetKey>) -> String {
        self.world
            .teams
            .and_then(pick)
            .map(|key| key.logical_name().to_owned())
            .unwrap_or_default()
    }

    fn gametype_text(&self, col: i32) -> Result<Operand, ExprError> {
        let token = self.dvar("g_gametype");
        let table = self
            .world
            .catalog
            .and_then(|c| c.string_table(GAMETYPES_TABLE))
            .ok_or(ExprError::Host("gametypes table"))?;
        let row = table
            .lookup_row_in_col(0, &token)
            .ok_or(ExprError::Host("gametype row"))?;
        let key = table.cell(row, col);
        Ok(Operand::Str(
            self.localized(key).unwrap_or_else(|| key.to_owned()),
        ))
    }

    fn custom_class(&self, path: &[Operand]) -> Result<Operand, ExprError> {
        let index = match path.get(1) {
            Some(Operand::Str(s)) => s.trim().parse::<usize>().unwrap_or(usize::MAX),
            Some(other) => hud_iw4::source_int(other) as usize,
            None => usize::MAX,
        };
        if index >= CUSTOM_CLASS_SLOTS {
            return Err(ExprError::Host("custom class index"));
        }
        let slot = self.world.classes.and_then(|c| c.slots.get(index));
        let field = |at: usize| path.get(at).map(hud_iw4::source_str).unwrap_or_default();
        let int = |at: usize| path.get(at).map_or(-1, hud_iw4::source_int);
        let field_name = field(2).to_ascii_lowercase();
        if field_name == "inuse" {
            return Ok(Operand::Int(i32::from(slot.is_some())));
        }
        let Some(slot) = slot else {
            return Ok(Operand::Str(String::from("none")));
        };
        let reference = |key: &str| -> String {
            if asset_core::AssetKey::parse(key)
                .is_ok_and(|key| key.namespace != asset_core::AssetNamespace::Iw4)
            {
                return key.to_owned();
            }
            let name = key.rsplit('/').next().unwrap_or(key);
            if name.is_empty() {
                String::from("none")
            } else {
                name.to_owned()
            }
        };
        let base = |key: &str| {
            let name = reference(key);
            if name.contains(':') {
                return name;
            }
            name.strip_suffix("_mp").map(str::to_owned).unwrap_or(name)
        };
        let or_null = |perk: &str| {
            if perk.is_empty() {
                String::from("specialty_null")
            } else {
                reference(perk)
            }
        };
        Ok(Operand::Str(match field_name.as_str() {
            "name" => slot.name.clone(),
            "specialgrenade" => base(&slot.tactical),
            "perks" => match int(3) {
                0 => reference(&slot.lethal),
                n @ 1..=3 => or_null(&slot.perks[n as usize - 1]),
                4 => or_null(&slot.deathstreak),
                _ => return Err(ExprError::Host("perk index")),
            },
            "weaponsetups" => {
                let (weapon, attachments) = match int(3) {
                    0 => (&slot.primary, &slot.primary_attachments),
                    1 => (&slot.secondary, &slot.secondary_attachments),
                    _ => return Err(ExprError::Host("weapon setup index")),
                };
                match field(4).to_ascii_lowercase().as_str() {
                    "weapon" => base(weapon),
                    "camo" => slot
                        .camos
                        .get(usize::try_from(int(3)).unwrap_or(usize::MAX))
                        .filter(|camo| !camo.is_empty())
                        .cloned()
                        .unwrap_or_else(|| String::from("none")),
                    "attachment" => attachments
                        .get(usize::try_from(int(5)).unwrap_or(usize::MAX))
                        .map(|attachment| {
                            if base(weapon).contains(':') {
                                format!("{weapon}+{attachment}")
                            } else {
                                attachment.clone()
                            }
                        })
                        .unwrap_or_else(|| String::from("none")),
                    _ => return Err(ExprError::Host("weapon setup field")),
                }
            }
            _ => return Err(ExprError::Host("custom class field")),
        }))
    }

    fn weapon_table_field(
        &self,
        table: &str,
        column: i32,
        key: &str,
        result: i32,
    ) -> Option<String> {
        let (weapon, attachment) = key
            .split_once('+')
            .map_or((key, None), |(w, a)| (w, Some(a)));
        let family_key = asset_game::FamilyKey::parse(weapon)?;
        let family = self.world.weapons?.weapon_families().find(&family_key)?;
        let image_key = |image: &str| {
            if image.is_empty() || asset_core::AssetKey::parse(image).is_ok() {
                image.to_owned()
            } else {
                format!("{}:material/{image}", family.key.namespace.as_str())
            }
        };
        if column == 4 && table.eq_ignore_ascii_case("mp/statsTable.csv") {
            return Some(match result {
                2 => family.item_group.clone(),
                3 => family.name_key(),
                4 => key.to_owned(),
                6 => image_key(&family.image),
                _ => String::new(),
            });
        }
        if column == 4 && table.eq_ignore_ascii_case("mp/attachmentTable.csv") {
            let attachment = family
                .attachments
                .iter()
                .find(|a| Some(a.name.as_str()) == attachment)?;
            return Some(match result {
                3 => attachment.caption_key.trim_start_matches('@').to_owned(),
                4 => attachment.name.clone(),
                6 => image_key(&attachment.icon),
                _ => String::new(),
            });
        }
        if column == 1 && table.eq_ignore_ascii_case("mp/perkTable.csv") {
            return Some(match result {
                2 => family.display_key.trim_start_matches('@').to_owned(),
                3 => image_key(&family.image),
                _ => String::new(),
            });
        }
        None
    }

    fn static_name(&self, index: i32) -> Result<&str, ExprError> {
        self.menu
            .static_dvar_name(index)
            .ok_or(ExprError::Host("static dvar name"))
    }

    fn localized(&self, key: &str) -> Option<String> {
        self.world
            .localize
            .and_then(|l| l.text(key.trim_start_matches('@')))
            .map(str::to_owned)
    }
}

fn dvar_number(value: &str) -> f32 {
    let value = value.trim();
    if value.eq_ignore_ascii_case("true") {
        return 1.0;
    }
    value.parse::<f32>().unwrap_or(0.0)
}

impl ExprHost for MenuHost<'_> {
    fn localize_string(&self, args: &[Operand]) -> Result<String, ExprError> {
        let Some(Operand::Str(key)) = args.first() else {
            return Err(ExprError::Host("locstring template"));
        };
        let template = self
            .localized(key)
            .ok_or(ExprError::Host("locstring localization"))?;
        let values: Vec<String> = args[1..]
            .iter()
            .map(|arg| match arg {
                Operand::Int(v) => v.to_string(),
                Operand::Float(v) => format!("{v}"),
                Operand::Str(s) => self.localized(s).unwrap_or_else(|| s.clone()),
            })
            .collect();
        let mut out = String::new();
        let mut rest = template.as_str();
        while let Some(at) = rest.find("&&") {
            out.push_str(&rest[..at]);
            let tail = &rest[at + 2..];
            match tail.as_bytes().first() {
                Some(digit @ b'1'..=b'9') => {
                    out.push_str(values.get((digit - b'1') as usize).map_or("", |v| v));
                    rest = &tail[1..];
                }
                _ => {
                    out.push_str("&&");
                    rest = tail;
                }
            }
        }
        out.push_str(rest);
        Ok(out)
    }
    fn milliseconds(&self) -> i32 {
        self.world.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        self.dvar_int(self.static_name(index)?)
    }
    fn static_dvar_string(&self, index: i32) -> Result<String, ExprError> {
        Ok(self.dvar(self.static_name(index)?))
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        Ok(dvar_number(&self.dvar(name)) as i32)
    }
    fn dvar_bool(&self, name: &str) -> Result<i32, ExprError> {
        Ok(i32::from(dvar_number(&self.dvar(name)) != 0.0))
    }
    fn dvar_float(&self, name: &str) -> Result<f32, ExprError> {
        Ok(dvar_number(&self.dvar(name)))
    }
    fn dvar_string(&self, name: &str) -> Result<String, ExprError> {
        Ok(self.dvar(name))
    }
    fn team_field(&self, field: &str) -> Result<Operand, ExprError> {
        if field.eq_ignore_ascii_case("name") {
            Ok(Operand::Str(
                entity_iw4::get_team_name(self.world.team).to_owned(),
            ))
        } else if field.eq_ignore_ascii_case("score") {
            Ok(Operand::Int(
                self.world
                    .team_scores
                    .get(self.world.team as usize)
                    .copied()
                    .unwrap_or(0),
            ))
        } else {
            Err(ExprError::Host("team field"))
        }
    }
    fn player_field(&self, field: &str) -> Result<Operand, ExprError> {
        if field.eq_ignore_ascii_case("score") {
            Ok(Operand::Int(self.world.player_score))
        } else {
            Err(ExprError::Host("player field"))
        }
    }
    fn other_team_field(&self, field: &str) -> Result<Operand, ExprError> {
        if field.eq_ignore_ascii_case("score") {
            let other = if self.world.team == 1 { 2 } else { 1 };
            Ok(Operand::Int(self.world.team_scores[other]))
        } else {
            Err(ExprError::Host("other team field"))
        }
    }
    fn side_team_field(&self, allies: bool, field: &str) -> Result<Operand, ExprError> {
        if !field.eq_ignore_ascii_case("score") {
            return Err(ExprError::Host("side team field"));
        }
        let team = if allies { 2 } else { 1 };
        Ok(Operand::Int(self.world.team_scores[team]))
    }
    fn local_var_string(&self, name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(self.locals.string(name)))
    }
    fn local_var_int(&self, name: &str) -> Result<i32, ExprError> {
        Ok(self.locals.int(name))
    }
    fn time_left(&self) -> Result<i32, ExprError> {
        Ok(self.world.time_left_s)
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("score at rank"))
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        self.gametype_text(1)
    }
    fn gametype_description(&self) -> Result<Operand, ExprError> {
        self.gametype_text(2)
    }
    fn player_data(&self, path: &[Operand]) -> Result<Operand, ExprError> {
        let root = path.first().map(hud_iw4::source_str).unwrap_or_default();
        if root.eq_ignore_ascii_case("customClasses") {
            self.custom_class(path)
        } else if root.eq_ignore_ascii_case("killstreaks") {
            Ok(Operand::Str(String::from("none")))
        } else {
            Err(ExprError::Host("player data path"))
        }
    }
    fn table_lookup(
        &self,
        table: &str,
        col0: i32,
        key: &str,
        result_col: i32,
    ) -> Result<Operand, ExprError> {
        if let Some(value) = self.weapon_table_field(table, col0, key, result_col) {
            return Ok(Operand::Str(value));
        }
        let Some(t) = self.world.catalog.and_then(|c| c.string_table(table)) else {
            return Err(ExprError::Host("string table"));
        };
        Ok(Operand::Str(
            t.lookup_row_in_col(col0, key)
                .map(|row| t.cell(row, result_col).to_owned())
                .unwrap_or_default(),
        ))
    }
    fn table_lookup_by_row(&self, table: &str, row: i32, col: i32) -> Result<Operand, ExprError> {
        let Some(t) = self.world.catalog.and_then(|c| c.string_table(table)) else {
            return Err(ExprError::Host("string table"));
        };
        Ok(Operand::Str(t.cell(row, col).to_owned()))
    }
    fn ui_active(&self) -> Result<i32, ExprError> {
        Ok(1)
    }
    fn scoreboard_visible(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.world.scores_open))
    }
    fn menu_is_open(&self, name: &str) -> Result<i32, ExprError> {
        Ok(i32::from(
            self.open.iter().any(|open| open.eq_ignore_ascii_case(name)),
        ))
    }
    fn weapon_lock(&self) -> Result<WeaponLockView, ExprError> {
        Err(ExprError::Host("weapon lock"))
    }
    fn is_item_unlocked(&self, _item: &str) -> Result<i32, ExprError> {
        Ok(1)
    }
    fn party_flag(&self, flag: PartyFlag) -> Result<i32, ExprError> {
        let party = self.world.party;
        Ok(i32::from(match flag {
            PartyFlag::InLobby => party.in_lobby,
            PartyFlag::InPrivateParty => party.active,
            PartyFlag::PrivatePartyHost => party.active && party.is_host,
            PartyFlag::PrivatePartyHostInLobby => party.in_lobby && party.is_host,
        }))
    }
    fn radar_jam_intensity(&self) -> Result<f32, ExprError> {
        Ok(if self.world.emp_jammed || self.world.radar_blocked {
            1.0
        } else {
            0.0
        })
    }
    fn emp_jammed(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.world.emp_jammed))
    }
    fn focused_item_rect(&self) -> Result<[f32; 4], ExprError> {
        self.focus_rect.ok_or(ExprError::Host("no focused item"))
    }
    fn map_name(&self) -> Result<Operand, ExprError> {
        let map = self.dvar("mapname");
        if map.is_empty() {
            return Err(ExprError::Host("mapname"));
        }
        let key = format!(
            "MPUI_{}",
            map.strip_prefix("mp_").unwrap_or(&map).to_ascii_uppercase()
        );
        Ok(Operand::Str(self.localized(&key).unwrap_or(map)))
    }
}
