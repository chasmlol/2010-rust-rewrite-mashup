mod entropy;
mod fourier;

use std::fmt;

const FRAME: usize = crate::T5WmaProfile::FRAME_SAMPLES;
const MAX_PCM_SAMPLES: usize = 64 * 1024 * 1024;
// Two channels of ready/overlap/envelope/spectrum data plus block and FFT work
// fit below this bound. Shared immutable transform/entropy tables are separate.
const FRAME_WORKSPACE_BYTES: usize = 512 * 1024;

type Result<T> = std::result::Result<T, DecodeError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodeError(String);

impl DecodeError {
    fn new(message: &str) -> Self {
        Self(message.to_owned())
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
    limit: usize,
}

impl Cursor<'_> {
    fn preview(&self, width: usize) -> u32 {
        let mut value = 0;
        for offset in 0..width {
            let bit = self.position + offset;
            value <<= 1;
            if bit < self.limit {
                value |= u32::from((self.data[bit / 8] >> (7 - bit % 8)) & 1);
            }
        }
        value
    }

    fn take(&mut self, width: usize) -> Result<u32> {
        if width > 24 || width > self.limit.saturating_sub(self.position) {
            return Err(DecodeError::new("truncated bit field"));
        }
        let value = self.preview(width);
        self.position += width;
        Ok(value)
    }
}

struct Packet {
    start: usize,
    end: usize,
    carry: usize,
    frames: usize,
    continuation: bool,
}

fn packet_stream(
    data: &[u8],
    count: usize,
    alignment: usize,
) -> Result<(Vec<u8>, Vec<Packet>, usize)> {
    let length = count
        .checked_mul(alignment)
        .ok_or_else(|| DecodeError::new("packet length overflow"))?;
    let data = data
        .get(..length)
        .ok_or_else(|| DecodeError::new("truncated packets"))?;
    let mut bytes = Vec::with_capacity(length);
    let mut length = 0;
    let mut packets = Vec::with_capacity(count);
    let mut pending = false;
    for packet in data.chunks_exact(alignment) {
        let mut input = Cursor {
            data: packet,
            position: 4,
            limit: packet.len() * 8,
        };
        let frames = input.take(4)? as usize;
        let continuation = frames == 0 || (frames == 1 && !pending);
        if frames == 0 && !pending {
            return Err(DecodeError::new("continuation without a frame"));
        }
        let carry = if continuation {
            0
        } else {
            input.take(13)? as usize
        };
        if carry > input.limit - input.position {
            return Err(DecodeError::new("invalid frame boundary"));
        }
        let start = length;
        while input.position < input.limit {
            if length % 8 == 0 {
                bytes.push(0);
            }
            bytes[length / 8] |= (input.take(1)? as u8) << (7 - length % 8);
            length += 1;
        }
        packets.push(Packet {
            start,
            end: length,
            carry,
            frames: if continuation { 0 } else { frames - 1 },
            continuation,
        });
        pending = true;
    }
    Ok((bytes, packets, length))
}

#[derive(Default)]
struct Envelope {
    amplitudes: Vec<f32>,
    maximum: f32,
}

impl Envelope {
    fn refresh(&mut self, input: &mut Cursor<'_>, rate: u32, size: usize) -> Result<()> {
        self.amplitudes.clear();
        self.maximum = 0.0;
        let mut exponent = 36i32;
        for &width in entropy::format().bands(rate, size)? {
            exponent += i32::from(entropy::format().books[4].read(input)?.value);
            if !(-60..=95).contains(&exponent) {
                return Err(DecodeError::new("exponent out of range"));
            }
            let amplitude = 10f64.powf(f64::from(exponent) / 16.0) as f32;
            self.maximum = self.maximum.max(amplitude);
            self.amplitudes
                .resize(self.amplitudes.len() + width, amplitude);
        }
        Ok(())
    }
}

struct Synthesis {
    rate: u32,
    channels: usize,
    envelopes: Vec<Envelope>,
    ready: Vec<Vec<f32>>,
    overlap: Vec<Vec<f32>>,
    frames: usize,
    sizes: [usize; 3],
    reset: bool,
}

