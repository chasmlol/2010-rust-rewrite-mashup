//! `summary.json`: the same numbers as `report.txt`, for something that is not
//! a person.
//!
//! The text report is laid out to be read and diffed by eye, which makes it a
//! poor thing to parse: columns move when a name gets longer, and a row can be
//! MISS. This writes the same snapshot as data — every span and every counter,
//! both phases, with the units named — so an A/B comparison is a script and not
//! a careful reading of two terminals.
//!
//! It is a dump of what the recorder holds, deliberately not a verdict. There
//! is no "regression" field, no threshold and no pass/fail: deciding what a
//! difference means needs the manifest next to it and a human who knows what
//! changed.

use std::path::Path;

use perf::{CounterStats, Origin, Phase, SpanStats, Unit};
use serde_json::{Value, json};

use crate::bench::manifest::RuntimeFacts;

pub(crate) fn write(path: &Path, facts: &RuntimeFacts) -> Result<(), String> {
    let value = build(facts);
    let bytes =
        serde_json::to_vec_pretty(&value).map_err(|error| format!("encode summary: {error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn build(facts: &RuntimeFacts) -> Value {
    let anomalies = perf::stats::anomalies();
    json!({
        "format": "iw4l-bench-summary-1",
        "run": perf::run::id(),
        "zone": facts.zone,
        "phases": {
            "load": phase(Phase::Load),
            "live": phase(Phase::Live),
        },
        "audio": audio(),
        "recorder": {
            "percentile_relative_error": perf::stats::PRECISION,
            "unmatched_end": anomalies.unmatched_end,
            "reopened": anomalies.reopened,
            "misnested": anomalies.misnested,
            "unattributed_counters": anomalies.unattributed_counters,
            "counters_never_sampled": perf::stats::counters_never_sampled()
                .into_iter()
                .map(|counter| counter.name())
                .collect::<Vec<_>>(),
            "spans_never_sampled": perf::stats::spans_never_sampled()
                .into_iter()
                .map(|span| span.name())
                .collect::<Vec<_>>(),
        },
    })
}

/// The load-side audio numbers the text report prints, as data.
///
/// They are not in `phases` above: those are the recorder's per-frame
/// histograms, and these are process totals for work that happens once, on
/// worker threads, before a frame exists. Putting them in the same object
/// would invite a consumer to divide them by a frame count.
fn audio() -> Value {
    let prep = audio::clip_prep_cost();
    let pcm = audio::pcm_memory();
    let decode = audio::decode_memory();
    let xwma = asset_audio::xwma_decode_cost();
    json!({
        "workers": prep.workers,
        "requests": prep.requests,
        "queued": prep.queued,
        "queue_wait_ms": prep.queue_wait_ms,
        "queue_deferred": prep.queue_deferred,
        "request_limit": prep.request_limit,
        "late_prepares": prep.late,
        "decode_memory": { "limit_bytes": decode.limit_bytes, "live_bytes": decode.live_bytes, "peak_bytes": decode.peak_bytes, "refused": decode.refused },
        "pcm_memory": { "limit_bytes": pcm.limit_bytes, "live_bytes": pcm.live_bytes, "peak_bytes": pcm.peak_bytes, "refused": pcm.refused },
        "paths": prep.paths.iter().map(|(path, cost)| json!({
            "decoder": path.name(),
            "prepared": cost.prepared,
            "failed": cost.failed,
            "worker_ms": cost.wall_ms,
            "resident_pcm_bytes": cost.sample_bytes,
        })).collect::<Vec<_>>(),
        "xwma": {
            "cache_hit": xwma.hit,
            "cache_miss": xwma.miss,
            "backend": "native_wma2",
            "native_decodes": xwma.native,
            "failed": xwma.failed,
            "pcm_bytes": xwma.pcm_bytes,
            "decode_ms": xwma.decode_ms,
            "key_ms": xwma.key_ms,
            "cache_io_ms": xwma.io_ms,
        },
    })
}

fn phase(phase: Phase) -> Value {
    let spans = perf::stats::snapshot(phase);
    // The frame clock is the denominator for every per-frame figure, so it is
    // named once here rather than recomputed by each consumer — and left null
    // when the phase drew no frame, so a consumer dividing by it has to decide
    // what that means instead of getting a silent zero.
    let frames = spans
        .iter()
        .find(|row| row.span == perf::Span::FramesWallFrameMs)
        .map(|wall| wall.count);
    json!({
        "frames": frames,
        "spans": spans.iter().map(|row| span(row, frames)).collect::<Vec<_>>(),
        "counters": perf::stats::counter_snapshot(phase)
            .iter()
            .map(|row| counter(row, frames))
            .collect::<Vec<_>>(),
    })
}

fn span(row: &SpanStats, frames: Option<u64>) -> Value {
    json!({
        "name": row.span.name(),
        "parent": row.parent.map(perf::Span::name),
        "parent_varies": row.parent_varies,
        "unit": "ns",
        "calls": row.count,
        "calls_per_frame": frames.map(|frames| ratio(row.count as f64, frames)),
        "cost_per_call_ns": row.avg_ns(),
        "cost_per_frame_ns": frames.map(|frames| ratio(row.sum_ns as f64, frames)),
        "total_ns": row.sum_ns,
        "min_ns": row.min_ns,
        "max_ns": row.max_ns,
        "p50_ns": row.p50_ns,
        "p95_ns": row.p95_ns,
        "p99_ns": row.p99_ns,
    })
}

fn counter(row: &CounterStats, frames: Option<u64>) -> Value {
    json!({
        "name": row.counter.name(),
        "unit": match row.unit {
            Unit::Milliseconds => "ms",
            Unit::Count => "count",
        },
        "origin": match row.origin {
            Origin::Cpu => "cpu",
            Origin::Gpu => "gpu",
        },
        "samples": row.samples,
        "rejected": row.rejected,
        "samples_per_frame": frames.map(|frames| ratio(row.samples as f64, frames)),
        "per_call": row.avg(),
        "per_frame": frames.map(|frames| ratio(row.sum, frames)),
        "total": row.sum,
        "min": row.min,
        "max": row.max,
        "p50": row.p50,
        "p95": row.p95,
        "p99": row.p99,
    })
}

fn ratio(value: f64, frames: u64) -> Option<f64> {
    (frames > 0).then(|| value / frames as f64)
}
