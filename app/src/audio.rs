//! The real-time engine: owns the executor and runs it in the audio callback.
//!
//! ## Threading
//!
//! * The **input callback** pushes mono samples into a lock-free ring.
//! * The **output callback** drains the ring, runs the executor, and writes
//!   the interleaved result. No mutex, no allocation, no file I/O.
//! * The UI thread sends [`Cmd`]s over a lock-free queue and applies them
//!   inside the callback. The old executor goes back on a "trash" queue so
//!   it's freed by the UI thread, never in the callback.
//!
//! ## Graph swaps
//!
//! Compiling an executor takes milliseconds, so the UI builds it and sends it
//! over the queue. Until the audio thread takes it, the current one keeps
//! running (parameter changes still reach it), so a preset switch never
//! produces a gap.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rtrb::{Consumer, Producer, RingBuffer};

use dsp_graph::blocks::builtin_registry;
use dsp_graph::{Executor, Registry};

pub const MAX_BLOCK: usize = 64;

/// f32 in an atomic. Peaks only ever rise, so no CAS loop is needed.
#[allow(dead_code)]
pub fn read_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

#[derive(Default)]
pub struct Meters {
    pub in_peak: AtomicU32,
    pub out_peak: AtomicU32,
    /// Longest callback duration in the last second, seconds.
    pub callback_s: AtomicU32,
    pub xruns: AtomicUsize,
    pub out_frames: AtomicUsize,
    #[allow(dead_code)]
    pub dropped_executors: AtomicUsize,
}

/// Commands from the UI to the audio thread.
pub enum Cmd {
    /// Replace the graph. `bypass` pairs `(node_id, on)`.
    Swap(Executor, Vec<(String, bool)>),
    /// Set one param by index. Real-time safe.
    SetParam { node: usize, param: usize, value: f32 },
    SetBypass { node: usize, on: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    /// Sum the first N input channels (the interface's real inputs).
    First(usize),
    /// One specific input channel.
    #[allow(dead_code)]
    One(usize),
}

#[allow(dead_code)]
#[inline]
pub fn pick_channel(frame: &[f32], mode: ChannelMode) -> f32 {
    match mode {
        ChannelMode::First(n) => frame.iter().take(n).sum(),
        ChannelMode::One(i) => frame.get(i).copied().unwrap_or(0.0),
    }
}

/// Mono sample in, interleaved frames out.
#[inline]
pub fn spread(s: f32, frame: &mut [f32]) {
    match frame.len() {
        0 => {}
        1 => frame[0] = s,
        _ => {
            frame[0] = s;
            frame[1] = s;
        }
    }
}

#[derive(Clone, Copy)]
pub struct Setup {
    pub sample_rate: f32,
    pub out_channels: usize,
    pub block: usize,
    #[allow(dead_code)]
    pub channel_mode: ChannelMode,
    #[allow(dead_code)]
    pub latency_frames: usize,
}

/// What the audio thread owns.
pub struct AudioThread {
    pub exec: Option<Executor>,
    pub scratch_in: Vec<f32>,
    /// Executor output, mono, before interleaving.
    scratch_mono: Vec<f32>,
    pub scratch_out: Vec<f32>,
    pub block: usize,
    pub setup: Setup,
    pub meters: Arc<Meters>,
    cmd_rx: Consumer<Cmd>,
    trash_tx: Producer<Executor>,
    #[allow(dead_code)]
    ring: Ring,
    running: Arc<AtomicBool>,
    source: Arc<AtomicU8>,
    riff: Option<crate::store::Riff>,
}

/// SPSC ring of mono samples.
///
/// # Safety
/// `push` is called only by the input callback, `pop` only by the output
/// callback, and the write/read indices carry the happens-before edge.
pub struct Ring {
    data: Box<[UnsafeCell<f32>]>,
    mask: usize,
    write: AtomicUsize,
    read: AtomicUsize,
}

// SAFETY: see the struct docs — the access pattern is strictly SPSC.
unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(1024);
        Self {
            data: (0..cap).map(|_| UnsafeCell::new(0.0)).collect(),
            mask: cap - 1,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    /// Push one mono sample. Called by the input callback.
    pub fn push(&self, s: f32) {
        let w = self.write.load(Ordering::Relaxed);
        // SAFETY: SPSC — only the input callback writes, and only to the slot
        // it just claimed. `UnsafeCell` makes the write through `&self` sound.
        unsafe { *self.data[w & self.mask].get() = s };
        self.write.store(w.wrapping_add(1), Ordering::Release);
    }

    /// Pop up to `out.len()` samples; underrun yields silence and sets `starved`.
    pub fn pop(&self, out: &mut [f32]) -> bool {
        let w = self.write.load(Ordering::Acquire);
        let mut r = self.read.load(Ordering::Relaxed);
        let mut starved = false;
        for o in out.iter_mut() {
            if r != w {
                // SAFETY: SPSC — the write index was Acquire-loaded, so the
                // slot has been published by the producer.
                *o = unsafe { *self.data[r & self.mask].get() };
                r = r.wrapping_add(1);
            } else {
                *o = 0.0;
                starved = true;
            }
        }
        self.read.store(r, Ordering::Release);
        starved
    }

    pub fn available(&self) -> usize {
        self.write.load(Ordering::Acquire).wrapping_sub(self.read.load(Ordering::Relaxed))
    }

    /// Drop samples so `target` remain buffered.
    pub fn trim_to(&self, target: usize) {
        let w = self.write.load(Ordering::Acquire);
        let r = self.read.load(Ordering::Relaxed);
        let level = w.wrapping_sub(r);
        let keep = level.min(target);
        self.read.store(w.wrapping_sub(keep), Ordering::Release);
    }
}

impl AudioThread {
    pub fn new(
        setup: Setup,
        meters: Arc<Meters>,
        cmd_rx: Consumer<Cmd>,
        trash_tx: Producer<Executor>,
        running: Arc<AtomicBool>,
        source: Arc<AtomicU8>,
    ) -> Self {
        let block = setup.block;
        Self {
            exec: None,
            scratch_in: vec![0.0; block],
            scratch_mono: vec![0.0; block],
            scratch_out: vec![0.0; block * setup.out_channels.max(1)],
            block,
            setup,
            meters,
            cmd_rx,
            trash_tx,
            ring: Ring::new(8192),
            running,
            source,
            riff: None,
        }
    }

