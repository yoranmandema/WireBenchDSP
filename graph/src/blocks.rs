//! A concrete block library built on `pedal-dsp` primitives.
//!
//! These are real, working blocks — not stubs. They demonstrate the plugin
//! architecture and give the host simulator something to run.

use crate::prelude::*;
use pedal_dsp::{Biquad, BiquadKind};

/// A simple gain stage, in dB.
pub struct Gain {
    gain_db: f32,
}

impl Default for Gain {
    fn default() -> Self {
        Self { gain_db: 0.0 }
    }
}

impl Block for Gain {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "util.gain",
            name: "Gain",
            category: Category::Utility,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![ParamSpec {
                id: "gain",
                name: "Gain",
                unit: Unit::Decibels,
                min: -60.0,
                max: 24.0,
                default: 0.0,
                curve: Curve::AmpGain,
            }],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        self.gain_db = ctx.param(0);
        let g = pedal_dsp::util::db_to_lin(self.gain_db);
        let input = ctx.inputs[0];
        let output = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            output[i] = input[i] * g;
        }
        Ok(())
    }

    fn reset(&mut self) {}
}

/// A three-band tone stack (low / mid / high), like an amp's.
pub struct ToneStack {
    bands: [(BiquadKind, Biquad, f32, f32); 3],
}

impl Default for ToneStack {
    fn default() -> Self {
        Self {
            bands: [
                (BiquadKind::LowShelf, Biquad::identity(), 200.0, 0.7),
                (BiquadKind::Peak, Biquad::identity(), 1000.0, 1.0),
                (BiquadKind::HighShelf, Biquad::identity(), 4000.0, 0.7),
            ],
        }
    }
}

impl Block for ToneStack {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "tone.stack",
            name: "Tone Stack",
            category: Category::Tone,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "low",  name: "Low",  unit: Unit::Decibels, min: -20.0, max: 20.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "mid",  name: "Mid",  unit: Unit::Decibels, min: -20.0, max: 20.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "high", name: "High", unit: Unit::Decibels, min: -20.0, max: 20.0, default: 0.0, curve: Curve::AmpGain },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, _max: usize) -> Result<(), BlockError> {
        for (kind, band, f0, q) in &mut self.bands {
            *band = Biquad::make(*kind, sr, *f0, *q, 0.0);
        }
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let gains = [ctx.param(0), ctx.param(1), ctx.param(2)];
        for ((kind, band, f0, q), db) in self.bands.iter_mut().zip(gains) {
            band.design(*kind, ctx.sample_rate, *f0, *q, db);
        }
        let input = ctx.inputs[0];
        let output = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            let mut s = input[i];
            for (_, band, _, _) in self.bands.iter_mut() {
                s = band.process(s);
            }
            output[i] = s;
        }
        Ok(())
    }

    fn reset(&mut self) {
        for (_, band, _, _) in &mut self.bands {
            band.reset();
        }
    }
}

/// A soft-clipping overdrive.
pub struct Overdrive {
    drive: f32,
    level: f32,
}

impl Default for Overdrive {
    fn default() -> Self {
        Self { drive: 1.0, level: 1.0 }
    }
}

impl Block for Overdrive {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "dist.overdrive",
            name: "Overdrive",
            category: Category::Distortion,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "drive", name: "Drive", unit: Unit::Linear, min: 0.0, max: 10.0, default: 1.0, curve: Curve::Log },
                ParamSpec { id: "level", name: "Level", unit: Unit::Decibels, min: -30.0, max: 12.0, default: 0.0, curve: Curve::AmpGain },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let drive = ctx.param(0);
        let level = pedal_dsp::util::db_to_lin(ctx.param(1));
        self.drive = drive;
        self.level = level;
        let input = ctx.inputs[0];
        let output = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            let x = input[i] * drive;
            let y = pedal_dsp::util::tanh_approx(x);
            output[i] = y * level;
        }
        Ok(())
    }

    fn reset(&mut self) {}
}

