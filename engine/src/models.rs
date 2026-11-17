//! The local model index, and the `listModels`/`getModel`/`refreshIndex`/
//! `downloadModel`/`cancelDownload` IPC commands.
//!
//! `Models` owns the cached index (`$PEDAL_HOME/index/models.json`) and the
//! logic for browsing it: search, facets, sorting and paging. The UI never
//! touches the index; it only sees response payloads and the `event: models`
//! broadcasts.
//!
//! The download queue is deliberately minimal in v1: a single, sequential
//! download that verifies the sha256 and then atomically rewrites the index,
//! as specified in `docs/MODEL-BROWSE-AND-CACHE.md` §9.2. The full
//! queue/concurrency/resume logic is out of scope for this task (§13) and is
//! owned by a later card.
//!
//! # Threading
//!
//! The handlers below are synchronous: the engine's IPC server is a plain
//! synchronous loop, and a `downloadModel` blocks while the file downloads.
//! That is acceptable for v1 because the download is a user-initiated action
//! that happens on a background editor frame, not on the audio thread.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::io::Read;

use pedal_tone3000::index::{Index, LocalState, ModelRecord};
use pedal_tone3000::paths::Paths;
use pedal_tone3000::query::{self, Query, Page};
use sha2::Digest;

use crate::server::Shared;

/// The model-browsing service, owned by the engine process.
pub struct Models {
    index: Mutex<Index>,
    paths: Paths,
}

impl Models {
    /// Build from the process environment (`PEDAL_HOME`, `PEDAL_MODELS`).
    pub fn new() -> Self {
        let paths = Paths::from_env();
        let mut index = Index {
            schema: 1,
            provider: pedal_tone3000::PROVIDER.to_string(),
            generated_at: pedal_tone3000::index::now_utc(),
            count: 0,
            models: Vec::new(),
        };
        if let Ok(Some(loaded)) = pedal_tone3000::index::load_index(&paths) {
            index = loaded;
        }
        Self { index: Mutex::new(index), paths }
    }

    /// Load the index from disk. No-op if absent.
    pub fn load(&self) -> Result<(), String> {
        let loaded = pedal_tone3000::index::load_index(&self.paths)
            .map_err(|e| format!("index: {e}"))?
            .ok_or_else(|| "no model index yet".to_string())?;
        *self.index.lock().map_err(|e| e.to_string())? = loaded;
        Ok(())
    }

    /// The loaded index.
    fn index(&self) -> std::sync::MutexGuard<Index> {
        self.index
            .lock()
            .map_err(|e| e.to_string())
            .unwrap()
    }

    /// `listModels` — search, filter, sort and page the index (spec §10).
    pub fn list_models(&self, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        let query = Query::from_value(args)?;
        let page = query::list_models(&self.index(), &query);
        serde_json::to_value(&page).map_err(|e| e.to_string())
    }

    /// `getModel` — one record by index id (spec §10).
    pub fn get_model(&self, id: &str) -> Result<serde_json::Value, String> {
        let index = self.index();
        let record = index
            .models
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("no such model: {id}"))?;
        serde_json::to_value(record).map_err(|e| e.to_string())
    }

    /// `refreshIndex` — reload the index from disk (spec §10).
    pub fn refresh_index(&self) -> Result<serde_json::Value, String> {
        self.load()?;
        let index = self.index();
        Ok(serde_json::json!({
            "generated_at": index.generated_at,
            "count": index.count,
        }))
    }

    /// `downloadModel` — hand a model to the download queue (spec §9.2).
    pub fn download_model(&self, id: &str) -> Result<serde_json::Value, String> {
        let index = self.index();
        let record = index
            .models
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("no such model: {id}"))?;
        let local = self.download(record)?;
        serde_json::to_value(&local).map_err(|e| e.to_string())
    }

    /// Download a single model and update the index.
    ///
    /// Returns the updated `local` block so the UI can show it.
    fn download(&self, record: &ModelRecord) -> Result<LocalState, String> {
        // 1. Already downloaded and verified? Done.
        if let (Some(ref local_path), Some(ref local_state)) = (&record.local_path, &record.local) {
            let full = self.paths.models.join(local_path);
            if full.exists() {
                if let Some(hash) = sha256(&full) {
                    if format!("sha256:{hash}") == local_state.checksum {
                        return Ok(local_state.clone());
                    }
                }
            }
        }

        // 2. Download to <final>.part.
        let final_path = self.local_path(record);
        let part_path = self.part_path(&final_path);
        let mut client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("download client: {e}"))?;
        let body = client
            .get(record.download_url.as_deref().ok_or("no download URL")?)
            .send()
            .map_err(|e| format!("download: {e}"))?
            .error_for_status()
            .map_err(|e| format!("download: {}", e))?
            .bytes()
            .map_err(|e| format!("download: {e}"))?;

        // 3. Verify sha256.
        let mut hasher = sha2::Sha256::new();
        hasher.update(&body);
        let hash = hasher.finalize();
        let checksum = format!("sha256:{}", hex::encode(hash.as_ref()));

        std::fs::create_dir_all(final_path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&part_path, &body).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&part_path, &final_path) {
            let _ = std::fs::remove_file(&part_path);
            return Err(format!("could not move {part_path:?} onto {final_path:?}: {e}"));
        }

        // 4. Stale detection.
        let stale = record
            .content_hash
            .as_deref()
            .map(|h| h != &checksum)
            .unwrap_or(false);

        // 5. Atomically rewrite models.json with the new local block.
        let mut index = self.index();
        let record_local = index
            .models
            .iter_mut()
            .find(|r| r.id == record.id)
            .expect("record not found");
        record_local.local_path = Some(self.local_path(record).to_string_lossy().to_string());
        record_local.local = Some(LocalState {
            file_name: final_path
                .file_name()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            size_bytes: std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0),
            checksum,
            downloaded_at: pedal_tone3000::index::now_utc(),
            stale,
        });
        let updated = record_local.local.clone().expect("local not set");
        pedal_tone3000::index::write_index(&self.paths, &index).map_err(|e| e.to_string())?;

        Ok(updated)
    }

    /// The relative `local_path` for a record (spec §9.2).
    fn local_path(&self, record: &ModelRecord) -> PathBuf {
        let ext = if record.metadata.format == pedal_tone3000::index::Format::Ir {
            "wav"
        } else {
            "nam"
        };
        PathBuf::from_iter([
            record.provider.clone(),
            record.tone_id.clone(),
            format!(
                "{}_{}.{}",
                record.id,
                record.metadata.variant.as_wire(),
                ext
            ),
        ])
    }

    /// `<final>.part` (spec §9.2 step 2).
    fn part_path(&self, final_path: &Path) -> PathBuf {
        final_path.with_extension(format!(
            "{}.part",
            final_path
                .extension()
                .map(|e| e.to_string_lossy())
                .unwrap_or_default()
        ))
    }

    /// `cancelDownload` — a no-op in v1 (the full queue is a later card, §13).
    pub fn cancel_download(&self, _id: &str) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({}))
    }
}

/// SHA-256 of a file, hex-encoded, or `None` if the file could not be read.
fn sha256(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex::encode(hasher.finalize().as_ref()))
}

/// The hex writer used by `sha256`.
mod hex {
    pub fn encode(input: &[u8]) -> String {
        let mut out = String::with_capacity(input.len() * 2);
        for b in input {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
}
