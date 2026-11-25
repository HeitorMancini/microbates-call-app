//! Instant voice-activity detection on 48 kHz mono PCM, used for the speaking indicator.
//!
//! LiveKit's active-speaker list is smoothed server-side and misses short sounds, so the
//! indicator is driven by the audio level of the PCM itself.

/// RMS (of i16 samples) above which a chunk counts as voice: about -40 dBFS.
const THRESHOLD_RMS: f64 = 330.0;
/// Chunks (10 ms each) the indicator stays on after the last loud chunk.
const HANGOVER_CHUNKS: u32 = 25;

#[derive(Debug, Default)]
pub(crate) struct SpeechDetector {
    speaking: bool,
    quiet_chunks: u32,
}

impl SpeechDetector {
    /// Feeds one chunk; returns the new state when it changed.
    pub(crate) fn push(&mut self, samples: &[i16]) -> Option<bool> {
        if rms(samples) >= THRESHOLD_RMS {
            self.quiet_chunks = 0;
            return self.set(true);
        }
        self.quiet_chunks = self.quiet_chunks.saturating_add(1);
        if self.quiet_chunks > HANGOVER_CHUNKS {
            return self.set(false);
        }
        None
    }

    /// The audio stopped arriving (muted or gone): silence immediately.
    pub(crate) fn reset(&mut self) -> Option<bool> {
        self.quiet_chunks = 0;
        self.set(false)
    }

    fn set(&mut self, speaking: bool) -> Option<bool> {
        (std::mem::replace(&mut self.speaking, speaking) != speaking).then_some(speaking)
    }
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / samples.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOUD: [i16; 480] = [2000; 480];
    const QUIET: [i16; 480] = [10; 480];

    #[test]
    fn a_single_loud_chunk_turns_it_on_immediately() {
        let mut d = SpeechDetector::default();
        assert_eq!(d.push(&QUIET), None);
        assert_eq!(d.push(&LOUD), Some(true));
        assert_eq!(d.push(&LOUD), None);
    }

    #[test]
    fn stays_on_through_the_hangover_then_turns_off() {
        let mut d = SpeechDetector::default();
        d.push(&LOUD);
        for _ in 0..HANGOVER_CHUNKS {
            assert_eq!(d.push(&QUIET), None);
        }
        assert_eq!(d.push(&QUIET), Some(false));
    }

    #[test]
    fn a_loud_chunk_during_the_hangover_restarts_it() {
        let mut d = SpeechDetector::default();
        d.push(&LOUD);
        for _ in 0..HANGOVER_CHUNKS {
            d.push(&QUIET);
        }
        assert_eq!(d.push(&LOUD), None);
        for _ in 0..HANGOVER_CHUNKS {
            assert_eq!(d.push(&QUIET), None);
        }
    }

    #[test]
    fn reset_silences_only_when_speaking() {
        let mut d = SpeechDetector::default();
        assert_eq!(d.reset(), None);
        d.push(&LOUD);
        assert_eq!(d.reset(), Some(false));
    }
}
