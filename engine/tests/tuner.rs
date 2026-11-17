//! End-to-end tuner tests.
//!
//! These tests boot the real `pedal-engine` binary with `--headless --source live`,
//! connect over a real WebSocket, and check the `event: tuner` stream. Nothing is
//! ever sent to a speaker: the headless device discards its output.
//!
//! The engine's input callback feeds the tuner ring in the same branch that feeds
//! the audio ring, so a 220 Hz tone produces `event: tuner` with `note: "A"` and
//! `octave: 3` within a second of connecting.

use std::net::{SocketAddr, TcpListener};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

const BIN: &str = env!("CARGO_BIN_EXE_pedal-engine");
const TIMEOUT: Duration = Duration::from_secs(30);
/// The client waits at most this long for the first `event: tuner`.
const TUNER_TIMEOUT: Duration = Duration::from_secs(10);

/// An ephemeral port the engine can bind.
fn free_port() -> u16 {
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("valid addr");
    let probe = TcpListener::bind(addr).expect("probe bind");
    let port = probe.local_addr().expect("probe addr").port();
    drop(probe);
    port
}

/// Wait until the engine is accepting connections on the port.
async fn wait_for_listen(port: u16) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "engine never listened on port {port}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Spawn the real engine on a free port.
fn spawn_engine(port: u16) -> std::process::Child {
    Command::new(BIN)
        .args(["--headless", "--source", "live", "--port", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pedal-engine")
}

/// The next text frame, skipping ping/pong/close.
async fn next_text<S>(ws: &mut S) -> String
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => return t.to_string(),
            Some(Ok(_)) => continue,
            other => panic!("websocket closed or errored: {other:?}"),
        }
    }
}

/// The next `event` frame of the given kind, or `None` if the socket closed or
/// errored.
async fn next_event_opt<S>(ws: &mut S, event: &str) -> Option<Value>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    loop {
        let text = match ws.next().await {
            Some(Ok(Message::Text(t))) => t.to_string(),
            Some(Ok(_)) => continue,
            _ => return None,
        };
        let v: Value = serde_json::from_str(&text).expect("json");
        if v["type"] == "event" && v["event"] == event {
            return Some(v);
        }
    }
}

/// The next `event` frame of the given kind (panics on closed/errored).
async fn next_event<S>(ws: &mut S, event: &str) -> Value
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    loop {
        let v: Value = serde_json::from_str(&next_text(ws).await).expect("json");
        if v["type"] == "event" && v["event"] == event {
            return v;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tuner_event_on_live_headless_source() {
    let port = free_port();
    let mut child = spawn_engine(port);
    wait_for_listen(port).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("ws connect");

    // Greet with the state snapshot.
    let state = next_event(&mut ws, "state").await;
    assert_eq!(state["payload"]["preset"], "04-rainbow");

    // Within 10 s, receive a *first* tuner reading for the 220 Hz headless
    // tone whose cents are within tolerance (the first active reading can
    // start its window with pre-fill zeros, so the first reading is often a
    // bit off; keep waiting until a clean reading arrives).
    let deadline = Instant::now() + TUNER_TIMEOUT;
    let mut found = None;
    loop {
        if Instant::now() >= deadline {
            panic!("no usable tuner event within {TUNER_TIMEOUT:?}");
        }
        if let Some(v) = next_event_opt(&mut ws, "tuner").await {
            let payload = &v["payload"];
            if payload["active"] == true {
                let cents = payload["cents"].as_f64().unwrap_or(0.0);
                if cents.abs() < 10.0 {
                    found = Some(v);
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let v: Value = found.unwrap();
    let payload = &v["payload"];
    assert!(
        payload["active"] == true,
        "active was {}",
        payload["active"]
    );
    assert_eq!(payload["note"], "A", "note was {}", payload["note"]);
    assert_eq!(payload["octave"], 3, "octave was {}", payload["octave"]);
    assert!(
        payload["cents"]
            .as_f64()
            .unwrap_or(0.0)
            .abs()
            < 10.0,
        "cents was {}",
        payload["cents"]
    );
    assert!(
        payload["clarity"].as_f64().unwrap_or(0.0) > 0.5,
        "clarity was {}",
        payload["clarity"]
    );

    // Kill the child and await it. The engine is force-killed with SIGKILL,
    // so it exits with status 9 (not success) — that is the expected outcome.
    child.kill().expect("kill engine");
    let out = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("join wait")
        .expect("wait");
    assert_eq!(out.status.signal(), Some(9), "engine exited with {:?}", out.status);
}
