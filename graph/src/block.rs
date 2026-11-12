//! The `Block` trait — the entire plugin interface.
//!
//! An author implements four methods and gets, for free: parameter smoothing,
//! a generated UI card, JSON serialisation, MIDI learn, latency compensation,
//! and a host-simulation test harness.

use crate::types::{BlockError, BlockSpec};

/// Everything a block needs to process one block of audio.
///
/// Slices into the executor's pre-allocated buffer pool. A block never
/// allocates and never blocks.
pub struct BlockCtx<'a> {
    /// Audio inputs, one slice per input port, `n` frames each.
    pub inputs: &'a [&'a [f32]],
    /// Audio outputs, one slice per output port, `n` frames each.
    pub outputs: &'a mut [&'a mut [f32]],
    /// Smoothed parameter values, indexed by parameter position in the spec.
    pub params: &'a [f32],
    /// Control input values (LFO, envelope, expression pedal).
    pub controls_in: &'a [f32],
    /// Control output values (envelope follower, gate).
    pub controls_out: &'a mut [f32],
    /// Frames in this block.
    pub n: usize,
    pub sample_rate: f32,
    pub block_index: u64,
}

impl<'a> BlockCtx<'a> {
    /// Read a parameter by index.
    pub fn param(&self, index: usize) -> f32 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    /// Read a parameter as dB, converted to linear gain.
    pub fn param_db(&self, index: usize) -> f32 {
        pedal_dsp::util::db_to_lin(self.param(index))
    }
}

/// A processing node.
pub trait Block: Send {
    /// Static description: identity, ports, params.
    fn spec(&self) -> BlockSpec;

    /// Allocate and compute coefficients. Called at compile time, never in the
    /// audio callback. This is where IRs load and models parse.
    fn prepare(&mut self, sample_rate: f32, max_block: usize) -> Result<(), BlockError>;

    /// Process one block. Must not allocate, must not panic, must not block.
    ///
    /// **Must write all `ctx.n` frames of every audio output.** The executor
    /// does not zero buffers between blocks, so an unwritten frame carries
    /// stale audio from the previous block.
    fn process(&mut self, ctx: &mut BlockCtx<'_>) -> Result<(), BlockError>;

    /// Clear all internal state. Called on preset change and on bypass.
    fn reset(&mut self);

    /// Algorithmic delay in samples. Used by the latency pass.
    /// Default 0 — a causal effect declares nothing.
    fn latency_samples(&self) -> u32 {
        0
    }

    /// If this block needs a different sample rate than the host runs at,
    /// return `(expected_hz, host_hz)`. The executor collects these and the UI
    /// shows them. Default `None`.
    fn rate_mismatch(&self) -> Option<(f64, f32)> {
        None
    }

    /// Configure the block from its preset data. Called at build time, before
    /// [`prepare`](Self::prepare), never on the audio thread. The default
    /// ignores the value; blocks that load resources (a NAM model, a cab IR)
    /// override it.
    fn configure(&mut self, _key: &str, _value: &str) -> Result<(), BlockError> {
        Ok(())
    }
}

/// A block that can be constructed by the registry.
pub trait BlockFactory: Send {
    fn spec(&self) -> BlockSpec;
    fn build(&self) -> Box<dyn Block>;
}

/// Adapt a concrete block type into a `BlockFactory`.
pub struct FnFactory<F, B> {
    spec_fn: F,
    build_fn: fn() -> B,
}

impl<F, B> FnFactory<F, B> {
    pub const fn new(spec_fn: F, build_fn: fn() -> B) -> Self {
        Self { spec_fn, build_fn }
    }
}

impl<F, B> BlockFactory for FnFactory<F, B>
where
    F: Fn() -> BlockSpec + Send,
    B: Block + Default + 'static,
{
    fn spec(&self) -> BlockSpec {
        (self.spec_fn)()
    }
    fn build(&self) -> Box<dyn Block> {
        Box::new((self.build_fn)())
    }
}