/// A reverb, using `pedal-dsp`'s Freeverb.
pub struct Reverb {
    inner: Option<pedal_dsp::reverb::FreeverbMono<1760, 605>>,
    mix: f32,
}

impl Default for Reverb {
    fn default() -> Self {
        Self { inner: None, mix: 0.0 }
    }
}

impl Block for Reverb {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "time.reverb",
            name: "Reverb",
            category: Category::Time,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![ParamSpec { id: "mix", name: "Mix", unit: Unit::Percent, min: 0.0, max: 100.0, default: 30.0, curve: Curve::Linear }],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, _max: usize) -> Result<(), BlockError> {
        self.inner = Some(pedal_dsp::reverb::FreeverbMono::<1760, 605>::new(sr));
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let mix = ctx.param(0) / 100.0;
        self.mix = mix;
        let input = ctx.inputs[0];
        let output = &mut ctx.outputs[0][..];
        let rev = self.inner.as_mut().ok_or(BlockError::NotPrepared("time.reverb"))?;
        for i in 0..ctx.n {
            let wet = rev.process(input[i]);
            output[i] = pedal_dsp::util::mix(input[i], wet, mix);
        }
        Ok(())
    }

    fn reset(&mut self) {
        if let Some(r) = &mut self.inner {
            r.clear();
        }
    }
}

/// The graph's input node. The executor writes the raw input into this node's
/// output buffer before running the steps.
#[derive(Default)]
pub struct IoInput;

impl Block for IoInput {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "io.input",
            name: "Input",
            category: Category::Io,
            inputs: vec![],
            outputs: vec![PortSpec::audio("out")],
            params: vec![],
            controls_in: vec![],
            controls_out: vec![],
        }
    }
    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }
    fn process(&mut self, _ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        Ok(())
    }
    fn reset(&mut self) {}
}

/// The graph's output node. The executor reads this node's input buffer as the
/// graph's output.
#[derive(Default)]
pub struct IoOutput;

impl Block for IoOutput {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "io.output",
            name: "Output",
            category: Category::Io,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![],
            params: vec![],
            controls_in: vec![],
            controls_out: vec![],
        }
    }
    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }
    fn process(&mut self, _ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        Ok(())
    }
    fn reset(&mut self) {}
}

/// A two-input summer with per-input ratios. The recombine node.
pub struct MixSum {
    a: f32,
    b: f32,
}

impl Default for MixSum {
    fn default() -> Self {
        Self { a: 1.0, b: 1.0 }
    }
}

impl Block for MixSum {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "util.mix",
            name: "Mix",
            category: Category::Utility,
            inputs: vec![PortSpec::audio("in_a"), PortSpec::audio("in_b")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "a", name: "A", unit: Unit::Ratio, min: 0.0, max: 1.0, default: 1.0, curve: Curve::Linear },
                ParamSpec { id: "b", name: "B", unit: Unit::Ratio, min: 0.0, max: 1.0, default: 1.0, curve: Curve::Linear },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }
    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }
    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        self.a = ctx.param(0);
        self.b = ctx.param(1);
        let a = ctx.inputs[0];
        let b = ctx.inputs[1];
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            out[i] = a[i] * self.a + b[i] * self.b;
        }
        Ok(())
    }
    fn reset(&mut self) {}
}

/// A feedback delay.
pub struct Delay {
    inner: Option<pedal_dsp::FeedbackDelay<60000>>,
    time_ms: f32,
    feedback: f32,
    mix: f32,
}

impl Default for Delay {
    fn default() -> Self {
        Self { inner: None, time_ms: 300.0, feedback: 0.35, mix: 0.3 }
    }
}

