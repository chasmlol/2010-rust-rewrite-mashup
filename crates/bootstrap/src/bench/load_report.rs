//! Section one: where the time between the command and a playable map went.
//!
//! Two views of the same window, because one alone lies. The **waterfall** is
//! the serial story — request, walk, install, spawn, first drawn frame — and it
//! is what a player waits through. The **stages** are the parallel story: the
//! walk runs on the load pool, so a stage's own duration says nothing about
//! whether removing it would shorten anything.
//!
//! `sole_open` is the time a stage was the only one open. It is not an upper
//! bound on what deleting the stage would save: a stage that was never alone
//! can still be exactly what everything behind it is queued on, and deleting
//! it would then save its whole length.
//! The number says how much of the window this stage was the only thing the
//! load was doing. What deleting it would save is a question for
//! `load_jobs.csv` and its dependency edges.

use std::time::{Duration, Instant};

use asset_transport::LoadLaneTiming;

use crate::bench::milestones::Milestones;
use crate::bench::table::{Align, Table, bytes, items, mib, ms, secs};

/// Stages listed by name before the report rolls the rest into one line.
const TOP_STAGES: usize = 16;

pub(crate) fn render(bench: &Milestones, command_start: Option<Instant>, out: &mut Vec<String>) {
    out.push("[1/3] MAP LOAD".to_owned());
    waterfall(bench, command_start, out);
    out.push(String::new());
    stages(&bench.lanes, out);
    out.push(String::new());
    image_plans(out);
    out.push(String::new());
    audio(out);
    out.push(String::new());
    picture(bench, out);
}

/// Each image plan's time split into the four things it can be waiting on, and
/// what its decode bought.
///
/// One number for "a worker had the job" folds together the memory ceiling
/// holding a worker back and the decoding itself, and those have opposite
/// fixes: the first is a scheduling parameter, the second is the decoder.
/// `budget wait` and `decode` are the two apart; `prepared` is what
/// the plan decoded itself, `reused` what it served out of another plan's
/// payload without decoding anything, and `discarded` the part of the two the
/// merge had no row for. Only `prepared` is work, so only `prepared` belongs
/// next to `decode`.
fn image_plans(out: &mut Vec<String>) {
    let jobs = asset_transport::load_jobs::snapshot();
    let plans: Vec<_> = jobs
        .rows
        .iter()
        .filter(|row| row.kind == asset_transport::load_jobs::JobKind::ImageDecode)
        .collect();
    if plans.is_empty() {
        out.push("  image plans: MISS — no image decode job was recorded".to_owned());
        return;
    }
    out.push("  image plans — what each one waited on, and what its decode bought:".to_owned());
    let mut table = Table::new(
        2,
        &[
            ("plan", Align::Left),
            ("ready→queue", Align::Right),
            ("queue", Align::Right),
            ("budget wait", Align::Right),
            ("decode", Align::Right),
            ("done→join", Align::Right),
            ("prepared", Align::Right),
            ("reused", Align::Right),
            ("kept", Align::Right),
            ("discarded", Align::Right),
        ],
    );
    let gap = |from: Option<Instant>, to: Option<Instant>| -> String {
        match (from, to) {
            (Some(from), Some(to)) => ms(to.saturating_duration_since(from).as_nanos() as u64),
            _ => "MISS".to_owned(),
        }
    };
    let size = |value: Option<u64>| value.map_or_else(|| "MISS".to_owned(), mib);
    for row in &plans {
        table.row([
            row.canonical.clone().unwrap_or_else(|| row.id.to_string()),
            gap(row.plan_ready_at, row.enqueued_at),
            gap(row.enqueued_at, row.started_at),
            gap(row.started_at, row.decode_started_at),
            gap(row.decode_started_at, row.finished_at),
            gap(row.finished_at, row.joined_at),
            size(row.prepared_bytes),
            size(row.reused_bytes),
            size(row.retained_bytes),
            size(row.discarded_bytes),
        ]);
    }
    table.render(out);
    let held = plans.iter().filter(|row| row.waited_for_budget).count();
    out.push(format!(
        "  {held} of {} plans were held by the decode budget before they began; `budget wait` is that hold and `decode` is the work. The ceiling is a threshold on starting, not a cap: a plan that is let through decodes all of itself, so outstanding bytes can end above it.",
        plans.len()
    ));
    let (shared, shared_bytes) = asset_material::shared_variant_census();
    let (copied, moved) = asset_material::shared_payload_copy_cost();
    if shared > 0 {
        out.push(format!(
            "  {shared} payloads ({}) were answered out of another asker's decode instead of being read and decoded a second time. Sharing is keyed on the resolved archive entries and the map type, so a different colour space or sampler no longer costs a decode.",
            mib(shared_bytes),
        ));
    } else {
        out.push(
            "  no payload was shared between askers: every one resolved its own archive entries, so what one plan discarded was not another's decode repeated."
                .to_owned(),
        );
    }
    out.push(format!(
        "  placing the images moved {} out of the decode's own buffers and copied {}: the last asker for a payload takes it, and only an asker somebody else is still pointing at pays the memcpy. The copied figure is duplicate materialization; the moved one is bytes that had to exist either way.",
        mib(moved),
        mib(copied),
    ));
    let (payload_reads, header_reads) = asset_material::iwd_entry_reads();
    out.push(format!(
        "  {payload_reads} archive entries were inflated whole and {header_reads} only as far as the IWI header, which is where the cubemap question is answered. A 2D image whose mips the prepared cache already holds is a header read and no payload read at all."
    ));
}

