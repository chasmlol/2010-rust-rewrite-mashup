#[cfg(all(unix, feature = "native"))]
mod event;
#[cfg(not(all(unix, feature = "native")))]
#[path = "event_noop.rs"]
mod event;
pub mod frames;
pub mod run;
#[cfg(all(unix, feature = "native"))]
mod session;
#[cfg(not(all(unix, feature = "native")))]
#[path = "session_noop.rs"]
mod session;
pub mod stats;
mod switch;
#[cfg(all(unix, feature = "native"))]
mod vocabulary;
#[cfg(not(all(unix, feature = "native")))]
#[path = "vocabulary_noop.rs"]
mod vocabulary;
mod vocabulary_catalog;
mod vocabulary_types;

pub use event::{
    ambient_boot, ambient_hold, benchmark_mark, cgame_hold, corpse, death, feel, item,
    lighting_fail, match_installed, match_torn, net_leg, net_path, owner_shot, pickup,
    pickup_rejected, player_tick, projectile, projectile_phase, remote, render_owner_plan,
    render_owner_submit, sim_hold, swap, theater, truck, world_hold, world_ready,
};
pub use session::{RunMetadata, enabled, flush, start};
pub use switch::on as switch;

/// Whether *any* recorder is listening: the Perfetto session (`IW4L_PERF`) or
/// the in-process bench statistics (`IW4L_BENCH`). Census code that only asked
/// [`enabled`] was silently free on a `IW4L_BENCH=1` run, which left the bench
/// report's counter tables empty and indistinguishable from a workload that
/// genuinely never drew anything.
#[inline]
pub fn recording() -> bool {
    stats::enabled() || enabled()
}
pub use stats::{Anomalies, CounterStats, Phase, SpanStats};
pub use vocabulary::{Counter, Span, SpanGuard};
pub use vocabulary_types::{Origin, Unit};
