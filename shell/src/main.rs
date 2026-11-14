//! The Pedal desktop shell.
//!
//! A Tauri window hosting the shared Preact UI, plus the engine process that
//! the UI talks to. The shell owns the engine's lifecycle: it starts it on
//! launch and stops it on exit, so quitting the app never leaves a stray
//! process holding the audio device.
//!
//! The UI ships inside the binary (`frontendDist: ../ui/dist`) and is rendered
//! by the platform webview. A debug build uses the Vite `devUrl` instead, which
//! is standard Tauri behaviour; the embedded path is what `tauri build`
//! produces. Either way the page connects to ws://127.0.0.1:8737, which the
//! CSP in tauri.conf.json allows.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// Where the engine binary lives.
///
/// Order: `PEDAL_ENGINE`, then the directory holding this executable (where a
/// bundle would place a sidecar engine), then the workspace target directory
/// (development), then bare `pedal-engine` on `PATH`.
fn engine_path() -> PathBuf {
    if let Ok(p) = std::env::var("PEDAL_ENGINE") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("pedal-engine");
            if bundled.exists() {
                return bundled;
            }
        }
    }
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join(profile)
        .join("pedal-engine");
    if local.exists() {
        return local;
    }
    PathBuf::from("pedal-engine")
}

static ENGINE: Mutex<Option<Child>> = Mutex::new(None);

#[tauri::command]
fn engine_status() -> String {
    let mut guard = ENGINE.lock().unwrap();
    match guard.as_mut() {
        Some(c) => match c.try_wait() {
            Ok(Some(status)) => format!("exited: {status}"),
            Ok(None) => "running".into(),
            Err(e) => format!("unknown: {e}"),
        },
        None => "not started".into(),
    }
}

fn spawn_engine() {
    let path = engine_path();
    let mut cmd = Command::new(&path);
    // The engine's stderr is the diagnostic channel; keep it in a file so the
    // shell's own output stays readable and the log survives a restart.
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/pedal-engine.log")
        .ok();
    cmd.arg("--rig")
        .arg("04-rainbow")
        .stdout(Stdio::null())
        .stderr(match log {
            Some(f) => Stdio::from(f),
            None => Stdio::inherit(),
        });
    match cmd.spawn() {
        Ok(child) => {
            eprintln!("pedal-shell: engine started ({})", path.display());
            *ENGINE.lock().unwrap() = Some(child);
        }
        Err(e) => eprintln!("pedal-shell: could not start the engine at {}: {e}", path.display()),
    }
}

fn stop_engine() {
    if let Some(mut child) = ENGINE.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("pedal-shell: engine stopped");
    }
}

fn main() {
    spawn_engine();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine_status])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // `run` returns when the window closes.
    stop_engine();
}
