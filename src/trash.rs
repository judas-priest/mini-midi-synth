//! Deferred deallocation for the audio thread.
//!
//! The audio callback must not call free(). Anything heap-allocated that
//! reaches the engine — a boxed patch, a soundfont Arc, a drum kit — is handed
//! back through this SPSC queue and dropped by the GUI thread.
//!
//! If the queue is full the object is dropped on the spot: a rare glitch beats
//! an unbounded leak, and a full queue means the GUI has stalled for seconds.

use rtrb::{Consumer, Producer, RingBuffer};

use crate::synth::ControlEvent;

/// Heap-owning values that must not be freed on the audio thread.
///
/// The variants differ a lot in size — a spent `ControlEvent` is 32 bytes, a
/// displaced rustysynth instance is 616 — so every slot in the queue costs the
/// larger figure. Boxing the big variant is not an option: `Box::new` on the
/// audio thread is exactly the allocation this queue exists to avoid. At the
/// capacity below the whole queue is about 20 KB, which is cheaper than the
/// machinery needed to avoid it.
#[allow(clippy::large_enum_variant)]
pub enum Trash {
    /// A control event the engine has finished with. Still owns its Box.
    Control(ControlEvent),
    /// A soundfont and rustysynth instance the sampler just replaced. The
    /// soundfont alone can be tens of megabytes.
    Sf2(crate::synth::sampler::Displaced),
    /// The MIDI file and track list the sequencer just replaced.
    MidiSeq {
        data: Option<Box<crate::synth::midi_player::MidiSeqData>>,
        tracks: Vec<crate::synth::midi_player::TrackState>,
    },
}

pub struct TrashTx(Producer<Trash>);
pub struct TrashRx(Consumer<Trash>);

/// Sized for a human turning a knob, not for a stream: the GUI drains this
/// every frame, and patch or soundfont changes arrive at most a few per second.
const TRASH_CAPACITY: usize = 32;

pub fn channel() -> (TrashTx, TrashRx) {
    let (tx, rx) = RingBuffer::new(TRASH_CAPACITY);
    (TrashTx(tx), TrashRx(rx))
}

impl TrashTx {
    /// Hand a value over for deferred dropping. Real-time safe: the push
    /// itself never allocates. Falls back to dropping in place if the queue
    /// is full, which would mean the GUI thread has been stalled for seconds.
    #[inline]
    pub fn dispose(&mut self, item: Trash) {
        let _ = self.0.push(item);
    }
}

impl TrashRx {
    /// Drop everything the audio thread handed over. Call from the GUI thread.
    /// Returns how many items were freed.
    pub fn drain(&mut self) -> usize {
        let mut n = 0;
        while self.0.pop().is_ok() {
            n += 1;
        }
        n
    }
}
