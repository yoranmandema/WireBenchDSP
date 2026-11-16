//! Typed shapes for the TONE3000 v1 API responses (the subset browsing and
//! indexing need).
//!
//! Every struct is deliberately tolerant of *extra* upstream fields and of
//! missing optional ones — the recon documented real drift (UUID `user_id`,
//! `sizes: null`, deprecated `platform`, …). Only the paginated envelope is
//! strict: a wrong envelope is a malformed-response error, not a panic.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The gear taxonomy from the recon. Unknown values fall back to [`Gear::Unknown`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gear {
    Amp,
    AmpCab,
    Pedal,
    Outboard,
    Cab,
    Space,
    Experimental,
    /// Deprecated alias `full-rig`, normalised by the server.
    FullRig,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Capture format. Unknown values fall back to [`Format::Other`] and are
/// skipped by the indexer (the agreed schema only supports `nam` and `ir`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Nam,
    Ir,
    #[default]
    #[serde(other)]
    Other,
}

/// `architecture` query value. Omitting it on `/models` silently excludes A2,
/// so the indexer always sends one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Architecture {
    #[serde(rename = "1")]
    A1,
    #[serde(rename = "2")]
    A2,
    Custom,
}

impl Architecture {
    pub fn as_str(self) -> &'static str {
        match self {
            Architecture::A1 => "1",
            Architecture::A2 => "2",
            Architecture::Custom => "custom",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EmbeddedUser {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub is_verified: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Tag {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
}

impl Tag {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Make {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
}

impl Make {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// A tone, as returned by the feeds, search, and tone detail endpoints.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Tone {
    pub id: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub gear: Option<Gear>,
    #[serde(default)]
    pub format: Option<Format>,
    #[serde(default)]
    pub user: Option<EmbeddedUser>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub makes: Vec<Make>,
    #[serde(default)]
    pub images: Option<Vec<String>>,
    #[serde(default)]
    pub links: Option<Vec<String>>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub models_count: Option<u64>,
    #[serde(default)]
    pub a1_models_count: Option<u64>,
    #[serde(default)]
    pub a2_models_count: Option<u64>,
    #[serde(default)]
    pub custom_models_count: Option<u64>,
    #[serde(default)]
    pub irs_count: Option<u64>,
    #[serde(default)]
    pub downloads_count: Option<u64>,
    #[serde(default)]
    pub favorites_count: Option<u64>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub is_favorite: Option<bool>,
    #[serde(default)]
    pub is_public: Option<bool>,
}

/// A downloadable model inside a tone.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Model {
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
    /// Direct download locator (an `https` URL or an internal `r2://` ref).
    #[serde(default)]
    pub model_url: Option<String>,
    #[serde(default)]
    pub tone_id: Option<u64>,
    /// Equals the NAM file's `metadata.weights_hash` when the API exposes it.
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub architecture_version: Option<String>,
    /// Capture metadata, when the API includes it (`gear_make`, `gear_model`,
    /// `loudness`, `gain`, …).
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub size: Option<Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Model {
    pub fn meta_string(&self, key: &str) -> Option<&str> {
        self.metadata.as_ref()?.get(key)?.as_str()
    }

    pub fn meta_f64(&self, key: &str) -> Option<f64> {
        self.metadata.as_ref()?.get(key)?.as_f64()
    }

    pub fn meta_u64(&self, key: &str) -> Option<u64> {
        self.metadata.as_ref()?.get(key)?.as_u64()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PublicUser {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub is_verified: Option<bool>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub links: Option<Vec<String>>,
}

/// Strict paginated envelope: `{data, page, page_size, total, total_pages}`.
#[derive(Clone, Debug, Deserialize)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
    pub total_pages: u32,
}

/// Non-paginated feed envelope: `{data: [...]}` (trending, latest).
#[derive(Clone, Debug, Deserialize)]
pub struct ToneList {
    #[serde(default)]
    pub data: Vec<Tone>,
}

impl ToneList {
    pub fn into_inner(self) -> Vec<Tone> {
        self.data
    }
}

/// Parameters for `GET /tones/search`. Every field is optional; `to_pairs`
/// skips the unset ones so no empty filter reaches the API.
#[derive(Clone, Debug, Default)]
pub struct SearchParams<'a> {
    pub query: Option<&'a str>,
    pub sort: Option<&'a str>,
    pub gears: Option<&'a str>,
    pub format: Option<&'a str>,
    pub architecture: Option<&'a str>,
    pub tags: Option<&'a str>,
    pub makes: Option<&'a str>,
    pub creators: Option<&'a str>,
    pub calibrated: Option<bool>,
    pub verified: Option<bool>,
    pub sizes: Option<&'a str>,
}

impl SearchParams<'_> {
    pub fn to_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        let mut push = |k: &str, v: String| pairs.push((k.to_string(), v));
        if let Some(v) = self.query {
            push("query", v.to_string());
        }
        if let Some(v) = self.sort {
            push("sort", v.to_string());
        }
        if let Some(v) = self.gears {
            push("gears", v.to_string());
        }
        if let Some(v) = self.format {
            push("format", v.to_string());
        }
        if let Some(v) = self.architecture {
            push("architecture", v.to_string());
        }
        if let Some(v) = self.tags {
            push("tags", v.to_string());
        }
        if let Some(v) = self.makes {
            push("makes", v.to_string());
        }
        if let Some(v) = self.creators {
            push("creators", v.to_string());
        }
        if let Some(v) = self.calibrated {
            push("calibrated", v.to_string());
        }
        if let Some(v) = self.verified {
            push("verified", v.to_string());
        }
        if let Some(v) = self.sizes {
            push("sizes", v.to_string());
        }
        pairs
    }
}
