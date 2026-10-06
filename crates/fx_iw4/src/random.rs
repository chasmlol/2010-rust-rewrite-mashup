pub const FX_RANDOM_VERSION: u32 = 1;

#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub enum FxRandomChannel {
    AngularPitch = 1,
    AngularRoll = 2,
    AngularYaw = 3,
    Atlas = 4,
    Color = 5,
    Delay = 6,
    EmitDistance = 7,
    GlassCrack = 8,
    GlassShatter = 9,
    Gravity = 10,
    InitialRotation = 11,
    Life = 12,
    OneShotCount = 13,
    Reflection = 14,
    RotationDelta = 15,
    Scale = 16,
    Size = 17,
    SpawnAngleYaw = 18,
    SpawnHeight = 19,
    SpawnOriginX = 20,
    SpawnOriginY = 21,
    SpawnOriginZ = 22,
    SpawnPitch = 23,
    SpawnRadius = 24,
    SpawnRoll = 25,
    SpawnYaw = 26,
    VelocityX = 27,
    VelocityY = 28,
    VelocityZ = 29,
    Visual = 30,
}

const EFFECT_DOMAIN: &[u8; 24] = b"iw4l.random.effect......";
const ELEMENT_DOMAIN: &[u8; 24] = b"iw4l.random.element.....";
const TRAIL_DOMAIN: &[u8; 24] = b"iw4l.random.trail.......";
const SAMPLE_DOMAIN: &[u8; 24] = b"iw4l.random.sample......";

fn digest_word(tuple: &[u8]) -> u64 {
    let digest = blake3::hash(tuple);
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

pub fn effect_random_key(instance: u64, begin_msec: i32) -> u64 {
    let mut tuple = [0_u8; 40];
    tuple[..24].copy_from_slice(EFFECT_DOMAIN);
    tuple[24..28].copy_from_slice(&FX_RANDOM_VERSION.to_le_bytes());
    tuple[28..36].copy_from_slice(&instance.to_le_bytes());
    tuple[36..40].copy_from_slice(&begin_msec.to_le_bytes());
    digest_word(&tuple)
}

pub fn elem_random_seed(effect_key: u64, sequence: u8, begin_msec: i32) -> u64 {
    let mut tuple = [0_u8; 41];
    tuple[..24].copy_from_slice(ELEMENT_DOMAIN);
    tuple[24..28].copy_from_slice(&FX_RANDOM_VERSION.to_le_bytes());
    tuple[28..36].copy_from_slice(&effect_key.to_le_bytes());
    tuple[36] = sequence;
    tuple[37..41].copy_from_slice(&begin_msec.to_le_bytes());
    digest_word(&tuple)
}

pub fn trail_random_seed(effect_key: u64, sequence: i8) -> u64 {
    let mut tuple = [0_u8; 37];
    tuple[..24].copy_from_slice(TRAIL_DOMAIN);
    tuple[24..28].copy_from_slice(&FX_RANDOM_VERSION.to_le_bytes());
    tuple[28..36].copy_from_slice(&effect_key.to_le_bytes());
    tuple[36..37].copy_from_slice(&sequence.to_le_bytes());
    digest_word(&tuple)
}

fn sample_word(key: u64, channel: FxRandomChannel, index: u64) -> u64 {
    let mut tuple = [0_u8; 48];
    tuple[..24].copy_from_slice(SAMPLE_DOMAIN);
    tuple[24..28].copy_from_slice(&FX_RANDOM_VERSION.to_le_bytes());
    tuple[28..36].copy_from_slice(&key.to_le_bytes());
    tuple[36..40].copy_from_slice(&(channel as u32).to_le_bytes());
    tuple[40..48].copy_from_slice(&index.to_le_bytes());
    digest_word(&tuple)
}

pub fn sample_u16(key: u64, channel: FxRandomChannel) -> u16 {
    (sample_word(key, channel, 0) >> 48) as u16
}

pub fn sample_f32(key: u64, channel: FxRandomChannel) -> f32 {
    sample_at(key, channel, 0)
}

pub fn sample_at(key: u64, channel: FxRandomChannel, index: u64) -> f32 {
    (sample_word(key, channel, index) >> 40) as f32 / 16_777_216.0
}

pub fn elem_visual_index(count: u8, key: u64) -> usize {
    if count <= 1 {
        return 0;
    }
    ((u32::from(sample_u16(key, FxRandomChannel::Visual)) * u32::from(count)) >> 16) as usize
}
