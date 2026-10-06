use std::io::{BufWriter, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::{Duration, Instant};

static SINK: OnceLock<Option<Sink>> = OnceLock::new();
static DROPPED: AtomicU64 = AtomicU64::new(0);

struct Sink {
    tx: SyncSender<Message>,
    started: Instant,
}

enum Message {
    Line(f64, String),
    Flush(SyncSender<()>),
}

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("IW4L_AUDIO_DIAG").is_ok_and(|value| value == "1"))
}

pub fn emit(message: String) {
    if !enabled() {
        return;
    }
    let sink = SINK.get_or_init(|| {
        let file = std::env::var_os("IW4L_AUDIO_DIAG_PATH")
            .and_then(|path| std::fs::File::create(path).ok());
        let (tx, rx) = sync_channel::<Message>(8192);
        let worker = std::thread::Builder::new()
            .name("audio-diagnostics".into())
            .spawn(move || {
                let mut output = file.map(BufWriter::new);
                loop {
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(Message::Line(ms, message)) => {
                            if let Some(output) = &mut output {
                                let _ = writeln!(output, "diag_ms={ms:.3} {message}");
                            } else {
                                diag::info!(Audio, "diag_ms={ms:.3} {message}");
                            }
                        }
                        Ok(Message::Flush(ack)) => {
                            let dropped = DROPPED.load(Ordering::Relaxed);
                            if let Some(output) = &mut output {
                                let _ = writeln!(output, "audio diag: dropped={dropped}");
                                let _ = output.flush();
                            } else {
                                diag::info!(Audio, "audio diag: dropped={dropped}");
                            }
                            let _ = ack.send(());
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if let Some(output) = &mut output {
                                let _ = output.flush();
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        if worker.is_err() {
            return None;
        }
        diag::exit::at_exit(flush);
        Some(Sink {
            tx,
            started: Instant::now(),
        })
    });
    if sink.as_ref().is_none_or(|sink| {
        sink.tx
            .try_send(Message::Line(
                sink.started.elapsed().as_secs_f64() * 1000.0,
                message,
            ))
            .is_err()
    }) {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

fn flush() {
    if let Some(Some(sink)) = SINK.get() {
        let (tx, rx) = sync_channel(1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut message = Message::Flush(tx);
        loop {
            match sink.tx.try_send(message) {
                Ok(()) => {
                    let _ = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
                    break;
                }
                Err(std::sync::mpsc::TrySendError::Full(returned)) if Instant::now() < deadline => {
                    message = returned;
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => break,
            }
        }
    }
}

pub(crate) fn slow_stage(stage: &str, started: Instant) -> Instant {
    let now = Instant::now();
    if enabled() && now.duration_since(started) >= Duration::from_millis(10) {
        emit(format!(
            "audio diag: slow_control_stage stage={stage} wall_ms={:.3}",
            now.duration_since(started).as_secs_f64() * 1000.0
        ));
    }
    now
}

pub(crate) fn thread(name: &str) {
    if !enabled() {
        return;
    }
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/thread-self/status") {
        let affinity = status
            .lines()
            .find(|line| line.starts_with("Cpus_allowed_list:"))
            .unwrap_or("unknown");
        emit(format!("audio diag: thread={name} {affinity}"));
    }
}
