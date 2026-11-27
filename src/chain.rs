//! The module chain: every processing module, in the order the user chose.
//!
//! Each slot owns one module, an on switch and a position. The audio runs through the
//! switched-on slots in order of position (ties go to the earlier slot, so any combination
//! of position values is a valid order). Slots are only ever appended, so the index of a
//! host parameter never changes between versions.

use olm_api::{Kind, Module, ParamDesc};
use openplugin::prelude::*;

pub struct Slot {
    /// Prefix of the slot's parameter ids. Empty for the first slot, whose ids stay bare.
    pub key: &'static str,
    pub label: &'static str,
    module: Box<dyn Module>,
    /// Index of the slot's first host parameter, and how many the module has.
    pub base: usize,
    pub count: usize,
    on: bool,
    position: f32,
}

impl Slot {
    pub fn on_index(&self) -> usize {
        self.base + self.count
    }

    pub fn position_index(&self) -> usize {
        self.base + self.count + 1
    }
}

pub struct Chain {
    slots: Vec<Slot>,
    /// Slot indices in processing order.
    order: Vec<usize>,
    /// The right channel of a mono stream.
    scratch: Vec<f32>,
    max_block: usize,
    /// The sidechain key, kept long enough that each module reads it delayed by the
    /// latency of the modules before it, so key and main stay aligned along the chain.
    key_ring: [Vec<f32>; 2],
    key_pos: usize,
    /// The delayed key handed to one module.
    key_buf: [Vec<f32>; 2],
    /// The slot the editor watches: its input and output of the last block, and its read-out.
    watch: Option<usize>,
    tap: [Vec<f32>; 4],
    tap_len: usize,
    view: Vec<f32>,
    view_len: usize,
}

