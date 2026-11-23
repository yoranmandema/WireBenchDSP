//! Engine-owned lifecycle: every engine instance is a single owner of its
//! audio streams, IPC listener, shared state and executor. No global
//! singletons; each instance is independent and is torn down deterministically
//! by `Engine::stop()` (explicit, idempotent) and `Drop` (best-effort).
//!
//! Ownership contract
//! -----------------
//! * `Engine::new` builds the instance and returns it. The caller (the shell,
//!   via `engine/src/main.rs`) owns the `Engine` and drops it on exit.
//! * `Engine::stop()` is the explicit shutdown API: it sets `running=false` and
//!   signals the main loop. It is idempotent (a second call returns immediately).
//! * `Drop` runs the same teardown best-effort: drops the streams (stops the
//!   audio device), drops the listener (stops accepting), drops `Shared` (frees
//!   state, executor, rings), then the tuner thread handle.
//! * Parent death is detected with a control pipe: the shell keeps one end open
//!   and passes the other to the engine; on EOF the engine runs the same teardown
//!   and (via `main`) exits 0.
//! * `engine.wait` (a control message over the IPC socket) is the explicit,
//!   idempotent API for a clean shutdown: it sets `running=false`, wakes the main
//!   loop, and the process exits 0. `Engine::stop()` is idempotent.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::audio::{new_engine, Handle, Setup};
use crate::headless;
use crate::server::{serve_listener, Shared};
use crate::store::Store;

/// Loopback-only default port for the shipped single-instance app (`docs/IPC-PROTOCOL.md`).
pub const PORT: u16 = 8737;

/// Default pidfile path for an instance: `/tmp/pedal-engine-<instance>.pid`.
pub fn default_pidfile(instance: &str) -> PathBuf {
    PathBuf::from("/tmp").join(format!("pedal-engine-{instance}.pid"))
}

/// Default log path for an instance: `$PEDAL_ENGINE_LOG`, else
/// `/tmp/pedal-engine-<instance>.log`.
pub fn default_log_path(instance: &str) -> PathBuf {
    if let Ok(p) = std::env::var("PEDAL_ENGINE_LOG") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("/tmp").join(format!("pedal-engine-{instance}.log"))
}

/// The engine's lifecycle state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineStatus {
    /// The engine is running and serving clients.
    Running,
    /// The engine is stopped (graceful or via `engine.wait`).
    Stopped,
}

impl EngineStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineStatus::Running => "running",
            EngineStatus::Stopped => "stopped",
        }
    }
}

/// Audio streams owned by the engine. They are kept alive for the lifetime of
/// the process and dropped on shutdown so the device is released.
///
/// The device-backed case owns the cpal input + output streams. The headless
/// case owns nothing here (its device is a simulated thread owned by the
/// engine process and released when the process exits).
pub enum AudioStreams {
    Device {
        input: cpal::Stream,
        output: cpal::Stream,
    },
}

impl AudioStreams {
    /// The device-backed streams.
    pub fn device(input: cpal::Stream, output: cpal::Stream) -> Self {
        Self::Device { input, output }
    }
}

/// Arguments the engine process accepts at startup. All new per-instance
/// flags are additive to the existing surface (the shipped app keeps the old
/// defaults: port 8737, no flags).
#[derive(Clone, Debug)]
pub struct EngineArgs {
    pub instance: String,
    pub port: u16,
    pub control_pipe: Option<PathBuf>,
    pub pidfile: Option<PathBuf>,
    pub log: Option<PathBuf>,
    pub bench: Option<f32>,
    pub headless: bool,
    pub rig: String,
    pub migrate_presets: bool,
    /// Headless input source: 0 riff (default), 1 live tone, 2 silence.
    pub source: u8,
}

