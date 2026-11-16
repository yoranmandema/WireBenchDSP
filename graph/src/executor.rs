//! Compile pipeline and executor.
//!
//! Turns a `Graph` into a flat `Vec<Step>` over a pre-allocated buffer pool.
//! The audio callback walks the step list; it never traverses the graph.
//!
//! Pipeline: validate → topologically sort → **latency pass** → buffer plan →
//! emit. Cycles are rejected in v1; the loop pass is the next stage.
//!
//! **Parameter convention.** The graph stores parameters *normalised* (0..1),
//! which is what the UI and the JSON format want. Blocks read *real* values
//! (dB, Hz, ms, %). The executor denormalises at compile time and whenever a
//! parameter changes, so a block never has to know a curve exists.
//!
//! **Latency convention.** A block reports *unintentional* algorithmic delay
//! via `latency_samples()` — a FIR's group delay, a pitch shifter's window.
//! Intentional delay (a delay effect, a reverb tail) reports 0, because that
//! delay is the sound. The latency pass then delays the *faster* branches so
//! parallel paths recombine in phase.

use std::cell::UnsafeCell;
use std::collections::HashMap;

use crate::block::{Block, BlockCtx};
use crate::graph::{Graph, GraphError, NodeId};
use crate::registry::Registry;
use crate::types::{BlockError, BlockSpec, PortKind};

/// A reference to a buffer slot in the pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferRef(pub usize);

/// One step in the executor.
///
/// A step is either a node (`delay == None`) or a latency-compensation delay
/// line (`delay == Some(index)`). Both read and write the same pool.
#[derive(Debug)]
pub struct Step {
    /// Node id, or `"{node}.__delay__{port}"` for a compensation delay.
    pub node: NodeId,
    /// Input buffer for each audio input port, in spec order.
    pub inputs: Vec<BufferRef>,
    /// Output buffer for each audio output port, in spec order.
    pub outputs: Vec<BufferRef>,
    /// Index into the executor's node vec, if this step runs a node.
    pub node_index: Option<usize>,
    /// Index into the executor's delay vec, if this step is a delay line.
    pub delay: Option<usize>,
}

/// A fixed-length delay line used by the latency pass.
struct DelayLine {
    delay: usize,
    buf: Vec<f32>,
    write: usize,
}

