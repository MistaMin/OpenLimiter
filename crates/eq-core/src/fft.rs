//! In-place radix-2 complex FFT. Tables are built in `new` (allocates); `forward` and `inverse`
//! never allocate. The forward transform is unscaled, the inverse is scaled by 1/N, as in the
//! FFT the original used.

use crate::cx::C32;

pub struct Fft {
    n: usize,
    twiddle: Vec<C32>,
    rev: Vec<u32>,
}

impl Fft {
    /// `order` is log2 of the size.
    pub fn new(order: u32) -> Self {
        let n = 1usize << order;
        let mut twiddle = Vec::with_capacity(n / 2);
        for k in 0..n / 2 {
            let a = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
            twiddle.push(C32::new(a.cos() as f32, a.sin() as f32));
        }
        let mut rev = vec![0u32; n];
        for (i, r) in rev.iter_mut().enumerate() {
            let mut x = i;
            let mut y = 0;
            for _ in 0..order {
                y = (y << 1) | (x & 1);
                x >>= 1;
            }
            *r = y as u32;
        }
        Fft { n, twiddle, rev }
    }

    pub fn size(&self) -> usize {
        self.n
    }

    fn run(&self, buf: &mut [C32], inverse: bool) {
        let n = self.n;
        if buf.len() < n {
            return;
        }
        let buf = &mut buf[..n];
        for i in 0..n {
            let j = self.rev[i] as usize;
            if j > i {
                buf.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            let mut start = 0;
            while start < n {
                for k in 0..half {
                    let mut w = self.twiddle[k * step];
                    if inverse {
                        w.im = -w.im;
                    }
                    let a = buf[start + k];
                    let b = buf[start + k + half].mul(w);
                    buf[start + k] = a.add(b);
                    buf[start + k + half] = a.sub(b);
                }
                start += len;
            }
            len <<= 1;
        }
        if inverse {
            let s = 1.0 / n as f32;
            for v in buf.iter_mut() {
                *v = v.scale(s);
            }
        }
    }

    pub fn forward(&self, buf: &mut [C32]) {
        self.run(buf, false);
    }

    pub fn inverse(&self, buf: &mut [C32]) {
        self.run(buf, true);
    }
}
