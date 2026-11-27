//! RNNoise noise suppression (via the pure-Rust `nnnoiseless` port) on 48 kHz mono PCM.
//!
//! RNNoise works on exactly 10 ms frames at 48 kHz, which is the chunk size of the whole
//! capture pipeline, so one chunk is one frame and no extra buffering (or latency) is needed.

use nnnoiseless::DenoiseState;

use crate::hub::SAMPLES_PER_10MS;

const _: () = assert!(DenoiseState::FRAME_SIZE == SAMPLES_PER_10MS);

pub(crate) struct Denoiser {
    state: Box<DenoiseState<'static>>,
    input: [f32; SAMPLES_PER_10MS],
    output: [f32; SAMPLES_PER_10MS],
    /// RNNoise's first output frame after (re)start is a fade-in artifact: emit silence.
    first: bool,
}

impl Denoiser {
    pub(crate) fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            input: [0.0; SAMPLES_PER_10MS],
            output: [0.0; SAMPLES_PER_10MS],
            first: true,
        }
    }

    /// Forgets everything RNNoise learned about the noise (used after it was bypassed).
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    /// Denoises one chunk in place. `chunk` must hold exactly [`SAMPLES_PER_10MS`] samples.
    pub(crate) fn process(&mut self, chunk: &mut [i16]) {
        debug_assert_eq!(chunk.len(), SAMPLES_PER_10MS);
        // nnnoiseless expects f32 in the i16 range, not normalised to +-1.
        for (dst, &src) in self.input.iter_mut().zip(chunk.iter()) {
            *dst = f32::from(src);
        }
        self.state.process_frame(&mut self.output, &self.input);
        if std::mem::take(&mut self.first) {
            chunk.fill(0);
            return;
        }
        for (dst, &src) in chunk.iter_mut().zip(self.output.iter()) {
            *dst = src.round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic uniform white noise in `-amplitude..=amplitude`.
    fn noise(seed: &mut u32, amplitude: i32) -> Vec<i16> {
        (0..SAMPLES_PER_10MS)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let unit = (*seed >> 8) as f32 / (1u32 << 24) as f32;
                ((unit * 2.0 - 1.0) * amplitude as f32) as i16
            })
            .collect()
    }

    fn rms(samples: &[i16]) -> f64 {
        let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    #[test]
    fn silence_stays_silence() {
        let mut d = Denoiser::new();
        for _ in 0..50 {
            let mut chunk = vec![0i16; SAMPLES_PER_10MS];
            d.process(&mut chunk);
            assert!(chunk.iter().all(|&s| s.abs() <= 1));
        }
    }

    /// Fan/rumble-like noise (low-passed white noise) is what RNNoise is built to remove.
    /// Measured: 50 dB or more of attenuation for every amplitude and seed tried. Plain white
    /// noise and a pure tone are *not* a reliable proxy (RNNoise attenuates white noise
    /// erratically and treats a steady hum as voiced speech), so they are not asserted.
    #[test]
    fn low_frequency_noise_loses_energy() {
        for amplitude in [300, 3000] {
            for seed in [1u32, 2, 3] {
                let mut d = Denoiser::new();
                let mut seed = seed;
                let mut lowpass = 0.0f32;
                let (mut before, mut after) = (0.0, 0.0);
                for i in 0..400 {
                    let mut chunk = noise(&mut seed, amplitude);
                    for sample in chunk.iter_mut() {
                        lowpass = lowpass * 0.95 + f32::from(*sample) * 0.3;
                        *sample = lowpass as i16;
                    }
                    let input_rms = rms(&chunk);
                    d.process(&mut chunk);
                    if i >= 300 {
                        before += input_rms;
                        after += rms(&chunk);
                    }
                }
                assert!(
                    after < before * 0.1,
                    "amplitude {amplitude}: before {before:.0}, after {after:.0}"
                );
            }
        }
    }

    #[test]
    fn preserves_chunk_length() {
        let mut d = Denoiser::new();
        let mut chunk = vec![100i16; SAMPLES_PER_10MS];
        d.process(&mut chunk);
        assert_eq!(chunk.len(), SAMPLES_PER_10MS);
    }

    #[test]
    fn reset_restarts_from_a_clean_state() {
        let mut d = Denoiser::new();
        let mut seed = 3;
        for _ in 0..20 {
            d.process(&mut noise(&mut seed, 3000));
        }
        d.reset();
        let mut chunk = noise(&mut seed, 3000);
        d.process(&mut chunk);
        assert!(
            chunk.iter().all(|&s| s == 0),
            "first frame after reset is silent"
        );
    }
}
