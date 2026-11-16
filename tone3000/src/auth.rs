//! Credentials from the environment.
//!
//! Only the variable *names* live here. Values are held in [`Secret`], whose
//! `Debug`/`Display` are redacted, and are never placed in an error message,
//! a log line, or a URL.

use std::fmt;

/// Which kind of Bearer credential was loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthKind {
    /// A per-user OAuth access token (`T3K_ACCESS_TOKEN`). Preferred.
    AccessToken,
    /// An account secret key (`T3K_SECRET_KEY`). Server-side only; using it to
    /// serve many end users conflicts with the TONE3000 terms.
    SecretKey,
}

/// A credential value that must never appear in logs or error messages.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// The raw value. Only for building an `Authorization` header in memory.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(\"***\")")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// A Bearer credential, loaded from the environment or built explicitly.
#[derive(Clone, Debug)]
pub struct Credentials {
    token: Secret,
    kind: AuthKind,
}

impl Credentials {
    /// Read credentials from the environment.
    ///
    /// `T3K_ACCESS_TOKEN` wins over `T3K_SECRET_KEY`. Empty values are treated
    /// as absent. Returns `None` when neither is set (public endpoints still
    /// work; authenticated ones will fail with [`crate::client::Error::Auth`]).
    pub fn from_env() -> Option<Self> {
        if let Some(token) = non_empty_env("T3K_ACCESS_TOKEN") {
            return Some(Self {
                token: Secret::new(token),
                kind: AuthKind::AccessToken,
            });
        }
        if let Some(token) = non_empty_env("T3K_SECRET_KEY") {
            return Some(Self {
                token: Secret::new(token),
                kind: AuthKind::SecretKey,
            });
        }
        None
    }

    /// Build credentials explicitly (tests, embedded use). Prefer
    /// [`Credentials::from_env`] so nothing secret is written into source.
    pub fn from_token(token: impl Into<String>, kind: AuthKind) -> Self {
        Self {
            token: Secret::new(token),
            kind,
        }
    }

    pub fn kind(&self) -> AuthKind {
        self.kind
    }

    /// Value for the `Authorization` header. Never log or display this.
    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.token.as_str())
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_is_redacted_in_debug_and_display() {
        let creds = Credentials::from_token("t3k-super-secret-value", AuthKind::AccessToken);
        let dbg = format!("{creds:?}");
        assert!(!dbg.contains("super-secret"));
        assert_eq!(
            dbg,
            "Credentials { token: Secret(\"***\"), kind: AccessToken }"
        );
        assert_eq!(
            creds.authorization_header(),
            "Bearer t3k-super-secret-value"
        );

        let inner = crate::auth::Secret::new("t3k-inner-secret");
        assert_eq!(format!("{inner}"), "***");
        assert_eq!(format!("{inner:?}"), "Secret(\"***\")");
    }
}