impl DelayLine {
    fn new(delay: usize, max_block: usize) -> Self {
        let size = delay + max_block.max(1);
        Self {
            delay,
            buf: vec![0.0; size],
            write: 0,
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let n = input.len().min(output.len());
        let size = self.buf.len();
        for i in 0..n {
            self.buf[self.write] = input[i];
            // The sample `delay` positions back. When the write pointer is
            // behind `delay`, wrap into the tail of the ring.
            let read = if self.write >= self.delay {
                self.write - self.delay
            } else {
                self.write + size - self.delay
            };
            output[i] = self.buf[read];
            self.write = (self.write + 1) % size;
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }
}

/// The most audio ports one node may have on either side. The executor's
/// per-step port tables live on the stack at this size, so `run()` never
/// allocates. Checked at compile time.
pub const MAX_PORTS: usize = 8;

/// The buffer pool: a flat array where buffer `i` occupies
/// `[i * max_block .. (i + 1) * max_block]`.
///
/// Storage is `UnsafeCell<f32>`, so handing out `&mut [f32]` views through a
/// shared `&Pool` is interior mutability, not a write through a shared
/// reference. Exclusivity is the caller's job, and the planner guarantees it:
///
/// **A step's input and output buffer slots are always distinct**, because the
/// buffer planner gives every node output — and every delay output — its own
/// slot. So no mutable view created for a step overlaps any other view of that
/// step. Views never outlive the step that created them (except the final
/// output view, which is shared-only and returned after all steps ran).
pub struct Pool {
    data: Box<[UnsafeCell<f32>]>,
    max_block: usize,
}

impl Pool {
    fn new(slots: usize, max_block: usize) -> Self {
        let len = slots.max(1) * max_block;
        Self {
            data: (0..len).map(|_| UnsafeCell::new(0.0)).collect(),
            max_block,
        }
    }

    /// Number of allocated slots.
    pub fn slots(&self) -> usize {
        self.data.len() / self.max_block.max(1)
    }

    /// Raw pointer and length for slot `i`. Bounds are checked always — an
    /// out-of-range slot is a planner bug and must not become UB.
    fn raw(&self, i: usize, n: usize) -> (*mut f32, usize) {
        assert!(i < self.slots(), "buffer slot {i} out of range");
        let start = i * self.max_block;
        // `UnsafeCell<f32>` is `repr(transparent)`, so the cell array is an
        // f32 array, and `raw_get` yields a pointer we may write through.
        let ptr = UnsafeCell::raw_get(self.data[start..].as_ptr());
        (ptr, n.min(self.max_block))
    }

    /// Shared view of slot `i`, `n` frames.
    ///
    /// # Safety
    /// No mutable view of slot `i` may be live while this one is.
    unsafe fn view(&self, i: usize, n: usize) -> &[f32] {
        let (ptr, len) = self.raw(i, n);
        std::slice::from_raw_parts(ptr, len)
    }

    /// Mutable view of slot `i`, `n` frames.
    ///
    /// # Safety
    /// No other view (shared or mutable) of slot `i` may be live while this
    /// one is.
    #[allow(clippy::mut_from_ref)]
    unsafe fn view_mut(&self, i: usize, n: usize) -> &mut [f32] {
        let (ptr, len) = self.raw(i, n);
        std::slice::from_raw_parts_mut(ptr, len)
    }
}

/// A compiled, runnable graph.
pub struct Executor {
    nodes: Vec<Box<dyn Block>>,
    steps: Vec<Step>,
    /// Latency-compensation delay lines, indexed by `Step::delay`.
    delays: Vec<DelayLine>,
    /// Per-node specs, indexed by node position.
    specs: Vec<BlockSpec>,
    /// Per-node normalised params (0..1), indexed by node position.
    params_norm: Vec<Vec<f32>>,
    /// Per-node real params (dB, Hz, ...), indexed by node position.
    params_real: Vec<Vec<f32>>,
    /// Per-node output latency in samples, indexed by node position.
    out_latency: Vec<u32>,
    /// Node ids, indexed by node position (topological order).
    ids: Vec<NodeId>,
    /// Per-node bypass flags, indexed by node position.
    bypass: Vec<bool>,
    /// Nodes whose sample rate doesn't match the host: `(node id, expected, host)`.
    rate_mismatches: Vec<(NodeId, f64, f32)>,
    /// Position of the `io.output` node.
    output_node: usize,
    pool: Pool,
    /// The buffer slot that receives the raw input.
    input_slot: usize,
    /// The buffer slot that holds the graph's output.
    output_slot: usize,
    sample_rate: f32,
    max_block: usize,
    /// Blocks processed since compile/reset; passed to `BlockCtx`.
    block_index: u64,
}

impl std::fmt::Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor")
            .field("nodes", &self.node_count())
            .field("steps", &self.steps)
            .field("delays", &self.delays.len())
            .field("pool_slots", &self.pool.slots())
            .field("total_latency", &self.total_latency())
            .finish()
    }
}

/// Why compilation failed.
#[derive(Debug)]
pub enum CompileError {
    Graph(GraphError),
    Prepare { node: NodeId, error: BlockError },
    /// A node has more audio ports on one side than `MAX_PORTS`.
    TooManyPorts { node: NodeId, ports: usize },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Graph(g) => write!(f, "{g}"),
            CompileError::Prepare { node, error } => {
                write!(f, "prepare failed on node '{node}': {error}")
            }
            CompileError::TooManyPorts { node, ports } => write!(
                f,
                "node '{node}' has {ports} audio ports on one side; the limit is {MAX_PORTS}"
            ),
        }
    }
}

impl std::error::Error for CompileError {}

/// The plan for one input edge: where it reads from, and how much it must be
/// delayed to align with the node's other inputs.
struct EdgePlan {
    source_slot: usize,
    delay: usize,
    /// The delay line's own output slot, if `delay > 0`.
    delay_slot: Option<usize>,
}

impl Executor {
    /// Compile an owned graph: validate, sort, latency pass, buffer plan, emit.
    ///
    /// The graph is consumed; nodes are moved into the executor.
    pub fn compile_owned(
        graph: &mut Graph,
        sample_rate: f32,
        max_block: usize,
    ) -> Result<Self, CompileError> {
        graph.validate().map_err(CompileError::Graph)?;
        let order = graph.topo_sort().map_err(CompileError::Graph)?;
        Self::plan(graph, None, &order, sample_rate, max_block, true)
    }