    /// Install an executor directly (used by the headless self-test).
    pub fn set_executor(&mut self, ex: Executor) {
        self.exec = Some(ex);
    }

    pub fn source(&self) -> u8 {
        self.source.load(Ordering::Relaxed)
    }

    pub fn set_block(&mut self, block: usize) {
        self.block = block.min(MAX_BLOCK).max(1);
        self.scratch_in.resize(self.block, 0.0);
        self.scratch_mono.resize(self.block, 0.0);
        // `scratch_out` holds interleaved frames: block * channels.
        self.scratch_out.resize(self.block * self.setup.out_channels.max(1), 0.0);
    }

    /// Copy mono samples into the engine's input scratch.
    pub fn copy_input(&mut self, mono: &[f32]) {
        let n = mono.len().min(self.block);
        self.scratch_in[..n].copy_from_slice(&mono[..n]);
    }

    /// Fill `scratch_in[..n]` with the selected source.
    pub fn fill_source(&mut self, n: usize) {
        match self.source.load(Ordering::Relaxed) {
            1 => {} // live input: the ring already holds it
            2 => self.scratch_in[..n].fill(0.0),
            _ => {
                if self.riff.is_none() {
                    self.riff = Some(crate::store::Riff::new(self.setup.sample_rate));
                }
                if let Some(r) = &mut self.riff {
                    for s in self.scratch_in[..n].iter_mut() {
                        *s = r.tick(self.setup.sample_rate);
                    }
                }
            }
        }
    }

