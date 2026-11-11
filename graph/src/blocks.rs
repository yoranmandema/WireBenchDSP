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
