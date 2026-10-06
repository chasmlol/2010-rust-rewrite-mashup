use anim_iw4::{
    FrameKind, Quat, Vec3, full_quat, half_quat, quantized_trans, sample_quat, sample_vec3,
    time_to_frame,
};

const ANIM_LOOP: u8 = 0x1;
const ANIM_DELTA: u8 = 0x2;

#[derive(Debug, Clone, Default)]
pub struct RawXAnimParts {
    pub name: String,
    pub data_byte: Vec<u8>,
    pub data_short: Vec<u16>,
    pub data_int: Vec<u32>,
    pub random_data_byte: Vec<u8>,
    pub random_data_short: Vec<u16>,
    pub random_data_int: Vec<u32>,
    pub numframes: u16,
    pub flags: u8,

    pub bone_count: [u8; 10],
    pub framerate: f32,
    pub names: Vec<String>,
    pub notifies: Vec<ClipNotify>,
    pub indices: Vec<u16>,

    pub delta_trans: Option<RawDeltaTrans>,
}

#[derive(Debug, Clone, Default)]
pub struct RawDeltaTrans {
    pub size: u16,
    pub small: bool,

    pub mins: [f32; 3],
    pub step: [f32; 3],
    pub indices: Vec<u16>,

    pub packed: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipError {
    Exhausted {
        pool: &'static str,
        needed: usize,
        remaining: usize,
    },
    Trailing {
        name: String,
        pool: &'static str,
        remaining: usize,
    },
    InvalidBoneCounts {
        name: String,
        names: usize,
        quats: usize,
        trans: usize,
    },
    InvalidBoneIndex {
        name: String,
        bone: usize,
        tracks: usize,
    },
    DuplicateTranslation {
        name: String,
        bone: usize,
    },
    MissingTranslation {
        name: String,
        bone: usize,
    },
    NonMonotonicIndices {
        name: String,
    },
}

pub type Result<T> = core::result::Result<T, ClipError>;

#[derive(Debug, Clone)]
pub struct Keyed<T> {
    pub values: Vec<T>,
    pub frames: FrameIndices,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameIndices {
    Dense,
    Sparse(Vec<u16>),
}

#[derive(Debug, Clone)]
pub enum Rotation {
    Default,
    HalfKeyed(Keyed<Quat>),
    FullKeyed(Keyed<Quat>),
    HalfConstant(Quat),
    FullConstant(Quat),
}

#[derive(Debug, Clone)]
pub enum Translation {
    Default,
    SmallKeyed(Keyed<Vec3>),
    FullKeyed(Keyed<Vec3>),
    Constant(Vec3),
}

#[derive(Debug, Clone)]
pub struct Track {
    pub name: String,
    pub rotation: Rotation,
    pub translation: Translation,
}

#[derive(Debug, Clone)]
pub struct AnimClip {
    pub name: String,
    pub framerate: f32,
    pub numframes: u16,
    pub looping: bool,
    pub tracks: Vec<Track>,
    pub notifies: Vec<ClipNotify>,

