#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoldierKit {
    pub body: String,
    pub head: Option<String>,

    pub arms: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoldierKits {
    pub allies: Option<SoldierKit>,
    pub axis: Option<SoldierKit>,
}

impl SoldierKits {
    pub fn kit(&self, axis: bool) -> Option<&SoldierKit> {
        let (mine, theirs) = if axis {
            (&self.axis, &self.allies)
        } else {
            (&self.allies, &self.axis)
        };
        mine.as_ref().or(theirs.as_ref())
    }
}

pub fn ffa_assignment_is_axis(ffa_team: Option<u8>) -> bool {
    ffa_team == Some(1)
}

pub fn kit_assignment_is_axis(client_state_team: i32, ffa_team: Option<u8>) -> bool {
    match client_state_team {
        1 => true,
        2 => false,
        _ => ffa_assignment_is_axis(ffa_team),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Allies,
    Axis,
}

pub fn soldier_kits(names: &[String]) -> SoldierKits {
    let bodies: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| is_body_model(name))
        .collect();
    let heads: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| is_head_model(name))
        .collect();
    let arms: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| is_arms_model(name))
        .collect();

    let mut kits = SoldierKits {
        allies: choose_kit(&bodies, &heads, &arms, Some(Side::Allies)),
        axis: choose_kit(&bodies, &heads, &arms, Some(Side::Axis)),
    };
    if kits.allies.is_none() && kits.axis.is_none() {
        kits.allies = choose_kit(&bodies, &heads, &arms, None);
    }
    kits
}

fn is_head_model(name: &str) -> bool {
    name.starts_with("head_") || name.contains("_mp_head_")
}

pub fn is_arms_model(name: &str) -> bool {
    name.starts_with("viewmodel_") && name.ends_with("_arms")
}

pub fn is_body_model(name: &str) -> bool {
    name.starts_with("mp_body_")
        || name.contains("_mp_body_")
        || (name.starts_with("c_") && name.contains("_mp_") && name.ends_with("_fb"))
}

pub fn body_has_tp_attach_bones(bone_names: &[String]) -> bool {
    bone_names
        .iter()
        .any(|n| n == xmodel_runtime::TP_HEAD_ATTACH_TAG)
        && xmodel_runtime::TP_WEAPON_ATTACH_TAGS
            .iter()
            .any(|tag| bone_names.iter().any(|n| n == *tag))
}

fn choose_kit(
    bodies: &[&str],
    heads: &[&str],
    arms: &[&str],
    side: Option<Side>,
) -> Option<SoldierKit> {
    let candidates: Vec<&str> = bodies
        .iter()
        .copied()
        .filter(|name| side.is_none_or(|side| classify(name) == Some(side)))
        .collect();
    let body = candidates.iter().copied().max_by(|left, right| {
        (
            body_score(left),
            family_size(left, &candidates),
            std::cmp::Reverse(left),
        )
            .cmp(&(
                body_score(right),
                family_size(right, &candidates),
                std::cmp::Reverse(right),
            ))
    })?;
    Some(SoldierKit {
        body: body.to_owned(),
        head: choose_head(body, heads, side),
        arms: choose_arms(body, arms, side),
    })
}

fn classify(name: &str) -> Option<Side> {
    let mut side = None;
    for token in name.split('_') {
        let candidate = match token {
            "op" | "opforce" | "militia" | "miltia" | "airborne" | "vtn" | "nva" | "rus"
            | "spet" | "spetwin" => Side::Axis,
            "ally" | "allies" | "tf141" | "udt" | "seal" | "army" | "usa" | "sog" | "cia"
            | "ciawin" => Side::Allies,
            _ => continue,
        };
        if candidate == Side::Axis {
            return Some(candidate);
        }
        side = Some(candidate);
    }
    side
}

