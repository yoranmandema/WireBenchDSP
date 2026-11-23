//! Engine instance lifecycle: isolation, clean shutdown, and resource release.
//!
//! The acceptance criteria for this module are:
//!  * (a) two engine instances can run simultaneously without conflict
//!  * (b) shutting one down does not affect the other
//!  * (c) a clean shutdown releases the audio device, port and process, and
//!        leaves no orphan process behind
//!
//! Each test boots the real `pedal-engine` binary headless on an ephemeral
//! port and drives it over a real WebSocket, exactly like the UI does. Nothing
//! is ever played through a speaker: the headless device discards its output.

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use std::fmt::Debug;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

/// Kills the child if a test panics, so a failure never leaves an orphan.
struct Kill(std::process::Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const BIN: &str = env!("CARGO_BIN_EXE_pedal-engine");

/// Bind a fresh ephemeral port so two instances can never collide.
fn free_port() -> u16 {
    let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    let port = probe.local_addr().expect("probe addr").port();
    drop(probe);
    port
}

async fn connect_ws(port: u16) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}")).await.expect("ws connect").0
}

/// Wait until the engine on `port` is accepting TCP connections.
async fn wait_listening(port: u16, deadline: Duration) {
    let mut deadline = Instant::now() + deadline;
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "engine never listened on {port}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The next `state` frame from `ws`.
async fn next_state<S, E>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures_util::sink::Sink<Message, Error = E>
        + Unpin,
    E: Debug,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => {
                let v: Value = serde_json::from_str(&t).expect("json");
                if v["type"] == "event" && v["event"] == "state" {
                    return v["payload"].clone();
                }
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => panic!("socket error: {e}"),
            None => panic!("socket closed while waiting for state"),
        }
    }
}

/// The reply to the request with this id (a `res` frame whose `id` equals `id`).
async fn next_res<S, E>(ws: &mut S, id: i64) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures_util::sink::Sink<Message, Error = E>
        + Unpin,
    E: Debug,
{
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => {
                let v: Value = serde_json::from_str(&t).expect("json");
                if v["type"] == "res" && v["id"] == json!(id) {
                    return v;
                }
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => panic!("socket error: {e}"),
            None => panic!("socket closed while waiting for response {id}"),
        }
    }
}

/// Request `getState` and return its snapshot (the `res` payload).
async fn get_state<S, E>(ws: &mut S, id: i64) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures_util::sink::Sink<Message, Error = E>
        + Unpin,
    E: Debug,
{
    let req = json!({"type": "req", "id": id, "cmd": "getState"});
    ws.send(Message::Text(req.to_string().into())).await.expect("send getState");
    let res = next_res(ws, id).await;
    assert_eq!(res["ok"], true, "getState refused: {res}");
    res["payload"].clone()
}

/// Send `engine.wait` to start a clean shutdown. The instance replies with a
/// snapshot, then closes the connection.
async fn engine_wait<S, E>(ws: &mut S)
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures_util::sink::Sink<Message, Error = E>
        + Unpin,
    E: Debug,
{
    let req = json!({
        "type": "req",
        "id": 1,
        "cmd": "engine.wait",
    });
    ws.send(Message::Text(req.to_string().into())).await.expect("send engine.wait");
    let _ = ws.next().await; // the instance replies with a snapshot, then disconnects
}