impl Block for Delay {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "time.delay",
            name: "Delay",
            category: Category::Time,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "time", name: "Time", unit: Unit::Milliseconds, min: 20.0, max: 1000.0, default: 300.0, curve: Curve::Log },
                ParamSpec { id: "feedback", name: "Feedback", unit: Unit::Percent, min: 0.0, max: 95.0, default: 35.0, curve: Curve::Linear },
                ParamSpec { id: "mix", name: "Mix", unit: Unit::Percent, min: 0.0, max: 100.0, default: 30.0, curve: Curve::Linear },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, _max: usize) -> Result<(), BlockError> {
        self.inner = Some(pedal_dsp::FeedbackDelay::<60000>::new(sr, self.time_ms));
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        self.time_ms = ctx.param(0);
        self.feedback = ctx.param(1) / 100.0;
        self.mix = ctx.param(2) / 100.0;
        let input = ctx.inputs[0];
        let output = &mut ctx.outputs[0][..];
        let d = self.inner.as_mut().ok_or(BlockError::NotPrepared("time.delay"))?;
        d.set_delay_ms(self.time_ms);
        d.set_feedback(self.feedback);
        for i in 0..ctx.n {
            let wet = d.process(input[i]);
            output[i] = pedal_dsp::util::mix(input[i], wet, self.mix);
        }
        Ok(())
    }

    fn reset(&mut self) {
        if let Some(d) = &mut self.inner {
            d.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// Amp, cab, pitch delay, master
// ---------------------------------------------------------------------------

use pedal_dsp::waveshape::{Shaper, ShaperKind};
use pedal_dsp::{DcBlocker, PitchMode};

/// The amp voicing a `Choice` param selects.
const AMP_VOICES: &[&str] = &["Clean", "Crunch", "Lead"];

/// A two-stage amp: tighten → preamp clip → tone stack → power-amp sag-ish
/// soft clip → level. Both nonlinear stages are 4× oversampled (`Shaper`), so
/// high gain doesn't alias. This is the placeholder the NAM block replaces.
pub struct Amp {
    tight: Biquad,
    pre: Shaper,
    dc: Option<DcBlocker>,
    bass: Biquad,
    mid: Biquad,
    treble: Biquad,
    power: Shaper,
    fizz: Biquad,
}

impl Default for Amp {
    fn default() -> Self {
        Self {
            tight: Biquad::identity(),
            pre: Shaper::new(ShaperKind::Tube),
            dc: None,
            bass: Biquad::identity(),
            mid: Biquad::identity(),
            treble: Biquad::identity(),
            power: Shaper::new(ShaperKind::Tanh),
            fizz: Biquad::identity(),
        }
    }
}

impl Block for Amp {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "amp.basic",
            name: "Amp",
            category: Category::Modelling,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "voice", name: "Voice", unit: Unit::Choice(AMP_VOICES), min: 0.0, max: 2.0, default: 1.0, curve: Curve::Linear },
                ParamSpec { id: "gain", name: "Gain", unit: Unit::Linear, min: 0.0, max: 10.0, default: 5.0, curve: Curve::Linear },
                ParamSpec { id: "bass", name: "Bass", unit: Unit::Decibels, min: -12.0, max: 12.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "mid", name: "Mid", unit: Unit::Decibels, min: -12.0, max: 12.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "treble", name: "Treble", unit: Unit::Decibels, min: -12.0, max: 12.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "level", name: "Level", unit: Unit::Decibels, min: -40.0, max: 12.0, default: -6.0, curve: Curve::AmpGain },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, _max: usize) -> Result<(), BlockError> {
        self.dc = Some(DcBlocker::new(10.0, sr));
        self.pre.reset();
        self.power.reset();
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let sr = ctx.sample_rate;
        let voice = ctx.param(0).round().clamp(0.0, 2.0) as usize;
        let gain = ctx.param(1).clamp(0.0, 10.0) / 10.0;
        let level = pedal_dsp::util::db_to_lin(ctx.param(5));

        // Voicing: how hard each stage is driven and how tight the low end is.
        let (tight_hz, pre_max, power_drive, mid_hz) = match voice {
            0 => (60.0, 4.0, 1.0, 500.0),   // clean: barely breaks up
            1 => (90.0, 20.0, 1.6, 650.0),  // crunch
            _ => (130.0, 20.0, 2.5, 750.0), // lead: tight and saturated
        };
        self.tight.design(BiquadKind::HighPass, sr, tight_hz, 0.7, 0.0);
        self.bass.design(BiquadKind::LowShelf, sr, 120.0, 0.7, ctx.param(2));
        self.mid.design(BiquadKind::Peak, sr, mid_hz, 0.8, ctx.param(3));
        self.treble.design(BiquadKind::HighShelf, sr, 3200.0, 0.7, ctx.param(4));
        self.fizz.design(BiquadKind::LowPass, sr, 9000.0, 0.7, 0.0);
        // Drive is exponential in the knob: 0 → 0.5, 10 → pre_max.
        self.pre.set_drive(0.5 * (pre_max / 0.5f32).powf(gain));
        self.pre.set_level(1.0);
        self.power.set_drive(power_drive);
        self.power.set_level(1.0);
        // The lead voice gets a second preamp stage worth of gain.
        let lead_boost = if voice == 2 { 1.0 + 3.0 * gain } else { 1.0 };

        let dc = self.dc.as_mut().ok_or(BlockError::NotPrepared("amp.basic"))?;
        let input = ctx.inputs[0];
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            let mut x = self.tight.process(input[i]);
            x = self.pre.process(x * lead_boost);
            x = dc.process(x);
            x = self.treble.process(self.mid.process(self.bass.process(x)));
            x = self.power.process(x);
            out[i] = self.fizz.process(x) * level;
        }
        Ok(())
    }

    fn reset(&mut self) {
        for b in [&mut self.tight, &mut self.bass, &mut self.mid, &mut self.treble, &mut self.fizz] {
            b.reset();
        }
        self.pre.reset();
        self.power.reset();
    }
}