fn body_score(name: &str) -> i32 {
    name.split('_').fold(0, |score, token| {
        score
            + match token {
                "assault" | "standard" => 3,
                "a" | "armor" => 1,
                "smg" | "lmg" | "shotgun" | "camo" | "flak" | "utility" => -1,
                "sniper" => -6,
                "ghillie" => -8,
                "riot" => -10,
                _ => 0,
            }
    })
}

fn head_score(name: &str) -> i32 {
    name.split('_').fold(0, |score, token| {
        score
            + match token {
                "hat" => -1,
                "sniper" => -6,
                "ghillie" => -8,
                "riot" => -10,
                _ => 0,
            }
    })
}

fn family_size(name: &str, candidates: &[&str]) -> usize {
    let tokens: Vec<&str> = distinctive_tokens(name).collect();
    candidates
        .iter()
        .filter(|other| distinctive_tokens(other).any(|token| tokens.contains(&token)))
        .count()
}

fn choose_head(body: &str, heads: &[&str], side: Option<Side>) -> Option<String> {
    let body_tokens: Vec<&str> = distinctive_tokens(body).collect();
    heads
        .iter()
        .copied()
        .filter(|name| match (side, classify(name)) {
            (Some(expected), Some(actual)) => expected == actual,
            _ => true,
        })
        .max_by(|left, right| {
            let score = |name: &str| {
                distinctive_tokens(name)
                    .filter(|token| body_tokens.contains(token))
                    .count() as i32
                    + head_score(name)
            };
            (score(left), std::cmp::Reverse(left)).cmp(&(score(right), std::cmp::Reverse(right)))
        })
        .filter(|name| {
            distinctive_tokens(name)
                .filter(|token| body_tokens.contains(token))
                .count() as i32
                + head_score(name)
                > 0
        })
        .map(str::to_owned)
}

fn choose_arms(body: &str, arms: &[&str], side: Option<Side>) -> Option<String> {
    let body_tokens: Vec<&str> = distinctive_tokens(body).collect();
    let body_class: Vec<&str> = class_tokens(body).collect();
    arms.iter()
        .copied()
        .filter(|name| match (side, classify(name)) {
            (Some(expected), Some(actual)) => expected == actual,
            _ => true,
        })
        .max_by(|left, right| {
            (
                arms_overlap(left, &body_tokens, &body_class),
                std::cmp::Reverse(*left),
            )
                .cmp(&(
                    arms_overlap(right, &body_tokens, &body_class),
                    std::cmp::Reverse(*right),
                ))
        })
        .filter(|name| arms_overlap(name, &body_tokens, &body_class) > 0)
        .map(str::to_owned)
}

fn class_tokens(name: &str) -> impl Iterator<Item = &str> {
    name.split('_')
        .filter(|token| matches!(*token, "standard" | "armor" | "camo" | "flak" | "utility"))
}

fn arms_overlap(name: &str, body_tokens: &[&str], body_class: &[&str]) -> i32 {
    let family = distinctive_tokens(name)
        .filter(|token| *token != "viewmodel" && *token != "arms")
        .filter(|token| body_tokens.contains(token))
        .count() as i32;
    let class = class_tokens(name)
        .filter(|token| body_class.contains(token))
        .count() as i32;
    family + class * 4
}

pub fn arms_for_body(body: &str, arms: &[&str]) -> Option<String> {
    choose_arms(body, arms, classify(body))
}

fn distinctive_tokens(name: &str) -> impl Iterator<Item = &str> {
    name.split('_').filter(|token| {
        !matches!(
            *token,
            "mp" | "body"
                | "head"
                | "assault"
                | "smg"
                | "lmg"
                | "shotgun"
                | "sniper"
                | "riot"
                | "ghillie"
                | "op"
                | "ally"
                | "allies"
                | "c"
                | "standard"
                | "armor"
                | "camo"
                | "flak"
                | "utility"
                | "1"
                | "2"
                | "3"
                | "4"
                | "5"
        )
    })
}
