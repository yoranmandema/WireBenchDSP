//! The IPC server: a loopback WebSocket speaking `docs/IPC-PROTOCOL.md`.
//!
//! One [`Shared`] is owned by the process. WebSocket tasks mutate it behind
//! mutexes and push [`Cmd`]s to the audio thread, exactly as the egui app's UI
//! thread did. The audio thread never touches the WebSocket.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use dsp_graph::{Registry, RoutingEndpoint};

use crate::audio::{read_f32, Cmd, Handle};
use crate::channels::{master_id, Rig, IN};
use crate::store::{slugify, Store};

/// Loopback only. Nothing outside this machine can reach the engine.
pub const PORT: u16 = 8737;
/// Meters are pushed at this interval.
const METERS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// Everything the protocol can touch.
pub struct Shared {
    pub rig: Mutex<Rig>,
    pub store: Mutex<Store>,
    pub handle: Handle,
    pub registry: Arc<Registry>,
    /// 0 = riff, 1 = live input, 2 = silence.
    pub source: Arc<AtomicU8>,
    /// Current preset name, `""` when the rig has never been saved.
    pub preset: Mutex<String>,
    clients: Mutex<Vec<mpsc::UnboundedSender<String>>>,
    /// Bumped on every successful rebuild. The UI uses it to ignore a stale
    /// snapshot, so an out-of-order delivery can never roll the UI backwards.
    pub rev: AtomicU64,
}

impl Shared {
    pub fn new(handle: Handle, registry: Arc<Registry>, source: Arc<AtomicU8>, store: Store) -> Self {
        Self {
            rig: Mutex::new(Rig::default()),
            store: Mutex::new(store),
            handle,
            registry,
            source,
            preset: Mutex::new(String::new()),
            clients: Mutex::new(Vec::new()),
            rev: AtomicU64::new(0),
        }
    }

    /// Preset names, sorted, for the state snapshot and `listPresets`.
    fn presets(&self) -> Vec<String> {
        self.store.lock().unwrap().presets.iter().map(|(n, _)| n.clone()).collect()
    }

    fn source_name(&self) -> &'static str {
        match self.source.load(Ordering::Relaxed) {
            1 => "live",
            2 => "silence",
            _ => "riff",
        }
    }
}

// ---------------------------------------------------------------------------
// State snapshot
// ---------------------------------------------------------------------------

fn registry_json(reg: &Registry) -> Value {
    let mut out = BTreeMap::new();
    for spec in reg.specs() {
        let params: Vec<Value> = spec
            .params
            .iter()
            .map(|p| {
                let mut o = json!({
                    "id": p.id,
                    "name": p.name,
                    "min": p.min,
                    "max": p.max,
                    "default": p.default,
                    "unit": p.unit.to_string(),
                });
                if let Some(choices) = p.choices() {
                    o["kind"] = json!("choice");
                    o["choices"] = json!(choices);
                } else {
                    o["kind"] = json!("number");
                }
                o
            })
            .collect();
        out.insert(
            spec.id.to_string(),
            json!({
                "name": spec.name,
                "category": spec.category.to_string(),
                "params": params,
            }),
        );
    }
    Value::Object(out.into_iter().collect())
}

fn snapshot(sh: &Shared) -> Value {
    let rig = sh.rig.lock().unwrap();
    let channels: Vec<Value> = rig
        .channels
        .iter()
        .map(|c| {
            let blocks: Vec<Value> = c
                .blocks
                .iter()
                .map(|b| {
                    let mut params = serde_json::Map::new();
                    // Fill in defaults for anything the preset did not set, so
                    // the UI never has to merge.
                    if let Some(spec) = sh.registry.get(&b.block) {
                        for p in &spec.params {
                            let v = b.params.get(p.id).copied().unwrap_or(p.default);
                            params.insert(p.id.to_string(), json!(v));
                        }
                    }
                    json!({
                        "id": b.id,
                        "type": b.block,
                        "name": sh.registry.get(&b.block).map(|s| s.name).unwrap_or(&b.block),
                        "bypass": b.bypass,
                        "params": Value::Object(params),
                    })
                })
                .collect();
            json!({
                "id": c.id,
                "name": c.name,
                "volume": c.master_volume,
                "sources": c.sources,
                "blocks": blocks,
            })
        })
        .collect();
    let routings = serde_json::to_value(&rig.routings).unwrap_or_else(|_| json!([]));
    let preset = sh.preset.lock().unwrap().clone();
    let rev = sh.rev.load(Ordering::Acquire);
    json!({
        "rev": rev,
        "preset": preset,
        "presets": sh.presets(),
        "source": sh.source_name(),
        "registry": registry_json(&sh.registry),
        "channels": channels,
        "routings": routings,
    })
}

