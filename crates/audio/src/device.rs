use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::render_core::{QUANTUM, RenderShared, SAMPLE_RATE};

pub(crate) fn supervise(shared: Arc<RenderShared>, shutdown: Arc<AtomicBool>) {
    assets::session_load::use_process_cpus();
    crate::diagnostics::thread("audio-device");
    let failed = Arc::new(AtomicU8::new(0));
    let mut backoff = Duration::from_millis(20);
    let mut interrupted_at: Option<std::time::Instant> = None;
    while !shutdown.load(Ordering::Acquire) {
        failed.store(0, Ordering::Release);
        let (error_tx, error_rx) = sync_channel(1);
        match open(shared.clone(), failed.clone(), error_tx) {
            Ok(stream) => {
                if let Some(at) = interrupted_at.take()
                    && crate::diagnostics::enabled()
                {
                    crate::diagnostics::emit(format!(
                        "audio diag: output_recovered down_ms={:.3} frame={}",
                        at.elapsed().as_secs_f64() * 1000.0,
                        shared.frame.load(Ordering::Acquire)
                    ));
                }
                let opened = std::time::Instant::now();
                let mut last_callback = opened;
                let mut callbacks = shared.device_callbacks.load(Ordering::Acquire);
                while !shutdown.load(Ordering::Acquire) && failed.load(Ordering::Acquire) == 0 {
                    std::thread::sleep(Duration::from_millis(5));
                    let current = shared.device_callbacks.load(Ordering::Acquire);
                    if current != callbacks {
                        callbacks = current;
                        last_callback = std::time::Instant::now();
                    } else if last_callback.elapsed() >= Duration::from_millis(250) {
                        failed.store(4, Ordering::Release);
                    }
                }
                interrupted_at = Some(std::time::Instant::now());
                shared.device_active.store(false, Ordering::Release);
                if crate::diagnostics::enabled() {
                    crate::diagnostics::emit(format!(
                        "audio diag: output_lost frame={}",
                        shared.frame.load(Ordering::Acquire)
                    ));
                }
                drop(stream);
                let failure = failed.swap(0, Ordering::AcqRel);
                if failure != 0 {
                    let reason = match failure {
                        1 => "device unavailable",
                        2 => "stream invalidated",
                        4 => "callback stalled",
                        _ => "backend failure",
                    };
                    let detail = error_rx.try_recv().ok();
                    diag::warn!(
                        Audio,
                        "audio: output interrupted: {reason} detail={detail:?} frame={}",
                        shared.frame.load(Ordering::Acquire)
                    );
                }
                if opened.elapsed() >= Duration::from_secs(1) {
                    backoff = Duration::from_millis(20);
                    continue;
                }
            }
            Err(error) => {
                shared.device_active.store(false, Ordering::Release);
                diag::warn!(
                    Audio,
                    "audio: device unavailable, using null transport: {error}"
                );
            }
        }
        let until = std::time::Instant::now() + backoff;
        while std::time::Instant::now() < until {
            if shutdown.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        backoff = (backoff * 2).min(Duration::from_secs(1));
    }
}

pub(crate) fn open(
    shared: Arc<RenderShared>,
    failed: Arc<AtomicU8>,
    error_tx: SyncSender<cpal::StreamError>,
) -> Result<cpal::Stream, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no default output device")?;
    let config = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    if config.channels() == 0 || config.sample_rate() < 8000 {
        return Err("invalid output channel count or unsupported low device rate".into());
    }
    let format = config.sample_format();
    let config = device
        .supported_output_configs()
        .ok()
        .and_then(|mut ranges| {
            ranges.find(|range| {
                range.channels() == config.channels()
                    && range.sample_format() == format
                    && range.min_sample_rate() <= SAMPLE_RATE
                    && range.max_sample_rate() >= SAMPLE_RATE
            })
        })
        .map_or(config, |range| range.with_sample_rate(SAMPLE_RATE));
    let buffer = *config.buffer_size();
    let mut config = config.config();
    if let cpal::SupportedBufferSize::Range { min, max } = buffer
        && min <= 256
        && 256 <= max
    {
        config.buffer_size = cpal::BufferSize::Fixed(256);
    }
    let transport = shared.clone();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, shared, failed, error_tx),
        SampleFormat::F64 => build::<f64>(&device, &config, shared, failed, error_tx),
        SampleFormat::I8 => build::<i8>(&device, &config, shared, failed, error_tx),
        SampleFormat::I16 => build::<i16>(&device, &config, shared, failed, error_tx),
        SampleFormat::I24 => build::<cpal::I24>(&device, &config, shared, failed, error_tx),
        SampleFormat::I32 => build::<i32>(&device, &config, shared, failed, error_tx),
        SampleFormat::I64 => build::<i64>(&device, &config, shared, failed, error_tx),
        SampleFormat::U8 => build::<u8>(&device, &config, shared, failed, error_tx),
        SampleFormat::U16 => build::<u16>(&device, &config, shared, failed, error_tx),
        SampleFormat::U32 => build::<u32>(&device, &config, shared, failed, error_tx),
        SampleFormat::U64 => build::<u64>(&device, &config, shared, failed, error_tx),
        _ => return Err(format!("unsupported device format {format:?}")),
    }
    .map_err(|error| error.to_string())?;
    transport.device_active.store(true, Ordering::Release);
    stream.play().map_err(|error| error.to_string())?;
    diag::info!(
        Audio,
        "audio: output running rate={} channels={} format={format:?} buffer={:?}",
        config.sample_rate,
        config.channels,
        config.buffer_size
    );
    Ok(stream)
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<RenderShared>,
    failed: Arc<AtomicU8>,
    error_tx: SyncSender<cpal::StreamError>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = usize::from(config.channels);
    let step = f64::from(SAMPLE_RATE) / f64::from(config.sample_rate);
    let mut block = [[0.0; 2]; QUANTUM];
    let mut position = QUANTUM;
    let mut fractional = 0.0;
    let mut a = [0.0; 2];
    let mut b = [0.0; 2];
    let mut primed = false;
    let errors = shared.clone();
    device.build_output_stream(
        config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            shared.device_callbacks.fetch_add(1, Ordering::Release);
            let mut next = || {
                if position == QUANTUM {
                    shared.render_for(&mut block, Some(true));
                    position = 0;
                }
                let sample = block[position];
                position += 1;
                sample
            };
            if !primed {
                a = next();
                b = next();
                primed = true;
            }
            let mut frames = output.chunks_exact_mut(channels);
            for frame in frames.by_ref() {
                let t = fractional as f32;
                let left = a[0] + (b[0] - a[0]) * t;
                let right = a[1] + (b[1] - a[1]) * t;
                frame[0] = T::from_sample(if channels == 1 {
                    (left + right) * 0.5
                } else {
                    left
                });
                if channels > 1 {
                    frame[1] = T::from_sample(right);
                }
                for sample in &mut frame[2.min(channels)..] {
                    *sample = T::EQUILIBRIUM;
                }
                fractional += step;
                while fractional >= 1.0 {
                    a = b;
                    b = next();
                    fractional -= 1.0;
                }
            }
            frames.into_remainder().fill(T::EQUILIBRIUM);
        },
        move |error| match error {
            cpal::StreamError::BufferUnderrun => {
                errors.device_underruns.fetch_add(1, Ordering::Relaxed);
            }
            error => {
                let code = match &error {
                    cpal::StreamError::DeviceNotAvailable => 1,
                    cpal::StreamError::StreamInvalidated => 2,
                    _ => 3,
                };
                let _ = error_tx.try_send(error);
                failed.store(code, Ordering::Release);
            }
        },
        Some(Duration::from_secs(1)),
    )
}