impl EngineArgs {
    /// Parse the command line into per-instance values.
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let flag = |name: &str| args.iter().any(|a| a == name);
        let value = |name: &str| {
            args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
        };
        Self {
            instance: value("--instance").unwrap_or_else(Self::default_instance_id),
            port: value("--port")
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(PORT),
            control_pipe: value("--control-pipe").map(PathBuf::from),
            pidfile: value("--pidfile").map(PathBuf::from),
            log: value("--log").map(PathBuf::from),
            bench: value("--bench")
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|s| *s > 0.0),
            headless: flag("--headless"),
            rig: value("--rig").unwrap_or_else(|| "04-rainbow".into()),
            migrate_presets: flag("--migrate-presets"),
            source: match value("--source").as_deref() {
                Some("live") => 1,
                Some("silence") => 2,
                _ => 0,
            },
        }
    }

    fn default_instance_id() -> String {
        // `<pid>-<random16>`, hex.
        let pid = std::process::id() as u64;
        let mut state = pid.wrapping_mul(0x9E3779B97F4A7C15);
        state ^= state >> 33;
        state = state.wrapping_mul(0xC870C39B3C2325B1);
        let mut out = [0u8; 16];
        for b in out.iter_mut() {
            *b = (state & 0xFF) as u8;
            state = state.wrapping_mul(0x9E3779B97F4A7C15);
        }
        // The 16 bytes are random non-UTF-8, so render them as a hex string.
        let hex: String = out.iter().map(|b| format!("{:02x}", b)).collect();
        format!("{}\n", hex)
    }

    /// Where the engine writes its pidfile and prints the `ready` line. The
    /// shell coordinates the `ready` line with port allocation.
    pub fn pidfile(&self) -> PathBuf {
        self.pidfile.clone().unwrap_or_else(|| default_pidfile(&self.instance))
    }

    pub fn log_path(&self) -> PathBuf {
        self.log.clone().unwrap_or_else(|| default_log_path(&self.instance))
    }
}

/// An engine process instance: owns the audio streams, the listener and the
/// shared state, and tears everything down in the right order.
pub struct Engine {
    pub sh: Arc<Shared>,
    /// The audio thread's running flag. `false` starts the shutdown sequence.
    pub running: Arc<AtomicBool>,
    /// Ordering handles the teardown of the audio device and the listener.
    pub streams: Option<AudioStreams>,
    pub listener: Option<TcpListener>,
    /// The detached tuner thread. It is detached on purpose (like the existing
    /// code); the process exits when `main` returns, and the OS reclaims all
    /// threads.
    pub tuner: Option<thread::JoinHandle<()>>,
    /// The engine wakes its main loop via the `shutdown` field of `Shared`.
    stopped: AtomicBool,
    /// Per-instance identity and endpoints.
    pub instance: String,
    pub port: u16,
    /// The simulated headless device (only used when `--headless`).
    pub headless: Option<headless::Headless>,
}