fn state_event(sh: &Shared) -> String {
    json!({ "type": "event", "event": "state", "payload": snapshot(sh) }).to_string()
}

/// Push a state snapshot to every client except `skip` (the one that asked).
fn send_state(sh: &Shared, skip: Option<&mpsc::UnboundedSender<String>>) {
    broadcast(sh, state_event(sh), skip);
}

fn broadcast(sh: &Shared, text: String, skip: Option<&mpsc::UnboundedSender<String>>) {
    let mut clients = sh.clients.lock().unwrap();
    clients.retain(|c| {
        if skip.map(|s| s.same_channel(c)).unwrap_or(false) {
            return true;
        }
        c.send(text.clone()).is_ok()
    });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Rebuild the executor from the rig and hand it to the audio thread.
fn rebuild(sh: &Shared) -> Result<(), String> {
    let doc = sh.rig.lock().unwrap().to_doc();
    let mut g = doc
        .build_in(&sh.registry, &sh.handle.preset_dir)
        .map_err(|e| e.to_string())?;
    let block = sh.handle.block();
    let ex = dsp_graph::Executor::compile_owned(&mut g, sh.handle.sample_rate, block)
        .map_err(|e| e.to_string())?;
    let ids = ex.node_ids().to_vec();
    let bypass: Vec<(String, bool)> = sh
        .rig
        .lock()
        .unwrap()
        .channels
        .iter()
        .flat_map(|c| c.blocks.iter().filter(|b| b.bypass).map(|b| (b.id.clone(), true)))
        .collect();
    sh.handle.push(Cmd::Swap(ex, bypass)).map_err(|e| e.to_string())?;
    *sh.handle.node_ids.lock().unwrap() = ids;
    *sh.handle.rate_mismatches.lock().unwrap() = Vec::new();
    sh.rev.fetch_add(1, Ordering::Release);
    Ok(())
}

fn set_param(sh: &Shared, channel: &str, block: &str, param: &str, value: f32) -> Result<(), String> {
    let (nidx, pidx) = {
        let rig = sh.rig.lock().unwrap();
        let ch = rig.channel(channel).ok_or_else(|| format!("no such channel: {channel}"))?;
        let node = ch.blocks.iter().find(|b| b.id == block).ok_or_else(|| format!("no such block: {block}"))?;
        let spec = sh.registry.get(&node.block).ok_or_else(|| format!("unknown block type: {}", node.block))?;
        let pidx = spec.params.iter().position(|p| p.id == param).ok_or_else(|| format!("no such param: {param}"))?;
        (sh.handle.node_index(block), pidx)
    };
    let nidx = nidx.ok_or_else(|| format!("block not in the compiled graph: {block}"))?;
    {
        let mut rig = sh.rig.lock().unwrap();
        let ch = rig.channel_mut(channel).unwrap();
        let node = ch.blocks.iter_mut().find(|b| b.id == block).unwrap();
        node.params.insert(param.to_string(), value);
    }
    sh.handle.push(Cmd::SetParam { node: nidx, param: pidx, value }).map_err(|e| e.to_string())?;
    Ok(())
}

fn set_master_volume(sh: &Shared, channel: &str, value: f32) -> Result<(), String> {
    let nidx = {
        let mut rig = sh.rig.lock().unwrap();
        let ch = rig.channel_mut(channel).ok_or_else(|| format!("no such channel: {channel}"))?;
        ch.master_volume = value;
        sh.handle.node_index(&master_id(channel))
    };
    if let Some(nidx) = nidx {
        sh.handle.push(Cmd::SetParam { node: nidx, param: 0, value }).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_bypass(sh: &Shared, channel: &str, block: &str, on: bool) -> Result<(), String> {
    let nidx = {
        let mut rig = sh.rig.lock().unwrap();
        let ch = rig.channel_mut(channel).ok_or_else(|| format!("no such channel: {channel}"))?;
        let node = ch.blocks.iter_mut().find(|b| b.id == block).ok_or_else(|| format!("no such block: {block}"))?;
        node.bypass = on;
        sh.handle.node_index(block)
    };
    if let Some(nidx) = nidx {
        sh.handle.push(Cmd::SetBypass { node: nidx, on }).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn add_block(sh: &Shared, channel: &str, btype: &str, name: &str) -> Result<(), String> {
    if sh.registry.get(btype).is_none() {
        return Err(format!("unknown block type: {btype}"));
    }
    {
        let mut rig = sh.rig.lock().unwrap();
        if rig.channel(channel).is_none() {
            return Err(format!("no such channel: {channel}"));
        }
        // Placeholder position; the UI does not use it.
        rig.add_block_as(channel, btype, name, [0.0, 0.0]);
    }
    rebuild(sh)
}

fn remove_block(sh: &Shared, channel: &str, block: &str) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        rig.remove_block(channel, block);
    }
    rebuild(sh)
}

fn move_block(sh: &Shared, channel: &str, block: &str, to: usize) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        rig.move_block(channel, block, to);
    }
    rebuild(sh)
}

fn create_routing(sh: &Shared, source: RoutingEndpoint, dest: RoutingEndpoint) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        rig.create_routing(&source, &dest)?;
    }
    rebuild(sh)
}

