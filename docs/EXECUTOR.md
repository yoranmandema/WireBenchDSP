# Executor: Compile Pipeline, Buffer Planner, Audio Callback

Status: draft v0.1 — design before code.
Depends on: `product-model.md` (nodes, edges, params, latency).
Goal: a graph you can edit freely, and an audio callback that allocates nothing.

---

## 1. The two worlds

| | Graph (edit time) | Executor (audio time) |
|---|---|---|
| When it runs | UI thread, worker thread | Audio thread |
| Data structure | Nodes + edges, pointer-based | Flat step list, index-based |
| Allocations | Freely | Never |
| Latency | Unknown / being edited | Fixed at compile |
| Can fail? | Yes — validation errors | No — compiled already |

**The rule: everything that can fail or allocate happens at compile time. The
audio callback only walks a pre-built list.**

The audio thread never sees a `Node`, an `Edge`, or a `Box<dyn Trait>`. It sees
a `Vec<Step>` and a `Vec<Buffer>`.

---

## 2. Compile pipeline

Six stages. Each is a pure function of the graph; each can be unit-tested.

```
Graph ──► 1. Validate ──► 2. Resolve cycles ──► 3. Topo sort
                              │
                              ▼
                    4. Latency pass ──► 5. Buffer plan ──► 6. Emit
                                                              │
                                                              ▼
                                                        Executor
```

### 2.1 Validate

Reject at compile time, with a useful message:

- An input port with no incoming edge where one is required.
- A type mismatch (audio into control, gate into audio).
- A node whose `prepare()` fails (missing IR file, corrupt model).
- A parameter value outside its declared range.
- A sample rate the node can't handle.

**Output:** `Result<Graph, Vec<GraphError>>`. The UI shows the errors inline next
to the offending node — this is a product feature, not just a debug aid.

### 2.2 Resolve cycles