/// A filter-based speaker cabinet: low cut, speaker resonance, a mud dip,
/// presence, and a steep (24 dB/oct) high cut. Without a cab, any amp sounds
/// like fizz. Real IR convolution replaces this later.
pub struct Cab {
    filters: [Biquad; 6],
}

impl Default for Cab {
    fn default() -> Self {
        Self { filters: [Biquad::identity(); 6] }
    }
}

impl Block for Cab {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "cab.sim",
            name: "Cab",
            category: Category::Modelling,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "low_cut", name: "Low cut", unit: Unit::Hertz, min: 40.0, max: 200.0, default: 80.0, curve: Curve::Log },
                ParamSpec { id: "resonance", name: "Resonance", unit: Unit::Decibels, min: 0.0, max: 12.0, default: 4.0, curve: Curve::AmpGain },
                ParamSpec { id: "presence", name: "Presence", unit: Unit::Decibels, min: -9.0, max: 9.0, default: 2.0, curve: Curve::AmpGain },
                ParamSpec { id: "high_cut", name: "High cut", unit: Unit::Hertz, min: 2000.0, max: 10000.0, default: 5000.0, curve: Curve::Log },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let sr = ctx.sample_rate;
        let hc = ctx.param(3);
        let f = &mut self.filters;
        f[0].design(BiquadKind::HighPass, sr, ctx.param(0), 0.7, 0.0);
        f[1].design(BiquadKind::Peak, sr, 110.0, 1.4, ctx.param(1));
        f[2].design(BiquadKind::Peak, sr, 400.0, 1.0, -4.0);
        f[3].design(BiquadKind::Peak, sr, 2500.0, 1.2, ctx.param(2));
        f[4].design(BiquadKind::LowPass, sr, hc, 0.54, 0.0);
        f[5].design(BiquadKind::LowPass, sr, hc, 1.31, 0.0);
        let input = ctx.inputs[0];
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            let mut x = input[i];
            for b in f.iter_mut() {
                x = b.process(x);
            }
            out[i] = x;
        }
        Ok(())
    }

    fn reset(&mut self) {
        for b in &mut self.filters {
            b.reset();
        }
    }
}

const PITCH_MODES: &[&str] = &["Target", "Sweep", "Shepard"];