fn waterfall(bench: &Milestones, command_start: Option<Instant>, out: &mut Vec<String>) {
    let origin = command_start.unwrap_or(bench.t0);
    let source = if command_start.is_some() {
        "the shell command"
    } else {
        "process start"
    };
    match bench.playable() {
        Some(playable) => out.push(format!(
            "  command → playable: {} (from {source})",
            secs(playable.saturating_duration_since(origin))
        )),
        None => out.push(format!(
            "  command → playable: MISS — the map never became playable (from {source})"
        )),
    }
    out.push(String::new());

    let mut table = Table::new(
        2,
        &[
            ("milestone", Align::Left),
            ("at", Align::Right),
            ("+step", Align::Right),
            ("what happened in the step", Align::Left),
        ],
    );
    let mut steps: Vec<(&str, Option<Instant>, &str)> = vec![
        (
            "process start",
            Some(bench.t0),
            "argument parsing, window, plugins",
        ),
        ("load requested", bench.request, "zone discovery"),
        ("match installed", bench.installed, "the load-pool walk"),
        (
            "world spawned",
            bench.spawned,
            "scene conversion, GPU upload",
        ),
        (
            "loading screen down",
            bench.overlay_down,
            "overlay torn down",
        ),
        ("in game", bench.ingame, "session handed over"),
        (
            "first drawn frame",
            bench.rendered_spawn,
            "pipelines compiled",
        ),
        ("ambient booted", bench.ambient, "map ambience started"),
    ];
    // In the order they actually happened, not the order they are listed: two
    // of these race, and a "+step" measured against a later milestone would
    // read as a zero rather than as the overlap it is. A milestone that was
    // never reached sorts to the end, where it reads as a gap in the story
    // rather than as its beginning.
    steps.sort_by_key(|(_, at, _)| (at.is_none(), *at));
    let mut previous: Option<Instant> = command_start;
    for (name, at, note) in steps {
        let Some(at) = at else {
            table.row([
                name.to_owned(),
                "MISS".to_owned(),
                "—".to_owned(),
                note.to_owned(),
            ]);
            continue;
        };
        let step = previous.map(|previous| at.saturating_duration_since(previous));
        table.row([
            name.to_owned(),
            secs(at.saturating_duration_since(origin)),
            step.map_or_else(|| "—".to_owned(), secs),
            note.to_owned(),
        ]);
        previous = Some(at);
    }
    table.render(out);
}

fn stages(lanes: &[LoadLaneTiming], out: &mut Vec<String>) {
    if lanes.is_empty() {
        out.push("  load stages: MISS — the walk opened no stage on this run".to_owned());
        return;
    }
    let window = lanes
        .iter()
        .map(LoadLaneTiming::end)
        .max()
        .unwrap_or(Duration::ZERO);
    let held: Duration = lanes.iter().map(|lane| lane.elapsed).sum();
    let covered = union(lanes);
    let sole_open = sole_open(lanes);

    let occupancy = if covered.is_zero() {
        "—".to_owned()
    } else {
        format!("{:.1}", held.as_secs_f64() / covered.as_secs_f64())
    };
    out.push(format!(
        "  load stages: {} stages holding {} across a {} window; {} of that window had no stage open",
        lanes.len(),
        secs(held),
        secs(window),
        secs(window.saturating_sub(covered)),
    ));
    out.push(format!(
        "  mean occupancy {occupancy} stages while anything was open. That is how many stages overlapped on average — not a speed-up, and not a count of busy cores: a stage that spends its time waiting on I/O is open and occupying nothing."
    ));
    out.push(
        "  `sole_open` is the time a stage was the only one open — how much of the window this stage was the only thing the load was doing. It is not a bound on what deleting it would save: a stage that was never alone can still be the dependency everything behind it is queued on, and deleting that one saves its whole length."
            .to_owned(),
    );

    let mut ordered: Vec<(usize, &LoadLaneTiming)> = lanes.iter().enumerate().collect();
    ordered.sort_by_key(|(index, lane)| (std::cmp::Reverse(lane.elapsed), *index));

    let mut table = Table::new(
        2,
        &[
            ("stage", Align::Left),
            ("at", Align::Right),
            ("held", Align::Right),
            ("sole_open", Align::Right),
            ("items", Align::Right),
            ("memory", Align::Left),
        ],
    );
    for (index, lane) in ordered.iter().take(TOP_STAGES) {
        table.row([
            format!("{}{}", lane.label, lane.outcome_suffix()),
            secs(lane.at),
            secs(lane.elapsed),
            secs(sole_open[*index]),
            items(lane.done, lane.total),
            lane.mem_suffix().trim().to_owned(),
        ]);
    }
    table.render(out);
    if ordered.len() > TOP_STAGES {
        let rest: Duration = ordered
            .iter()
            .skip(TOP_STAGES)
            .map(|(_, lane)| lane.elapsed)
            .sum();
        out.push(format!(
            "  … and {} shorter stages holding {}",
            ordered.len() - TOP_STAGES,
            secs(rest)
        ));
    }
}

