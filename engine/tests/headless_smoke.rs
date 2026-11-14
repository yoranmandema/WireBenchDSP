//! Headless smoke test — the real `pedal-engine` binary, no audio device.
//!
//! `pedal-engine --headless --port P --bench 3` boots the engine on the
//! simulated device, serves the IPC protocol, and reports the CPU cost of the
//! rig. This test spawns that binary, drives it over a real WebSocket, and
//! checks the whole path: boot, snapshot, a parameter change, and the bench
//! numbers. It is the same test to run on the Pi for US-3.2 / US-5.4.
//!
//! Nothing is ever played out of a speaker: the headless device discards its
//! output (see [`pedal_engine::headless`]).
//!
//! The `--bench` window is real time, but the simulated device's pace is not
//! guaranteed on the development host: `std::thread::sleep` overshoots by an
//! order of magnitude there (5 ms → ~21 ms), so `realtime` can be well under
//! 100%. That is a property of the host, not of the engine — the assertion is
//! only that frames advanced.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

const BIN: &str = env!("CARGO_BIN_EXE_pedal-engine");
const PRESETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../presets");
/// Long enough to boot, connect, and finish the bench window.
const TIMEOUT: Duration = Duration::from_secs(30);

async fn next_text<S>(ws: &mut S) -> String
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => return t.to_string(),
            Some(Ok(_)) => continue,
            other => panic!("websocket closed or errored: {other:?}"),
        }
    }
}

/// The next `event` frame of the given kind. Meters arrive on their own timer
/// and can overtake a reply.
async fn next_event<S>(ws: &mut S, event: &str) -> Value
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let v: Value = serde_json::from_str(&next_text(ws).await).expect("json");
        if v["type"] == "event" && v["event"] == event {
            return v;
        }
    }
}

/// The reply to the request with this id.
async fn next_res<S>(ws: &mut S, id: i64) -> Value
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let v: Value = serde_json::from_str(&next_text(ws).await).expect("json");
        if v["type"] == "res" && v["id"] == json!(id) {
            return v;
        }
    }
}

/// The value of `key=` in a `bench done …` line.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let needle = format!("{key}=");
    line.split_whitespace()
        .find(|w| w.starts_with(&needle))
        .and_then(|w| w.split_once('=').map(|(_, v)| v))
        .unwrap_or_else(|| panic!("no {key} in {line:?}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn headless_engine_serves_the_ui_and_reports_cpu() {
    // An ephemeral port: 8737 may be held by a stale engine from an earlier run.
    let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe bind");
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let child = Command::new(BIN)
        .args(["--headless", "--port", &port.to_string(), "--bench", "3"])
        .env("PEDAL_PRESETS", PRESETS)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pedal-engine");

    // The engine must be listening before the UI can connect.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "engine never listened on {port}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // The UI: a real WebSocket client against the real binary.
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("ws connect");
    let state = next_event(&mut ws, "state").await;
    assert_eq!(state["payload"]["preset"], "04-rainbow");
    // The channel id is the preset's master prefix (`master`), not `ch1`.
    assert_eq!(state["payload"]["channels"][0]["id"], "master");
    assert!(
        state["payload"]["channels"][0]["blocks"]
            .as_array()
            .is_some_and(|b| !b.is_empty()),
        "the preset rig did not load: {state}"
    );

    // A knob turn, the way the UI does it.
    ws.send(Message::Text(
        json!({
            "type": "req", "id": 1, "cmd": "setParam",
            "args": {"channel": "master", "block": "amp", "param": "gain", "value": 8.0}
        })
        .to_string()
        .into(),
    ))
    .await
    .expect("send");
    let res = next_res(&mut ws, 1).await;
    assert_eq!(res["ok"], true, "setParam refused: {res}");
    assert_eq!(res["payload"]["channels"][0]["blocks"][0]["params"]["gain"], 8.0);

    // The bench window ends with the process exiting on its own.
    let out = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("join wait")
        .expect("wait");
    let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&out.stderr));

    assert!(
        out.status.success(),
        "engine exited {:?}\n{log}",
        out.status
    );
    let markers = [
        "headless (no audio device)".to_string(),
        format!("ipc listening on ws://127.0.0.1:{port}"),
        "client connected".to_string(),
        "bench done".to_string(),
    ];
    for marker in &markers {
        assert!(log.contains(marker), "missing {marker:?} in engine log:\n{log}");
    }

    let done = log
        .lines()
        .find(|l| l.contains("bench done"))
        .expect("bench done line");
    let callbacks: u64 = field(done, "callbacks").parse().expect("callbacks");
    let frames: u64 = field(done, "frames").parse().expect("frames");
    let xruns: u64 = field(done, "xruns").parse().expect("xruns");
    let cpu: f64 = field(done, "cpu").parse().expect("cpu");
    let realtime: f64 = field(done, "realtime").parse().expect("realtime");

    assert!(callbacks > 0, "no audio callbacks ran");
    assert!(frames > 0, "no audio frames were rendered");
    assert_eq!(xruns, 0, "the headless device underran");
    assert!(cpu.is_finite() && cpu >= 0.0, "cpu was {cpu}");
    assert!(realtime > 0.0, "realtime was {realtime}");
    println!("headless smoke: {done}");
}