impl Engine {
    /// Construct a new engine instance from parsed arguments.
    ///
    /// This does the per-instance setup: instance identity, pidfile, the
    /// `ready` log line, the initial rig, the audio streams, the IPC listener,
    /// the tuner thread and the control-pipe watcher. It does not block on the
    /// accept loop.
    ///
    /// # Errors
    ///
    /// Returns an error if the pidfile cannot be written. A failure to bind
    /// the listener is reported as an error from `run`, not from `new`.
    pub async fn new(args: &EngineArgs) -> std::io::Result<Self> {
        let instance = args.instance.clone();
        let pid = std::process::id() as u32;
        let port = args.port;

        // 1. Per-instance pidfile + `ready` line (before the listener binds).
        let pidfile = args.pidfile();
        if !pidfile.as_os_str().is_empty() {
            write_pidfile(&pidfile, pid)?;
        }
        eprintln!("pedal-engine: ready pid={pid} port={port} instance={instance}");

        // 2. Initial rig (compiled before the streams start).
        let config = crate::host::Config::low_latency();
        let devices = if args.headless {
            None
        } else {
            Some(crate::host::pick(Some(&config)))
        };
        let setup = Setup {
            sample_rate: devices
                .as_ref()
                .map(|d| d.rate as f32)
                .unwrap_or(48_000.0),
            out_channels: devices
                .as_ref()
                .map(|d| d.out_channels)
                .unwrap_or(2),
            block: crate::host::BLOCK,
            channel_mode: crate::audio::ChannelMode::First(2),
            latency_frames: crate::host::LATENCY_FRAMES,
        };
        let source = Arc::new(AtomicU8::new(2)); // silence by default

        let (mut handle, cmd_rx, trash_tx, running) = new_engine(setup, source.clone());
        let store = Store::discover();
        handle.preset_dir = store.dir.clone();
        let registry = handle.registry.clone();

        // Compile and install the initial rig.
        let doc = store
            .presets
            .iter()
            .find(|(n, _)| *n == args.rig)
            .map(|(_, d)| d.clone())
            .or_else(|| store.presets.first().map(|(_, d)| d.clone()))
            .unwrap_or_else(|| crate::channels::Rig::new("Untitled").to_doc());
        let rig = crate::channels::Rig::from_doc(&doc);
        {
            let resolver = crate::model_index::resolver();
            let mut g = rig.to_doc().build_in_with(&handle.registry, &handle.preset_dir, &resolver)
                .unwrap_or_else(|e| {
                    eprintln!("pedal-engine: could not build initial rig: {e}");
                    std::process::exit(1);
                });
            rig.apply_routings(&mut g, &handle.registry)
                .unwrap_or_else(|e| {
                    eprintln!("pedal-engine: rig routings: {e}");
                    std::process::exit(1);
                });
            let ex = dsp_graph::Executor::compile_owned(&mut g, setup.sample_rate, crate::host::BLOCK)
                .unwrap_or_else(|e| {
                    eprintln!("pedal-engine: initial rig compile failed: {e}");
                    std::process::exit(1);
                });
            *handle.node_ids.lock().unwrap() = ex.node_ids().to_vec();
            handle
                .push(crate::audio::Cmd::Swap(ex, Vec::new()))
                .unwrap_or_else(|e| eprintln!("pedal-engine: initial swap failed: {e}"));
        }
        // Silence on a real device (the riff is opt-in); headless defaults to
        // the riff so a bench measures a real load.
        handle.set_source(if args.headless { args.source } else { 2 });

        // 3. Per-instance log path (the shell also sets PEDAL_ENGINE_LOG).
        let _log = args.log_path();

        // 4. Audio streams (owned by Engine for headless; device-backed for live).
        let (streams, tuner_ring);
        let mut headless_device;
        if args.headless {
            let headless = crate::headless::start(
                setup,
                cmd_rx,
                trash_tx,
                running.clone(),
                handle.meters.clone(),
                handle.source.clone(),
            );
            tuner_ring = headless.tuner_ring.clone();
            streams = None;
            // The headless device is owned by `Engine` and dropped with it.
            headless_device = Some(headless);
        } else {
            match crate::host::start(
                &devices.as_ref().expect("audio device"),
                &config,
                cmd_rx,
                trash_tx,
                running.clone(),
                handle.meters.clone(),
                source.clone(),
            ) {
                Ok((s, tr)) => {
                    streams = Some(AudioStreams::device(s.input, s.output));
                    tuner_ring = tr;
                }
                Err(e) => {
                    eprintln!("pedal-engine: failed to start audio streams: {e}");
                    std::process::exit(1);
                }
            }
            headless_device = None;
        }

        // 5. IPC shared state.
        let running_flag = running.clone();
        let models = Arc::new(crate::models::Models::new());
        let sh = Arc::new(Shared::with_running(handle, registry, source, store, models.clone(), running_flag));
        // Install the compiled initial rig (built from the preset named by
        // `--rig`, or the compiled-in default `04-rainbow`). The empty rig
        // `Rig::default()` only supplies slot ids; the shipped presets own
        // content, so the UI's `state` must report the preset's channels.
        *sh.rig.lock().unwrap() = rig;
        // The initial rig is compiled from the rig named by `--rig` (or the
        // compiled-in default `04-rainbow`); report it as the current preset.
        *sh.preset.lock().unwrap() = args.rig.clone();
        // `self.running` keeps the flag; `sh.running` mirrors it so the server
        // (`apply`) can observe it.

        // 6. Shutdown signal: the main loop wakes on `shutdown`, woken by
        //    `engine.wait` (handled in `server.rs`'s `apply()`) or parent death
        //    (control-pipe EOF).
        // 7. Parent-death detection: read the control pipe until EOF.
        if let Some(cp) = args.control_pipe.clone() {
            let sh = sh.clone();
            thread::spawn(move || {
                if let Err(e) = read_control_pipe(&cp) {
                    eprintln!("pedal-engine: control pipe {:?}: {e}", cp);
                }
                // The shell is gone (EOF or unreachable): same teardown as
                // `engine.wait`.
                sh.running.store(false, Ordering::Relaxed);
                sh.shutdown.notify_one();
            });
        }

        // 8. IPC listener + tuner thread.
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        eprintln!("pedal-engine: ipc listening on ws://127.0.0.1:{port}");
        let tuner = crate::tuner::spawn(tuner_ring, setup.sample_rate, sh.handle.meters.clone());

        Ok(Self {
            sh,
            running,
            streams,
            listener: Some(listener),
            tuner: Some(tuner),
            stopped: AtomicBool::new(false),
            instance,
            port,
            headless: headless_device,
        })
    }

