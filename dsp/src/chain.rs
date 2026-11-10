//! The effect chain: the musical heart of the pedal.
//!
//! Signal flow (per channel unless noted):
//!
//!   in -> DC block -> distortion -> tone stack -> delay -> reverb -> DC block -> out
//!
//! Everything is stereo.  Parameters are plain `f32` fields so the control
//! loop can update them from pots/footswitch without allocating.
//!
//! This module lives in `pedal-dsp` (not in the firmware crate) so the host
//! reference model in `examples/wav_process.rs` runs the *exact same* code
//! as the MCU.  No `std`, no allocator.

use crate::reverb::Reverb48k;
use crate::{
    BiquadKind, DcBlocker, EnvelopeFollower, FeedbackDelay, Shaper, ShaperKind, StereoBiquad,
};

/// Maximum delay line length in samples (100 ms at 48 kHz).
pub const MAX_DELAY_SAMPLES: usize = 4800;

pub struct EffectChain {
    sample_rate: f32,

    dc_in_l: DcBlocker,
    dc_in_r: DcBlocker,
    dc_out_l: DcBlocker,
    dc_out_r: DcBlocker,

    // Distortion
    shaper_l: Shaper,
    shaper_r: Shaper,

    // Tone stack
    tone_low: StereoBiquad,
    tone_high: StereoBiquad,

    // Time-based
    delay_l: FeedbackDelay<MAX_DELAY_SAMPLES>,
    delay_r: FeedbackDelay<MAX_DELAY_SAMPLES>,
    reverb: Reverb48k,

    // Dynamics (envelope follower drives an optional auto-gain)
    follower: EnvelopeFollower,

    // ---- parameters (written by the control loop) ----
    /// Distortion drive, 0.1 .. 20
    pub drive: f32,
    /// Distortion output level, 0 .. 4
    pub distortion_level: f32,
    /// Dry/wet mix of the distortion, 0..1
    pub distortion_mix: f32,
    /// Tone, -1 .. 1 (maps to -12 .. +12 dB on both shelves)
    pub tone: f32,
    /// Master output level, 0 .. 2
    pub level: f32,
    /// Delay time in ms
    pub delay_ms: f32,
    /// Delay mix, 0..1
    pub delay_mix: f32,
    /// Delay feedback, 0..0.95
    pub delay_feedback: f32,
    /// Reverb mix, 0..1
    pub reverb_mix: f32,
    /// Effect bypass
    pub enabled: bool,
}

impl EffectChain {
    pub fn new(sample_rate: f32) -> Self {
        let mut chain = Self {
            sample_rate,
            dc_in_l: DcBlocker::new(20.0, sample_rate),
            dc_in_r: DcBlocker::new(20.0, sample_rate),
            dc_out_l: DcBlocker::new(20.0, sample_rate),
            dc_out_r: DcBlocker::new(20.0, sample_rate),
            shaper_l: Shaper::new(ShaperKind::Tanh),
            shaper_r: Shaper::new(ShaperKind::Tanh),
            tone_low: StereoBiquad::make(BiquadKind::LowShelf, sample_rate, 200.0, 0.707, 0.0),
            tone_high: StereoBiquad::make(BiquadKind::HighShelf, sample_rate, 4_000.0, 0.707, 0.0),
            delay_l: FeedbackDelay::new(sample_rate, 250.0),
            delay_r: FeedbackDelay::new(sample_rate, 250.0),
            reverb: Reverb48k::new_48k(),
            follower: EnvelopeFollower::new(2.0, 50.0, sample_rate),
            drive: 3.0,
            distortion_level: 0.8,
            distortion_mix: 1.0,
            tone: 0.0,
            level: 0.9,
            delay_ms: 250.0,
            delay_mix: 0.0,
            delay_feedback: 0.35,
            reverb_mix: 0.0,
            enabled: true,
        };
        chain.apply_static_params();
        chain
    }

