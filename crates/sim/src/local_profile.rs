use bevy_ecs::prelude::Resource;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalPlayerProfile {
    pub percent_complete_sp: u8,
    pub percent_complete_mp: u8,
    pub percent_complete_so: u8,
}

impl LocalPlayerProfile {
    pub const FIELDS: [&str; 3] = [
        "percentcompletesp",
        "percentcompletemp",
        "percentcompleteso",
    ];

    pub fn get(&self, name: &str) -> Option<u8> {
        if name.eq_ignore_ascii_case(Self::FIELDS[0]) {
            Some(self.percent_complete_sp)
        } else if name.eq_ignore_ascii_case(Self::FIELDS[1]) {
            Some(self.percent_complete_mp)
        } else if name.eq_ignore_ascii_case(Self::FIELDS[2]) {
            Some(self.percent_complete_so)
        } else {
            None
        }
    }

    pub fn set(&mut self, name: &str, value: u8) -> bool {
        let slot = if name.eq_ignore_ascii_case(Self::FIELDS[0]) {
            &mut self.percent_complete_sp
        } else if name.eq_ignore_ascii_case(Self::FIELDS[1]) {
            &mut self.percent_complete_mp
        } else if name.eq_ignore_ascii_case(Self::FIELDS[2]) {
            &mut self.percent_complete_so
        } else {
            return false;
        };
        *slot = value;
        true
    }
}