/// What preparing the match's clips cost, split by the decoder each clip took.
///
/// The stage table above has `preparing match audio` as one row holding one
/// number, which on every run so far has been the largest stage in the load —
/// and a single number cannot say whether that is thousands of cheap byte
/// loops or hundreds of external processes. These are the same clips counted
/// by the decoder that ran, so the two questions have different answers.
///
/// Every duration here is summed over the prep workers, several of which run
/// at once. They are worker time, and adding them to a milestone is wrong.
fn audio(out: &mut Vec<String>) {
    let prep = audio::clip_prep_cost();
    if prep.requests == 0 {
        out.push("  match audio: MISS — this run asked for no clip".to_owned());
        return;
    }
    let prepared: u64 = prep.paths.iter().map(|(_, cost)| cost.prepared).sum();
    let failed: u64 = prep.paths.iter().map(|(_, cost)| cost.failed).sum();
    out.push(format!(
        "  match audio: {prepared} clips prepared, {failed} failed, {} still in flight when the run ended",
        prep.queued.saturating_sub(prepared + failed),
    ));
    out.push(format!(
        "  {} asks resolved to {} clips: {} of the asks were for a clip somebody had already asked for, and the store queued each clip once. {} clips were queued after AudioReady.",
        prep.requests,
        prep.queued,
        prep.requests.saturating_sub(prep.queued),
        prep.late,
    ));
    out.push(format!(
        "  a job waited {} on average before a worker took it ({} over {} jobs, {} prep workers). The sum is worker queue time, not a stretch of the load.",
        ms_secs(prep.queue_wait_ms / prep.queued.max(1) as f64),
        ms_secs(prep.queue_wait_ms),
        prep.queued,
        prep.workers,
    ));

    let mut table = Table::new(
        2,
        &[
            ("decoder", Align::Left),
            ("prepared", Align::Right),
            ("failed", Align::Right),
            ("worker time", Align::Right),
            ("per clip", Align::Right),
            ("resident pcm", Align::Left),
        ],
    );
    let mut silent = Vec::new();
    for (path, cost) in &prep.paths {
        let n = cost.prepared + cost.failed;
        if n == 0 {
            silent.push(path.name());
            continue;
        }
        table.row([
            path.name().to_owned(),
            cost.prepared.to_string(),
            cost.failed.to_string(),
            ms_secs(cost.wall_ms),
            format!("{:.2} ms", cost.wall_ms / n as f64),
            mib(cost.sample_bytes),
        ]);
    }
    table.render(out);
    if !silent.is_empty() {
        out.push(format!("  no clip on this run took: {}", silent.join(", ")));
    }
    xwma(out);
}

fn xwma(out: &mut Vec<String>) {
    let cost = asset_audio::xwma_decode_cost();
    if cost.hit + cost.miss + cost.failed == 0 {
        out.push(
            "  t5 xwma: MISS — no T5 XWMA clip was requested, so the cache answered nothing and nothing was stored."
                .to_owned(),
        );
        return;
    }
    out.push(format!(
        "  t5 xwma: {} cached, {} decoded, {} failed; {} native WMA2 decodes for {} of samples",
        cost.hit,
        cost.miss,
        cost.failed,
        cost.native,
        mib(cost.pcm_bytes),
    ));
    out.push(format!(
        "  t5 xwma worker time: {} in native WMA2, {} hashing payloads into keys, {} reading and writing the cache",
        ms_secs(cost.decode_ms),
        ms_secs(cost.key_ms),
        ms_secs(cost.io_ms),
    ));
    if cost.miss == 0 && cost.failed == 0 {
        out.push(
            "  the cache answered every clip. A warm cache is not a faster decoder: the first run on these zones still paid for all of it, and a zone that changes pays again."
                .to_owned(),
        );
    }
}

