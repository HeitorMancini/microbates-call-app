//! Synthetic "screen" test pattern in I420: SMPTE-like color bars, a moving
//! box (motion) and a pseudo-random noise strip (forces the encoder to spend
//! bits, similar to a busy game scene).

/// BT.601 limited-range YUV for white, yellow, cyan, green, magenta, red, blue, black.
const BARS: [(u8, u8, u8); 8] = [
    (235, 128, 128),
    (210, 16, 146),
    (170, 166, 16),
    (145, 54, 34),
    (106, 202, 222),
    (81, 90, 240),
    (41, 240, 110),
    (16, 128, 128),
];

pub struct TestPattern {
    width: u32,
    height: u32,
    base_y: Vec<u8>,
    base_u: Vec<u8>,
    base_v: Vec<u8>,
}

/// Mutable views over the three I420 planes of a destination buffer.
pub struct PlanesMut<'a> {
    pub y: &'a mut [u8],
    pub stride_y: usize,
    pub u: &'a mut [u8],
    pub stride_u: usize,
    pub v: &'a mut [u8],
    pub stride_v: usize,
}

impl TestPattern {
    /// `width` and `height` must be even and non-zero.
    pub fn new(width: u32, height: u32) -> Self {
        assert!(width >= 2 && height >= 2 && width.is_multiple_of(2) && height.is_multiple_of(2));
        let (w, h) = (width as usize, height as usize);
        let (cw, ch) = (w / 2, h / 2);
        let bar = |x: usize, total: usize| BARS[(x * BARS.len() / total).min(BARS.len() - 1)];
        let mut base_y = vec![0u8; w * h];
        for row in base_y.chunks_exact_mut(w) {
            for (x, px) in row.iter_mut().enumerate() {
                *px = bar(x, w).0;
            }
        }
        let mut base_u = vec![0u8; cw * ch];
        let mut base_v = vec![0u8; cw * ch];
        for (row_u, row_v) in base_u.chunks_exact_mut(cw).zip(base_v.chunks_exact_mut(cw)) {
            for x in 0..cw {
                let (_, u, v) = bar(x, cw);
                row_u[x] = u;
                row_v[x] = v;
            }
        }
        Self {
            width,
            height,
            base_y,
            base_u,
            base_v,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Rows (luma) at the bottom filled with per-frame noise.
    fn noise_rows(&self) -> usize {
        (self.height as usize / 4) & !1
    }

    /// Position (x, y, size) of the moving box in luma pixels for `frame`.
    pub fn box_rect(&self, frame: u64) -> (usize, usize, usize) {
        let (w, h) = (self.width as usize, self.height as usize);
        let size = ((h / 6).max(2)) & !1;
        let travel = w.saturating_sub(size).max(1);
        let step = (w / 120).max(2);
        let x = ((frame as usize * step) % travel) & !1;
        let y = ((h - self.noise_rows()).saturating_sub(size) / 2) & !1;
        (x, y, size)
    }

    /// Renders frame number `frame` into `dst`.
    pub fn render(&self, frame: u64, dst: PlanesMut<'_>) {
        let (w, h) = (self.width as usize, self.height as usize);
        let (cw, ch) = (w / 2, h / 2);
        for (row, src) in self.base_y.chunks_exact(w).enumerate().take(h) {
            dst.y[row * dst.stride_y..row * dst.stride_y + w].copy_from_slice(src);
        }
        for row in 0..ch {
            let src = row * cw..row * cw + cw;
            dst.u[row * dst.stride_u..row * dst.stride_u + cw]
                .copy_from_slice(&self.base_u[src.clone()]);
            dst.v[row * dst.stride_v..row * dst.stride_v + cw].copy_from_slice(&self.base_v[src]);
        }

        let (bx, by, size) = self.box_rect(frame);
        for row in by..by + size {
            dst.y[row * dst.stride_y + bx..row * dst.stride_y + bx + size].fill(235);
        }
        for row in by / 2..(by + size) / 2 {
            dst.u[row * dst.stride_u + bx / 2..row * dst.stride_u + (bx + size) / 2].fill(128);
            dst.v[row * dst.stride_v + bx / 2..row * dst.stride_v + (bx + size) / 2].fill(128);
        }

        // 8x8 luma blocks with random levels: changes every frame like busy game
        // content, but stays compressible enough for realistic bitrates.
        const BLOCK: usize = 8;
        let mut state = frame.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut levels = vec![0u8; w.div_ceil(BLOCK)];
        let noise_start = h - self.noise_rows();
        for row in noise_start..h {
            if (row - noise_start).is_multiple_of(BLOCK) {
                for level in levels.iter_mut() {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    *level = 16 + (state % 220) as u8;
                }
            }
            let line = &mut dst.y[row * dst.stride_y..row * dst.stride_y + w];
            for (chunk, &level) in line.chunks_mut(BLOCK).zip(&levels) {
                chunk.fill(level);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(p: &TestPattern, frame: u64) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let (w, h) = (p.width() as usize, p.height() as usize);
        let mut y = vec![0; w * h];
        let mut u = vec![0; w * h / 4];
        let mut v = vec![0; w * h / 4];
        p.render(
            frame,
            PlanesMut {
                y: &mut y,
                stride_y: w,
                u: &mut u,
                stride_u: w / 2,
                v: &mut v,
                stride_v: w / 2,
            },
        );
        (y, u, v)
    }

    #[test]
    fn deterministic_and_animated() {
        let p = TestPattern::new(320, 180);
        assert_eq!(render(&p, 7), render(&p, 7));
        assert_ne!(render(&p, 7).0, render(&p, 8).0);
        assert_ne!(p.box_rect(0).0, p.box_rect(1).0);
    }

    #[test]
    fn bars_and_box_have_expected_luma() {
        let p = TestPattern::new(320, 180);
        let (y, _, _) = render(&p, 0);
        // First bar is white, last bar is black (top row, above the box).
        assert_eq!(y[0], 235);
        assert_eq!(y[319], 16);
        let (bx, by, size) = p.box_rect(10);
        let (y, _, _) = render(&p, 10);
        assert_eq!(y[(by + size / 2) * 320 + bx + size / 2], 235);
    }

    #[test]
    fn box_stays_inside_frame() {
        let p = TestPattern::new(1920, 1080);
        for f in 0..1000 {
            let (x, y, s) = p.box_rect(f);
            assert!(x + s <= 1920 && y + s <= 1080 - 270);
        }
    }

    #[test]
    fn respects_padded_strides() {
        let p = TestPattern::new(64, 32);
        let (sy, sc) = (80, 48);
        let mut y = vec![0xAA; sy * 32];
        let mut u = vec![0xAA; sc * 16];
        let mut v = vec![0xAA; sc * 16];
        p.render(
            3,
            PlanesMut {
                y: &mut y,
                stride_y: sy,
                u: &mut u,
                stride_u: sc,
                v: &mut v,
                stride_v: sc,
            },
        );
        // Padding bytes beyond the visible width stay untouched.
        assert!(y.chunks(sy).all(|r| r[64..].iter().all(|&b| b == 0xAA)));
        assert!(u.chunks(sc).all(|r| r[32..].iter().all(|&b| b == 0xAA)));
    }
}
