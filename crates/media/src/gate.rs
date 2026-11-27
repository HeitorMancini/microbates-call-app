//! Noise gate driven by the "microphone sensitivity" setting, plus the level meter fed to
//! the settings screen.
//!
//! The gate looks one chunk (10 ms) ahead, so it opens *before* the first loud chunk and the
//! start of a word is not cut. The price is a constant 10 ms of latency in the capture path.

use crate::hub::SAMPLES_PER_10MS;

/// Level reported for digital silence / no signal.
pub(crate) const SILENCE_DBFS: f32 = -100.0;
/// Sensitivity at (and above) which the gate is disabled.
pub(crate) const SENSITIVITY_OPEN: u32 = 100;
/// Chunks (10 ms each) the gate stays fully open after the last loud chunk.
const HOLD_CHUNKS: u32 = 20;
/// Per-sample gain change when opening (about 2 ms from closed to open).
const ATTACK_STEP: f32 = 1.0 / 96.0;
/// Per-sample gain change when closing (about 50 ms from open to closed). Together with the
/// hold above this makes a release of about 250 ms without a hard cut.
const RELEASE_STEP: f32 = 1.0 / 2400.0;
/// How fast the meter falls: 60 dB in about 400 ms.
const METER_FALL_DB: f32 = 1.5;

/// Gate threshold in dBFS for a sensitivity of `0..=100`; `None` means the gate is open.
///
/// 99 maps to -59.6 dBFS and 0 to -20 dBFS. The UI mirrors this in `micProcessing.ts`.
pub(crate) fn threshold_dbfs(sensitivity: u32) -> Option<f32> {
    (sensitivity < SENSITIVITY_OPEN).then(|| -60.0 + (SENSITIVITY_OPEN - sensitivity) as f32 * 0.4)
}

/// RMS level of a chunk in dBFS (full scale = 32768), floored at [`SILENCE_DBFS`].
pub(crate) fn rms_dbfs(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return SILENCE_DBFS;
    }
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    let rms = (sum / samples.len() as f64).sqrt();
    if rms < 1.0 {
        return SILENCE_DBFS;
    }
    (20.0 * (rms / 32768.0).log10()) as f32
}

pub(crate) struct NoiseGate {
    /// The chunk waiting for the next one to be judged (the lookahead).
    delayed: [i16; SAMPLES_PER_10MS],
    delayed_db: f32,
    primed: bool,
    gain: f32,
    hold: u32,
}

impl NoiseGate {
    pub(crate) fn new() -> Self {
        Self {
            delayed: [0; SAMPLES_PER_10MS],
            delayed_db: SILENCE_DBFS,
            primed: false,
            gain: 1.0,
            hold: 0,
        }
    }

    /// Swaps `chunk` with the previous one and gates that one.
    ///
    /// Returns `false` on the very first call, when there is no previous chunk yet and
    /// `chunk` must not be used. `level_db` is the level of the incoming `chunk`.
    pub(crate) fn process(&mut self, chunk: &mut [i16], level_db: f32, sensitivity: u32) -> bool {
        debug_assert_eq!(chunk.len(), SAMPLES_PER_10MS);
        let incoming_db = level_db;
        let was_primed = std::mem::replace(&mut self.primed, true);
        let outgoing_db = std::mem::replace(&mut self.delayed_db, incoming_db);
        self.delayed.swap_with_slice(chunk);
        if !was_primed {
            return false;
        }

        let Some(threshold) = threshold_dbfs(sensitivity) else {
            // Gate disabled: pass through untouched and forget any previous state.
            self.gain = 1.0;
            self.hold = 0;
            return true;
        };

        let loud = outgoing_db >= threshold;
        let open = loud || incoming_db >= threshold || self.hold > 0;
        if loud {
            self.hold = HOLD_CHUNKS;
        } else {
            self.hold = self.hold.saturating_sub(1);
        }

        let target = if open { 1.0 } else { 0.0 };
        for sample in chunk.iter_mut() {
            self.gain = if self.gain < target {
                (self.gain + ATTACK_STEP).min(target)
            } else {
                (self.gain - RELEASE_STEP).max(target)
            };
            *sample = (f32::from(*sample) * self.gain).round() as i16;
        }
        true
    }
}

/// Peak meter with instant rise and a slow fall, so a 66 ms poll still sees speech peaks.
pub(crate) struct LevelMeter {
    level: f32,
}

impl LevelMeter {
    pub(crate) fn new() -> Self {
        Self {
            level: SILENCE_DBFS,
        }
    }