fn remove_routing(sh: &Shared, id: &str) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        if !rig.remove_routing(id) {
            return Err(format!("no such routing: {id}"));
        }
    }
    rebuild(sh)
}

fn set_routing_enabled(sh: &Shared, id: &str, enabled: bool) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        rig.set_enabled(id, enabled)?;
    }
    rebuild(sh)
}

fn set_routing_gain(sh: &Shared, id: &str, gain: f32) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        rig.set_gain(id, gain)?;
    }
    rebuild(sh)
}

fn list_routings(sh: &Shared) -> Value {
    let rig = sh.rig.lock().unwrap();
    json!({ "routings": serde_json::to_value(&rig.routings).unwrap_or_else(|_| json!([])) })
}

/// Parse `{ "channel": "...", "block": "..." }` from a command's args.
fn endpoint(args: &Value, key: &str) -> Result<RoutingEndpoint, String> {
    let o = args.get(key).ok_or_else(|| key.to_string())?;
    let channel = o.get("channel").and_then(|v| v.as_str()).ok_or("channel")?;
    let block = o.get("block").and_then(|v| v.as_str()).ok_or("block")?;
    Ok(RoutingEndpoint::new(channel, block))
}

fn set_sources(sh: &Shared, channel: &str, sources: Vec<String>) -> Result<(), String> {
    {
        let mut rig = sh.rig.lock().unwrap();
        let ch = rig.channel_mut(channel).ok_or_else(|| format!("no such channel: {channel}"))?;
        ch.sources = if sources.is_empty() { vec![IN.to_string()] } else { sources };
    }
    rebuild(sh)
}

fn load_preset(sh: &Shared, name: &str) -> Result<(), String> {
    let doc = {
        let store = sh.store.lock().unwrap();
        store.presets.iter().find(|(n, _)| n == name).map(|(_, d)| d.clone()).ok_or_else(|| format!("no such preset: {name}"))?
    };
    let rig = Rig::from_doc(&doc);
    {
        let mut r = sh.rig.lock().unwrap();
        *r = rig;
    }
    *sh.preset.lock().unwrap() = name.to_string();
    rebuild(sh)
}

fn save_preset(sh: &Shared, name: &str) -> Result<(), String> {
    let doc = sh.rig.lock().unwrap().to_doc();
    {
        let mut store = sh.store.lock().unwrap();
        store.save(name, &doc).map_err(|e| e.to_string())?;
        store.reload();
    }
    *sh.preset.lock().unwrap() = slugify(name);
    Ok(())
}