/// Milliseconds as seconds. The audio numbers are sums over worker threads and
/// run to tens of seconds, where `ms` would print five digits.
fn ms_secs(ms: f64) -> String {
    format!("{:.3}s", ms / 1000.0)
}

fn picture(bench: &Milestones, out: &mut Vec<String>) {
    let mut lines = Vec::new();
    if let Some(bytes) = bench.progress.zone_image_bytes() {
        lines.push(format!("zone image {}", mib(bytes)));
    }
    match (
        bench.progress.rss_at_open_bytes(),
        asset_transport::peak_resident_bytes(),
    ) {
        (Some(open), Some(peak)) => lines.push(format!("rss {} → {}", mib(open), mib(peak))),
        (_, Some(peak)) => lines.push(format!("rss peak {}", mib(peak))),
        _ => {}
    }
    if !lines.is_empty() {
        out.push(format!("  memory: {}", lines.join(", ")));
        out.push(
            "  RSS is the whole process. A stage's delta is what the process grew across its window, which on a parallel walk includes every other stage that was open — so the per-stage deltas do not add up to the peak and none of them is that stage's ownership."
                .to_owned(),
        );
    }

    let frames = perf::stats::snapshot(perf::Phase::Load)
        .into_iter()
        .find(|stats| stats.span == perf::Span::FramesWallFrameMs);
    match frames {
        Some(frames) => out.push(format!(
            "  frames drawn while loading: {} at {} ms average (the loading screen)",
            frames.count,
            ms(frames.avg_ns())
        )),
        None => out.push("  frames drawn while loading: none reached the frame clock".to_owned()),
    }

    match (bench.screenshot_bytes(), bench.shot) {
        (Some(size), _) => out.push(format!(
            "  screenshot: {} ({}, {} world batches, g0 {})",
            bench.screenshot.display(),
            bytes(size),
            bench
                .batches_at_shot
                .map_or_else(|| "MISS".to_owned(), |n| n.to_string()),
            bench
                .g0_at_shot
                .map_or_else(|| "MISS".to_owned(), |n| n.to_string()),
        )),
        _ => out.push(format!(
            "  screenshot: MISS — nothing was written to {}",
            bench.screenshot.display()
        )),
    }
}

/// Total time during which at least one stage was running.
fn union(lanes: &[LoadLaneTiming]) -> Duration {
    let mut windows: Vec<(Duration, Duration)> =
        lanes.iter().map(|lane| (lane.at, lane.end())).collect();
    windows.sort();
    let mut covered = Duration::ZERO;
    let mut open: Option<(Duration, Duration)> = None;
    for (start, end) in windows {
        match open {
            Some((from, until)) if start <= until => open = Some((from, until.max(end))),
            Some((from, until)) => {
                covered += until.saturating_sub(from);
                open = Some((start, end));
            }
            None => open = Some((start, end)),
        }
    }
    if let Some((from, until)) = open {
        covered += until.saturating_sub(from);
    }
    covered
}

/// Per stage, the time it was the only stage running. A sweep over the edges:
/// between two consecutive edges the set of running stages does not change, so
/// a gap with exactly one stage open belongs to that stage.
fn sole_open(lanes: &[LoadLaneTiming]) -> Vec<Duration> {
    let mut edges: Vec<(Duration, isize, usize)> = Vec::with_capacity(lanes.len() * 2);
    for (index, lane) in lanes.iter().enumerate() {
        edges.push((lane.at, 1, index));
        edges.push((lane.end(), -1, index));
    }
    // Closing edges first at the same instant: a stage that ends where another
    // begins was never alone with it.
    edges.sort_by_key(|(at, delta, index)| (*at, -*delta, *index));

    let mut out = vec![Duration::ZERO; lanes.len()];
    let mut open: Vec<usize> = Vec::new();
    let mut previous = edges
        .first()
        .map(|(at, _, _)| *at)
        .unwrap_or(Duration::ZERO);
    for (at, delta, index) in edges {
        if at > previous && open.len() == 1 {
            out[open[0]] += at - previous;
        }
        previous = at;
        if delta > 0 {
            open.push(index);
        } else if let Some(position) = open.iter().position(|open| *open == index) {
            open.remove(position);
        }
    }
    out
}
