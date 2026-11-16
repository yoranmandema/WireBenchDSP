//! Model-reference resolution for NAM blocks.
//!
//! A preset node's `model` is one of two things:
//!
//! * an **index id** — `"t3k:379978"` — resolved through the local model index
//!   to the downloaded file's `local_path`;
//! * a **path** — absolute, or relative to the preset file — the legacy form,
//!   kept working so old presets still load.
//!
//! [`ModelResolver`] maps a reference to a file path at load time.
//! [`PathResolver`] is the legacy path-only behaviour; [`IndexResolver`]
//! resolves index ids and falls back to it. The engine builds one from the
//! cached index (`$PEDAL_HOME/index/models.json`, see
//! `docs/MODEL-BROWSE-AND-CACHE.md` §10.1).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Maps a preset's `model` reference to a `.nam` file path.
///
/// Implementations must be infallible: a reference that cannot be resolved is
/// returned unchanged so the block's own loader reports the missing file.
pub trait ModelResolver {
    /// `reference` is the raw `model` string; `base` is the preset's directory.
    fn resolve(&self, reference: &str, base: &Path) -> String;
}

/// Legacy resolver: absolute paths are used verbatim, relative paths resolve
/// against the preset's directory.
#[derive(Clone, Copy, Debug, Default)]
pub struct PathResolver;

impl ModelResolver for PathResolver {
    fn resolve(&self, reference: &str, base: &Path) -> String {
        let path = Path::new(reference);
        if path.is_absolute() {
            reference.to_string()
        } else {
            base.join(path).to_string_lossy().into_owned()
        }
    }
}

/// One cached index record, reduced to the fields preset loading needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRecord {
    /// `<provider>:<model_id>`, e.g. `t3k:379978`.
    pub id: String,
    /// Provider id, e.g. `t3k`.
    pub provider: String,
    /// Path relative to `$PEDAL_MODELS`, POSIX separators. `None` when the
    /// model has not been downloaded.
    pub local_path: Option<String>,
    /// Upstream content hash, when the provider exposes one. Equals the `.nam`
    /// file's `metadata.weights_hash`, which is how a legacy file is matched to
    /// its record during migration.
    pub content_hash: Option<String>,
}

/// The subset of the cached model index that preset loading needs.
///
/// Only `models[].id`, `provider`, `local_path` and `content_hash` are read;
/// every other field is ignored, so a newer index schema still loads.
#[derive(Clone, Debug, Default)]
pub struct ModelIndex {
    records: HashMap<String, ModelRecord>,
}

#[derive(Deserialize)]
struct IndexDoc {
    #[serde(default)]
    models: Vec<RecordDoc>,
}

#[derive(Deserialize)]
struct RecordDoc {
    id: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    local_path: Option<String>,
    #[serde(default)]
    content_hash: Option<String>,
}

impl ModelIndex {
    /// Parse an index document.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: IndexDoc = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut records = HashMap::with_capacity(doc.models.len());
        for r in doc.models {
            records.insert(
                r.id.clone(),
                ModelRecord {
                    id: r.id,
                    provider: r.provider,
                    local_path: r.local_path,
                    content_hash: r.content_hash,
                },
            );
        }
        Ok(Self { records })
    }

    /// Load the index from `path`; `Ok(None)` when it does not exist yet.
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Whether `reference` is a known index id. This is the discriminator
    /// between an index reference and a legacy path.
    pub fn contains(&self, reference: &str) -> bool {
        self.records.contains_key(reference)
    }

    /// The downloaded file's path relative to `$PEDAL_MODELS`, when the id is
    /// known and the model is downloaded.
    pub fn local_path(&self, reference: &str) -> Option<&str> {
        self.records.get(reference).and_then(|r| r.local_path.as_deref())
    }

    /// Find a record by the `.nam` file's `metadata.weights_hash`.
    pub fn by_content_hash(&self, hash: &str) -> Option<&ModelRecord> {
        self.records.values().find(|r| r.content_hash.as_deref() == Some(hash))
    }

    /// Find a record by provider and numeric model id — the `.nam` file's
    /// `metadata.t3k_id`, e.g. `("t3k", "872412")`.
    pub fn by_provider_model(&self, provider: &str, model_id: &str) -> Option<&ModelRecord> {
        let id = format!("{provider}:{model_id}");
        self.records.get(&id)
    }
}