/// The Rainbow-Machine-style delay: a pitch shifter in the feedback path, so
/// every repeat is shifted again and the pitch climbs (or falls).
pub struct PitchDelay {
    inner: Option<Box<pedal_dsp::FeedbackDelay<60000, 2400>>>,
}

impl Default for PitchDelay {
    fn default() -> Self {
        Self { inner: None }
    }
}

impl Block for PitchDelay {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "time.pitchdelay",
            name: "Pitch Delay",
            category: Category::Pitch,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "time", name: "Time", unit: Unit::Milliseconds, min: 20.0, max: 1000.0, default: 380.0, curve: Curve::Log },
                ParamSpec { id: "feedback", name: "Feedback", unit: Unit::Percent, min: 0.0, max: 95.0, default: 55.0, curve: Curve::Linear },
                ParamSpec { id: "pitch", name: "Pitch", unit: Unit::Cents, min: -1200.0, max: 1200.0, default: 700.0, curve: Curve::Linear },
                ParamSpec { id: "rate", name: "Sweep rate", unit: Unit::Cents, min: -1200.0, max: 1200.0, default: 0.0, curve: Curve::Linear },
                ParamSpec { id: "mode", name: "Mode", unit: Unit::Choice(PITCH_MODES), min: 0.0, max: 2.0, default: 0.0, curve: Curve::Linear },
                ParamSpec { id: "mix", name: "Mix", unit: Unit::Percent, min: 0.0, max: 100.0, default: 40.0, curve: Curve::Linear },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, _max: usize) -> Result<(), BlockError> {
        // Built straight into a Box: the delay is ~250 KB, too big to bounce
        // through the stack on a small thread.
        let mut d: Box<pedal_dsp::FeedbackDelay<60000, 2400>> = Box::new(pedal_dsp::FeedbackDelay::new(sr, 380.0));
        d.mix = 1.0; // the block does its own dry/wet
        self.inner = Some(d);
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let d = self.inner.as_mut().ok_or(BlockError::NotPrepared("time.pitchdelay"))?;
        d.set_delay_ms(ctx.param(0));
        d.set_feedback(ctx.param(1) / 100.0);
        d.set_pitch_cents(ctx.param(2));
        d.set_pitch_rate(ctx.param(3));
        d.set_pitch_mode(match ctx.param(4).round() as i32 {
            1 => PitchMode::Sweep,
            2 => PitchMode::Shepard,
            _ => PitchMode::Target,
        });
        d.mix = 1.0;
        let mix = ctx.param(5) / 100.0;
        let input = ctx.inputs[0];
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            let wet = d.process(input[i]);
            out[i] = pedal_dsp::util::mix(input[i], wet, mix);
        }
        Ok(())
    }

    fn reset(&mut self) {
        if let Some(d) = &mut self.inner {
            d.clear();
        }
    }
}

/// Output volume plus a soft limiter, so nothing downstream ever sees more
/// than the ceiling. `ceiling * tanh(x / ceiling)`: transparent well below the
/// ceiling, smooth into it, zero latency.
#[derive(Default)]
pub struct Master;

impl Block for Master {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "util.master",
            name: "Master",
            category: Category::Utility,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "volume", name: "Volume", unit: Unit::Decibels, min: -60.0, max: 12.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "ceiling", name: "Ceiling", unit: Unit::Decibels, min: -24.0, max: 0.0, default: -1.0, curve: Curve::AmpGain },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }
    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }
    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        let vol = pedal_dsp::util::db_to_lin(ctx.param(0));
        let ceil = pedal_dsp::util::db_to_lin(ctx.param(1));
        let input = ctx.inputs[0];
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            out[i] = ceil * (input[i] * vol / ceil).tanh();
        }
        Ok(())
    }
    fn reset(&mut self) {}
}

/// A registry holding every block in this library — the palette.
pub fn builtin_registry() -> crate::Registry {
    let mut r = crate::Registry::new();
    r.add::<IoInput>();
    r.add::<IoOutput>();
    r.add::<Gain>();
    r.add::<MixSum>();
    r.add::<Sum>();
    r.add::<Master>();
    r.add::<Overdrive>();
    r.add::<Amp>();
    r.add::<Cab>();
    r.add::<ToneStack>();
    r.add::<Delay>();
    r.add::<PitchDelay>();
    r.add::<Reverb>();
    r.add::<Nam>();
    r
}