impl Synthesis {
    fn frame(&mut self, input: &mut Cursor<'_>) -> Result<()> {
        if (self.frames + 3) * FRAME * self.channels > MAX_PCM_SAMPLES {
            return Err(DecodeError::new("PCM limit exceeded"));
        }
        let mut offset = 0;
        while offset < FRAME {
            if self.reset {
                self.sizes[0] = Self::size(input)?;
                self.sizes[1] = Self::size(input)?;
                self.reset = false;
            } else {
                self.sizes[0] = self.sizes[1];
                self.sizes[1] = self.sizes[2];
            }
            self.sizes[2] = Self::size(input)?;
            let [before, size, after] = self.sizes;
            if size + offset > FRAME {
                return Err(DecodeError::new("block exceeds frame"));
            }
            let joint = self.channels == 2 && input.take(1)? != 0;
            let coded = (0..self.channels)
                .map(|_| input.take(1).map(|bit| bit != 0))
                .collect::<Result<Vec<_>>>()?;
            let mut spectrum = vec![vec![0.0; size]; self.channels];
            if coded.iter().any(|&value| value) {
                let mut gain = 1u32;
                loop {
                    let part = input.take(7)?;
                    gain += part;
                    if gain > 1024 {
                        return Err(DecodeError::new("gain out of range"));
                    }
                    if part != 127 {
                        break;
                    }
                }
                let magnitude_bits = match gain {
                    0..15 => 13,
                    15..32 => 12,
                    32..40 => 11,
                    40..45 => 10,
                    _ => 9,
                };
                let refresh = size == FRAME || input.take(1)? != 0;
                for (channel, &present) in coded.iter().enumerate() {
                    if present && refresh {
                        self.envelopes[channel].refresh(input, self.rate, size)?;
                    }
                }
                for (channel, &present) in coded.iter().enumerate() {
                    if !present {
                        continue;
                    }
                    let envelope = &self.envelopes[channel];
                    if envelope.amplitudes.is_empty() {
                        return Err(DecodeError::new("missing exponent envelope"));
                    }
                    let book =
                        if self.rate == 44100 { 0 } else { 2 } + usize::from(joint && channel == 1);
                    let multiplier = (10f64.powf(f64::from(gain) * 0.05)
                        / f64::from(envelope.maximum)) as f32
                        * (2.0 / size as f32);
                    let cutoff = 1864 >> (FRAME.trailing_zeros() - size.trailing_zeros());
                    let mut bin = 0;
                    while bin < cutoff {
                        let word = entropy::format().books[book].read(input)?;
                        let (zeros, magnitude) = match word.operation {
                            0 => {
                                let magnitude = input.take(magnitude_bits)? as i32;
                                (input.take(11)? as usize, magnitude)
                            }
                            1 => break,
                            2 => (usize::from(word.zeros), i32::from(word.value)),
                            _ => return Err(DecodeError::new("invalid coefficient word")),
                        };
                        bin += zeros;
                        let sign = if input.take(1)? != 0 { 1.0 } else { -1.0 };
                        if bin >= size {
                            return Err(DecodeError::new("coefficient run exceeds block"));
                        }
                        if bin < cutoff {
                            spectrum[channel][bin] = (magnitude as f32 * sign)
                                * envelope.amplitudes[bin * envelope.amplitudes.len() / size]
                                * multiplier;
                        }
                        bin += 1;
                    }
                }
            }
            if joint && let [left, right] = &mut spectrum[..] {
                for (middle, difference) in left.iter_mut().zip(right.iter_mut()).take(size) {
                    (*middle, *difference) = (*middle + *difference, *middle - *difference);
                }
            }
            let start = FRAME / 2 + offset - size / 2;
            let mut block = vec![0.0; 2 * size];
            for (channel, coefficients) in spectrum.iter().enumerate() {
                fourier::plan(size).inverse(coefficients, &mut block);
                for (index, &value) in block.iter().enumerate() {
                    let weight = fourier::window(index, size, before, after);
                    let destination = &mut self.overlap[channel][start + index];
                    if index >= size || (weight == 1.0 && size > before) {
                        *destination = value * weight;
                    } else if weight != 0.0 {
                        *destination += value * weight;
                    }
                }
            }
            offset += size;
        }
        for channel in 0..self.channels {
            self.ready[channel][..].copy_from_slice(&self.overlap[channel][..FRAME]);
            self.overlap[channel].copy_within(FRAME..2 * FRAME, 0);
        }
        self.frames += 1;
        Ok(())
    }

