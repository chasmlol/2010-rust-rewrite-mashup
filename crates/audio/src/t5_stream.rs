use crate::media::PcmBuffer;
use crate::pcm::{DecodeError, decode_audio_bytes};

pub(crate) fn decode(bytes: &[u8]) -> Result<PcmBuffer, DecodeError> {
    let (data, frames, rate, channels) = (|| {
        let field = |offset: usize| -> Option<u32> {
            Some(u32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        if field(0)? != 1 || field(28)? != 6 {
            return None;
        }
        let frames = field(4)?;
        let rate = field(8)?;
        let channels = field(12)?;
        let header = field(16)? as usize;
        let size = field(48)? as usize;
        if header < 56 {
            return None;
        }
        let data = bytes.get(header..header.checked_add(size)?)?;
        Some((data, frames, rate, channels))
    })()
    .ok_or(DecodeError::Decode)?;
    decode_adpcm(data, frames, rate, channels)
}

pub(crate) fn decode_adpcm(
    data: &[u8],
    frames: u32,
    rate: u32,
    channels: u32,
) -> Result<PcmBuffer, DecodeError> {
    let _wrapper = crate::decode_budget::DecodeReservation::reserve(
        data.len().checked_add(90).ok_or(DecodeError::Decode)?,
    )
    .map_err(DecodeError::Pcm)?;
    let wav = (|| {
        let size = data.len();
        if !(1..=2).contains(&channels) || rate == 0 || size == 0 {
            return None;
        }
        let align = 262 * channels;
        if !size.is_multiple_of(align as usize)
            || frames == 0
            || u64::from(frames) > (size as u64 / u64::from(align)) * 512
        {
            return None;
        }
        let mut wav = Vec::with_capacity(size + 90);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(82_u32.checked_add(size.try_into().ok()?)?).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&50_u32.to_le_bytes());
        for value in [2_u16, channels as u16] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate.checked_mul(align)? / 512).to_le_bytes());
        for value in [align as u16, 4, 32, 512, 7] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        for (a, b) in [
            (256_i16, 0_i16),
            (512, -256),
            (0, 0),
            (192, 64),
            (240, 0),
            (460, -208),
            (392, -232),
        ] {
            wav.extend_from_slice(&a.to_le_bytes());
            wav.extend_from_slice(&b.to_le_bytes());
        }
        wav.extend_from_slice(b"fact");
        wav.extend_from_slice(&4_u32.to_le_bytes());
        wav.extend_from_slice(&frames.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(size as u32).to_le_bytes());
        wav.extend_from_slice(data);
        Some(wav)
    })()
    .ok_or(DecodeError::Decode)?;
    let pcm = decode_audio_bytes(&wav)?;
    if u32::from(pcm.channels()) != channels {
        return Err(DecodeError::Decode);
    }
    let frames = usize::try_from(frames).map_err(|_| DecodeError::Decode)?;
    pcm.prefix_frames(frames).map_err(DecodeError::Pcm)
}
