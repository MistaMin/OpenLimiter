//! Sliding-window lower convex hull with tangent queries (Glide engine).
//!
//! Two stacks, as in sliding-window aggregation:
//!
//! * the **back** hull holds the newest points, oldest first, built by the monotone chain
//!   (amortised O(1) per push);
//! * the **front** hull holds the older points. It is built newest to oldest; every insert
//!   finds its place by binary search and records what it overwrote, so removing the oldest
//!   point (always the last one inserted into the front) is an O(1) undo.
//!
//! When the front runs empty, the raw points of the back segment move to the front (each
//! point moves once, O(log L) each). A query asks for the smallest slope from a point to
//! the left of the window to any point in it; on a lower hull that slope is unimodal along
//! the vertices, so each stack answers by binary search and the result is the smaller of the
//! two. Every array is allocated in `alloc`; nothing allocates afterwards.

#[derive(Default)]
pub struct Hull {
    xs: Vec<f64>,
    ys: Vec<f64>,
    mask: usize,
    /// Raw index of the oldest point and one past the newest.
    head: usize,
    tail: usize,
    /// Raw index where the back segment starts (front = head..split).
    split: usize,
    back: Vec<usize>,
    back_len: usize,
    front: Vec<usize>,
    front_len: usize,
    undo_pos: Vec<usize>,
    undo_val: Vec<usize>,
    undo_len: Vec<usize>,
}

#[inline]
fn cross(ox: f64, oy: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    (ax - ox) * (by - oy) - (ay - oy) * (bx - ox)
}

impl Hull {
    pub fn alloc(&mut self, cap: usize) {
        let n = (cap + 2).next_power_of_two();
        self.xs = vec![0.0; n];
        self.ys = vec![0.0; n];
        self.mask = n - 1;
        self.back = vec![0; n];
        self.front = vec![0; n];
        self.undo_pos = vec![0; n];
        self.undo_val = vec![0; n];
        self.undo_len = vec![0; n];
        self.clear();
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.split = 0;
        self.back_len = 0;
        self.front_len = 0;
    }

    pub fn len(&self) -> usize {
        self.tail - self.head
    }

    pub fn is_empty(&self) -> bool {
        self.tail == self.head
    }

    /// Index (sample time) of the oldest point.
    pub fn oldest(&self) -> Option<f64> {
        if self.is_empty() {
            None
        } else {
            self.xs.get(self.head & self.mask).copied()
        }
    }

    #[inline]
    fn pt(&self, i: usize) -> (f64, f64) {
        let j = i & self.mask;
        (self.xs[j], self.ys[j])
    }

    /// Appends a point to the right of all others. Ignored when full.
    #[inline]
    pub fn push(&mut self, x: f64, y: f64) {
        if self.xs.is_empty() || self.len() >= self.mask {
            return;
        }
        let i = self.tail;
        let j = i & self.mask;
        self.xs[j] = x;
        self.ys[j] = y;
        self.tail += 1;
        while self.back_len >= 2 {
            let (ox, oy) = self.pt(self.back[self.back_len - 2]);
            let (ax, ay) = self.pt(self.back[self.back_len - 1]);
            if cross(ox, oy, ax, ay, x, y) <= 0.0 {
                self.back_len -= 1;
            } else {
                break;
            }
        }
        self.back[self.back_len] = i;
        self.back_len += 1;
    }

    /// Inserts raw point `i`, left of every point in the front hull.
    #[inline]
    fn front_insert(&mut self, i: usize) {
        let (px, py) = self.pt(i);
        // front[0] is the rightmost vertex, front[len-1] the leftmost. Vertex j (> 0) stays
        // when (p, front[j], front[j-1]) turns left; the vertices that stay are a prefix.
        let len = self.front_len;
        let mut keep = 0usize; // largest j that stays (j = 0 always stays)
        if len > 1 {
            let (mut lo, mut hi) = (1usize, len - 1);
            // find the largest j in [1, len-1] that stays, if any
            let stays = |s: &Self, j: usize| {
                let (ax, ay) = s.pt(s.front[j]);
                let (bx, by) = s.pt(s.front[j - 1]);
                cross(px, py, ax, ay, bx, by) > 0.0
            };
            if stays(self, 1) {
                while lo < hi {
                    let mid = (lo + hi + 1) / 2;
                    if stays(self, mid) {
                        lo = mid;
                    } else {
                        hi = mid - 1;
                    }
                }
                keep = lo;
            }
        }
        let pos = if len == 0 { 0 } else { keep + 1 };
        let slot = i & self.mask;
        self.undo_pos[slot] = pos;
        self.undo_val[slot] = self.front[pos];
        self.undo_len[slot] = len;
        self.front[pos] = i;
        self.front_len = pos + 1;
    }

    /// Removes the oldest point.
    #[inline]
    pub fn pop(&mut self) {
        if self.is_empty() {
            return;
        }
        if self.head == self.split {
            // move the back segment to the front, newest first
            let mut i = self.tail;
            while i > self.split {
                i -= 1;
                self.front_insert(i);
            }
            self.split = self.tail;
            self.back_len = 0;
        }
        let slot = self.head & self.mask;
        let pos = self.undo_pos[slot];
        self.front[pos] = self.undo_val[slot];
        self.front_len = self.undo_len[slot];
        self.head += 1;
    }

    /// Smallest slope from `(px, py)` (left of every point) to any point; `+inf` when empty.
    #[inline]
    pub fn min_slope(&self, px: f64, py: f64) -> f64 {
        let mut best = f64::INFINITY;
        // back: vertices back[0..len], increasing x
        if self.back_len > 0 {
            let n = self.back_len;
            let v = |k: usize| self.pt(self.back[k]);
            best = best.min(Self::search(px, py, n, v));
        }
        if self.front_len > 0 {
            let n = self.front_len;
            let v = |k: usize| self.pt(self.front[n - 1 - k]);
            best = best.min(Self::search(px, py, n, v));
        }
        best
    }

    #[inline]
    fn search<F: Fn(usize) -> (f64, f64)>(px: f64, py: f64, n: usize, v: F) -> f64 {
        // first k with slope(P, v_{k+1}) >= slope(P, v_k), i.e. cross(P, v_k, v_{k+1}) >= 0
        let (mut lo, mut hi) = (0usize, n - 1);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (ax, ay) = v(mid);
            let (bx, by) = v(mid + 1);
            if cross(px, py, ax, ay, bx, by) >= 0.0 {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let (x, y) = v(lo);
        let dx = x - px;
        if dx > 0.0 {
            (y - py) / dx
        } else {
            f64::NEG_INFINITY
        }
    }
}
