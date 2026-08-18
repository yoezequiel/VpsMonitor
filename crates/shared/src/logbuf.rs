//! A bounded, in-process ring buffer of recent log lines. A `tracing`
//! `Layer` (installed in `monitor-bin::main`, the only crate that depends
//! on `tracing-subscriber`) pushes into it on every event; the Agent
//! reporting loop and the Monitor's self-monitor loop drain it each cycle
//! and ship the entries onward (see `LogSource::Agent` / `LogSource::Monitor`).
//! Living here, rather than in `monitor-core` or `agent-core`, lets both
//! depend on it without depending on each other.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};

use crate::LogLevel;

/// Caps memory use and bounds how much history a freshly (re)connected
/// Agent can flood the Monitor with — old lines are simply dropped rather
/// than blocking the process that's producing them.
const CAPACITY: usize = 500;

pub struct BufferedLogEntry {
    pub ts: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

fn buffer() -> &'static Mutex<VecDeque<BufferedLogEntry>> {
    static BUFFER: OnceLock<Mutex<VecDeque<BufferedLogEntry>>> = OnceLock::new();
    BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

pub fn push(level: LogLevel, message: String) {
    let mut buf = buffer().lock().unwrap_or_else(|e| e.into_inner());
    if buf.len() >= CAPACITY {
        buf.pop_front();
    }
    buf.push_back(BufferedLogEntry {
        ts: Utc::now(),
        level,
        message,
    });
}

/// Takes everything currently buffered. Each call only returns entries
/// pushed since the previous drain, so the same line is never shipped
/// twice.
pub fn drain() -> Vec<BufferedLogEntry> {
    let mut buf = buffer().lock().unwrap_or_else(|e| e.into_inner());
    buf.drain(..).collect()
}