fn delete_preset(sh: &Shared, name: &str) -> Result<(), String> {
    let path = {
        let store = sh.store.lock().unwrap();
        store.dir.join(format!("{name}.json"))
    };
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    {
        let mut store = sh.store.lock().unwrap();
        store.reload();
    }
    {
        let mut preset = sh.preset.lock().unwrap();
        if preset.as_str() == name {
            preset.clear();
        }
    }
    Ok(())
}

fn rename_preset(sh: &Shared, from: &str, to: &str) -> Result<(), String> {
    let doc = {
        let store = sh.store.lock().unwrap();
        store
            .presets
            .iter()
            .find(|(n, _)| n == from)
            .map(|(_, d)| d.clone())
            .ok_or_else(|| format!("no such preset: {from}"))?
    };
    let slug = slugify(to);
    if slug == from {
        return Ok(());
    }
    {
        let mut store = sh.store.lock().unwrap();
        store.write_exact(&slug, &doc)?;
        let old = store.dir.join(format!("{from}.json"));
        let _ = std::fs::remove_file(old);
        store.reload();
    }
    {
        let mut preset = sh.preset.lock().unwrap();
        if preset.as_str() == from {
            *preset = slug;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

fn apply(sh: &Shared, cmd: &str, args: &Value) -> Result<Value, String> {
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).map(|v| v.to_string());
    let f = |k: &str| args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let b = |k: &str| args.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

    match cmd {
        "getState" | "setParam" | "setMasterVolume" | "setBypass" | "addBlock"
        | "removeBlock" | "moveBlock" | "setSources" | "createRouting"
        | "removeRouting" | "setEnabled" | "setGain"
        | "loadPreset" | "setSource" | "savePreset" | "deletePreset" | "renamePreset" => {
            match cmd {
                "getState" => {}
                "setParam" => {
                    set_param(sh, &s("channel").ok_or("channel")?, &s("block").ok_or("block")?, &s("param").ok_or("param")?, f("value"))?;
                }
                "setMasterVolume" => {
                    set_master_volume(sh, &s("channel").ok_or("channel")?, f("value"))?;
                }
                "setBypass" => {
                    set_bypass(sh, &s("channel").ok_or("channel")?, &s("block").ok_or("block")?, b("bypass"))?;
                }
                "addBlock" => {
                    add_block(sh, &s("channel").ok_or("channel")?, &s("type").ok_or("type")?, &s("name").unwrap_or_default())?;
                }
                "removeBlock" => {
                    remove_block(sh, &s("channel").ok_or("channel")?, &s("block").ok_or("block")?)?;
                }
                "moveBlock" => {
                    let to = args.get("to").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    move_block(sh, &s("channel").ok_or("channel")?, &s("block").ok_or("block")?, to)?;
                }
                "setSources" => {
                    let sources: Vec<String> = args
                        .get("sources")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    set_sources(sh, &s("channel").ok_or("channel")?, sources)?;
                }
                "createRouting" => {
                    let source = endpoint(args, "source")?;
                    let dest = endpoint(args, "dest")?;
                    create_routing(sh, source, dest)?;
                }
                "removeRouting" => {
                    remove_routing(sh, &s("id").ok_or("id")?)?;
                }
                "setEnabled" => {
                    set_routing_enabled(sh, &s("id").ok_or("id")?, b("enabled"))?;
                }
                "setGain" => {
                    set_routing_gain(sh, &s("id").ok_or("id")?, f("gain"))?;
                }
                "loadPreset" => {
                    load_preset(sh, &s("name").ok_or("name")?)?;
                }
                "setSource" => {
                    let v = match s("source").as_deref() {
                        Some("live") => 1u8,
                        Some("silence") => 2u8,
                        _ => 0u8,
                    };
                    sh.source.store(v, Ordering::Relaxed);
                }
                "savePreset" => {
                    save_preset(sh, &s("name").ok_or("name")?)?;
                }
                "deletePreset" => {
                    delete_preset(sh, &s("name").ok_or("name")?)?;
                }
                "renamePreset" => {
                    rename_preset(sh, &s("from").ok_or("from")?, &s("to").ok_or("to")?)?;
                }
                _ => unreachable!(),
            }
            Ok(snapshot(sh))
        }
        "listPresets" => Ok(json!({ "presets": sh.presets() })),
        "listRoutings" => Ok(list_routings(sh)),
        other => Err(format!("unknown command: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

async fn handle_client(ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, sh: Arc<Shared>) {
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    sh.clients.lock().unwrap().push(tx.clone());
    eprintln!("pedal-engine: client connected ({} total)", sh.clients.lock().unwrap().len());

    // Greet with a full snapshot so a reconnecting UI never has to ask.
    if sink.send(Message::Text(state_event(&sh).into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(msg)) = incoming else { break };
                let Message::Text(text) = msg else { continue };
                let reply = match serde_json::from_str::<Value>(&text) {
                    Ok(req) => {
                        let id = req.get("id").cloned().unwrap_or(Value::Null);
                        let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let args = req.get("args").cloned().unwrap_or(json!({}));
                        eprintln!("pedal-engine: <- {cmd}");
                        match apply(&sh, &cmd, &args) {
                            Ok(payload) => {
                                if cmd != "getState" {
                                    // Tell everyone, the requester included. The
                                    // reply carries the state too and the UI drops
                                    // duplicates by `rev`, so if the reply is lost
                                    // the broadcast still lands — that redundancy
                                    // is what keeps the UI from freezing.
                                    send_state(&sh, None);
                                }
                                json!({"type":"res","id":id,"ok":true,"payload":payload})
                            }
                            Err(e) => json!({"type":"res","id":id,"ok":false,"error":e}),
                        }
                    }
                    Err(e) => json!({"type":"res","id":Value::Null,"ok":false,"error":format!("bad JSON: {e}")}),
                };
                if sink.send(Message::Text(reply.to_string().into())).await.is_err() {
                    break;
                }
            }
            out = rx.recv() => {
                let Some(text) = out else { break };
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }

    sh.clients.lock().unwrap().retain(|c| !c.is_closed());
}

/// Broadcast meters to every connected client.
async fn meters_loop(sh: Arc<Shared>) {
    let mut tick = tokio::time::interval(METERS_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        if sh.clients.lock().unwrap().is_empty() {
            continue;
        }
        let m = &sh.handle.meters;
        let out = read_f32(&m.out_peak);
        let cpu = read_f32(&m.callback_s) / (crate::audio::MAX_BLOCK as f32 / sh.handle.sample_rate) * 100.0;
        let xruns = m.xruns.load(Ordering::Relaxed);
        let text = json!({
            "type":"event","event":"meters",
            "payload":{"in":0.0,"out":out,"cpu":cpu,"xruns":xruns}
        })
        .to_string();
        broadcast(&sh, text, None);
    }
}

/// Bind loopback and serve until the process exits.
pub async fn serve(sh: Arc<Shared>) -> Result<(), String> {
    serve_port(sh, PORT).await
}

/// Bind loopback on an explicit port and serve until the process exits.
///
/// Split out from [`serve`] so a test (or a Pi whose [`PORT`] is taken) can
/// bind somewhere else.
pub async fn serve_port(sh: Arc<Shared>, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| e.to_string())?;
    eprintln!("pedal-engine: ipc listening on ws://127.0.0.1:{port}");
    serve_listener(sh, listener).await
}

/// Serve on an already-bound listener.
///
/// Split out from [`serve`] so a test can bind an ephemeral port instead of
/// the production [`PORT`] (which a stale engine from an earlier session may
/// still hold).
pub async fn serve_listener(sh: Arc<Shared>, listener: TcpListener) -> Result<(), String> {
    eprintln!(
        "pedal-engine: ipc listening on {}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    tokio::spawn(meters_loop(sh.clone()));
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let sh = sh.clone();
        tokio::spawn(async move {
            match tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => handle_client(ws, sh).await,
                Err(_) => {}
            }
        });
    }
}