/// A four-input summer. The output stage of a multi-channel rig: every
/// channel master feeds one input, so the pedal's output is their sum.
#[derive(Default)]
pub struct Sum {
    levels: [f32; 4],
}

impl Block for Sum {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "util.sum",
            name: "Sum",
            category: Category::Split,
            inputs: vec![
                PortSpec::audio("a"),
                PortSpec::audio("b"),
                PortSpec::audio("c"),
                PortSpec::audio("d"),
            ],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "a", name: "A", unit: Unit::Ratio, min: 0.0, max: 1.0, default: 1.0, curve: Curve::Linear },
                ParamSpec { id: "b", name: "B", unit: Unit::Ratio, min: 0.0, max: 1.0, default: 1.0, curve: Curve::Linear },
                ParamSpec { id: "c", name: "C", unit: Unit::Ratio, min: 0.0, max: 1.0, default: 1.0, curve: Curve::Linear },
                ParamSpec { id: "d", name: "D", unit: Unit::Ratio, min: 0.0, max: 1.0, default: 1.0, curve: Curve::Linear },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }
    fn prepare(&mut self, _sr: f32, _max: usize) -> Result<(), BlockError> {
        Ok(())
    }
    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        for i in 0..4 {
            self.levels[i] = ctx.param(i);
        }
        let input = ctx.inputs;
        let out = &mut ctx.outputs[0][..];
        for i in 0..ctx.n {
            let mut s = 0.0;
            for (k, inp) in input.iter().enumerate() {
                if k < 4 {
                    s += inp.get(i).copied().unwrap_or(0.0) * self.levels[k];
                }
            }
            out[i] = s;
        }
        Ok(())
    }
    fn reset(&mut self) {}
}

// ---------------------------------------------------------------------------
// NAM (Neural Amp Modeler)
// ---------------------------------------------------------------------------

use std::cell::RefCell;

/// A Neural Amp Modeler model: the actual amp capture.
///
/// `nam-rs` does the inference (WaveNet / LSTM / A2 containers) with no
/// allocation on the audio path. The model is loaded at compile time and held
/// behind a `RefCell` because the `Block` trait's `process` takes `&mut self`
/// but the model is built once and only its streaming state changes.
///
/// **The model must be fed at its own sample rate.** A `.nam` captured at
/// 48 kHz fed 44.1 kHz produces silently wrong output. `prepare` records the
/// mismatch and the executor reports it; the UI shows it.
pub struct Nam {
    model: RefCell<Option<nam_rs::Model>>,
    nam: RefCell<Option<nam_rs::NamModel>>,
    in_gain: f32,
    out_level: f32,
    mix: f32,
    /// Model sample rate vs. the rate the block was prepared at.
    rate_mismatch: RefCell<Option<(f64, f32)>>,
    scratch: RefCell<Vec<f32>>,
}

impl Default for Nam {
    fn default() -> Self {
        Self {
            model: RefCell::new(None),
            nam: RefCell::new(None),
            in_gain: 0.0,
            out_level: 0.0,
            mix: 1.0,
            rate_mismatch: RefCell::new(None),
            scratch: RefCell::new(Vec::new()),
        }
    }
}

impl Nam {
    /// Load a `.nam` file. Call this before `prepare`; the model is built at
    /// prepare time (off the audio thread).
    pub fn load(&self, path: &str) -> Result<(), String> {
        let m = nam_rs::NamModel::from_file(path).map_err(|e| e.to_string())?;
        *self.nam.borrow_mut() = Some(m);
        Ok(())
    }

    /// Load from JSON text (used by tests and by the preset loader).
    pub fn load_json(&self, json: &str) -> Result<(), String> {
        let m = nam_rs::NamModel::from_json_str(json).map_err(|e| e.to_string())?;
        *self.nam.borrow_mut() = Some(m);
        Ok(())
    }