    /// Compile a graph without consuming it: the nodes stay behind, so the same
    /// graph can be compiled again. Blocks are rebuilt from the registry, which
    /// is what makes this possible at all — a compiled executor owns its blocks.
    ///
    /// Used for the empty-rig fast path, where the tiny bypass graph is kept and
    /// recompiled whenever the audio thread needs a fresh copy of it.
    pub fn compile_borrowed(
        graph: &mut Graph,
        reg: &Registry,
        sample_rate: f32,
        max_block: usize,
    ) -> Result<Self, CompileError> {
        graph.validate().map_err(CompileError::Graph)?;
        let order = graph.topo_sort().map_err(CompileError::Graph)?;
        Self::plan(graph, Some(reg), &order, sample_rate, max_block, false)
    }

    /// Validate, sort, latency-pass, plan buffers and emit the step list.
    ///
    /// `take` decides whether the nodes are moved out of `graph` (owned) or
    /// rebuilt from `reg` (borrowed). Either way the executor owns its own
    /// `Box<dyn Block>` values.
    fn plan(
        graph: &mut Graph,
        reg: Option<&Registry>,
        order: &[NodeId],
        sample_rate: f32,
        max_block: usize,
        take: bool,
    ) -> Result<Self, CompileError> {
        // Snapshot specs, params, and each block's declared algorithmic
        // latency, in topological order.
        let mut specs = Vec::with_capacity(order.len());
        let mut params_norm = Vec::with_capacity(order.len());
        let mut params_real: Vec<Vec<f32>> = Vec::with_capacity(order.len());
        let mut own_latency = Vec::with_capacity(order.len());
        for id in order {
            let node = graph
                .node(id)
                .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
            specs.push(node.spec.clone());
            params_norm.push(node.params.clone());
            params_real.push(
                node.spec
                    .params
                    .iter()
                    .zip(&node.params)
                    .map(|(p, &norm)| p.denormalise(norm))
                    .collect(),
            );
            own_latency.push(node.block.latency_samples());
        }

        // The executor's per-step port tables are fixed-size.
        for (id, spec) in order.iter().zip(&specs) {
            let count = |ports: &[crate::types::PortSpec]| {
                ports.iter().filter(|p| p.kind == PortKind::Audio).count()
            };
            let ports = count(&spec.inputs).max(count(&spec.outputs));
            if ports > MAX_PORTS {
                return Err(CompileError::TooManyPorts { node: id.clone(), ports });
            }
        }

        // Move blocks out of the graph, or rebuild them, and prepare them.
        let mut nodes: Vec<Box<dyn Block>> = Vec::with_capacity(order.len());
        for (id, spec) in order.iter().zip(&specs) {
            let mut block = if take {
                graph
                    .take_node(id)
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?
                    .block
            } else {
                // A borrowed graph keeps its blocks, so the executor builds its
                // own from the registry — a fresh instance, no streaming state.
                reg.expect("a borrowed compile needs the registry")
                    .build(&spec.id)
                    .ok_or_else(|| CompileError::Prepare {
                        node: id.clone(),
                        error: BlockError::Other(format!(
                            "no factory for block type '{}'",
                            spec.id
                        )),
                    })?
            };
            block
                .prepare(sample_rate, max_block)
                .map_err(|error| CompileError::Prepare { node: id.clone(), error })?;
            nodes.push(block);
        }

        // Sample-rate mismatches, for the UI.
        let mut rate_mismatches = Vec::new();
        for (id, block) in order.iter().zip(&nodes) {
            if let Some((expected, host)) = block.rate_mismatch() {
                rate_mismatches.push((id.clone(), expected, host));
            }
        }

        // --- latency pass -------------------------------------------------
        // `out_latency[i]` = samples between node i's input and its output.
        // A node's inputs are aligned to the *slowest* incoming branch, so the
        // faster branches get a delay line.
        let mut out_latency = vec![0u32; order.len()];
        for (i, id) in order.iter().enumerate() {
            let mut target = 0u32;
            for edge in graph.edges() {
                if edge.to.0 == *id {
                    let src = order
                        .iter()
                        .position(|n| n == &edge.from.0)
                        .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                    target = target.max(out_latency[src]);
                }
            }
            out_latency[i] = target + own_latency[i];
        }

        // Index edges by their destination port, for the buffer plan.
        let mut edge_src: HashMap<(NodeId, String), (NodeId, String)> = HashMap::new();
        for edge in graph.edges() {
            edge_src.insert((edge.to.0.clone(), edge.to.1.clone()), edge.from.clone());
        }

        // --- buffer plan --------------------------------------------------
        // One slot per node audio output, then one slot per compensation delay.
        let mut slot_of: HashMap<(NodeId, String), usize> = HashMap::new();
        let mut next_slot = 0usize;
        for (i, id) in order.iter().enumerate() {
            for port in &specs[i].outputs {
                if port.kind != PortKind::Audio {
                    continue;
                }
                slot_of.insert((id.clone(), port.name.to_string()), next_slot);
                next_slot += 1;
            }
        }

        // Plan each input edge: read from the source's output slot, delay by
        // the difference to the node's target latency.
        let mut edge_plan: HashMap<(NodeId, String), EdgePlan> = HashMap::new();
        for (i, id) in order.iter().enumerate() {
            let mut target = 0u32;
            for port in &specs[i].inputs {
                if port.kind != PortKind::Audio {
                    continue;
                }
                let from = edge_src
                    .get(&(id.clone(), port.name.to_string()))
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                let src = order
                    .iter()
                    .position(|n| n == &from.0)
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                target = target.max(out_latency[src]);
            }

            for port in &specs[i].inputs {
                if port.kind != PortKind::Audio {
                    continue;
                }
                let from = edge_src
                    .get(&(id.clone(), port.name.to_string()))
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                let src = order
                    .iter()
                    .position(|n| n == &from.0)
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                let source_slot = *slot_of
                    .get(&(from.0.clone(), from.1.to_string()))
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                let delay = (target - out_latency[src]).max(0) as usize;
                let delay_slot = if delay > 0 {
                    let s = next_slot;
                    next_slot += 1;
                    Some(s)
                } else {
                    None
                };
                edge_plan.insert(
                    (id.clone(), port.name.to_string()),
                    EdgePlan {
                        source_slot,
                        delay,
                        delay_slot,
                    },
                );
            }
        }

        // --- emit ---------------------------------------------------------
        // Delay steps are emitted immediately before their destination node,
        // which is valid because the source always precedes it topologically.
        let mut steps = Vec::with_capacity(order.len());
        let mut delays: Vec<DelayLine> = Vec::new();
        for (i, id) in order.iter().enumerate() {
            // Compensation delays first.
            for port in &specs[i].inputs {
                if port.kind != PortKind::Audio {
                    continue;
                }
                let plan = &edge_plan[&(id.clone(), port.name.to_string())];
                if let Some(ds) = plan.delay_slot {
                    let di = delays.len();
                    delays.push(DelayLine::new(plan.delay, max_block));
                    steps.push(Step {
                        node: format!("{}.__delay__{}", id, port.name),
                        inputs: vec![BufferRef(plan.source_slot)],
                        outputs: vec![BufferRef(ds)],
                        node_index: None,
                        delay: Some(di),
                    });
                }
            }

            // Then the node itself.
            let mut inputs = Vec::new();
            for port in &specs[i].inputs {
                if port.kind != PortKind::Audio {
                    continue;
                }
                let plan = &edge_plan[&(id.clone(), port.name.to_string())];
                inputs.push(BufferRef(plan.delay_slot.unwrap_or(plan.source_slot)));
            }
            let mut outputs = Vec::new();
            for port in &specs[i].outputs {
                if port.kind != PortKind::Audio {
                    continue;
                }
                let slot = *slot_of
                    .get(&(id.clone(), port.name.to_string()))
                    .ok_or(CompileError::Graph(GraphError::Cycle(Vec::new())))?;
                outputs.push(BufferRef(slot));
            }
            steps.push(Step {
                node: id.clone(),
                inputs,
                outputs,
                node_index: Some(i),
                delay: None,
            });
        }

        // Locate the I/O nodes by type, not by position: a dangling branch
        // (a block whose output goes nowhere yet) can sort after the output.
        // `validate()` guarantees exactly one of each.
        let find = |kind: &str| {
            specs
                .iter()
                .position(|s| s.id == kind)
                .ok_or_else(|| CompileError::Graph(GraphError::Io(format!("no {kind} node"))))
        };
        let input_node = find("io.input")?;
        let output_node = find("io.output")?;
        let input_slot = *slot_of
            .get(&(order[input_node].clone(), "out".to_string()))
            .ok_or_else(|| CompileError::Graph(GraphError::Io("io.input has no 'out' port".into())))?;
        let output_slot = {
            let plan = edge_plan
                .get(&(order[output_node].clone(), "in".to_string()))
                .ok_or_else(|| CompileError::Graph(GraphError::Io("io.output has no 'in' port".into())))?;
            plan.delay_slot.unwrap_or(plan.source_slot)
        };

        let pool = Pool::new(next_slot, max_block);

        Ok(Self {
            nodes,
            steps,
            delays,
            specs,
            params_norm,
            params_real,
            out_latency,
            bypass: vec![false; order.len()],
            rate_mismatches: rate_mismatches,
            ids: order.to_vec(),
            output_node,
            pool,
            input_slot,
            output_slot,
            sample_rate,
            max_block,
            block_index: 0,
        })
    }