/// Resolves index ids through the cached index and falls back to
/// [`PathResolver`] for legacy path references.
#[derive(Clone, Debug)]
pub struct IndexResolver {
    index: ModelIndex,
    models_dir: PathBuf,
}

impl IndexResolver {
    pub fn new(index: ModelIndex, models_dir: impl Into<PathBuf>) -> Self {
        Self { index, models_dir: models_dir.into() }
    }

    /// The directory `local_path` is relative to (`$PEDAL_MODELS`).
    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    pub fn index(&self) -> &ModelIndex {
        &self.index
    }
}

impl ModelResolver for IndexResolver {
    fn resolve(&self, reference: &str, base: &Path) -> String {
        if let Some(record) = self.index.records.get(reference) {
            return match &record.local_path {
                // Downloaded: the index owns the location.
                Some(rel) => self.models_dir.join(rel).to_string_lossy().into_owned(),
                // Known but not downloaded: there is no file yet. Return the id
                // so the block reports it rather than a bogus path.
                None => reference.to_string(),
            };
        }
        // Not a known index id: a legacy path, resolved exactly as before.
        PathResolver.resolve(reference, base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{
        "schema": 1,
        "provider": "t3k",
        "generated_at": "2026-11-14T23:40:00Z",
        "count": 2,
        "models": [
            {
                "id": "t3k:379978",
                "provider": "t3k",
                "local_path": "t3k/28199/379978_a2.nam",
                "content_hash": "nam-content-v1:aaa",
                "metadata": { "name": "API 512c" }
            },
            {
                "id": "t3k:379979",
                "provider": "t3k",
                "local_path": null,
                "content_hash": null,
                "metadata": { "name": "not downloaded" }
            }
        ]
    }"#;

    #[test]
    fn absolute_paths_pass_through() {
        let p = if cfg!(windows) { r"C:\models\a.nam" } else { "/models/a.nam" };
        assert_eq!(PathResolver.resolve(p, Path::new("/elsewhere")), p);
    }

    #[test]
    fn relative_paths_resolve_against_base() {
        assert_eq!(
            PathResolver.resolve("../models/a.nam", Path::new("/presets")),
            "/presets/../models/a.nam"
        );
    }

    #[test]
    fn parses_the_fields_presets_need_and_ignores_the_rest() {
        let index = ModelIndex::from_json(INDEX).unwrap();
        assert_eq!(index.len(), 2);
        assert!(index.contains("t3k:379978"));
        assert!(!index.contains("t3k:999999"));
        assert_eq!(index.local_path("t3k:379978"), Some("t3k/28199/379978_a2.nam"));
        assert_eq!(index.local_path("t3k:379979"), None);
        assert_eq!(
            index.by_content_hash("nam-content-v1:aaa").map(|r| r.id.as_str()),
            Some("t3k:379978")
        );
        assert_eq!(
            index.by_provider_model("t3k", "379978").map(|r| r.id.as_str()),
            Some("t3k:379978")
        );
        assert!(index.by_provider_model("t3k", "999999").is_none());
    }

    #[test]
    fn index_id_resolves_through_local_path() {
        let index = ModelIndex::from_json(INDEX).unwrap();
        let resolver = IndexResolver::new(index, "/models");
        assert_eq!(
            resolver.resolve("t3k:379978", Path::new("/presets")),
            "/models/t3k/28199/379978_a2.nam"
        );
    }

    #[test]
    fn known_id_that_is_not_downloaded_is_returned_unchanged() {
        let index = ModelIndex::from_json(INDEX).unwrap();
        let resolver = IndexResolver::new(index, "/models");
        assert_eq!(resolver.resolve("t3k:379979", Path::new("/presets")), "t3k:379979");
    }

    #[test]
    fn unknown_reference_falls_back_to_the_legacy_path() {
        let index = ModelIndex::from_json(INDEX).unwrap();
        let resolver = IndexResolver::new(index, "/models");
        assert_eq!(
            resolver.resolve("../models/a.nam", Path::new("/presets")),
            "/presets/../models/a.nam"
        );
        assert_eq!(resolver.resolve("/abs/a.nam", Path::new("/presets")), "/abs/a.nam");
    }

    #[test]
    fn missing_index_file_is_none() {
        let dir = std::env::temp_dir().join(format!("pedal-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ModelIndex::load(&dir.join("models.json")).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