    /// Load a `.nam` file from disk. Convenience over reading +
    /// [`Self::load_json`]; kept here so the block owns its file format.
    pub fn load_file(&self, path: &str) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.load_json(&text)
    }

    pub fn is_loaded(&self) -> bool {
        self.nam.borrow().is_some()
    }

    /// The model's expected sample rate, if loaded.
    pub fn expected_rate(&self) -> Option<f64> {
        self.nam.borrow().as_ref().map(|m| m.expected_sample_rate())
    }

    /// The model's display name, if the file carries metadata.
    pub fn display_name(&self) -> String {
        self.nam
            .borrow()
            .as_ref()
            .map(|m| {
                let md = m.metadata_typed();
                md.name.unwrap_or_else(|| "NAM model".into())
            })
            .unwrap_or_else(|| "NAM (no model)".into())
    }
}

impl Block for Nam {
    fn spec(&self) -> BlockSpec {
        BlockSpec {
            id: "mod.nam",
            name: "NAM",
            category: Category::Modelling,
            inputs: vec![PortSpec::audio("in")],
            outputs: vec![PortSpec::audio("out")],
            params: vec![
                ParamSpec { id: "gain", name: "Input gain", unit: Unit::Decibels, min: -24.0, max: 24.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "level", name: "Output level", unit: Unit::Decibels, min: -30.0, max: 12.0, default: 0.0, curve: Curve::AmpGain },
                ParamSpec { id: "mix", name: "Mix", unit: Unit::Percent, min: 0.0, max: 100.0, default: 100.0, curve: Curve::Linear },
            ],
            controls_in: vec![],
            controls_out: vec![],
        }
    }

    fn prepare(&mut self, sr: f32, max_block: usize) -> Result<(), BlockError> {
        self.rate_mismatch.borrow_mut().take();
        let expected = match self.nam.borrow().as_ref() {
            Some(n) => n.expected_sample_rate(),
            None => {
                // No model: the block passes audio through, and the UI says so.
                return Ok(());
            }
        };
        let nam = self.nam.borrow();
        let nam = nam.as_ref().expect("checked above");
        let model = nam_rs::Model::from_nam(nam).map_err(|e| BlockError::Resource(e.to_string()))?;
        if (expected - sr as f64).abs() > 0.5 {
            *self.rate_mismatch.borrow_mut() = Some((expected, sr));
        }
        self.scratch.borrow_mut().resize(max_block, 0.0);
        *self.model.borrow_mut() = Some(model);
        Ok(())
    }

    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError> {
        self.in_gain = ctx.param(0);
        self.out_level = ctx.param(1);
        self.mix = ctx.param(2) / 100.0;
        let g = pedal_dsp::util::db_to_lin(self.in_gain);
        let out_gain = pedal_dsp::util::db_to_lin(self.out_level);
        let input = ctx.inputs[0];
        let out = &mut ctx.outputs[0][..];

        let mut model = self.model.borrow_mut();
        let Some(model) = model.as_mut() else {
            // No model loaded: pass through, so the rest of the rig still works.
            for i in 0..ctx.n {
                out[i] = input[i] * g;
            }
            return Ok(());
        };
        let mut scratch = self.scratch.borrow_mut();
        for i in 0..ctx.n {
            scratch[i] = input[i] * g;
        }
        model.process_buffer(&mut scratch[..ctx.n]);
        for i in 0..ctx.n {
            let wet = scratch[i] * out_gain;
            out[i] = pedal_dsp::util::mix(input[i] * g, wet, self.mix);
        }
        Ok(())
    }

    fn reset(&mut self) {
        if let Some(m) = self.model.borrow_mut().as_mut() {
            m.reset();
        }
    }

    fn configure(&mut self, key: &str, value: &str) -> Result<(), BlockError> {
        if key == "model" {
            self.load_file(value).map_err(BlockError::Resource)
        } else {
            Ok(())
        }
    }
}

