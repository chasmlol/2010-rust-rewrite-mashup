use std::fmt;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use asset_transport::{cache_flight, cache_open, cache_put_with, fnv1a64, fnv1a64_more};

pub const T5_WMA: i32 = 7;

const XWMA_CACHE_FORMAT: u32 = 2;
const XWMA_CACHE_KIND: &str = "xwma_pcm";
const XWMA_CACHE_MAGIC: &[u8; 8] = b"IWLXWMA\n";
const XWMA_CACHE_HEADER: usize = 8 + 4 * 4;
const MAX_PCM_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XwmaDecodeError {
    Decode(String),
    EmptyPcm,
    UnsupportedProfile { channels: u32, rate: u32 },
    MemoryLimit,
    CacheRead,
    InvalidSink,
}

impl fmt::Display for XwmaDecodeError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(output, "T5 WMA2: {error}"),
            Self::EmptyPcm => output.write_str("T5 WMA2: empty PCM"),
            Self::UnsupportedProfile { channels, rate } => write!(
                output,
                "T5 WMA2: unsupported profile channels={channels} rate={rate}"
            ),
            Self::MemoryLimit => output.write_str("T5 WMA2: memory limit"),
            Self::CacheRead => output.write_str("T5 WMA2: cache read failed"),
            Self::InvalidSink => output.write_str("T5 WMA2: output must start empty"),
        }
    }
}

impl From<crate::wma_t5::DecodeError> for XwmaDecodeError {
    fn from(error: crate::wma_t5::DecodeError) -> Self {
        Self::Decode(error.to_string())
    }
}

pub trait XwmaPcmSink {
    type Workspace;

    fn reserve_workspace(&mut self, bytes: usize) -> Result<Self::Workspace, XwmaDecodeError>;
    fn extend(&mut self, samples: &[i16]) -> Result<(), XwmaDecodeError>;
    fn sample_count(&self) -> usize;
    fn visit_samples(
        &self,
        visitor: &mut dyn FnMut(&[i16]) -> std::io::Result<()>,
    ) -> std::io::Result<()>;
}