Feedback loops are allowed (decision #4), but each loop must contain exactly
one **delay-owning node** — reverb, delay, looper. That node owns the feedback
buffer and declares how much delay it contributes.

Algorithm:

1. Find strongly connected components (Tarjan's algorithm).
2. For each SCC with more than one node, verify it contains exactly one
   delay-owning node. If not, reject with an error naming the loop.
3. Break each loop at the delay node's output. The delay node's feedback input
   is fed from a dedicated feedback buffer that the executor carries across
   blocks.

This is what makes reverb and feedback legal without making arbitrary cycles
legal. A cycle with no delay would be a comb filter with infinite gain at some
frequency — it has to be rejected.

### 2.3 Topological sort

Order nodes so every node runs after all its sources. With cycles broken at the
delay nodes, the graph is a DAG and a standard Kahn or DFS sort works.

**Output:** `Vec<NodeId>` in execution order.

### 2.4 Latency pass

This is the stage that makes parallel branches phase-coherent.

1. Each node reports `latency_samples()`.
2. Process nodes in topological order. For each node:
   - `in_latency = max(latency of each incoming edge's source, plus that
     source's latency)`
   - If the node has multiple inputs, `target = max(all in_latencies)`.
   - Insert a **delay line** on every input edge whose latency is below
     `target`, of length `target - in_latency`.
   - `node.out_latency = target + node.own_latency`.
3. The compensation delay lines are ordinary buffers in the buffer plan.

**Result:** every node sees time-aligned inputs, and every path from input to
output has the same total latency. Parallel branches recombine in phase.

> Implementation note: a delay line on an edge is just a circular buffer of
> `n` samples. For small `n` it's cheap; for large `n` it's still just a
> circular buffer. The cost is memory, not CPU.

**Recompile trigger:** latency changes when a node's latency changes — e.g.
swapping a cab IR, or a node that changes its look-ahead. The compile pipeline
re-runs; the swap happens at a block boundary.

### 2.5 Buffer plan

Goal: **zero allocation in the callback.** Pre-allocate every buffer.

Strategy:

1. Walk the topological order. Each node writes to one or more output buffers.
2. Use **in-place processing where possible**: if a node has exactly one input
   and one output, and its `process` supports in-place, it reads and writes the
   same buffer. Most effects can do this (gain, EQ, distortion, compressor).
3. Otherwise allocate a **scratch buffer** from a fixed pool.
4. Track buffer lifetimes with a simple linear-scan allocator: a buffer is born
   at the step that writes it and dies at the last step that reads it. Reuse
   buffers whose lifetimes don't overlap.

**Output:** a `Vec<Buffer>` of fixed size (`max_block_size` frames), each with a
slot index, plus a mapping from (node, port) → buffer slot.

The pool is allocated once at compile time. The callback indexes into it.

**Why this matters:** a `Vec` push or a `Box` allocation in the audio callback
can hit the allocator, take a lock, and overrun the block deadline. That's an
xrun. Pre-allocating makes the callback's worst case bounded.

### 2.6 Emit

Produce the flat step list the callback runs:

```
struct Step {
    node_idx: u16,
    inputs: [BufferRef; N_IN],     // fixed-size array, e.g. 4
    outputs: [BufferRef; N_OUT],   // fixed-size array, e.g. 4
    control_ins: [ControlRef; K],  // params + modulation sources
    control_outs: [ControlRef; M],
}
```

Fixed-size arrays, not `Vec`. The step is a plain struct the callback can
iterate without bounds checks in release mode.

The executor is then:

```
for step in steps {
    node = &mut nodes[step.node_idx]
    node.process(step.inputs, step.outputs, step.control_ins, step.control_outs)
}
```

No graph traversal, no dynamic dispatch beyond the node trait object, no
allocation.

---

## 3. The audio callback

### 3.1 Threading model

- **One audio thread**, pinned to an isolated core (e.g. core 2 or 3).
- **SCHED_FIFO** priority, `mlockall(MCL_CURRENT | MCL_FUTURE)`.
- The thread owns the ALSA PCM handles and the executor.
- No other thread touches the executor or the buffers.

### 3.2 The loop

```
loop {
    // 1. Wait for ALSA to have a full capture period and a free playback period
    snd_pcm_wait(capture, timeout)

    // 2. Read the input period (mmap, zero copy)
    input_ptr = snd_pcm_mmap_capture_begin(period)

    // 3. Copy input into the executor's input buffer
    //    (or, if the interface allows, point the first step at the mmap buffer)
    copy(input_ptr, executor.input_buffer)

    // 4. Advance the ALSA pointers
    snd_pcm_mmap_commit(capture, period)

    // 5. Run the executor — this is the only DSP work
    executor.run()

    // 6. Write the output period (mmap, zero copy)
    output_ptr = snd_pcm_mmap_playback_begin(period)
    copy(executor.output_buffer, output_ptr)
    snd_pcm_mmap_commit(playback, period)

    // 7. Swap in a pending new executor, if the UI built one
    if pending_swap { executor = pending_swap.take(); }

    // 8. Drain the parameter queue (see §4)
    apply_pending_params()
}
```

**Step 7 is the hot-swap.** The UI thread builds a new executor and publishes it
through an atomic pointer. The audio thread takes it at a block boundary. The
old executor is dropped on the UI thread (or deferred to a cleanup queue).

### 3.3 Block size and periods

- Block size: 64 frames (decision #3).
- ALSA periods: 2 or 3. For USB interfaces, 3 is safer (more slack for the USB
  scheduler). For I2S, 2 is fine.
- The ALSA period size can be larger than the DSP block size — the callback runs
  the executor `period_size / block_size` times per wakeup. With 64-frame blocks
  and 128-frame periods, that's 2 executor runs per wakeup.

**Why not make the ALSA period equal the DSP block?** Because ALSA wakeups are
not free, and a larger period gives the USB scheduler more slack. The DSP block
size and the ALSA period are independent knobs.

---

## 4. Parameter path: UI → audio

The parameter system is where most real-time bugs live. The rule:

**The UI never writes a parameter directly. It pushes a change into a lock-free
queue. The audio thread drains the queue once per block and applies the values.**

### 4.1 The queue

A single-producer / single-consumer ring buffer (or a `crossbeam` queue):

```
struct ParamChange {
    node_idx: u16,
    param_idx: u16,
    value: f32,          // already in the param's internal units
    smoothing: Smoothing, // how to apply it
}
```

The UI thread pushes; the audio thread pops. No locks, no allocation (fixed
capacity, e.g. 256 entries; if full, drop the oldest — a dropped knob move is
better than an xrun).

### 4.2 Applying changes

Once per block, before running the steps:

1. Drain the queue into a small array.
2. For each change, look up the target node and param.
3. Set the param's target value.
4. The node's `process` reads the *smoothed* value, which advances one step per
   block.

Smoothing is a one-pole per param:

```
smoothed += (target - smoothed) * coef
coef = 1 - exp(-2π * smoothing_freq / sample_rate)
```

Where `smoothing_freq` is derived from the param's declared time constant (e.g.
5 ms for gain, 20 ms for filter frequency to avoid zipper noise).

**Key detail:** the smoothing coefficient depends on the *block size*, not the
sample count. If the block size changes, recompute the coefficient at compile
time. This is the pitfall the `embedded-audio-dsp` skill calls out: a one-pole
coefficient must use the sample rate, not the buffer length.

### 4.3 Modulation

Modulation (LFO → gain, envelope → cutoff) is resolved at compile time into the
same per-block read. The modulation source is a node's control output; the
modulation destination is a param. The executor computes the modulated value
before the node runs:

```
value = base_value + modulation_amount * control_source_value
```

No extra allocations — the control values live in a fixed array indexed by the
step's `control_ins`.

---

## 5. Hot-swap: changing the graph while playing

The UI builds a new graph. The compile pipeline runs on a worker thread. The
result is a new `Executor`. The swap:

1. UI thread: build new executor, publish via `ArcSwap<Executor>` or an atomic
   pointer.
2. Audio thread: at a block boundary, take the new executor.
3. The new executor's nodes are `reset()` (clear state) and `prepare()`d before
   the swap, so the first block is already valid.
4. The old executor is dropped on the UI thread.

**Click-free swap:** the new executor starts from a clean state, so there's a
discontinuity. Two options:

- **Hard swap** — acceptable for preset changes where a click is expected
  (like a real pedalboard).
- **Crossfade** — run both executors for a few blocks and crossfade the outputs.
  Costs 2× CPU for the crossfade duration. Worth it for scene changes.

Decision: hard swap for preset changes, crossfade for scene changes (where the
routing is the same and only the params differ — in that case, just apply the
param changes and don't rebuild the graph at all).

**Insight:** a scene change that keeps the same routing doesn't need a new
executor. Only parameter values change. So scenes are cheap; presets are the
expensive ones.

---

## 6. What the audio callback may never do

A checklist, enforced by code review and by the type system where possible:

- No allocation (`Vec::push`, `Box::new`, `String`, `format!`).
- No locks (no `Mutex`, no `RwLock`).
- No file I/O.
- No syscalls other than the ALSA mmap calls.
- No `unwrap()` / `expect()` on anything that can fail.
- No floating-point exceptions (denormals — flush to zero).
- No logging (no `println!`, no `log::info!`).

**Denormals matter.** A long feedback tail can denormalise the FPU and cost
100× on some cores. Enable flush-to-zero (FTZ) and denormals-are-zero (DAZ) at
startup.

---

## 7. Failure modes and how the design handles them

| Failure | Handling |
|---|---|
| Node overruns the block | The callback detects it (cycle counter) and logs; the block is late but not lost. Tune the graph or the block size. |
| ALSA xrun | Log it, continue. A few xruns are tolerable; many mean the CPU budget is wrong. |
| Param queue full | Drop the oldest change. Better a missed knob than an xrun. |
| Compile fails | The old executor keeps running. The UI shows the error. |
| Node panics | Catch at the callback boundary; fall back to bypass. A bad node must not take down the audio. |
| IR file missing | Compile fails; the old executor keeps running. |

**The old executor keeps running** is the key resilience rule. A bad edit never
silences the pedal.

---

## 8. Testing the executor

The executor is the most testable part of the system, because it's pure:

1. **Unit: compile a graph, assert the step order and buffer assignments.**
2. **Unit: latency compensation — two branches with different delays recombine
   in phase.** Test with an impulse and check the output is a single impulse.
3. **Unit: buffer plan — no two live buffers share a slot.**
4. **Integration: run a graph over a WAV file in the host simulator and compare
   to a reference.**
5. **Bench: measure cycles per block for a given graph; assert it's under
   budget.**
6. **Stress: hot-swap the executor 1,000 times while running; assert no xruns.**

The host simulator runs the *same* executor code as the Pi. That's the whole
point of compiling to a flat step list — it's platform-independent.

---

## 9. Open details to settle

1. **In-place vs scratch.** Which nodes support in-place? Default to yes for
   single-in/single-out, no otherwise. Measure the win.
2. **Buffer pool sizing.** Worst case is a graph with many parallel branches.
   Size the pool for the largest graph the UI allows, or cap the graph size.
3. **Crossfade length.** 5 ms? 20 ms? Make it a setting.
4. **Param queue capacity.** 256 is a starting point; measure the UI's burst
   rate.
5. **ALSA period vs DSP block.** Start with 128-frame periods and 64-frame DSP
   blocks; measure on hardware.
6. **Thread affinity.** Audio on core 2, UI on core 0, control on core 1. Core 3
   free for the OS. Confirm on the Pi 5 (4 cores).