    fn apply_static_params(&mut self) {
        self.shaper_l.set_drive(self.drive);
        self.shaper_l.set_level(self.distortion_level);
        self.shaper_l.set_mix(self.distortion_mix);
        self.shaper_r.set_drive(self.drive);
        self.shaper_r.set_level(self.distortion_level);
        self.shaper_r.set_mix(self.distortion_mix);

        let tone_db = self.tone * 12.0;
        self.tone_low
            .design(BiquadKind::LowShelf, self.sample_rate, 200.0, 0.707, tone_db);
        self.tone_high
            .design(BiquadKind::HighShelf, self.sample_rate, 4_000.0, 0.707, tone_db);

        self.delay_l.set_delay_ms(self.delay_ms);
        self.delay_l.set_feedback(self.delay_feedback);
        self.delay_l.mix = self.delay_mix;
        self.delay_r.set_delay_ms(self.delay_ms);
        self.delay_r.set_feedback(self.delay_feedback);
        self.delay_r.mix = self.delay_mix;

        self.reverb.set_wet(self.reverb_mix);
        self.reverb.set_dry(1.0 - self.reverb_mix);
    }

    /// Call after changing any parameter.
    pub fn update_params(&mut self) {
        self.apply_static_params();
    }

    /// Swap the distortion type.  The shapers are recreated (their internal
    /// state resets), which is acceptable — the user is changing character.
    pub fn set_shaper(&mut self, kind: ShaperKind) {
        self.shaper_l = Shaper::new(kind);
        self.shaper_r = Shaper::new(kind);
        self.apply_static_params();
    }

    #[inline]
    pub fn process_stereo(&mut self, l_in: f32, r_in: f32) -> (f32, f32) {
        // Input DC removal.  Each channel needs its OWN blocker: sharing one
        // instance would filter the right channel twice (the second call sees
        // the state left by the first), leaving L and R inconsistent.
        let mut l = self.dc_in_l.process(l_in);
        let mut r = self.dc_in_r.process(r_in);

        if !self.enabled {
            return (l * self.level, r * self.level);
        }

        // Distortion
        l = self.shaper_l.process(l);
        r = self.shaper_r.process(r);

        // Tone stack
        let (tl, tr) = self.tone_low.process(l, r);
        let (tl, tr) = self.tone_high.process(tl, tr);
        l = tl;
        r = tr;

        // Delay (each channel has its own line)
        l = self.delay_l.process(l);
        r = self.delay_r.process(r);

        // Reverb
        let (rl, rr) = self.reverb.process(l, r);

        // Output DC removal + master level
        l = self.dc_out_l.process(rl) * self.level;
        r = self.dc_out_r.process(rr) * self.level;

        // Soft-limit the output so the codec never sees a clipped word.
        (soft_limit(l), soft_limit(r))
    }

    /// Envelope follower, useful for tuner/dynamics ideas.
    #[inline]
    pub fn envelope(&mut self, l: f32, r: f32) -> f32 {
        let e = self.follower.process(l);
        let _ = r;
        e
    }

    pub fn reset(&mut self) {
        self.dc_in_l.reset();
        self.dc_in_r.reset();
        self.dc_out_l.reset();
        self.dc_out_r.reset();
        self.shaper_l.reset();
        self.shaper_r.reset();
        self.tone_low.reset();
        self.tone_high.reset();
        self.delay_l.clear();
        self.delay_r.clear();
        self.reverb.clear();
        self.follower.reset();
    }
}

/// Gentle output limiter.
///
/// This must be a SOFT clipper: a hard `if x > 1 { 1 }` corner is a
/// nonlinearity too, and it folds harmonics back into the audio band,
/// which is exactly the aliasing we oversample to avoid.
#[inline]
fn soft_limit(x: f32) -> f32 {
    // Cubic saturation: unity slope at 0, reaches +/-1 with zero slope,
    // so the knee is smooth and adds no aliasing of its own.
    let x = if x > 1.0 {
        1.0
    } else if x < -1.0 {
        -1.0
    } else {
        x
    };
    1.5 * x - 0.5 * x * x * x
}