pub fn decode_t5_xwma(
    packets: &[u8],
    seek_table: &[u32],
    channels: u32,
    rate: u32,
    sink: &mut impl XwmaPcmSink,
) -> Result<(), XwmaDecodeError> {
    let result = (|| {
        crate::T5WmaProfile::from_geometry(channels, rate)
            .ok_or(XwmaDecodeError::UnsupportedProfile { channels, rate })?;
        if sink.sample_count() != 0 {
            return Err(XwmaDecodeError::InvalidSink);
        }
        let key_at = Instant::now();
        let key = cache_key(packets, seek_table, channels, rate);
        KEY_NS.fetch_add(key_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
        let _flight = cache_flight(XWMA_CACHE_KIND, &key);
        if cached(&key, channels, rate, sink)? {
            return Ok(());
        }
        let decode_at = Instant::now();
        NATIVE.fetch_add(1, Ordering::Relaxed);
        let result = crate::wma_t5::decode(packets, seek_table, channels, rate, sink);
        DECODE_NS.fetch_add(decode_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
        result?;
        MISS.fetch_add(1, Ordering::Relaxed);
        PCM_BYTES.fetch_add((sink.sample_count() * 2) as u64, Ordering::Relaxed);
        store(&key, channels, rate, sink);
        Ok(())
    })();
    if result.is_err() {
        FAILED.fetch_add(1, Ordering::Relaxed);
    }
    result
}

fn cached(
    key: &str,
    channels: u32,
    rate: u32,
    sink: &mut impl XwmaPcmSink,
) -> Result<bool, XwmaDecodeError> {
    let io_at = Instant::now();
    let result = (|| {
        let Some(mut file) = cache_open(XWMA_CACHE_KIND, key) else {
            return Ok(false);
        };
        let mut header = [0u8; XWMA_CACHE_HEADER];
        if file.read_exact(&mut header).is_err() || &header[..8] != XWMA_CACHE_MAGIC {
            return Ok(false);
        }
        let word = |at| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        let len = word(20) as usize;
        if word(8) != XWMA_CACHE_FORMAT
            || word(12) != channels
            || word(16) != rate
            || len == 0
            || len > MAX_PCM_BYTES
            || !len.is_multiple_of(channels as usize * 2)
            || file
                .metadata()
                .map_or(true, |meta| meta.len() != (XWMA_CACHE_HEADER + len) as u64)
        {
            return Ok(false);
        }
        let mut bytes = [0u8; 8192];
        let mut samples = [0i16; 4096];
        let mut remaining = len;
        while remaining != 0 {
            let take = remaining.min(bytes.len());
            file.read_exact(&mut bytes[..take])
                .map_err(|_| XwmaDecodeError::CacheRead)?;
            for (sample, pair) in samples.iter_mut().zip(bytes[..take].as_chunks::<2>().0) {
                *sample = i16::from_le_bytes(*pair);
            }
            sink.extend(&samples[..take / 2])?;
            remaining -= take;
        }
        HIT.fetch_add(1, Ordering::Relaxed);
        PCM_BYTES.fetch_add(len as u64, Ordering::Relaxed);
        Ok(true)
    })();
    IO_NS.fetch_add(io_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    result
}

fn store(key: &str, channels: u32, rate: u32, sink: &impl XwmaPcmSink) {
    let io_at = Instant::now();
    let result = cache_put_with(XWMA_CACHE_KIND, key, |file| {
        let len = sink
            .sample_count()
            .checked_mul(2)
            .and_then(|len| u32::try_from(len).ok())
            .ok_or_else(|| std::io::Error::other("PCM length overflow"))?;
        file.write_all(XWMA_CACHE_MAGIC)?;
        for word in [XWMA_CACHE_FORMAT, channels, rate, len] {
            file.write_all(&word.to_le_bytes())?;
        }
        let mut bytes = [0u8; 8192];
        sink.visit_samples(&mut |samples| {
            for chunk in samples.chunks(bytes.len() / 2) {
                for (pair, sample) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(chunk) {
                    *pair = sample.to_le_bytes();
                }
                file.write_all(&bytes[..chunk.len() * 2])?;
            }
            Ok(())
        })
    });
    if let Err(error) = result {
        diag::warn!(Audio, "xwma cache store {key}: {error}");
    }
    IO_NS.fetch_add(io_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
}

fn cache_key(packets: &[u8], seek_table: &[u32], channels: u32, rate: u32) -> String {
    let mut hash = fnv1a64(&XWMA_CACHE_FORMAT.to_le_bytes());
    hash = fnv1a64_more(hash, &T5_WMA.to_le_bytes());
    hash = fnv1a64_more(hash, &channels.to_le_bytes());
    hash = fnv1a64_more(hash, &rate.to_le_bytes());
    hash = fnv1a64_more(hash, &(seek_table.len() as u64).to_le_bytes());
    for entry in seek_table {
        hash = fnv1a64_more(hash, &entry.to_le_bytes());
    }
    hash = fnv1a64_more(hash, &(packets.len() as u64).to_le_bytes());
    hash = fnv1a64_more(hash, packets);
    format!("{XWMA_CACHE_FORMAT:08x}-{hash:016x}")
}

static HIT: AtomicU64 = AtomicU64::new(0);
static MISS: AtomicU64 = AtomicU64::new(0);
static NATIVE: AtomicU64 = AtomicU64::new(0);
static FAILED: AtomicU64 = AtomicU64::new(0);
static PCM_BYTES: AtomicU64 = AtomicU64::new(0);
static DECODE_NS: AtomicU64 = AtomicU64::new(0);
static KEY_NS: AtomicU64 = AtomicU64::new(0);
static IO_NS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XwmaDecodeCost {
    pub hit: u64,
    pub miss: u64,
    pub native: u64,
    pub failed: u64,
    pub pcm_bytes: u64,
    pub decode_ms: f64,
    pub key_ms: f64,
    pub io_ms: f64,
}

pub fn xwma_decode_cost() -> XwmaDecodeCost {
    XwmaDecodeCost {
        hit: HIT.load(Ordering::Relaxed),
        miss: MISS.load(Ordering::Relaxed),
        native: NATIVE.load(Ordering::Relaxed),
        failed: FAILED.load(Ordering::Relaxed),
        pcm_bytes: PCM_BYTES.load(Ordering::Relaxed),
        decode_ms: DECODE_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        key_ms: KEY_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
        io_ms: IO_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
    }
}
