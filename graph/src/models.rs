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
//! [`PathResolver`] is the legacy path-only behaviour; the engine supplies an
//! index-aware implementation.

use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
