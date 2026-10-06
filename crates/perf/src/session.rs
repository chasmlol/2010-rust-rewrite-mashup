use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use perfetto_sdk::heap_buffer::HeapBuffer;
use perfetto_sdk::pb_msg::{PbMsg, PbMsgWriter};
use perfetto_sdk::producer::{Backends, Producer, ProducerInitArgsBuilder};
use perfetto_sdk::protos::config::data_source_config::DataSourceConfig;
use perfetto_sdk::protos::config::trace_config::{
    TraceConfig, TraceConfigBufferConfig, TraceConfigDataSource,
};
use perfetto_sdk::protos::config::track_event::track_event_config::TrackEventConfig;
use perfetto_sdk::tracing_session::TracingSession;
use perfetto_sdk::track_event::TrackEvent;

const ENV: &str = "IW4L_PERF";

const TRACE_BUFFER_KB: u32 = 256 * 1024;

#[derive(Clone, Debug)]
pub struct RunMetadata {
    pub zone: Option<String>,
    pub role: String,

    pub focus: Option<String>,
}

struct SessionState {
    session: TracingSession,
    dir: PathBuf,
    metadata: RunMetadata,
    command_line: Vec<String>,
    git: Result<String, String>,
    started_at: Instant,
    started_unix_ms: u128,
    flushed: bool,
}

unsafe impl Send for SessionState {}

static STATE: Mutex<Option<SessionState>> = Mutex::new(None);
static ATEXIT_ARMED: AtomicBool = AtomicBool::new(false);

pub fn enabled() -> bool {
    crate::switch::on(ENV)
}

pub fn start(metadata: RunMetadata) -> Result<Option<PathBuf>, String> {
    if !enabled() {
        return Ok(None);
    }
    let mut guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(state) = guard.as_ref() {
        return Ok(Some(state.dir.clone()));
    }

    let dir = crate::run::dir()?;

    Producer::init(
        ProducerInitArgsBuilder::new()
            .backends(Backends::IN_PROCESS)
            .build(),
    );
    TrackEvent::init();
    crate::vocabulary::register_categories();

    let mut session = TracingSession::in_process()
        .map_err(|error| format!("TracingSession::in_process: {error:?}"))?;
    session.setup(&trace_config());
    session.start_blocking();

    *guard = Some(SessionState {
        session,
        dir: dir.clone(),
        metadata,
        command_line: std::env::args().collect(),
        git: git_revision(),
        started_at: Instant::now(),
        started_unix_ms: unix_ms(),
        flushed: false,
    });
    drop(guard);
    arm_atexit();
    announce(&format!(
        "perf: recording {}",
        dir.join("trace.pftrace").display()
    ));
    Ok(Some(dir))
}

pub fn flush() -> Result<Option<PathBuf>, String> {
    let mut guard = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(state) = guard.as_mut() else {
        return Ok(None);
    };
    if state.flushed {
        return Ok(Some(state.dir.clone()));
    }
    state.flushed = true;
    state.session.flush_blocking(Duration::from_secs(5));
    state.session.stop_blocking();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&bytes);
    state.session.read_trace_blocking(move |chunk, _has_more| {
        sink.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extend_from_slice(chunk);
    });
    let bytes = Arc::try_unwrap(bytes)
        .map_err(|_| "trace sink still shared".to_owned())?
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());
    let trace_path = state.dir.join("trace.pftrace");
    std::fs::write(&trace_path, &bytes)
        .map_err(|error| format!("write {}: {error}", trace_path.display()))?;
    write_manifest(state)?;
    announce(&format!(
        "perf: wrote {} ({} bytes)",
        trace_path.display(),
        bytes.len()
    ));
    Ok(Some(state.dir.clone()))
}

fn write_manifest(state: &SessionState) -> Result<(), String> {
    let (git, git_error) = match &state.git {
        Ok(revision) => (Some(revision.as_str()), None),
        Err(error) => (None, Some(error.as_str())),
    };
    let manifest = serde_json::json!({
        "format": "iw4l-perf-1",
        "zone": state.metadata.zone,
        "role": state.metadata.role,
        "focus": state.metadata.focus,
        "git": git,
        "git_error": git_error,
        "command_line": state.command_line,
        "start_unix_ms": state.started_unix_ms,
        "stop_unix_ms": unix_ms(),
        "elapsed_ms": state.started_at.elapsed().as_secs_f64() * 1000.0,
        "buffer_size_kb": TRACE_BUFFER_KB,
    });
    let path = state.dir.join("manifest.json");
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("encode manifest: {error}"))?;
    std::fs::write(&path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn trace_config() -> Vec<u8> {
    let writer = PbMsgWriter::new();
    let hb = HeapBuffer::new(writer.stream_writer());
    let mut msg = PbMsg::new(&writer).expect("TraceConfig PbMsg");
    {
        let mut cfg = TraceConfig { msg: &mut msg };
        cfg.set_buffers(|buffer: &mut TraceConfigBufferConfig| {
            buffer.set_size_kb(TRACE_BUFFER_KB);
        });
        cfg.set_data_sources(|sources: &mut TraceConfigDataSource| {
            sources.set_config(|source: &mut DataSourceConfig| {
                source.set_name("track_event");
                source.set_track_event_config(|events: &mut TrackEventConfig| {
                    events.set_enabled_categories("iw4l.*");
                });
            });
        });
    }
    msg.finalize();
    let mut bytes = vec![0; writer.stream_writer().get_written_size()];
    hb.copy_into(&mut bytes);
    bytes
}

fn git_revision() -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("git rev-parse: {error}"))?;
    if !output.status.success() {
        return Err(format!("git rev-parse exited {}", output.status));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| format!("git revision utf8: {error}"))
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

fn announce(line: &str) {
    let _ = writeln!(std::io::stdout(), "{line}");
    let _ = std::io::stdout().flush();
}

fn arm_atexit() {
    if ATEXIT_ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    diag::exit::at_exit(|| {
        let _ = flush();
    });
}
