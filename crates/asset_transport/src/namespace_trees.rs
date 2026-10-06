use std::path::{Path, PathBuf};

use asset_core::AssetNamespace;

use crate::ZoneGame;
use crate::discover::{GamesRoot, find_zone_file_version, zone_game_for_path, zone_version};
use crate::iwd::game_main_for_zone;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespaceTree {
    pub anchor: PathBuf,

    pub main: Option<PathBuf>,
}

impl NamespaceTree {
    fn from_anchor(anchor: PathBuf) -> Self {
        let main = game_main_for_zone(&anchor).ok();
        Self { anchor, main }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamespaceTrees {
    iw4: Option<NamespaceTree>,
    t5: Option<NamespaceTree>,
    iw5: Option<NamespaceTree>,
    t6: Option<NamespaceTree>,
}

impl NamespaceTrees {
    pub fn discover(root: &GamesRoot) -> Self {
        let mut trees = Self::default();
        for ns in [
            AssetNamespace::Iw4,
            AssetNamespace::T5,
            AssetNamespace::Iw5,
            AssetNamespace::T6,
        ] {
            let version = zone_version(zone_game_of(ns));
            if let Ok(found) = find_zone_file_version(root, "common_mp", version) {
                *trees.slot_mut(ns) = Some(NamespaceTree::from_anchor(found.path));
            }
        }
        trees
    }

    pub fn adopt_zone(&mut self, zone_ff: &Path) {
        let Some(game) = zone_game_for_path(zone_ff) else {
            return;
        };
        let ns = AssetNamespace::from_zone_game(game);
        *self.slot_mut(ns) = Some(NamespaceTree::from_anchor(zone_ff.to_path_buf()));
    }

    pub fn get(&self, ns: AssetNamespace) -> Option<&NamespaceTree> {
        match ns {
            AssetNamespace::Iw4 => self.iw4.as_ref(),
            AssetNamespace::T5 => self.t5.as_ref(),
            AssetNamespace::Iw5 => self.iw5.as_ref(),
            AssetNamespace::T6 => self.t6.as_ref(),
        }
    }

    pub fn main_for(&self, ns: AssetNamespace) -> Option<&Path> {
        self.get(ns).and_then(|tree| tree.main.as_deref())
    }

    pub fn is_empty(&self) -> bool {
        self.iw4.is_none() && self.t5.is_none() && self.iw5.is_none() && self.t6.is_none()
    }

    pub fn present(&self) -> impl Iterator<Item = (AssetNamespace, &NamespaceTree)> {
        [
            (AssetNamespace::Iw4, self.iw4.as_ref()),
            (AssetNamespace::T5, self.t5.as_ref()),
            (AssetNamespace::Iw5, self.iw5.as_ref()),
            (AssetNamespace::T6, self.t6.as_ref()),
        ]
        .into_iter()
        .filter_map(|(ns, tree)| tree.map(|tree| (ns, tree)))
    }

    pub fn report_lines(&self) -> Vec<String> {
        [
            AssetNamespace::Iw4,
            AssetNamespace::T5,
            AssetNamespace::Iw5,
            AssetNamespace::T6,
        ]
        .into_iter()
        .map(|ns| match self.get(ns) {
            Some(tree) => format!(
                "namespace tree {}: main={} anchor={}",
                ns.as_str(),
                tree.main
                    .as_deref()
                    .map_or_else(|| "-".to_owned(), |m| m.display().to_string()),
                tree.anchor.display(),
            ),
            None => format!("namespace tree {}: not installed", ns.as_str()),
        })
        .collect()
    }

    fn slot_mut(&mut self, ns: AssetNamespace) -> &mut Option<NamespaceTree> {
        match ns {
            AssetNamespace::Iw4 => &mut self.iw4,
            AssetNamespace::T5 => &mut self.t5,
            AssetNamespace::Iw5 => &mut self.iw5,
            AssetNamespace::T6 => &mut self.t6,
        }
    }
}

const fn zone_game_of(ns: AssetNamespace) -> ZoneGame {
    match ns {
        AssetNamespace::Iw4 => ZoneGame::Iw4,
        AssetNamespace::T5 => ZoneGame::T5,
        AssetNamespace::Iw5 => ZoneGame::Iw5,
        AssetNamespace::T6 => ZoneGame::T6,
    }
}

#[derive(Debug, Default)]
pub struct NamespaceSoundIwd {
    iw4: Option<crate::iwd::IwdSoundIndex>,
    t5: Option<crate::iwd::IwdSoundIndex>,
    iw5: Option<crate::iwd::IwdSoundIndex>,
}

impl NamespaceSoundIwd {
    pub fn open(trees: &NamespaceTrees) -> (Self, Vec<String>) {
        let mut opened = Self::default();
        let mut lines = Vec::new();
        for (ns, tree) in trees.present() {
            let Some(main) = tree.main.as_deref() else {
                lines.push(format!(
                    "audio: IWD sounds {}: tree {} has no archive directory",
                    ns.as_str(),
                    tree.anchor.display()
                ));
                continue;
            };
            match crate::iwd::IwdSoundIndex::open(main) {
                Ok(index) => {
                    lines.push(format!(
                        "audio: IWD sounds {} from {} — {} paths",
                        ns.as_str(),
                        index.indexed_dir().display(),
                        index.sound_count()
                    ));
                    if let Some(slot) = opened.slot_mut(ns) {
                        *slot = Some(index);
                    }
                }
                Err(e) => lines.push(format!(
                    "audio: IWD sounds {} unavailable ({e})",
                    ns.as_str()
                )),
            }
        }
        (opened, lines)
    }

    pub fn index(&self, ns: AssetNamespace) -> Option<&crate::iwd::IwdSoundIndex> {
        match ns {
            AssetNamespace::Iw4 => self.iw4.as_ref(),
            AssetNamespace::T5 => self.t5.as_ref(),
            AssetNamespace::Iw5 => self.iw5.as_ref(),
            AssetNamespace::T6 => None,
        }
    }

    pub fn read_sound(
        &self,
        ns: AssetNamespace,
        relative: &str,
    ) -> Option<Result<Vec<u8>, String>> {
        self.index(ns)?.read_sound(relative)
    }

    pub fn is_empty(&self) -> bool {
        self.iw4.is_none() && self.t5.is_none() && self.iw5.is_none()
    }

    fn slot_mut(&mut self, ns: AssetNamespace) -> Option<&mut Option<crate::iwd::IwdSoundIndex>> {
        match ns {
            AssetNamespace::Iw4 => Some(&mut self.iw4),
            AssetNamespace::T5 => Some(&mut self.t5),
            AssetNamespace::Iw5 => Some(&mut self.iw5),
            AssetNamespace::T6 => None,
        }
    }
}