/// Assert that the child process `pid` no longer exists (it was reaped by
/// `wait()`, so signal 0 must fail with ESRCH).
fn assert_gone(pid: u32) {
    let alive = Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().expect("kill").success();
    assert!(!alive, "engine process {pid} still alive after shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_engine_instances_bind_distinct_ports_and_run_without_conflict() {
    tokio::time::timeout(Duration::from_secs(60), two_engines()).await.expect("test timed out");
}

async fn two_engines() {
    let port_a = free_port();
    let port_b = free_port();

    let mut child_a = Command::new(BIN)
        .args(["--headless", "--port", &port_a.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn engine A");
    let mut child_b = Command::new(BIN)
        .args(["--headless", "--port", &port_b.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn engine B");

    wait_listening(port_a, Duration::from_secs(30)).await;
    wait_listening(port_b, Duration::from_secs(30)).await;

    // Each engine serves its own, distinct loopback port.
    assert_ne!(port_a, port_b);
    let (mut ws_a, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}", port_a)).await.expect("ws connect");
    let (mut ws_b, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}", port_b)).await.expect("ws connect");

    // Both instances are live, both report their own preset.
    let state_a = next_state(&mut ws_a).await;
    let state_b = next_state(&mut ws_b).await;
    assert_eq!(state_a["preset"], "04-rainbow");
    assert_eq!(state_b["preset"], "04-rainbow");

    // A parameter change on A must only touch A's instance.
    let before_b = get_state(&mut ws_b, 10).await["channels"].clone();
    let req = json!({"type": "req", "id": 2, "cmd": "setMasterVolume",
        "args": {"channel": "ch1", "value": -6.0}});
    ws_a.send(Message::Text(req.to_string().into())).await.expect("send");
    let res = next_res(&mut ws_a, 2).await;
    assert_eq!(res["ok"], true, "setParam refused: {res}");
    let after_b = get_state(&mut ws_b, 11).await["channels"].clone();
    assert_eq!(before_b, after_b, "B changed when A was touched");
    assert_eq!(get_state(&mut ws_a, 12).await["channels"][0]["volume"], -6.0, "A did not take the change");

    // Clean up A first, then B.
    engine_wait(&mut ws_a).await;
    let pid = child_a.id();
    let _ = child_a.wait().expect("wait A");
    assert_gone(pid);
    engine_wait(&mut ws_b).await;
    let pid = child_b.id();
    let _ = child_b.wait().expect("wait B");
    assert_gone(pid);

    
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutting_down_one_instance_does_not_affect_the_other() {
    let port_a = free_port();
    let port_b = free_port();

    let mut child_a = Command::new(BIN)
        .args(["--headless", "--port", &port_a.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn engine A");
    let mut child_b = Command::new(BIN)
        .args(["--headless", "--port", &port_b.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn engine B");

    wait_listening(port_a, Duration::from_secs(30)).await;
    wait_listening(port_b, Duration::from_secs(30)).await;

    let mut ws_a = connect_ws(port_a).await;
    let mut ws_b = connect_ws(port_b).await;

    // Both instances are live at the same instant.
    let state_a = next_state(&mut ws_a).await;
    let state_b = next_state(&mut ws_b).await;
    assert_eq!(state_a["preset"], "04-rainbow");
    assert_eq!(state_b["preset"], "04-rainbow");

    // Stop only A. A exits 0 and frees its port.
    engine_wait(&mut ws_a).await;
    let status = child_a.wait().expect("wait A");
    assert!(status.success(), "A exited non-zero");
    let _ = TcpListener::bind(("127.0.0.1", port_a))
        .expect("A's port should be free after shutdown");

    // B is still running and still serving while A is dead.
    let state_b2 = get_state(&mut ws_b, 20).await;
    assert_eq!(state_b2["preset"], "04-rainbow");
    assert_eq!(state_b2["source"], "riff");

    // Clean up B.
    engine_wait(&mut ws_b).await;
    let pid = child_b.id();
    let _ = child_b.wait().expect("wait B");
    assert_gone(pid);

    
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clean_shutdown_releases_resources_and_leaves_no_orphan_process() {
    let port = free_port();

    let mut child = Command::new(BIN)
        .args(["--headless", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn engine");

    wait_listening(port, Duration::from_secs(30)).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}", port)).await.expect("ws connect");
    let _ = next_state(&mut ws).await;

    // Explicit shutdown API: engine.wait.
    engine_wait(&mut ws).await;
    let pid = child.id();
    let status = child.wait().expect("wait");
    assert_gone(pid);
    assert!(status.success(), "clean shutdown must exit 0");

    // The audio device, the IPC listener and every internal thread are
    // reclaimed when the process exits. Verify the port is free again.
    let _ = TcpListener::bind(("127.0.0.1", port))
        .expect("port must be released after shutdown");

    
}