/// Key history in samples, a power of two: covers 1.3 s of chain latency at 48 kHz.
pub const KEY_RING: usize = 1 << 16;

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The host group for a module: '/' separates nested groups, so "Compressor / De-esser" becomes "Compressor & De-esser".
fn host_group(label: &'static str) -> &'static str {
    if label.contains('/') { leak(label.replace(" / ", " & ").replace('/', "&")) } else { label }
}

/// The module's name at the start of its host parameter names. Pro Tools cuts a name at 31
/// characters, so the two modules whose names would then be the same get a shorter one
/// ("Compressor / De-esser Auto Release Time 1" and "... 2" both ended in "Auto Rele").
/// The editor shows `label` itself; ids do not change.
fn host_prefix(label: &'static str) -> &'static str {
    match label {
        "Compressor / De-esser" => "Comp De-ess",
        "Harmonic Compressor" => "Harmonic Comp",
        other => other,
    }
}

pub(crate) fn info_for(key: &'static str, label: &'static str, d: &ParamDesc) -> ParamInfo {
    let unit = match d.kind {
        Kind::Db => Unit::Db,
        Kind::Ms => Unit::Ms,
        Kind::Hz => Unit::Hz,
        Kind::Percent => Unit::Percent,
        Kind::Ratio => Unit::Ratio,
        Kind::Plain => Unit::None,
        Kind::Toggle => Unit::Toggle,
        Kind::Choice(_) => Unit::Choice,
    };
    let (id, name) = if key.is_empty() { (d.id, d.name) } else { (leak(format!("{key}.{}", d.id)), leak(format!("{} {}", host_prefix(label), d.name))) };
    let mut info = ParamInfo::new(id, name, d.min, d.max, d.default, unit).with_steps(d.steps);
    if d.log {
        info = info.with_taper(Taper::Log);
    }
    if let Kind::Choice(labels) = d.kind {
        info = info.with_choices(labels);
    }
    if !key.is_empty() {
        info = info.with_group(host_group(label));
    }
    info
}

impl Chain {
    /// `modules` is `(key, label, module)` per slot, in the order of their host parameters.
    pub fn new(modules: Vec<(&'static str, &'static str, Box<dyn Module>)>) -> Self {
        let mut base = 0;
        let slots: Vec<Slot> = modules
            .into_iter()
            .enumerate()
            .map(|(i, (key, label, module))| {
                let count = module.params().len();
                let slot = Slot { key, label, module, base, count, on: i == 0, position: (i + 1) as f32 };
                base += count + 2;
                slot
            })
            .collect();
        let order = (0..slots.len()).collect();
        Self { slots, order, scratch: Vec::new(), max_block: 0, key_ring: [Vec::new(), Vec::new()], key_pos: 0, key_buf: [Vec::new(), Vec::new()], watch: None, tap: Default::default(), tap_len: 0, view: Vec::new(), view_len: 0 }
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Every host parameter: per slot the module's own, then its on switch and its position.
    pub fn param_infos(&self) -> Vec<ParamInfo> {
        let n = self.slots.len();
        let mut out = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            out.extend(slot.module.params().iter().map(|d| info_for(slot.key, slot.label, d)));
            let key = if slot.key.is_empty() { "chain0" } else { slot.key };
            out.push(ParamInfo::new(leak(format!("{key}.on")), leak(format!("{} On", host_prefix(slot.label))), 0.0, 1.0, if i == 0 { 1.0 } else { 0.0 }, Unit::Toggle).with_group(host_group(slot.label)));
            out.push(ParamInfo::new(leak(format!("{key}.position")), leak(format!("{} Position", host_prefix(slot.label))), 1.0, n.max(2) as f32, (i + 1) as f32, Unit::None).with_steps(n.max(2) as u32).with_group(host_group(slot.label)));
        }
        out
    }

    /// Sets host parameter `index` to a plain, already clamped value. No allocation.
    pub fn set(&mut self, index: usize, value: f32) -> bool {
        let Some(i) = self.slots.iter().position(|s| index >= s.base && index < s.base + s.count + 2) else { return false };
        let slot = &mut self.slots[i];
        match index - slot.base {
            local if local < slot.count => slot.module.set_param(local, value),
            local if local == slot.count => {
                let on = value >= 0.5;
                if on && !slot.on {
                    // A module that was out of the path starts from silence, not from old audio.
                    slot.module.reset();
                }
                slot.on = on;
            }
            _ => {
                slot.position = value;
                let slots = &self.slots;
                self.order.sort_unstable_by(|&a, &b| slots[a].position.total_cmp(&slots[b].position).then(a.cmp(&b)));
            }
        }
        true
    }

    pub fn prepare(&mut self, sample_rate: f32, max_block: usize) {
        self.max_block = max_block.max(1);
        self.scratch = vec![0.0; self.max_block];
        self.key_ring = [vec![0.0; KEY_RING], vec![0.0; KEY_RING]];
        self.key_buf = [vec![0.0; self.max_block], vec![0.0; self.max_block]];
        self.key_pos = 0;
        self.tap = std::array::from_fn(|_| vec![0.0; self.max_block]);
        self.view = vec![0.0; crate::shared::VIEW_LEN];
        for slot in &mut self.slots {
            slot.module.prepare(sample_rate, self.max_block);
        }
    }

    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.module.reset();
        }
    }

    /// Which slot's audio and read-out to capture (`None`: none). Out of range: none.
    pub fn set_watch(&mut self, slot: Option<usize>) {
        self.watch = slot.filter(|&s| s < self.slots.len());
    }

    /// The watched slot's input and output of the last processed sub-block
    /// (pre L, pre R, post L, post R), empty when it did not run.
    pub fn taps(&self) -> [&[f32]; 4] {
        let n = self.tap_len;
        [&self.tap[0][..n], &self.tap[1][..n], &self.tap[2][..n], &self.tap[3][..n]]
    }

    /// The watched slot's read-out after the last block.
    pub fn view(&self) -> &[f32] {
        &self.view[..self.view_len]
    }

    /// One slot's meters and latency.
    pub fn slot_meters(&self, slot: usize) -> Option<(olm_api::Meters, usize, bool)> {
        self.slots.get(slot).map(|s| (s.module.meters(), s.module.latency(), s.on))
    }

    /// Tells every module whether the host renders offline.
    pub fn set_offline(&mut self, offline: bool) {
        for slot in &mut self.slots {
            slot.module.set_offline(offline);
        }
    }

    /// One slot's added gain in the last block, dB (0 when it is off).
    pub fn slot_lift(&self, slot: usize) -> f32 {
        self.slots.get(slot).filter(|s| s.on).map_or(0.0, |s| s.module.lift_db().max(0.0))
    }

    /// The deepest reduction of the last block: the switched-on modules' reductions added, in dB.
    pub fn reduction_db(&self) -> f32 {
        self.slots.iter().filter(|s| s.on).map(|s| s.module.meters().reduction_db.max(0.0)).sum()
    }

    /// The latencies of the switched-on modules add up.
    pub fn latency(&self) -> usize {
        self.slots.iter().filter(|s| s.on).map(|s| s.module.latency()).sum()
    }

    /// Processes in place. `right` is `None` for a mono stream.
    #[cfg(test)]
    pub fn process(&mut self, left: &mut [f32], right: Option<&mut [f32]>) {
        self.process_keyed(left, right, None);
    }

    /// Processes in place with an optional sidechain key (left, right; equal lengths, at
    /// least `left.len()`). Without a key, modules fall back to their own input.
    pub fn process_keyed(&mut self, left: &mut [f32], mut right: Option<&mut [f32]>, key: Option<[&[f32]; 2]>) {
        if self.max_block == 0 {
            return;
        }
        let mask = KEY_RING - 1;
        let key = key.filter(|k| k[0].len() >= left.len() && k[1].len() >= left.len() && self.key_ring[0].len() == KEY_RING);
        let mut start = 0;
        while start < left.len() {
            let end = (start + self.max_block).min(left.len());
            let n = end - start;
            let l = &mut left[start..end];
            let r: &mut [f32] = match right.as_deref_mut() {
                Some(r) => &mut r[start..end],
                None => {
                    let r = &mut self.scratch[..n];
                    r.copy_from_slice(l);
                    r
                }
            };
            if let Some(k) = key {
                for (ring, key) in self.key_ring.iter_mut().zip(k) {
                    for (j, &v) in key[start..start + n].iter().enumerate() {
                        ring[(self.key_pos + j) & mask] = v;
                    }
                }
            }
            let mut delay = 0usize;
            self.tap_len = 0;
            for &i in &self.order {
                let slot = &mut self.slots[i];
                if !slot.on {
                    continue;
                }
                let watched = self.watch == Some(i) && self.tap[0].len() >= n;
                if watched {
                    self.tap[0][..n].copy_from_slice(l);
                    self.tap[1][..n].copy_from_slice(r);
                }
                if key.is_some() {
                    let d = delay.min(mask);
                    for c in 0..2 {
                        let ring = &self.key_ring[c];
                        let buf = &mut self.key_buf[c][..n];
                        for (j, out) in buf.iter_mut().enumerate() {
                            *out = ring[(self.key_pos + j + KEY_RING - d) & mask];
                        }
                    }
                    let [kl, kr] = &self.key_buf;
                    slot.module.process_keyed(l, r, &kl[..n], &kr[..n]);
                } else {
                    slot.module.process(l, r);
                }
                delay += slot.module.latency();
                if watched {
                    self.tap[2][..n].copy_from_slice(l);
                    self.tap[3][..n].copy_from_slice(r);
                    self.tap_len = n;
                    self.view_len = slot.module.view(&mut self.view);
                }
            }
            if key.is_some() {
                self.key_pos = (self.key_pos + n) & mask;
            }
            start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Multiplies by a gain, then adds an offset: order-sensitive, so the chain order shows.
    struct Affine {
        gain: f32,
        offset: f32,
        delay: usize,
    }

    static AFFINE: [ParamDesc; 1] = [ParamDesc::new("gain", "Gain", 0.0, 10.0, 1.0, Kind::Plain)];

    impl Module for Affine {
        fn params(&self) -> &'static [ParamDesc] {
            &AFFINE
        }
        fn prepare(&mut self, _: f32, _: usize) {}
        fn reset(&mut self) {}
        fn set_param(&mut self, index: usize, value: f32) {
            if index == 0 {
                self.gain = value;
            }
        }
        fn latency(&self) -> usize {
            self.delay
        }
        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            for s in left.iter_mut().chain(right) {
                *s = *s * self.gain + self.offset;
            }
        }
    }

    fn chain() -> Chain {
        let mut c = Chain::new(vec![
            ("", "First", Box::new(Affine { gain: 2.0, offset: 0.0, delay: 5 })),
            ("b", "Second", Box::new(Affine { gain: 1.0, offset: 1.0, delay: 7 })),
        ]);
        c.prepare(48_000.0, 4);
        c
    }

    fn run(c: &mut Chain) -> f32 {
        let (mut l, mut r) = ([1.0f32; 9], [1.0f32; 9]);
        c.process(&mut l, Some(&mut r));
        assert_eq!(l, r);
        l[8]
    }

    /// Delays both channels by `delay` samples and reports it as latency.
    struct Delay {
        delay: usize,
        line: Vec<f32>,
        pos: usize,
    }

    impl Module for Delay {
        fn params(&self) -> &'static [ParamDesc] {
            &AFFINE
        }
        fn prepare(&mut self, _: f32, _: usize) {
            self.line = vec![0.0; 2 * self.delay.max(1)];
        }
        fn reset(&mut self) {}
        fn set_param(&mut self, _: usize, _: f32) {}
        fn latency(&self) -> usize {
            self.delay
        }
        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            let len = self.line.len() / 2;
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                let (dl, dr) = (self.line[2 * self.pos], self.line[2 * self.pos + 1]);
                self.line[2 * self.pos] = *l;
                self.line[2 * self.pos + 1] = *r;
                self.pos = (self.pos + 1) % len;
                *l = dl;
                *r = dr;
            }
        }
    }

    /// Puts the key into the right channel, so a test can compare key and main.
    struct KeyToRight;

    impl Module for KeyToRight {
        fn params(&self) -> &'static [ParamDesc] {
            &AFFINE
        }
        fn prepare(&mut self, _: f32, _: usize) {}
        fn reset(&mut self) {}
        fn set_param(&mut self, _: usize, _: f32) {}
        fn latency(&self) -> usize {
            0
        }
        fn process(&mut self, _: &mut [f32], right: &mut [f32]) {
            right.fill(-1.0);
        }
        fn process_keyed(&mut self, _: &mut [f32], right: &mut [f32], key_left: &[f32], _: &[f32]) {
            right.copy_from_slice(key_left);
        }
    }

    #[test]
    fn the_key_reaches_each_module_aligned_with_the_main_signal() {
        let mut c = Chain::new(vec![
            ("", "Delay", Box::new(Delay { delay: 5, line: Vec::new(), pos: 0 })),
            ("k", "Key", Box::new(KeyToRight)),
            ("d", "Delay 2", Box::new(Delay { delay: 9, line: Vec::new(), pos: 0 })),
        ]);
        let key_on = c.slots()[1].on_index();
        let order = [(0, 0.0), (2, 1.0), (1, 2.0)];
        assert!(c.set(key_on, 1.0));
        let delay2_on = c.slots()[2].on_index();
        assert!(c.set(delay2_on, 1.0));
        for (slot, place) in order {
            let i = c.slots()[slot].position_index();
            assert!(c.set(i, place));
        }
        c.prepare(48_000.0, 4);
        // Order: delay 5, delay 9, key. The key must arrive 14 samples late, like the main signal.
        let ramp: Vec<f32> = (0..37).map(|i| i as f32 + 1.0).collect();
        let (mut l, mut r) = (ramp.clone(), ramp.clone());
        c.process_keyed(&mut l, Some(&mut r), Some([&ramp, &ramp]));
        assert_eq!(l, r, "key and main differ");
        assert_eq!(l[14], 1.0);
        // Without a key the module falls back to its own input.
        let (mut l, mut r) = (ramp.clone(), ramp.clone());
        c.process(&mut l, Some(&mut r));
        assert!(r.iter().all(|&x| x == -1.0));
    }

    #[test]
    fn parameters_are_laid_out_per_slot_and_named() {
        let c = chain();
        let p = c.param_infos();
        let ids: Vec<&str> = p.iter().map(|p| p.id).collect();
        assert_eq!(ids, ["gain", "chain0.on", "chain0.position", "b.gain", "b.on", "b.position"]);
        assert_eq!(p[3].name, "Second Gain");
        assert_eq!((c.slots()[1].base, c.slots()[1].on_index(), c.slots()[1].position_index()), (3, 4, 5));
    }

    #[test]
    fn only_switched_on_modules_run_and_count_towards_latency() {
        let mut c = chain();
        assert_eq!((run(&mut c), c.latency()), (2.0, 5));
        assert!(c.set(4, 1.0));
        assert_eq!((run(&mut c), c.latency()), (3.0, 12));
        assert!(c.set(1, 0.0));
        assert_eq!((run(&mut c), c.latency()), (2.0, 7));
    }

    #[test]
    fn position_decides_the_order_and_any_values_are_valid() {
        let mut c = chain();
        c.set(4, 1.0);
        assert_eq!(run(&mut c), 3.0, "x2 then +1");
        c.set(2, 2.0); // first slot to position 2: a tie, the earlier slot still leads
        assert_eq!(run(&mut c), 3.0);
        c.set(5, 1.0); // second slot to position 1
        assert_eq!(run(&mut c), 4.0, "+1 then x2");
        assert!(!c.set(6, 0.0), "no such parameter");
    }

    #[test]
    fn a_mono_stream_runs_and_long_blocks_are_split() {
        let mut c = chain();
        let mut l = [1.0f32; 11];
        c.process(&mut l, None);
        assert_eq!(l, [2.0; 11]);
    }
}
