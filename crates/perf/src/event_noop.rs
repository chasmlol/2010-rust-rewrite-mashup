#![allow(clippy::too_many_arguments)]

pub fn player_tick(
    _: u32,
    _: bool,
    _: i32,
    _: i32,
    _: [f32; 3],
    _: f32,
    _: f32,
    _: f32,
    _: i32,
    _: u32,
    _: i32,
    _: i32,
    _: Option<i32>,
    _: i32,
    _: i32,
    _: &'static str,
) {
}
pub fn death(_: u32, _: Option<u32>, _: u8, _: u32) {}
pub fn owner_shot(_: &str, _: u32, _: u32) {}
pub fn net_leg(_: &str, _: u32, _: u32) {}
pub fn net_path(_: &str, _: u64, _: u64, _: u64, _: u64, _: u64) {}
pub fn projectile(_: u32) {}
pub fn projectile_phase(_: u32, _: u32, _: &str, _: i32) {}
pub fn truck(_: u32, _: Option<i64>, _: Option<i64>, _: Option<&str>, _: Option<&str>) {}
pub fn feel(
    _: i32,
    _: Option<&str>,
    _: Option<i32>,
    _: Option<i32>,
    _: Option<&str>,
    _: Option<i32>,
    _: Option<[f32; 3]>,
    _: Option<[f32; 3]>,
    _: Option<f32>,
    _: Option<i64>,
    _: Option<i32>,
    _: Option<i32>,
) {
}
pub fn corpse(_: i64, _: Option<f32>, _: Option<i64>, _: Option<&str>) {}
pub fn item(_: i32, _: Option<i32>, _: Option<i32>, _: Option<&str>) {}
pub fn pickup(_: i32) {}
pub fn pickup_rejected(_: u32, _: i32, _: i32) {}
pub fn remote(_: u32, _: i32, _: i32, _: [f32; 3], _: Option<[f32; 3]>, _: Option<&str>) {}
pub fn lighting_fail() {}
pub fn render_owner_plan(
    _: u64,
    _: Option<u64>,
    _: &str,
    _: u32,
    _: Option<&str>,
    _: &str,
    _: Option<u16>,
    _: Option<[f32; 3]>,
    _: Option<bool>,
    _: Option<u32>,
    _: u32,
) {
}
pub fn render_owner_submit(
    _: u64,
    _: &str,
    _: u32,
    _: Option<u16>,
    _: &str,
    _: Option<&str>,
    _: Option<&str>,
    _: u32,
    _: u32,
    _: u32,
    _: u32,
    _: u32,
) {
}
pub fn match_torn(_: &str) {}
pub fn match_installed(_: &str, _: i64) {}
pub fn world_hold(_: i64, _: i64, _: i64) {}
pub fn world_ready(_: i64) {}
pub fn sim_hold(_: i64, _: i64, _: i64) {}
pub fn ambient_hold(_: i64) {}
pub fn ambient_boot(_: &str, _: Option<&str>) {}
pub fn swap(_: u64, _: &str, _: &str) {}
pub fn theater(_: i64, _: Option<i64>, _: Option<i64>) {}
pub fn cgame_hold(_: &str, _: i64) {}

pub fn benchmark_mark(_label: &str, _sequence: u64, _elapsed_ns: u64) {}
