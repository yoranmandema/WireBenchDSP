//! Shared overdrive building blocks (design §1).
//!
//! These are the reusable pieces every overdrive variant is built from:
//!
//! - **B1** input high-pass (20 Hz, fixed),
//! - **B2/B5** tone bins (pre-bass / pre-mid / pre-treble / post-tone /
//!   presence / Big Muff tone) and the tone stage that cascades them,
//! - **B3** waveshapers (soft clip, tanh, Tube, Fuzz, DiodeClip — see
//!   `pedal_dsp::waveshape`),
//! - **B4** anti-aliasing (4× oversampling — see
//!   `pedal_dsp::oversample`),
//! - **B6** level/gain staging (the variant blocks apply `db_to_lin` to the
//!   final `level` knob).
//!
//! The variant blocks in `graph/src/blocks.rs` compose these helpers.

use pedal_dsp::biquad::Biquad;
use pedal_dsp::biquad::BiquadKind;

/// B1 — input high-pass, fixed at 20 Hz (design §1).
///
/// Unity-gain, Q 0.707, no parameter. Designed once per graph (`Block::prepare`)
/// and applied per sample.
pub struct InputHpf {
    filt: Biquad,
}

impl InputHpf {
    /// Build the fixed 20 Hz, Q 0.707, unity-gain high-pass at `sr`.
    pub fn new(sr: f32) -> Self {
        Self {
            filt: Biquad::make(BiquadKind::HighPass, sr, 20.0, 0.707, 0.0),
        }
    }

    /// Process one sample.
    pub fn process(&mut self, x: f32) -> f32 {
        self.filt.process(x)
    }

    /// Clear internal state.
    pub fn reset(&mut self) {
        self.filt.reset();
    }
}

/// B2/B5 — the shared tone stage: a cascade of Biquad bins.
///
/// All bins default to pass-through (`Biquad::identity()`). Each variant sets
/// the bins it needs. `process` cascades the bins in a fixed order:
/// pre_bass → pre_mid → pre_treble → post_tone → presence → big_muff_tone.
///
/// Bin frequencies are fixed by the design doc (B2) except `big_muff_tone`,
/// whose centre frequency is swept from a Hertz knob. Coefficients are
/// designed at the current sample rate per block (never per sample).
pub struct ToneStage {
    pre_bass: Biquad,
    pre_mid: Biquad,
    pre_treble: Biquad,
    post_tone: Biquad,
    presence: Biquad,
    big_muff_tone: Biquad,
}

impl ToneStage {
    /// Build a `ToneStage` with every bin set to pass-through.
    pub fn new(sr: f32) -> Self {
        Self {
            pre_bass: Biquad::identity(),
            pre_mid: Biquad::identity(),
            pre_treble: Biquad::identity(),
            post_tone: Biquad::identity(),
            presence: Biquad::identity(),
            big_muff_tone: Biquad::identity(),
        }
    }

    /// Pre-bass low shelf (B2: f0 200 Hz, Q 0.707).
    pub fn set_pre_bass(&mut self, sr: f32, db: f32) {
        self.pre_bass = Biquad::make(BiquadKind::LowShelf, sr, 200.0, 0.707, db);
    }

    /// Pre-mid peaking filter (B2: f0 720 Hz, Q 1.2).
    pub fn set_pre_mid(&mut self, sr: f32, db: f32) {
        self.pre_mid = Biquad::make(BiquadKind::Peak, sr, 720.0, 1.2, db);
    }

    /// Pre-treble high shelf (B2: f0 4000 Hz, Q 0.707).
    pub fn set_pre_treble(&mut self, sr: f32, db: f32) {
        self.pre_treble = Biquad::make(BiquadKind::HighShelf, sr, 4000.0, 0.707, db);
    }

    /// Post-tone low shelf (B2: f0 3200 Hz, Q 0.707).
    pub fn set_post_tone(&mut self, sr: f32, db: f32) {
        self.post_tone = Biquad::make(BiquadKind::LowShelf, sr, 3200.0, 0.707, db);
    }

    /// Presence high shelf (B2: f0 5000 Hz, Q 0.8).
    pub fn set_presence(&mut self, sr: f32, db: f32) {
        self.presence = Biquad::make(BiquadKind::HighShelf, sr, 5000.0, 0.8, db);
    }

    /// Big Muff variable peaking (B2: Peak at `f0`, Q 0.7, gain dB).
    pub fn set_bigmuff_tone(&mut self, sr: f32, f0: f32, db: f32) {
        self.big_muff_tone = Biquad::make(BiquadKind::Peak, sr, f0, 0.7, db);
    }

    /// Process one sample through the active bins.
    pub fn process(&mut self, x: f32) -> f32 {
        let mut s = x;
        s = self.pre_bass.process(s);
        s = self.pre_mid.process(s);
        s = self.pre_treble.process(s);
        s = self.post_tone.process(s);
        s = self.presence.process(s);
        s = self.big_muff_tone.process(s);
        s
    }

    /// Clear internal state on all bins.
    pub fn reset(&mut self) {
        // Identity biquads carry no state, but keep the interface uniform.
        let _ = self.pre_bass.reset();
        let _ = self.pre_mid.reset();
        let _ = self.pre_treble.reset();
        let _ = self.post_tone.reset();
        let _ = self.presence.reset();
        let _ = self.big_muff_tone.reset();
    }
}
