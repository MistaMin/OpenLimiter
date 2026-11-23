//! What the engine shows its editor: per-channel peaks and a level history.
//!
//! One writer (the audio thread) and one reader (the editor, main thread).
//! Everything is an atomic, so the audio thread never blocks or allocates.

use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};

/// History entries kept. One entry covers `1 / HIST_RATE` seconds.
pub const HIST: usize = 4096;
/// History entries per second.
pub const HIST_RATE: f32 = 200.0;
/// Loudness entries kept, and how many arrive per second.
pub const LOUD_HIST: usize = 1024;
pub const LOUD_RATE: f32 = 20.0;
/// At or below this a loudness reading means silence.
pub const LOUD_FLOOR: f32 = -70.0;

/// Loudness readings, in LUFS (LU for the range, dBTP for the true peak).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loud {
    pub momentary_in: f32,
    pub momentary_out: f32,
    pub short_in: f32,
    pub short_out: f32,
    /// Of the output, since the last reset.
    pub integrated: f32,
    /// Of the input, since the last reset.
    pub integrated_in: f32,
    pub range: f32,
    pub true_peak: f32,
}

impl Loud {
    pub const SILENT: Loud = Loud { momentary_in: -200.0, momentary_out: -200.0, short_in: -200.0, short_out: -200.0, integrated: -200.0, integrated_in: -200.0, range: 0.0, true_peak: -200.0 };
}

/// One history entry: peak in, peak out (linear), and the lowest gain (linear, 1 = no reduction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub input: f32,
    pub output: f32,
    pub gain: f32,
    /// Mean square of the output (after the dither) over the entry, left and right.
    pub ms: [f32; 2],
}

pub struct Shared {
    hist_in: Box<[AtomicU32]>,
    hist_out: Box<[AtomicU32]>,
    hist_gain: Box<[AtomicU32]>,
    hist_ms: [Box<[AtomicU32]>; 2],
    /// Entries written so far.
    head: AtomicUsize,
    in_peak: [AtomicU32; 2],
    out_peak: [AtomicU32; 2],
    latency: AtomicU32,
    /// The gain Autogain applies now, dB, as f32 bits.
    autogain_db: AtomicU32,
    sample_rate: AtomicU32,
    /// Set when a soft clipper worked since the editor last looked.
    soft_clip: AtomicU32,
    /// Sidechain key: whether the host feeds one, and its peak since the editor last looked.
    key_present: AtomicU32,
    key_peak: AtomicU32,
    loud_in: Box<[AtomicU32]>,
    loud_out: Box<[AtomicU32]>,
    loud_head: AtomicUsize,
    /// Short-term in, short-term out, integrated, range, true peak, integrated in.
    loud_now: [AtomicU32; 6],
    loud_reset: AtomicU32,
    /// Per chain slot: deepest reduction since the editor last looked (dB), and latency.
    slot_gr: Box<[AtomicU32]>,
    /// Per chain slot: largest added gain since the editor last looked (dB).
    slot_lift: Box<[AtomicU32]>,
    slot_latency: Box<[AtomicU32]>,
    /// The slot the editor shows (`u32::MAX` for none): its read-out and its taps.
    watch: AtomicU32,
    view: Box<[AtomicU32]>,
    view_len: AtomicU32,
    view_slot: AtomicU32,
    /// Per read-out value: 0 newest wins, 1 largest, 2 smallest, until the editor reads.
    view_rule: Box<[AtomicU8]>,
    /// The editor has read the read-out: the next block starts afresh.
    view_fresh: AtomicU32,
    /// Audio around the watched slot: pre L, pre R, post L, post R, key L, key R.
    taps: Box<[AtomicU32]>,
    tap_head: AtomicUsize,
    /// Every host parameter's plain value, so a new editor starts from the real state.
    values: Box<[AtomicU32]>,
}

fn zeros(len: usize) -> Box<[AtomicU32]> {
    (0..len).map(|_| AtomicU32::new(0)).collect()
}