    /// Node ids in executor order; the position is the `node` index used by
    /// [`set_param_at`](Self::set_param_at) and [`set_bypass_at`](Self::set_bypass_at).
    pub fn node_ids(&self) -> &[NodeId] {
        &self.ids
    }

    /// Set a parameter by index, real value. Real-time safe (no lookup, no
    /// allocation) — this is what the audio thread calls when the UI turns a
    /// knob. Returns false if either index is out of range.
    pub fn set_param_at(&mut self, node: usize, param: usize, real: f32) -> bool {
        let Some(spec) = self.specs.get(node).and_then(|s| s.params.get(param)) else {
            return false;
        };
        let norm = spec.normalise(real);
        let real = spec.denormalise(norm); // clamp into range
        self.params_norm[node][param] = norm;
        self.params_real[node][param] = real;
        true
    }

    /// Bypass a node: its first input is copied to every output. Real-time
    /// safe. Note the latency pass does not re-run, so bypassing a block that
    /// declares latency shifts its branch earlier by that amount.
    pub fn set_bypass_at(&mut self, node: usize, on: bool) -> bool {
        match self.bypass.get_mut(node) {
            Some(b) => {
                *b = on;
                true
            }
            None => false,
        }
    }

    /// The index of a node in the executor, by id.
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.steps
            .iter()
            .find(|s| s.delay.is_none() && s.node == id)
            .and_then(|s| s.node_index)
    }

    /// Set a parameter to a real value (denormalised by the spec).
    pub fn set_param_real(
        &mut self,
        node_id: &str,
        param: &str,
        value: f32,
    ) -> Result<(), BlockError> {
        let idx = self
            .node_index(node_id)
            .ok_or_else(|| BlockError::Other(format!("unknown node: {node_id}")))?;
        let pidx = self.specs[idx]
            .param_index(param)
            .ok_or_else(|| BlockError::Other(format!("unknown param: {param}")))?;
        self.params_norm[idx][pidx] = self.specs[idx].params[pidx].normalise(value);
        self.params_real[idx][pidx] = value;
        Ok(())
    }

    /// Set a parameter to a normalised value.
    pub fn set_param_norm(
        &mut self,
        node_id: &str,
        param: &str,
        value: f32,
    ) -> Result<(), BlockError> {
        let idx = self
            .node_index(node_id)
            .ok_or_else(|| BlockError::Other(format!("unknown node: {node_id}")))?;
        let pidx = self.specs[idx]
            .param_index(param)
            .ok_or_else(|| BlockError::Other(format!("unknown param: {param}")))?;
        let v = value.clamp(0.0, 1.0);
        self.params_norm[idx][pidx] = v;
        self.params_real[idx][pidx] = self.specs[idx].params[pidx].denormalise(v);
        Ok(())
    }

    /// Read a parameter's current real value.
    pub fn param_real(&self, node_id: &str, param: &str) -> Option<f32> {
        let idx = self.node_index(node_id)?;
        let pidx = self.specs[idx].param_index(param)?;
        self.params_real[idx].get(pidx).copied()
    }

    /// Read a parameter's current normalised value (0..1). Hosts that expose
    /// parameters as automation (VST3, CLAP) work in normalised space.
    pub fn param_norm(&self, node_id: &str, param: &str) -> Option<f32> {
        let idx = self.node_index(node_id)?;
        let pidx = self.specs[idx].param_index(param)?;
        self.params_norm[idx].get(pidx).copied()
    }

    /// Static specs in executor order, parallel to [`node_ids`](Self::node_ids).
    pub fn specs(&self) -> &[BlockSpec] {
        &self.specs
    }

    /// The graph's total output latency in samples.
    ///
    /// This is the delay between the input and the output of the whole graph.
    /// It is inherent to the blocks chosen (a FIR, a pitch shifter) plus any
    /// compensation needed to keep parallel branches in phase.
    pub fn total_latency(&self) -> u32 {
        self.out_latency.get(self.output_node).copied().unwrap_or(0)
    }

    /// Nodes that need a different sample rate than the host runs at.
    pub fn rate_mismatches(&self) -> &[(NodeId, f64, f32)] {
        &self.rate_mismatches
    }

    /// The output latency of one node, in samples.
    pub fn node_latency(&self, id: &str) -> Option<u32> {
        let idx = self.node_index(id)?;
        self.out_latency.get(idx).copied()
    }

    /// Run one block. Returns the graph's output, `n` frames (clamped to
    /// `max_block` and to `input.len()`).
    ///
    /// Real-time safe: no allocation, no locks. Port tables are fixed-size
    /// stack arrays; buffers are views into the pre-allocated pool. The pool
    /// is not cleared — every block must write all `n` frames of every output
    /// (see `Block::process`), and the input slot is overwritten here.
    pub fn run(&mut self, input: &[f32], n: usize) -> Result<&[f32], BlockError> {
        let n = n.min(self.max_block).min(input.len());
        let pool = &self.pool;

        // SAFETY: no views exist yet.
        unsafe { pool.view_mut(self.input_slot, n) }.copy_from_slice(&input[..n]);

        for step in &self.steps {
            let ni = step.inputs.len();
            let no = step.outputs.len();
            let mut ins: [&[f32]; MAX_PORTS] = [&[]; MAX_PORTS];
            let mut outs: [&mut [f32]; MAX_PORTS] = Default::default();
            // SAFETY: the planner gives every output its own slot, distinct
            // from every input of the same step, so the one mutable view per
            // output slot is exclusive. Inputs may share a slot with each
            // other (fan-in of one source) — those are shared views, fine.
            // All views drop at the end of this iteration.
            for (k, r) in step.inputs.iter().enumerate() {
                ins[k] = unsafe { pool.view(r.0, n) };
            }
            for (k, r) in step.outputs.iter().enumerate() {
                outs[k] = unsafe { pool.view_mut(r.0, n) };
            }

            if let Some(di) = step.delay {
                self.delays[di].process(ins[0], &mut outs[0][..]);
                continue;
            }

            let Some(idx) = step.node_index else {
                return Err(BlockError::Param("step has neither node nor delay"));
            };
            if self.bypass[idx] {
                for out in outs[..no].iter_mut() {
                    match ins[..ni].first() {
                        Some(src) => out.copy_from_slice(src),
                        None => out.fill(0.0),
                    }
                }
                continue;
            }

            let mut ctx = BlockCtx {
                inputs: &ins[..ni],
                outputs: &mut outs[..no],
                params: &self.params_real[idx],
                controls_in: &[],
                controls_out: &mut [],
                n,
                sample_rate: self.sample_rate,
                block_index: self.block_index,
            };
            self.nodes[idx].process(&mut ctx)?;

        }

        self.block_index = self.block_index.wrapping_add(1);
        // SAFETY: all step views have dropped; this is the only view.
        Ok(unsafe { pool.view(self.output_slot, n) })
    }

    /// Clear all node and delay state. Called on preset change and bypass.
    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.reset();
        }
        for d in &mut self.delays {
            d.reset();
        }
        self.block_index = 0;
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn delay_count(&self) -> usize {
        self.delays.len()
    }

    pub fn pool_slots(&self) -> usize {
        self.pool.slots()
    }
}
