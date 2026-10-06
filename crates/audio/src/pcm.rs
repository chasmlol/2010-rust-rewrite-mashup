#[path = "sab_media.rs"]
pub(crate) mod sab_media;

#[path = "t5_stream.rs"]
pub(crate) mod t5_stream;

use crate::decode_budget::{DecodeReservation, DecodeSamples};
use crate::media::PcmBuffer;

#[derive(Debug)]
pub(crate) enum DecodeError {
    Decode,
    Read,
    UnsupportedCodec(asset_audio::SabCodec),
    MetadataMismatch,
    Pcm(crate::media::PcmError),
}

pub(crate) fn decode_audio_bytes(bytes: &[u8]) -> Result<PcmBuffer, DecodeError> {
    if let Some(pcm) = riff_pcm(bytes) {
        return pcm;
    }
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let _input = DecodeReservation::reserve(bytes.len()).map_err(DecodeError::Pcm)?;
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| DecodeError::Decode)?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(DecodeError::Decode)?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.ok_or(DecodeError::Decode)?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| DecodeError::Decode)?;

    let mut samples = DecodeSamples::new().map_err(DecodeError::Pcm)?;
    let mut channels: u16 = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::ResetRequired) => continue,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(_) => continue,
        };
        let count = decoded.spec().channels.count();
        if !(1..=2).contains(&count) {
            return Err(DecodeError::Pcm(
                crate::media::PcmError::UnsupportedChannels,
            ));
        }
        channels = channels.max(count as u16);
        let bytes = decoded
            .capacity()
            .checked_mul(count)
            .and_then(|n| n.checked_mul(size_of::<i16>()))
            .ok_or(DecodeError::Pcm(crate::media::PcmError::MemoryLimit))?;
        let _interleaved = DecodeReservation::reserve(bytes).map_err(DecodeError::Pcm)?;
        let mut interleaved = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        interleaved.copy_interleaved_ref(decoded);
        samples
            .extend(interleaved.samples())
            .map_err(DecodeError::Pcm)?;
    }
    samples
        .into_pcm(channels, sample_rate)
        .map_err(DecodeError::Pcm)
}

fn riff_pcm(bytes: &[u8]) -> Option<Result<PcmBuffer, DecodeError>> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut fmt = None;
    let mut data = None;
    let mut at = 12usize;
    while let Some(header) = bytes.get(at..at.checked_add(8)?) {
        let size = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let body = at + 8;
        let Some(end) = body.checked_add(size) else {
            return Some(Err(DecodeError::Decode));
        };
        let Some(chunk) = bytes.get(body..end) else {
            return Some(Err(DecodeError::Decode));
        };
        match &header[..4] {
            b"fmt " if chunk.len() >= 16 => fmt = Some(chunk),
            b"data" => data = Some(chunk),
            _ => {}
        }
        at = body.checked_add(size)?.checked_add(size & 1)?;
    }
    let fmt = fmt?;
    let field = |offset: usize| u16::from_le_bytes([fmt[offset], fmt[offset + 1]]);
    let (tag, channels, bits) = (field(0), field(2), field(14));
    let rate = u32::from_le_bytes(fmt[4..8].try_into().ok()?);
    if tag != 1 || !matches!(bits, 8 | 16) {
        return None;
    }
    let data = data?;
    Some((|| {
        PcmBuffer::validate_geometry(channels, rate).map_err(DecodeError::Pcm)?;
        let width = usize::from(bits / 8);
        let frame_bytes = width * usize::from(channels);
        if !data.len().is_multiple_of(frame_bytes) {
            return Err(DecodeError::Pcm(crate::media::PcmError::PartialFrame));
        }
        let mut samples = DecodeSamples::for_frames(data.len() / frame_bytes, channels, rate)
            .map_err(DecodeError::Pcm)?;
        let mut block = [0i16; 4096];
        for frame in data.chunks(block.len() * width) {
            if bits == 16 {
                for (sample, pair) in block.iter_mut().zip(frame.as_chunks::<2>().0) {
                    *sample = i16::from_le_bytes([pair[0], pair[1]]);
                }
            } else {
                for (sample, &byte) in block.iter_mut().zip(frame) {
                    *sample = (i16::from(byte) - 128) << 8;
                }
            }
            samples
                .extend(&block[..frame.len() / width])
                .map_err(DecodeError::Pcm)?;
        }
        samples.into_pcm(channels, rate).map_err(DecodeError::Pcm)
    })())
}
