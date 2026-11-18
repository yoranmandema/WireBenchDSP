//! Engine lifecycle tests: the engine owns its audio streams, IPC listener and
//! shared state, and tears them down deterministically on `Engine::stop()` /
//! `Drop`. No `std::process::exit` in any normal path; closing the parent (the
//! control pipe EOF) stops the engine as well.
//!
//! Run: `cargo test -p pedal-engine --test lifecycle`

use std::net::Shutdown;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::io::Write;

use pedal_engine::lifecycle::{Engine, EngineArgs};
use tokio::net::TcpListener;

/// Resolve the built `pedal-engine` binary. The test target's `--bin` env var is
/// not set for arbitrary `--test` targets, so resolve it from `CARGO_TARGET_DIR`
/// (agent-env.sh points it at the shared build directory).
fn engine_bin() -> std::path::PathBuf {
    if let Ok(t) = std::env::var("CARGO_TARGET_DIR") {
        return std::path::PathBuf::from(t).join("debug").join("pedal-engine");
    }
    let mut p = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    p.push("target/debug/pedal-engine");
    p
}

/// The default headless instance arguments used by the process-level tests.
fn headless_args(instance: &str, port: u16) -> EngineArgs {
    EngineArgs {
        instance: instance.to_string(),
        port,
        control_pipe: None,
        pidfile: None,
        log: None,
        bench: None,
        headless: true,
        rig: "04-rainbow".to_string(),
        migrate_presets: false,
    }
}

// ---------------------------------------------------------------------------
// 1. Single instance: `stop()` starts the teardown; `run()` finishes it.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn single_instance_shuts_down_cleanly() {
    let mut engine = Engine::new(&headless_args("t1", 0)).await.unwrap();
    assert_eq!(engine.status(), pedal_engine::lifecycle::EngineStatus::Running);

    engine.stop().unwrap();
    // Teardown order: streams -> listener -> Shared. `run` returns on the
    // shutdown branch of the `tokio::select!`.
    assert!(engine.run().await.is_ok());
    assert_eq!(engine.status(), pedal_engine::lifecycle::EngineStatus::Stopped);
    drop(engine); // release the tuner thread handle
}

// ---------------------------------------------------------------------------
// 2. Two instances: independent ports, pidfiles and teardown.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_instances_run_concurrently_and_stop() {
    let a = headless_args("a", 0);
    let b = headless_args("b", 0);
    let mut ea = Engine::new(&a).await.unwrap();
    let mut eb = Engine::new(&b).await.unwrap();
    assert_eq!(ea.status(), pedal_engine::lifecycle::EngineStatus::Running);
    assert_eq!(eb.status(), pedal_engine::lifecycle::EngineStatus::Running);

    ea.stop().unwrap();
    eb.stop().unwrap();
    assert!(ea.run().await.is_ok());
    assert!(eb.run().await.is_ok());
    assert_eq!(ea.status(), pedal_engine::lifecycle::EngineStatus::Stopped);
    assert_eq!(eb.status(), pedal_engine::lifecycle::EngineStatus::Stopped);
}

// ---------------------------------------------------------------------------
// 3. Stale engine: a port already held cannot be taken by a new instance.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stale_engine_cannot_take_a_port() {
    // Occupy an ephemeral port with a dummy listener.
    let dummy = TcpListener::bind("127.0.0.1:0").await.expect("bind dummy");
    let port = dummy.local_addr().unwrap().port();

    let mut cmd = Command::new(engine_bin());
    cmd.arg("--headless")
        .arg("--port")
        .arg(port.to_string())
        .arg("--instance")
        .arg("stale")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd.output().expect("spawn");
    assert!(!out.status.success(), "a stale engine must not start");
    assert_eq!(out.status.code(), Some(1), "stale engine exits 1");
}

// ---------------------------------------------------------------------------
// 4. Parent death: closing the control pipe makes the engine tear down and
//    exit 0 (the same path the shell uses on window close).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn parent_death_stops_the_engine() {
    let control_path = std::env::temp_dir().join(format!(
        "t_9e90c111_control_{}",
        std::process::id()
    ));
    let listener = UnixListener::bind(&control_path).expect("control pipe");
    let cleanup = || std::fs::remove_file(&control_path);

    // The stderr file must not live on the control-pipe socket path above.
    let stderr_path = std::env::temp_dir().join(format!(
        "t_9e90c111_stderr_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    let stderr_file = std::fs::File::create(&stderr_path).expect("stderr file");
    let cleanup_stderr = || { let _ = std::fs::remove_file(&stderr_path); };
    let mut cmd = Command::new(engine_bin());
    cmd.arg("--headless")
        .arg("--port")
        .arg("0")
        .arg("--instance")
        .arg("cp")
        .arg("--control-pipe")
        .arg(&control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr_file));
    let mut child = cmd.spawn().expect("spawn engine");

    // The engine is a client of the control pipe; the test is the listener
    // (the shell's side). Accept once, then close the socket: EOF -> teardown.
    eprintln!("PT: before accept");
    let (_socket, _addr) = listener.accept().expect("engine connected");
    eprintln!("PT: accepted control socket");
    drop(_socket);
    let out = child.wait_with_output().expect("wait");
    eprintln!("PT: child exited {:?}", out.status);
    eprintln!("PARENT_DEATH_TEST: child exited with {:?}, stderr:", out.status);
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "engine should exit 0 after parent death");
    let err = std::fs::read_to_string(&stderr_path)
        .expect("stderr file should be readable");
    assert!(err.contains("control pipe")
        || err.contains("closed")
        || err.contains("EOF"), "expected the control-pipe teardown path: {err}");
    cleanup();
    cleanup_stderr();
}

// ---------------------------------------------------------------------------
// 5. Graceful shutdown request: writing `engine.wait` over the control pipe
//    asks the engine to tear itself down and exit 0 (the same command the
//    shell sends, via `EngineHandle::stop`).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn engine_wait_stops_the_engine_cleanly() {
    let control_path = std::env::temp_dir().join(format!(
        "t_9e90c111_wait_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    let listener = UnixListener::bind(&control_path).expect("control pipe");
    let stderr_path = std::env::temp_dir().join(format!(
        "t_9e90c111_wait_stderr_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    let stderr_file = std::fs::File::create(&stderr_path).expect("stderr file");
    let mut cmd = Command::new(engine_bin());
    cmd.arg("--headless")
        .arg("--port")
        .arg("0")
        .arg("--instance")
        .arg("wait")
        .arg("--control-pipe")
        .arg(&control_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn engine");
    // The engine is a client of the control pipe; the test is the listener.
    // Accept the engine's connection, then drive it through the same
    // shutdown path the shell uses: send the `engine.wait` command and close
    // the write end (EOF) so the engine tears itself down and exits 0.
    let mut stream = listener.accept().expect("engine connected").0;
    stream.write_all(b"engine.wait\n").expect("write shutdown command");
    stream.shutdown(Shutdown::Both).ok();
    let out = child
        .wait_with_output()
        .expect("wait with output");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "engine should exit 0 after `engine.wait`, got {:?}\nstderr: {err}",
        out.status
    );
    assert!(
        err.contains("ready"),
        "engine should have printed `ready` before exiting: {err}"
    );
    let _ = std::fs::remove_file(&control_path);
    let _ = std::fs::remove_file(&stderr_path);
}