    pub delta_translation: Translation,
}

#[derive(Debug, Clone)]
pub struct ClipNotify {
    pub name: String,
    pub time: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SampledTrack {
    pub rotation: Option<Quat>,
    pub translation: Option<Vec3>,
}

impl AnimClip {
    pub fn from_parts(parts: &RawXAnimParts) -> Result<Self> {
        let quats: usize = parts.bone_count[..5].iter().map(|&n| n as usize).sum();
        let trans: usize = parts.bone_count[5..9].iter().map(|&n| n as usize).sum();
        if parts.names.len() != quats || parts.names.len() != trans {
            return Err(ClipError::InvalidBoneCounts {
                name: parts.name.clone(),
                names: parts.names.len(),
                quats,
                trans,
            });
        }

        let mut tracks = parts
            .names
            .iter()
            .cloned()
            .map(|name| Track {
                name,
                rotation: Rotation::Default,
                translation: Translation::Default,
            })
            .collect::<Vec<_>>();
        let mut cursor = Cursor::new(parts);
        let byte_indices = parts.numframes < 256;
        let mut bone = 0;

        for _ in 0..parts.bone_count[0] {
            tracks[bone].rotation = Rotation::Default;
            bone += 1;
        }
        for _ in 0..parts.bone_count[1] {
            let size = cursor.data_u16()?;
            let indices = cursor.indices(size, byte_indices)?;
            let values = (0..=size)
                .map(|_| Ok(half_quat([cursor.random_short()?, cursor.random_short()?])))
                .collect::<Result<Vec<_>>>()?;
            tracks[bone].rotation = Rotation::HalfKeyed(Keyed::new(values, indices, &parts.name)?);
            bone += 1;
        }
        for _ in 0..parts.bone_count[2] {
            let size = cursor.data_u16()?;
            let indices = cursor.indices(size, byte_indices)?;
            let values = (0..=size)
                .map(|_| {
                    Ok(full_quat([
                        cursor.random_short()?,
                        cursor.random_short()?,
                        cursor.random_short()?,
                        cursor.random_short()?,
                    ]))
                })
                .collect::<Result<Vec<_>>>()?;
            tracks[bone].rotation = Rotation::FullKeyed(Keyed::new(values, indices, &parts.name)?);
            bone += 1;
        }
        for _ in 0..parts.bone_count[3] {
            tracks[bone].rotation =
                Rotation::HalfConstant(half_quat([cursor.data_short()?, cursor.data_short()?]));
            bone += 1;
        }
        for _ in 0..parts.bone_count[4] {
            tracks[bone].rotation = Rotation::FullConstant(full_quat([
                cursor.data_short()?,
                cursor.data_short()?,
                cursor.data_short()?,
                cursor.data_short()?,
            ]));
            bone += 1;
        }

        let mut assigned = vec![false; tracks.len()];
        for _ in 0..parts.bone_count[5] {
            let bone = cursor.bone_index(tracks.len(), &parts.name)?;
            assign_translation(&mut assigned, bone, &parts.name)?;
            let size = cursor.data_u16()?;
            let mins = cursor.float3()?;
            let step = cursor.float3()?;
            let indices = cursor.indices(size, byte_indices)?;
            let values = (0..=size)
                .map(|_| {
                    Ok(quantized_trans(
                        mins,
                        step,
                        [
                            cursor.random_byte()?,
                            cursor.random_byte()?,
                            cursor.random_byte()?,
                        ],
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            tracks[bone].translation =
                Translation::SmallKeyed(Keyed::new(values, indices, &parts.name)?);
        }
        for _ in 0..parts.bone_count[6] {
            let bone = cursor.bone_index(tracks.len(), &parts.name)?;
            assign_translation(&mut assigned, bone, &parts.name)?;
            let size = cursor.data_u16()?;
            let mins = cursor.float3()?;
            let step = cursor.float3()?;
            let indices = cursor.indices(size, byte_indices)?;
            let values = (0..=size)
                .map(|_| {
                    Ok(quantized_trans(
                        mins,
                        step,
                        [
                            cursor.random_short()? as u16,
                            cursor.random_short()? as u16,
                            cursor.random_short()? as u16,
                        ],
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            tracks[bone].translation =
                Translation::FullKeyed(Keyed::new(values, indices, &parts.name)?);
        }
        for _ in 0..parts.bone_count[7] {
            let bone = cursor.bone_index(tracks.len(), &parts.name)?;
            assign_translation(&mut assigned, bone, &parts.name)?;
            tracks[bone].translation = Translation::Constant(cursor.float3()?);
        }
        for _ in 0..parts.bone_count[8] {
            let bone = cursor.bone_index(tracks.len(), &parts.name)?;
            assign_translation(&mut assigned, bone, &parts.name)?;
            tracks[bone].translation = Translation::Default;
        }

        for (bone, assigned) in assigned.into_iter().enumerate() {
            if !assigned {
                return Err(ClipError::MissingTranslation {
                    name: parts.name.clone(),
                    bone,
                });
            }
        }
        cursor.finish()?;

        let delta_translation = decode_delta_trans(parts).unwrap_or(Translation::Default);

        Ok(Self {
            name: parts.name.clone(),
            framerate: parts.framerate,
            numframes: parts.numframes,
            looping: parts.flags & ANIM_LOOP != 0,
            tracks,
            notifies: parts.notifies.clone(),
            delta_translation,
        })
    }

    pub fn duration(&self) -> f32 {
        if self.framerate > 0.0 {
            self.numframes as f32 / self.framerate
        } else {
            0.0
        }
    }

    pub fn frequency(&self) -> f32 {
        let duration = self.duration();
        if duration > 0.0 { 1.0 / duration } else { 0.0 }
    }

    pub fn time_to_frame(&self, time: f32) -> f32 {
        time_to_frame(time, self.framerate, self.numframes, self.looping)
    }

    pub fn sample_track(&self, track: usize, time: f32) -> Option<SampledTrack> {
        let track = self.tracks.get(track)?;
        let frame = self.time_to_frame(time);
        Some(SampledTrack {
            rotation: track.rotation.sample(frame),
            translation: track.translation.sample(frame),
        })
    }

    #[must_use]
    pub fn abs_delta_trans(&self, frac: f32) -> [f32; 3] {
        let Some(v) = self
            .delta_translation
            .sample(frac.clamp(0.0, 1.0) * f32::from(self.numframes))
        else {
            return [0.0; 3];
        };
        v
    }

    pub fn move_speed(&self) -> f32 {
        let duration = self.duration();
        if duration == 0.0 {
            return 0.0;
        }
        let start = self.abs_delta_trans(0.0);
        let end = self.abs_delta_trans(1.0);
        anim_iw4::vec3_distance(start, end) / duration
    }

    #[must_use]
    pub fn length_msec(&self) -> i32 {
        (self.duration() * 1000.0) as i32
    }

    #[must_use]
    pub fn has_delta(&self) -> bool {
        !matches!(self.delta_translation, Translation::Default)
    }
}

fn decode_delta_trans(parts: &RawXAnimParts) -> Result<Translation> {
    if parts.flags & ANIM_DELTA == 0 {
        return Ok(Translation::Default);
    }
    let Some(delta) = parts.delta_trans.as_ref() else {
        return Ok(Translation::Default);
    };
    if delta.size == 0 {
        return Ok(Translation::Constant(delta.mins));
    }
    let n = delta.size as usize + 1;
    if delta.indices.len() != n {
        return Ok(Translation::Default);
    }
    let mins = delta.mins;
    let step = delta.step;
    let values = if delta.small {
        if delta.packed.len() < n * 3 {
            return Ok(Translation::Default);
        }
        (0..n)
            .map(|i| {
                let o = i * 3;
                quantized_trans(
                    mins,
                    step,
                    [delta.packed[o], delta.packed[o + 1], delta.packed[o + 2]],
                )
            })
            .collect()
    } else {
        if delta.packed.len() < n * 6 {
            return Ok(Translation::Default);
        }
        (0..n)
            .map(|i| {
                let o = i * 6;
                let x = u16::from_le_bytes([delta.packed[o], delta.packed[o + 1]]);
                let y = u16::from_le_bytes([delta.packed[o + 2], delta.packed[o + 3]]);
                let z = u16::from_le_bytes([delta.packed[o + 4], delta.packed[o + 5]]);
                quantized_trans(mins, step, [x, y, z])
            })
            .collect()
    };
    Ok(if delta.small {
        Translation::SmallKeyed(Keyed::new(values, delta.indices.clone(), &parts.name)?)
    } else {
        Translation::FullKeyed(Keyed::new(values, delta.indices.clone(), &parts.name)?)
    })
}

impl<T> Keyed<T> {
    fn new(values: Vec<T>, frames: Vec<u16>, name: &str) -> Result<Self> {
        if frames.windows(2).any(|pair| pair[1] < pair[0]) {
            return Err(ClipError::NonMonotonicIndices {
                name: name.to_owned(),
            });
        }
        let dense = frames
            .iter()
            .enumerate()
            .all(|(i, &frame)| frame == i as u16);
        Ok(Self {
            values,
            frames: if dense {
                FrameIndices::Dense
            } else {
                FrameIndices::Sparse(frames)
            },
        })
    }

    fn kind_and_frames(&self) -> (FrameKind, &[u16]) {
        match &self.frames {
            FrameIndices::Dense => (FrameKind::Dense, &[]),
            FrameIndices::Sparse(frames) => (FrameKind::Sparse, frames.as_slice()),
        }
    }
}

impl Rotation {
    fn sample(&self, frame: f32) -> Option<Quat> {
        match self {
            Self::Default => None,
            Self::HalfConstant(value) | Self::FullConstant(value) => Some(*value),
            Self::HalfKeyed(keys) | Self::FullKeyed(keys) => {
                let (kind, frames) = keys.kind_and_frames();
                Some(sample_quat(kind, frames, &keys.values, frame))
            }
        }
    }
}

impl Translation {
    fn sample(&self, frame: f32) -> Option<Vec3> {
        match self {
            Self::Default => None,
            Self::Constant(value) => Some(*value),
            Self::SmallKeyed(keys) | Self::FullKeyed(keys) => {
                let (kind, frames) = keys.kind_and_frames();
                Some(sample_vec3(kind, frames, &keys.values, frame))
            }
        }
    }
}

fn assign_translation(assigned: &mut [bool], bone: usize, name: &str) -> Result<()> {
    if assigned[bone] {
        return Err(ClipError::DuplicateTranslation {
            name: name.to_owned(),
            bone,
        });
    }
    assigned[bone] = true;
    Ok(())
}

struct Cursor<'a> {
    parts: &'a RawXAnimParts,
    data_byte: usize,
    data_short: usize,
    data_int: usize,
    random_byte: usize,
    random_short: usize,
    indices: usize,
}

impl<'a> Cursor<'a> {
    fn new(parts: &'a RawXAnimParts) -> Self {
        Self {
            parts,
            data_byte: 0,
            data_short: 0,
            data_int: 0,
            random_byte: 0,
            random_short: 0,
            indices: 0,
        }
    }

    fn data_byte(&mut self) -> Result<u8> {
        let value = *self.parts.data_byte.get(self.data_byte).ok_or_else(|| {
            self.exhausted("dataByte", 1, self.parts.data_byte.len(), self.data_byte)
        })?;
        self.data_byte += 1;
        Ok(value)
    }

    fn data_short(&mut self) -> Result<i16> {
        let value = *self.parts.data_short.get(self.data_short).ok_or_else(|| {
            self.exhausted("dataShort", 1, self.parts.data_short.len(), self.data_short)
        })? as i16;
        self.data_short += 1;
        Ok(value)
    }

    fn data_u16(&mut self) -> Result<u16> {
        let value = *self.parts.data_short.get(self.data_short).ok_or_else(|| {
            self.exhausted("dataShort", 1, self.parts.data_short.len(), self.data_short)
        })?;
        self.data_short += 1;
        Ok(value)
    }

    fn random_byte(&mut self) -> Result<u8> {
        let value = *self
            .parts
            .random_data_byte
            .get(self.random_byte)
            .ok_or_else(|| {
                self.exhausted(
                    "randomDataByte",
                    1,
                    self.parts.random_data_byte.len(),
                    self.random_byte,
                )
            })?;
        self.random_byte += 1;
        Ok(value)
    }

    fn random_short(&mut self) -> Result<i16> {
        let value = *self
            .parts
            .random_data_short
            .get(self.random_short)
            .ok_or_else(|| {
                self.exhausted(
                    "randomDataShort",
                    1,
                    self.parts.random_data_short.len(),
                    self.random_short,
                )
            })? as i16;
        self.random_short += 1;
        Ok(value)
    }

    fn float3(&mut self) -> Result<Vec3> {
        Ok([self.float()?, self.float()?, self.float()?])
    }

    fn float(&mut self) -> Result<f32> {
        let value = *self.parts.data_int.get(self.data_int).ok_or_else(|| {
            self.exhausted("dataInt", 1, self.parts.data_int.len(), self.data_int)
        })?;
        self.data_int += 1;
        Ok(f32::from_bits(value))
    }

    fn bone_index(&mut self, tracks: usize, name: &str) -> Result<usize> {
        let bone = self.data_byte()? as usize;
        if bone >= tracks {
            return Err(ClipError::InvalidBoneIndex {
                name: name.to_owned(),
                bone,
                tracks,
            });
        }
        Ok(bone)
    }

    fn indices(&mut self, stored_size: u16, byte_indices: bool) -> Result<Vec<u16>> {
        let count = stored_size as usize + 1;
        if byte_indices {
            return (0..count)
                .map(|_| self.data_byte().map(u16::from))
                .collect();
        }
        if stored_size >= 64 {
            let end = self.indices + count;
            let values = self
                .parts
                .indices
                .get(self.indices..end)
                .ok_or_else(|| {
                    self.exhausted("indices", count, self.parts.indices.len(), self.indices)
                })?
                .to_vec();
            self.indices = end;
            let checkpoints = (count - 2) / 256 + 2;
            self.skip_short(checkpoints)?;
            Ok(values)
        } else {
            (0..count)
                .map(|_| self.data_short().map(|value| value as u16))
                .collect()
        }
    }

    fn skip_short(&mut self, count: usize) -> Result<()> {
        let end = self.data_short + count;
        if end > self.parts.data_short.len() {
            return Err(self.exhausted(
                "dataShort",
                count,
                self.parts.data_short.len(),
                self.data_short,
            ));
        }
        self.data_short = end;
        Ok(())
    }

    fn exhausted(
        &self,
        pool: &'static str,
        needed: usize,
        length: usize,
        cursor: usize,
    ) -> ClipError {
        ClipError::Exhausted {
            pool,
            needed,
            remaining: length.saturating_sub(cursor),
        }
    }

    fn finish(&self) -> Result<()> {
        for (pool, cursor, len) in [
            ("dataByte", self.data_byte, self.parts.data_byte.len()),
            ("dataShort", self.data_short, self.parts.data_short.len()),
            ("dataInt", self.data_int, self.parts.data_int.len()),
            (
                "randomDataByte",
                self.random_byte,
                self.parts.random_data_byte.len(),
            ),
            (
                "randomDataShort",
                self.random_short,
                self.parts.random_data_short.len(),
            ),
            ("indices", self.indices, self.parts.indices.len()),
        ] {
            if cursor != len {
                return Err(ClipError::Trailing {
                    name: self.parts.name.clone(),
                    pool,
                    remaining: len - cursor,
                });
            }
        }
        Ok(())
    }
}
