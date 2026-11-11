//! The simulator's block library: re-exports the canonical I/O and mix blocks,
//! plus the demo blocks from `dsp-graph`.

pub use dsp_graph::blocks::{
    Delay, Gain, IoInput, IoOutput, MixSum, Overdrive, Reverb, ToneStack,
};
