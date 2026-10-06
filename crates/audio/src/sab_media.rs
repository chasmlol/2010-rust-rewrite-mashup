use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use asset_audio::{SabCodec, SabMediaSource};

use crate::decode_budget::{DecodeReservation, DecodeSamples};
use crate::media::{PcmBuffer, PcmError};
use crate::pcm::DecodeError;

pub(crate) fn prepare(source: &SabMediaSource) -> Result<PcmBuffer, DecodeError> {
    let codec = source.codec();
    if let SabCodec::Unsupported(_) = codec {
        return Err(DecodeError::UnsupportedCodec(codec));
    }
    let channels = u16::from(source.entry.channels);
    let rate = source
        .entry
        .frame_rate()
        .ok_or(DecodeError::Pcm(PcmError::InvalidRate))?;
    let frames = source.entry.frame_count as usize;
    let mut samples =
        DecodeSamples::for_frames(frames, channels, rate).map_err(DecodeError::Pcm)?;
    let count = frames
        .checked_mul(usize::from(channels))
        .ok_or(DecodeError::Pcm(PcmError::MemoryLimit))?;
    let size = source.entry.size as usize;
    if codec == SabCodec::PcmS16 && count.checked_mul(size_of::<i16>()) != Some(size) {
        return Err(DecodeError::MetadataMismatch);
    }
    let _input = DecodeReservation::reserve(size).map_err(DecodeError::Pcm)?;
    let mut file = File::open(&source.bank).map_err(|_| DecodeError::Read)?;
    let end = u64::from(source.entry.offset) + u64::from(source.entry.size);
    if end > file.metadata().map_err(|_| DecodeError::Read)?.len() {
        return Err(DecodeError::Read);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| DecodeError::Pcm(PcmError::MemoryLimit))?;
    bytes.resize(size, 0);
    file.seek(SeekFrom::Start(u64::from(source.entry.offset)))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|_| DecodeError::Read)?;
    match codec {
        SabCodec::PcmS16 => {
            let mut block = [0i16; 4096];
            for chunk in bytes.chunks(block.len() * size_of::<i16>()) {
                for (sample, bytes) in block.iter_mut().zip(chunk.as_chunks::<2>().0) {
                    *sample = i16::from_le_bytes(*bytes);
                }
                samples
                    .extend(&block[..chunk.len() / 2])
                    .map_err(DecodeError::Pcm)?;
            }
            samples.into_pcm(channels, rate).map_err(DecodeError::Pcm)
        }
        SabCodec::Flac => decode_flac(bytes, samples, channels, rate, frames),
        SabCodec::Unsupported(_) => Err(DecodeError::UnsupportedCodec(codec)),
    }
}

fn decode_flac(
    bytes: Vec<u8>,
    mut samples: DecodeSamples,
    channels: u16,
    rate: u32,
    frames: usize,
) -> Result<PcmBuffer, DecodeError> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{CODEC_TYPE_FLAC, DecoderOptions};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let options = MediaSourceStreamOptions::default();
    let scratch = bytes
        .len()
        .checked_add(options.buffer_len)
        .ok_or(DecodeError::Pcm(PcmError::MemoryLimit))?;
    let _reader = DecodeReservation::reserve(scratch).map_err(DecodeError::Pcm)?;
    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), options);
    let mut format = symphonia::default::get_probe()
        .format(
            Hint::new().with_extension("flac"),
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| DecodeError::Decode)?
        .format;
    let track = format.default_track().ok_or(DecodeError::Decode)?;
    let track_id = track.id;
    let params = &track.codec_params;
    if params.codec != CODEC_TYPE_FLAC
        || params.sample_rate != Some(rate)
        || params.channels.map(|channels| channels.count()) != Some(usize::from(channels))
        || params
            .n_frames
            .is_some_and(|native| native != frames as u64)
    {
        return Err(DecodeError::MetadataMismatch);
    }
    // FLAC STREAMINFO's maximum block size is a u16. Symphonia's FLAC decoder
    // allocates one planar i32 buffer for that block when constructed.
    const MAX_BLOCK_FRAMES: usize = u16::MAX as usize;
    let _decoder =
        DecodeReservation::reserve(MAX_BLOCK_FRAMES * usize::from(channels) * size_of::<i32>())
            .map_err(DecodeError::Pcm)?;
    let mut decoder = symphonia::default::get_codecs()
        .make(params, &DecoderOptions { verify: true })
        .map_err(|_| DecodeError::Decode)?;
    let mut decoded_frames = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(_) => return Err(DecodeError::Decode),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet).map_err(|_| DecodeError::Decode)?;
        if decoded.spec().rate != rate || decoded.spec().channels.count() != usize::from(channels) {
            return Err(DecodeError::MetadataMismatch);
        }
        decoded_frames = decoded_frames
            .checked_add(decoded.frames())
            .filter(|&count| count <= frames)
            .ok_or(DecodeError::MetadataMismatch)?;
        if decoded.capacity() > MAX_BLOCK_FRAMES {
            return Err(DecodeError::MetadataMismatch);
        }
        let scratch = decoded
            .capacity()
            .checked_mul(usize::from(channels))
            .and_then(|count| count.checked_mul(size_of::<i16>()))
            .ok_or(DecodeError::Pcm(PcmError::MemoryLimit))?;
        let _interleaved = DecodeReservation::reserve(scratch).map_err(DecodeError::Pcm)?;
        let mut interleaved = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        interleaved.copy_interleaved_ref(decoded);
        samples
            .extend(interleaved.samples())
            .map_err(DecodeError::Pcm)?;
    }
    if decoded_frames != frames {
        return Err(DecodeError::MetadataMismatch);
    }
    if decoder.finalize().verify_ok == Some(false) {
        return Err(DecodeError::Decode);
    }
    samples.into_pcm(channels, rate).map_err(DecodeError::Pcm)
}
