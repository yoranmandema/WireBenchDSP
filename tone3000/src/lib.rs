//! Typed TONE3000 v1 API client + a refreshable local model index.
//!
//! Layout:
//!
//! * [`client`] — the HTTP layer: authentication from the environment,
//!   offset-pagination walking, timeout/retry/backoff, structured errors.
//! * [`index`] — the cached index: records, JSON-Schema validation, atomic
//!   persistence, the refresh builder, and the `listModels` query logic.
//! * [`paths`] — `PEDAL_HOME` / `PEDAL_MODELS` resolution.
//! * [`auth`] — credential handling that never exposes a secret.
//!
//! The canonical contracts live in `docs/MODEL-BROWSE-AND-CACHE.md` (index
//! schema + storage) and `docs/TONE3000-API-RECON.md` (endpoints). See
//! `docs/TONE3000-CLIENT.md` for usage.
//!
//! Credentials come from the environment and are never logged. No secret
//! value appears anywhere in this crate's source.

pub mod api;
pub mod auth;
pub mod client;
pub mod index;
pub mod paths;
pub mod query;

#[cfg(test)]
mod mock;

/// Provider id written into every record (`"<provider>:<model_id>"`).
pub const PROVIDER: &str = "t3k";
