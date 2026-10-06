#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityState {
    pub number: i32,
    pub e_type: i32,
    pub time2: i32,
    pub e_flags: u32,
    pub tr_time: i32,
    pub tr_type: i32,
    pub tr_duration: i32,
    pub tr_delta: [f32; 3],
    pub tr_base: [f32; 3],
    pub apos_tr_time: i32,
    pub apos_tr_type: i32,
    pub apos_tr_duration: i32,
    pub apos_tr_delta: [f32; 3],
    pub apos_tr_base: [f32; 3],
    pub cull_dist: f32,
    pub period: i32,
    pub anonymous_data_2: u32,
    pub waist_pitch: u32,
    pub torso_pitch: u32,
    pub other_entity_num: i32,
    pub attacker_entity_num: i32,
    pub ground_entity_num: i32,
    pub event_parm: i32,
    pub solid: u32,
    pub index: i32,
    pub event_sequence: i32,
    pub events: [i32; 4],
    pub event_parms: [i32; 4],
    pub client_num: i32,
    pub wes: u32,
    pub torso_anim: i32,
    pub legs_anim: i32,
    pub weapon_model: u8,
    pub client_link_info: u32,
    pub part_bits_0: u32,
    pub part_bits_1: u32,
    pub part_bits_2: u32,
    pub part_bits_3: u32,
    pub part_bits_4: u32,
    pub part_bits_5: u32,
    pub client_mask_0: u32,
}

impl EntityState {
    pub fn launch_time(self) -> i32 {
        self.cull_dist.to_bits() as i32
    }

    pub fn set_launch_time(&mut self, time_ms: i32) {
        self.cull_dist = f32::from_bits(time_ms as u32);
    }
}