    pub(crate) fn push(&mut self, db: f32) -> f32 {
        self.level = db.max(self.level - METER_FALL_DB).max(SILENCE_DBFS);
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = SAMPLES_PER_10MS;

    fn loud() -> Vec<i16> {
        vec![2000; N]
    }

    fn quiet() -> Vec<i16> {
        vec![100; N]
    }

    /// Feeds one chunk, returning the gated outgoing chunk (`None` while priming).
    fn feed(gate: &mut NoiseGate, mut chunk: Vec<i16>, sensitivity: u32) -> Option<Vec<i16>> {
        let db = rms_dbfs(&chunk);
        gate.process(&mut chunk, db, sensitivity).then_some(chunk)
    }

    #[test]
    fn threshold_table_matches_the_ui() {
        assert_eq!(threshold_dbfs(100), None);
        assert_eq!(threshold_dbfs(150), None);
        for (sensitivity, expected) in [(99, -59.6), (75, -50.0), (50, -40.0), (0, -20.0)] {
            let got = threshold_dbfs(sensitivity).unwrap();
            assert!((got - expected).abs() < 1e-4, "{sensitivity}: {got}");
        }
    }

    #[test]
    fn rms_dbfs_is_full_scale_relative() {
        assert_eq!(rms_dbfs(&[0; N]), SILENCE_DBFS);
        assert!(rms_dbfs(&[i16::MAX; N]).abs() < 0.01);
        assert!((rms_dbfs(&[16384; N]) + 6.02).abs() < 0.01);
    }

    #[test]
    fn first_call_only_primes_the_delay() {
        let mut gate = NoiseGate::new();
        assert!(feed(&mut gate, loud(), 100).is_none());
        assert!(feed(&mut gate, loud(), 100).is_some());
    }

    #[test]
    fn sensitivity_100_never_cuts_and_only_delays() {
        let mut gate = NoiseGate::new();
        let inputs: Vec<Vec<i16>> = (0..40)
            .map(|i| (0..N).map(|j| ((i * 31 + j) % 7) as i16 - 3).collect())
            .collect();
        let mut previous: Option<&Vec<i16>> = None;
        for input in &inputs {
            let out = feed(&mut gate, input.clone(), 100);
            assert_eq!(out.as_ref(), previous, "bit-exact, one chunk late");
            previous = Some(input);
        }
    }

    #[test]
    fn closed_gate_outputs_exact_silence() {
        let mut gate = NoiseGate::new();
        feed(&mut gate, quiet(), 50);
        let mut closed_chunks = 0;
        for _ in 0..60 {
            let was_closed = gate.gain == 0.0;
            let out = feed(&mut gate, quiet(), 50).unwrap();
            if was_closed {
                assert!(
                    out.iter().all(|&s| s == 0),
                    "a closed gate is exact silence"
                );
                closed_chunks += 1;
            }
        }
        assert!(
            closed_chunks > 20,
            "the gate closed ({closed_chunks} chunks)"
        );
    }

    #[test]
    fn opens_on_the_first_chunk_above_the_threshold() {
        let mut gate = NoiseGate::new();
        for _ in 0..60 {
            feed(&mut gate, quiet(), 50);
        }
        // The lookahead opens the gate while the chunk before the loud one goes out...
        let lead = feed(&mut gate, loud(), 50).unwrap();
        assert_eq!(
            lead[0],
            (100.0 * ATTACK_STEP).round() as i16,
            "ramps from closed"
        );
        assert_eq!(lead[N - 1], 100);
        // ...so the first loud chunk itself is already fully open.
        let out = feed(&mut gate, loud(), 50).unwrap();
        assert!(out.iter().all(|&s| s == 2000));
    }

    #[test]
    fn lookahead_keeps_the_chunk_right_before_a_loud_one() {
        let mut gate = NoiseGate::new();
        for _ in 0..60 {
            feed(&mut gate, quiet(), 50);
        }
        let before_onset = feed(&mut gate, quiet(), 50).unwrap();
        assert!(before_onset.iter().all(|&s| s == 0), "still closed");
        let onset_lead = feed(&mut gate, loud(), 50).unwrap();
        assert_eq!(
            onset_lead[N - 1],
            100,
            "quiet chunk before the word is audible"
        );
    }

    #[test]
    fn release_holds_then_fades_without_steps() {
        let mut gate = NoiseGate::new();
        feed(&mut gate, loud(), 50);
        feed(&mut gate, loud(), 50);
        // This call still outputs the last loud chunk; the quiet ones follow.
        feed(&mut gate, quiet(), 50);
        let mut last = 100i16;
        let mut quiet_chunks = 0;
        let mut silent_after = None;
        while silent_after.is_none() && quiet_chunks < 100 {
            let out = feed(&mut gate, quiet(), 50).unwrap();
            for &s in &out {
                let step = (i32::from(s) - i32::from(last)).abs();
                assert!(step <= 1, "no hard step: {last} -> {s}");
                last = s;
            }
            if out.iter().all(|&s| s == 0) {
                silent_after = Some(quiet_chunks);
            }
            quiet_chunks += 1;
        }
        let silent_after = silent_after.expect("gate eventually closes");
        // Hold (200 ms) plus fade (50 ms), give or take the chunk granularity and lookahead.
        assert!(
            (22..=26).contains(&silent_after),
            "closed after {silent_after} chunks"
        );
    }

    #[test]
    fn meter_rises_instantly_and_falls_slowly() {
        let mut meter = LevelMeter::new();
        assert_eq!(meter.push(-20.0), -20.0);
        assert_eq!(meter.push(-60.0), -21.5);
        for _ in 0..100 {
            meter.push(SILENCE_DBFS);
        }
        assert_eq!(meter.push(SILENCE_DBFS), SILENCE_DBFS);
    }
}