    /// Graceful shutdown: idempotent. Starts the sequence; the actual teardown
    /// happens in `run` when the shutdown signal fires the main loop.
    pub fn stop(&self) -> Result<ExitStatus, String> {
        if self.stopped.swap(true, Ordering::Relaxed) {
            return Ok(ExitStatus::default());
        }
        self.running.store(false, Ordering::Relaxed);
        // Wake the main loop so the streams/listener are dropped.
        self.sh.shutdown.notify_one();
        Ok(ExitStatus::default())
    }

    /// Current lifecycle state.
    pub fn status(&self) -> EngineStatus {
        if self.stopped.load(Ordering::Relaxed) {
            EngineStatus::Stopped
        } else if self.running.load(Ordering::Relaxed) {
            EngineStatus::Running
        } else {
            EngineStatus::Stopped
        }
    }

    /// The main loop: serve clients until the listener is dropped or the
    /// shutdown signal fires, then tear everything down and return.
    pub async fn run(&mut self) -> Result<(), String> {
        let listener = self.listener.take().expect("listener set in new()");
        // `serve_listener` runs the accept loop. When the shutdown signal fires
        // we drop the listener, which makes `accept()` return and the loop
        // unwind, releasing the port and clients.
        tokio::select! {
            res = serve_listener(self.sh.clone(), listener) => {
                if let Err(e) = res {
                    eprintln!("pedal-engine: ipc: {e}");
                }
                Ok(())
            }
            _ = self.sh.shutdown.notified() => {
                // Teardown order: streams first (stops the device and the
                // callback), then the listener (stops accepting clients),
                // then Shared (frees state, executor, rings). `self` itself
                // is dropped by the caller, releasing the tuner thread.
                self.streams = None;
                self.listener = None;
                Ok(())
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Best-effort teardown: never panics, never calls `exit()`.
        // Order: streams -> listener -> Shared (the struct fields are
        // dropped in declaration order, which is the order we want)
        // -> tuner (dropped last as it is the last field).
        self.streams = None;
        self.listener = None;
    }
}

/// Write a pidfile containing `pid`, creating parent directories as needed.
pub fn write_pidfile(path: &Path, pid: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{pid}\n"))?;
    Ok(())
}

/// Wait for the parent to die: returns when the control pipe is closed (EOF)
/// or an error occurs. Called in a background thread.
pub fn read_control_pipe(path: &Path) -> std::io::Result<()> {
    let mut stream = std::os::unix::net::UnixStream::connect(path)?;
    let mut buf = [0u8; 1];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return Ok(()), // EOF: the shell closed its write end
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}