fn raise(slot: &AtomicU32, v: f32) {
    if v > f32::from_bits(slot.load(Ordering::Relaxed)) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            hist_in: zeros(HIST),
            hist_out: zeros(HIST),
            hist_gain: zeros(HIST),
            hist_ms: [zeros(HIST), zeros(HIST)],
            head: AtomicUsize::new(0),
            in_peak: [AtomicU32::new(0), AtomicU32::new(0)],
            out_peak: [AtomicU32::new(0), AtomicU32::new(0)],
            latency: AtomicU32::new(0),
            autogain_db: AtomicU32::new(0),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            soft_clip: AtomicU32::new(0),
            key_present: AtomicU32::new(0),
            key_peak: AtomicU32::new(0),
            loud_in: zeros(LOUD_HIST),
            loud_out: zeros(LOUD_HIST),
            loud_head: AtomicUsize::new(0),
            loud_now: [Loud::SILENT.short_in, Loud::SILENT.short_out, Loud::SILENT.integrated, 0.0, Loud::SILENT.true_peak, Loud::SILENT.integrated_in].map(|v| AtomicU32::new(v.to_bits())),
            loud_reset: AtomicU32::new(0),
            slot_gr: zeros(MAX_SLOTS),
            slot_lift: zeros(MAX_SLOTS),
            slot_latency: zeros(MAX_SLOTS),
            watch: AtomicU32::new(u32::MAX),
            view: zeros(VIEW_LEN),
            view_len: AtomicU32::new(0),
            view_slot: AtomicU32::new(u32::MAX),
            view_rule: (0..VIEW_LEN).map(|_| AtomicU8::new(0)).collect(),
            view_fresh: AtomicU32::new(1),
            taps: zeros(6 * TAP_LEN),
            tap_head: AtomicUsize::new(0),
            values: crate::INFO.params.iter().map(|p| AtomicU32::new(p.default.to_bits())).collect(),
        }
    }
}

/// Chain slots the shared state has room for.
pub const MAX_SLOTS: usize = 32;
/// Values a module's read-out may publish.
pub const VIEW_LEN: usize = 1024;
/// Samples kept per tap channel, a power of two.
pub const TAP_LEN: usize = 8192;

impl Shared {
    // ---- audio thread ----

    pub fn raise_slot_gr(&self, slot: usize, db: f32) {
        if let Some(a) = self.slot_gr.get(slot) {
            raise(a, db);
        }
    }

    pub fn raise_slot_lift(&self, slot: usize, db: f32) {
        if let Some(a) = self.slot_lift.get(slot) {
            raise(a, db);
        }
    }

    /// The largest added gain of a slot since the last call, dB.
    pub fn take_slot_lift(&self, slot: usize) -> f32 {
        self.slot_lift.get(slot).map_or(0.0, |a| f32::from_bits(a.swap(0, Ordering::Relaxed)))
    }

    pub fn set_slot_latency(&self, slot: usize, samples: usize) {
        if let Some(a) = self.slot_latency.get(slot) {
            a.store(samples as u32, Ordering::Relaxed);
        }
    }

    /// The slot whose read-out and audio the editor wants, if any.
    pub fn watched(&self) -> Option<usize> {
        let w = self.watch.load(Ordering::Relaxed);
        (w != u32::MAX).then_some(w as usize)
    }

    /// Publishes a module's read-out. `values` longer than `VIEW_LEN` are cut. Between two
    /// reads by the editor, values merge by the rule the editor set for them.
    pub fn set_view(&self, slot: usize, values: &[f32]) {
        let n = values.len().min(VIEW_LEN);
        let same = self.view_slot.load(Ordering::Relaxed) == slot as u32 && self.view_len.load(Ordering::Relaxed) as usize == n;
        let fresh = !same || self.view_fresh.swap(0, Ordering::Relaxed) != 0;
        for ((a, v), rule) in self.view.iter().zip(&values[..n]).zip(self.view_rule.iter()) {
            let merged = match (fresh, rule.load(Ordering::Relaxed)) {
                (true, _) | (false, 0) => *v,
                (false, 1) => v.max(f32::from_bits(a.load(Ordering::Relaxed))),
                (false, _) => v.min(f32::from_bits(a.load(Ordering::Relaxed))),
            };
            a.store(merged.to_bits(), Ordering::Relaxed);
        }
        self.view_len.store(n as u32, Ordering::Relaxed);
        self.view_slot.store(slot as u32, Ordering::Relaxed);
    }