    fn size(input: &mut Cursor<'_>) -> Result<usize> {
        let index = input.take(3)?;
        if index > 4 {
            return Err(DecodeError::new("invalid block size"));
        }
        Ok(FRAME >> index)
    }

    fn emit(
        &self,
        channels: &[Vec<f32>],
        sink: &mut impl crate::XwmaPcmSink,
    ) -> std::result::Result<(), crate::XwmaDecodeError> {
        let mut output = [0i16; 2 * FRAME];
        for sample in 0..FRAME {
            for (channel, samples) in channels.iter().enumerate() {
                let value = samples[sample] * 32768.0;
                if !value.is_finite() {
                    return Err(crate::XwmaDecodeError::Decode("non-finite PCM".into()));
                }
                output[sample * self.channels + channel] =
                    value.round_ties_even().clamp(-32768.0, 32767.0) as i16;
            }
        }
        sink.extend(&output[..FRAME * self.channels])
    }

    fn emit_frame(
        &self,
        sink: &mut impl crate::XwmaPcmSink,
    ) -> std::result::Result<(), crate::XwmaDecodeError> {
        if self.frames > crate::T5WmaProfile::PRIMING_FRAMES {
            self.emit(&self.ready, sink)?;
        }
        Ok(())
    }

    fn finish(
        &self,
        sink: &mut impl crate::XwmaPcmSink,
    ) -> std::result::Result<(), crate::XwmaDecodeError> {
        if self.frames < crate::T5WmaProfile::PRIMING_FRAMES {
            return Err(crate::XwmaDecodeError::EmptyPcm);
        }
        self.emit(&self.overlap, sink)
    }
}

pub(crate) fn decode(
    data: &[u8],
    seek: &[u32],
    channels: u32,
    rate: u32,
    sink: &mut impl crate::XwmaPcmSink,
) -> std::result::Result<(), crate::XwmaDecodeError> {
    let profile = crate::T5WmaProfile::from_geometry(channels, rate)
        .ok_or_else(|| DecodeError::new("unsupported T5 WMA2 profile"))?;
    if seek.is_empty() {
        return Err(DecodeError::new("empty packets").into());
    }
    let alignment = profile.packet_bytes();
    let packet_bytes = seek
        .len()
        .checked_mul(alignment)
        .filter(|&bytes| bytes <= data.len())
        .ok_or_else(|| DecodeError::new("truncated packets"))?;
    let workspace = packet_bytes
        .checked_add(
            seek.len()
                .checked_mul(std::mem::size_of::<Packet>())
                .ok_or(crate::XwmaDecodeError::MemoryLimit)?,
        )
        .and_then(|bytes| bytes.checked_add(FRAME_WORKSPACE_BYTES))
        .ok_or(crate::XwmaDecodeError::MemoryLimit)?;
    // The guard covers packet repacking/table storage and bounded frame/FFT scratch.
    let _workspace = sink.reserve_workspace(workspace)?;
    let (bytes, packets, length) = packet_stream(data, seek.len(), alignment)?;
    let mut input = Cursor {
        data: &bytes,
        position: 0,
        limit: length,
    };
    let mut state = Synthesis {
        rate,
        channels: channels as usize,
        envelopes: (0..channels).map(|_| Envelope::default()).collect(),
        ready: vec![vec![0.0; FRAME]; channels as usize],
        overlap: vec![vec![0.0; 2 * FRAME]; channels as usize],
        frames: 0,
        sizes: [FRAME; 3],
        reset: true,
    };
    let mut pending = None;
    for (index, packet) in packets.iter().enumerate() {
        if packet.continuation {
            if pending.is_none() {
                pending = Some(packet.start);
            }
            continue;
        }
        let boundary = packet.start + packet.carry;
        input.limit = boundary;
        if let Some(position) = pending {
            input.position = position;
            state
                .frame(&mut input)
                .map_err(|error| DecodeError(format!("packet {index} carried frame: {error}")))?;
            state.emit_frame(sink)?;
        }
        input.position = boundary;
        input.limit = packet.end;
        state.reset = true;
        for frame in 0..packet.frames {
            state
                .frame(&mut input)
                .map_err(|error| DecodeError(format!("packet {index} frame {frame}: {error}")))?;
            state.emit_frame(sink)?;
        }
        pending = (input.position < packet.end).then_some(input.position);
    }
    state.finish(sink)
}
