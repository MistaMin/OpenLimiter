//! Fixed-capacity running-window helpers. Everything is allocated once in `with_window`;
//! `push` never allocates, locks or panics.

/// Maximum of the last `window` pushed values (monotonic queue in a ring).
pub struct SlidingMax<T> {
    val: Vec<T>,
    stamp: Vec<u32>,
    mask: usize,
    head: usize,
    len: usize,
    window: u32,
    now: u32,
    zero: T,
}

impl<T: Copy + PartialOrd> SlidingMax<T> {
    pub fn empty(zero: T) -> Self {
        Self { val: Vec::new(), stamp: Vec::new(), mask: 0, head: 0, len: 0, window: 1, now: 0, zero }
    }

    pub fn with_window(window: usize, zero: T) -> Self {
        let window = window.max(1);
        let cap = (window + 1).next_power_of_two();
        Self {
            val: vec![zero; cap],
            stamp: vec![0; cap],
            mask: cap - 1,
            head: 0,
            len: 0,
            window: window as u32,
            now: 0,
            zero,
        }
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
        self.now = 0;
    }

    #[inline]
    pub fn push(&mut self, v: T) -> T {
        if self.val.is_empty() {
            return v;
        }
        // Drop everything at the back that the new value dominates.
        while self.len > 0 {
            let b = (self.head + self.len - 1) & self.mask;
            if self.val[b] <= v {
                self.len -= 1;
            } else {
                break;
            }
        }
        if self.len > self.mask {
            // Cannot happen while capacity > window; kept so the ring can never overrun.
            self.head = (self.head + 1) & self.mask;
            self.len -= 1;
        }
        let b = (self.head + self.len) & self.mask;
        self.val[b] = v;
        self.stamp[b] = self.now;
        self.len += 1;
        // At most one entry leaves the window per step.
        if self.now.wrapping_sub(self.stamp[self.head]) >= self.window {
            self.head = (self.head + 1) & self.mask;
            self.len -= 1;
        }
        self.now = self.now.wrapping_add(1);
        if self.len == 0 {
            return self.zero;
        }
        self.val[self.head]
    }
}

/// Mean of the last `n` pushed values, on integers so the running sum never drifts and a
/// constant input gives exactly that constant back.
pub struct BoxMean {
    buf: Vec<u32>,
    pos: usize,
    sum: u64,
}

impl BoxMean {
    pub fn empty() -> Self {
        Self { buf: Vec::new(), pos: 0, sum: 0 }
    }

    pub fn with_len(n: usize) -> Self {
        Self { buf: vec![0; n.max(1)], pos: 0, sum: 0 }
    }

    pub fn clear(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0;
        }
        self.pos = 0;
        self.sum = 0;
    }

    #[inline]
    pub fn push(&mut self, v: u32) -> u32 {
        let n = self.buf.len();
        if n == 0 {
            return v;
        }
        if self.pos >= n {
            self.pos = 0;
        }
        let old = self.buf[self.pos];
        self.buf[self.pos] = v;
        self.pos += 1;
        self.sum = self.sum + v as u64 - old as u64;
        (self.sum / n as u64) as u32
    }
}
