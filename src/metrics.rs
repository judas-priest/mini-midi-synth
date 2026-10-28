//! Health counters for the audio path.
//!
//! Everything here is a plain atomic that real-time code can bump without
//! allocating or blocking; the GUI reads them once a frame.
//!
//! These are process-global on purpose. They are diagnostics, there is exactly
//! one audio path, and keeping them global means the counters survive an audio
//! reconnect — which is precisely when you want to know what was lost.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub struct Metrics {
    /// MIDI events dropped because the ring was full or the producer was busy.
    pub midi_dropped: AtomicU64,
    /// Control events the GUI could not enqueue.
    pub ctrl_dropped: AtomicU64,
    /// Audio callbacks that reported an error (xrun, device error).
    pub stream_errors: AtomicU32,
    /// Longest callback duration seen since the last read, in microseconds.
    worst_callback_us: AtomicU32,
}

/// The one instance. See the module note on why this is global.
pub static METRICS: Metrics = Metrics::new();

impl Metrics {
    const fn new() -> Self {
        Self {
            midi_dropped: AtomicU64::new(0),
            ctrl_dropped: AtomicU64::new(0),
            stream_errors: AtomicU32::new(0),
            worst_callback_us: AtomicU32::new(0),
        }
    }

    #[inline]
    pub fn note_midi_drop(&self) {
        self.midi_dropped.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn note_ctrl_drop(&self) {
        self.ctrl_dropped.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn note_stream_error(&self) {
        self.stream_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Records how long one audio callback took. Called from the callback, so
    /// it must stay a single atomic operation.
    #[inline]
    pub fn note_callback_time(&self, micros: u32) {
        self.worst_callback_us.fetch_max(micros, Ordering::Relaxed);
    }

    /// Reads and resets the peak, so the GUI shows a rolling worst case rather
    /// than the worst case since launch — which is always the first callback,
    /// while the process is still warming up.
    pub fn take_worst_callback_us(&self) -> u32 {
        self.worst_callback_us.swap(0, Ordering::Relaxed)
    }

    /// Events that never reached the engine, from either direction.
    pub fn total_dropped(&self) -> u64 {
        self.midi_dropped.load(Ordering::Relaxed) + self.ctrl_dropped.load(Ordering::Relaxed)
    }

    pub fn stream_error_count(&self) -> u32 {
        self.stream_errors.load(Ordering::Relaxed)
    }
}

/// A control-event producer that counts what it fails to enqueue.
///
/// Call sites use `let _ = ctrl_tx.push(event)` in dozens of places; wrapping
/// the producer rather than every call site means a full ring can never again
/// go unnoticed.
pub struct CountingProducer<T> {
    inner: rtrb::Producer<T>,
}

impl<T> CountingProducer<T> {
    pub fn new(inner: rtrb::Producer<T>) -> Self {
        Self { inner }
    }

    /// Enqueues an event, counting the drop if the ring is full. Returns the
    /// same `Result` as the underlying producer so existing call sites compile
    /// unchanged.
    #[inline]
    pub fn push(&mut self, value: T) -> Result<(), rtrb::PushError<T>> {
        let result = self.inner.push(value);
        if result.is_err() {
            METRICS.note_ctrl_drop();
        }
        result
    }
}