    /// Appends one block of audio around the watched slot. Missing key: zeros.
    pub fn push_taps(&self, pre: [&[f32]; 2], post: [&[f32]; 2], key: Option<[&[f32]; 2]>) {
        let head = self.tap_head.load(Ordering::Relaxed);
        let n = pre[0].len();
        let chans: [Option<&[f32]>; 6] = [Some(pre[0]), Some(pre[1]), Some(post[0]), Some(post[1]), key.map(|k| k[0]), key.map(|k| k[1])];
        for (c, src) in chans.iter().enumerate() {
            let base = c * TAP_LEN;
            for j in 0..n {
                let v = src.and_then(|s| s.get(j)).copied().unwrap_or(0.0);
                self.taps[base + ((head + j) & (TAP_LEN - 1))].store(v.to_bits(), Ordering::Relaxed);
            }
        }
        self.tap_head.store(head.wrapping_add(n), Ordering::Release);
    }

    pub fn push(&self, e: Entry) {
        let head = self.head.load(Ordering::Relaxed);
        let i = head % HIST;
        self.hist_in[i].store(e.input.to_bits(), Ordering::Relaxed);
        self.hist_out[i].store(e.output.to_bits(), Ordering::Relaxed);
        self.hist_gain[i].store(e.gain.to_bits(), Ordering::Relaxed);
        for c in 0..2 {
            self.hist_ms[c][i].store(e.ms[c].to_bits(), Ordering::Relaxed);
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
    }

    pub fn raise_peaks(&self, channel: usize, input: f32, output: f32) {
        raise(&self.in_peak[channel & 1], input);
        raise(&self.out_peak[channel & 1], output);
    }

    pub fn set_key_present(&self, present: bool) {
        self.key_present.store(present as u32, Ordering::Relaxed);
    }

    pub fn raise_key(&self, peak: f32) {
        raise(&self.key_peak, peak);
    }

    /// `(key bus present, key peak since the last call)`.
    #[allow(dead_code)] // read by the editor's key indicator, still to come
    pub fn take_key(&self) -> (bool, f32) {
        (self.key_present.load(Ordering::Relaxed) != 0, f32::from_bits(self.key_peak.swap(0, Ordering::Relaxed)))
    }

    pub fn mark_soft_clip(&self) {
        self.soft_clip.store(1, Ordering::Relaxed);
    }

    pub fn set_timing(&self, latency_samples: usize, sample_rate: f32) {
        self.latency.store(latency_samples as u32, Ordering::Relaxed);
        self.sample_rate.store(sample_rate.to_bits(), Ordering::Relaxed);
    }

    pub fn push_loud(&self, l: Loud) {
        let head = self.loud_head.load(Ordering::Relaxed);
        let i = head % LOUD_HIST;
        self.loud_in[i].store(l.momentary_in.to_bits(), Ordering::Relaxed);
        self.loud_out[i].store(l.momentary_out.to_bits(), Ordering::Relaxed);
        for (slot, v) in self.loud_now.iter().zip([l.short_in, l.short_out, l.integrated, l.range, l.true_peak, l.integrated_in]) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
        self.loud_head.store(head.wrapping_add(1), Ordering::Release);
    }

    /// Whether the editor asked for the loudness measurement to start again.
    pub fn take_loud_reset(&self) -> bool {
        self.loud_reset.swap(0, Ordering::Relaxed) != 0
    }

    pub fn set_value(&self, index: usize, value: f32) {
        if let Some(slot) = self.values.get(index) {
            slot.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    // ---- editor ----

    pub fn value(&self, index: usize) -> Option<f32> {
        self.values.get(index).map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
    }

    pub fn loud_head(&self) -> usize {
        self.loud_head.load(Ordering::Acquire)
    }

    /// Momentary loudness of entry `index`: `(input, output)`.
    pub fn loud_entry(&self, index: usize) -> (f32, f32) {
        let i = index % LOUD_HIST;
        (f32::from_bits(self.loud_in[i].load(Ordering::Relaxed)), f32::from_bits(self.loud_out[i].load(Ordering::Relaxed)))
    }

    /// The newest readings.
    pub fn loud(&self) -> Loud {
        let v = |i: usize| f32::from_bits(self.loud_now[i].load(Ordering::Relaxed));
        let head = self.loud_head();
        let (momentary_in, momentary_out) = if head == 0 { (Loud::SILENT.momentary_in, Loud::SILENT.momentary_out) } else { self.loud_entry(head - 1) };
        Loud { momentary_in, momentary_out, short_in: v(0), short_out: v(1), integrated: v(2), integrated_in: v(5), range: v(3), true_peak: v(4) }
    }

    pub fn request_loud_reset(&self) {
        self.loud_reset.store(1, Ordering::Relaxed);
    }

    /// Entries written so far. The editor keeps its own read position.
    pub fn head(&self) -> usize {
        self.head.load(Ordering::Acquire)
    }

    pub fn entry(&self, index: usize) -> Entry {
        let i = index % HIST;
        Entry {
            input: f32::from_bits(self.hist_in[i].load(Ordering::Relaxed)),
            output: f32::from_bits(self.hist_out[i].load(Ordering::Relaxed)),
            gain: f32::from_bits(self.hist_gain[i].load(Ordering::Relaxed)),
            ms: [0, 1].map(|c| f32::from_bits(self.hist_ms[c][i].load(Ordering::Relaxed))),
        }
    }

    /// Peaks since the last call: `([in L, in R], [out L, out R])`, linear.
    pub fn take_peaks(&self) -> ([f32; 2], [f32; 2]) {
        let take = |s: &AtomicU32| f32::from_bits(s.swap(0, Ordering::Relaxed));
        ([take(&self.in_peak[0]), take(&self.in_peak[1])], [take(&self.out_peak[0]), take(&self.out_peak[1])])
    }

    // ---- editor: chain slots ----

    /// Asks the audio thread for one slot's read-out and taps (`None` stops them).
    pub fn watch(&self, slot: Option<usize>) {
        self.watch.store(slot.map_or(u32::MAX, |s| s as u32), Ordering::Relaxed);
    }

    /// `(deepest reduction since the last call, latency in samples)` of a slot.
    pub fn take_slot(&self, slot: usize) -> (f32, u32) {
        let gr = self.slot_gr.get(slot).map_or(0.0, |a| f32::from_bits(a.swap(0, Ordering::Relaxed)));
        (gr, self.slot_latency.get(slot).map_or(0, |a| a.load(Ordering::Relaxed)))
    }

    /// How each read-out value merges between reads: 0 newest, 1 largest, 2 smallest.
    pub fn set_view_rules(&self, rules: &[u8]) {
        for (i, a) in self.view_rule.iter().enumerate() {
            a.store(rules.get(i).copied().unwrap_or(0), Ordering::Relaxed);
        }
    }

    /// Copies the latest read-out into `out` if it belongs to `slot`; returns how many values.
    /// The audio thread then starts merging afresh.
    pub fn view(&self, slot: usize, out: &mut [f32]) -> usize {
        if self.view_slot.load(Ordering::Relaxed) != slot as u32 {
            return 0;
        }
        let n = (self.view_len.load(Ordering::Relaxed) as usize).min(out.len());
        for (o, a) in out[..n].iter_mut().zip(self.view.iter()) {
            *o = f32::from_bits(a.load(Ordering::Relaxed));
        }
        self.view_fresh.store(1, Ordering::Relaxed);
        n
    }

    /// The newest `out.len()` samples (at most `TAP_LEN`) of tap channel `channel`
    /// (0 pre L, 1 pre R, 2 post L, 3 post R, 4 key L, 5 key R), oldest first.
    pub fn read_tap(&self, channel: usize, out: &mut [f32]) {
        let head = self.tap_head.load(Ordering::Acquire);
        let n = out.len().min(TAP_LEN);
        let base = (channel % 6) * TAP_LEN;
        for (j, o) in out[..n].iter_mut().enumerate() {
            let i = head.wrapping_sub(n).wrapping_add(j) & (TAP_LEN - 1);
            *o = f32::from_bits(self.taps[base + i].load(Ordering::Relaxed));
        }
    }

    /// Whether a soft clipper worked since the last call.
    pub fn take_soft_clip(&self) -> bool {
        self.soft_clip.swap(0, Ordering::Relaxed) != 0
    }

    /// `(latency in samples, sample rate)`.
    pub fn set_autogain_db(&self, db: f32) {
        self.autogain_db.store(db.to_bits(), Ordering::Relaxed);
    }

    pub fn autogain_db(&self) -> f32 {
        f32::from_bits(self.autogain_db.load(Ordering::Relaxed))
    }

    pub fn timing(&self) -> (u32, f32) {
        (self.latency.load(Ordering::Relaxed), f32::from_bits(self.sample_rate.load(Ordering::Relaxed)))
    }
}
