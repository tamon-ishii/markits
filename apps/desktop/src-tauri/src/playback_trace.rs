//! Timing-only diagnostics. Never record input text, window titles, or UI names.
use std::{cell::Cell, fs::{File, OpenOptions}, io::Write, path::PathBuf,
    sync::{Mutex, OnceLock, atomic::{AtomicU64, Ordering}}, time::{Instant, SystemTime, UNIX_EPOCH}};

thread_local! { static CONTEXT: Cell<(u64, usize)> = const { Cell::new((0, 0)) }; }
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static OUTPUT: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn log_path() -> PathBuf {
    std::env::var_os("MARKITS_PLAYBACK_TIMING_LOG").map(PathBuf::from).unwrap_or_else(||
        dirs::data_local_dir().unwrap_or_else(std::env::temp_dir)
            .join("markits").join("logs").join("playback-timing.jsonl"))
}

fn write(context: (u64, usize), phase: &str, event: &str, elapsed_ms: Option<u128>, expected_ms: Option<u64>) {
    if context.0 == 0 { return; }
    let row = serde_json::json!({
        "timestamp_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "pid": std::process::id(), "run_id": context.0, "step": context.1,
        "phase": phase, "event": event, "elapsed_ms": elapsed_ms, "expected_ms": expected_ms,
    });
    eprintln!("[playback-timing] {row}");
    let output = OUTPUT.get_or_init(|| {
        let path = log_path();
        let file = (|| {
            std::fs::create_dir_all(path.parent()?) .ok()?;
            OpenOptions::new().create(true).append(true).open(&path).ok()
        })();
        eprintln!("[playback-timing] log: {}", path.display());
        Mutex::new(file)
    });
    if let Ok(mut output) = output.lock() {
        if let Some(file) = output.as_mut() { let _ = writeln!(file, "{row}"); let _ = file.flush(); }
    }
}

pub struct Run((u64, usize));
impl Run {
    pub fn start() -> Self {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 * 1000
            + SEQUENCE.fetch_add(1, Ordering::Relaxed) % 1000;
        let previous = CONTEXT.with(|cell| cell.replace((id, 0)));
        Self(previous)
    }
}
impl Drop for Run { fn drop(&mut self) { CONTEXT.with(|cell| cell.set(self.0)); } }

pub fn step(index: usize) { CONTEXT.with(|cell| cell.set((cell.get().0, index))); }

pub struct Span { context: (u64, usize), phase: &'static str, started: Instant, expected_ms: Option<u64> }
impl Span {
    pub fn new(phase: &'static str) -> Self { Self::with_expected(phase, None) }
    pub fn with_expected(phase: &'static str, expected_ms: Option<u64>) -> Self {
        let context = CONTEXT.with(Cell::get);
        write(context, phase, "begin", None, expected_ms);
        Self { context, phase, started: Instant::now(), expected_ms }
    }
}
impl Drop for Span {
    fn drop(&mut self) { write(self.context, self.phase, "end", Some(self.started.elapsed().as_millis()), self.expected_ms); }
}

pub fn measure<T>(phase: &'static str, action: impl FnOnce() -> T) -> T {
    let _span = Span::new(phase);
    action()
}