    fn apply(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Swap(ex, bypass) => {
                if let Some(old) = self.exec.replace(ex) {
                    if self.trash_tx.push(old).is_err() {
                        // The UI is gone; drop it here. Not RT-safe, but there
                        // is no other option once the consumer is closed.
                        self.meters.dropped_executors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let ids = self.exec.as_ref().map(|e| e.node_ids().to_vec()).unwrap_or_default();
                if let Some(ex) = &mut self.exec {
                    for (id, on) in bypass {
                        if let Some(idx) = ids.iter().position(|n| n == &id) {
                            ex.set_bypass_at(idx, on);
                        }
                    }
                }
            }
            Cmd::SetParam { node, param, value } => {
                if let Some(ex) = &mut self.exec {
                    ex.set_param_at(node, param, value);
                }
            }
            Cmd::SetBypass { node, on } => {
                if let Some(ex) = &mut self.exec {
                    ex.set_bypass_at(node, on);
                }
            }
        }
    }

    /// Drain pending commands.
    fn poll(&mut self) {
        while let Ok(c) = self.cmd_rx.pop() {
            self.apply(c);
        }
    }

    /// Process one output callback: `n` mono frames in `scratch_in`, result in
    /// `scratch_out` as `n * out_channels` interleaved frames.
    pub fn render(&mut self, n: usize) -> &[f32] {
        let start = Instant::now();
        self.poll();
        if !self.running.load(Ordering::Relaxed) {
            self.scratch_out[..n * self.setup.out_channels].fill(0.0);
            return &self.scratch_out[..n * self.setup.out_channels];
        }
        let block = self.block;
        let mut peak = 0.0f32;
        let mut done = 0;
        while done < n {
            let take = (n - done).min(block);
            let src = self.run_chunk(take);
            // Copy out of the executor's buffer before touching `self` again.
            let ncpy = src.len().min(take);
            let mut tmp = [0.0f32; MAX_BLOCK];
            tmp[..ncpy].copy_from_slice(&src[..ncpy]);
            for &s in tmp[..ncpy].iter() {
                peak = peak.max(s.abs());
            }
            self.scratch_mono[done..done + ncpy].copy_from_slice(&tmp[..ncpy]);
            done += take;
        }
        self.interleave(n);
        self.meters.out_peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.meters.out_frames.fetch_add(n, Ordering::Relaxed);
        self.meters.callback_s.store(start.elapsed().as_secs_f32().to_bits(), Ordering::Relaxed);
        &self.scratch_out[..n * self.setup.out_channels]
    }

    /// Run one chunk through the executor. `scratch_in[..n]` is the input.
    fn run_chunk(&mut self, n: usize) -> &[f32] {
        match &mut self.exec {
            Some(ex) => {
                let out = ex.run(&self.scratch_in[..n], n).unwrap_or(&[]);
                out
            }
            None => &[],
        }
    }

    /// Mono `scratch_in[..n]` → interleaved `scratch_out`.
    fn interleave(&mut self, n: usize) {
        let ch = self.setup.out_channels.max(1);
        for i in 0..n {
            spread(self.scratch_mono[i], &mut self.scratch_out[i * ch..(i + 1) * ch]);
        }
    }
}

/// UI-side handle: everything the audio thread must not touch.
pub struct Handle {
    pub node_ids: Arc<Mutex<Vec<String>>>,
    pub registry: Arc<Registry>,
    pub meters: Arc<Meters>,
    pub cmd_tx: Arc<Mutex<Producer<Cmd>>>,
    pub trash_rx: Arc<Mutex<Consumer<Executor>>>,
    pub setup: Arc<Mutex<Setup>>,
    pub sample_rate: f32,
    /// 0 = riff, 1 = live input, 2 = silence.
    pub source: Arc<AtomicU8>,
}

impl Handle {
    /// Free executors the audio thread finished with. UI thread only.
    pub fn collect_trash(&self) {
        while self.trash_rx.lock().unwrap().pop().is_ok() {}
    }

    pub fn push(&self, cmd: Cmd) -> Result<(), String> {
        self.cmd_tx.lock().unwrap().push(cmd).map_err(|e| e.to_string())
    }

    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.node_ids.lock().unwrap().iter().position(|n| n == id)
    }

    pub fn set_source(&self, s: u8) {
        self.source.store(s, Ordering::Relaxed);
    }

    pub fn block(&self) -> usize {
        self.setup.lock().unwrap().block
    }
}

pub fn new_engine(
    setup: Setup,
    source: Arc<AtomicU8>,
) -> (Handle, Consumer<Cmd>, Producer<Executor>, Arc<AtomicBool>) {
    let (cmd_tx, cmd_rx) = RingBuffer::<Cmd>::new(64);
    let (trash_tx, trash_rx) = RingBuffer::<Executor>::new(8);
    let meters = Arc::new(Meters::default());
    let running = Arc::new(AtomicBool::new(true));
    let registry = Arc::new(builtin_registry());
    let handle = Handle {
        cmd_tx: Arc::new(Mutex::new(cmd_tx)),
        node_ids: Arc::new(Mutex::new(Vec::new())),
        registry,
        meters: meters.clone(),
        trash_rx: Arc::new(Mutex::new(trash_rx)),
        setup: Arc::new(Mutex::new(setup)),
        sample_rate: setup.sample_rate,
        source,
    };
    (handle, cmd_rx, trash_tx, running)
}
