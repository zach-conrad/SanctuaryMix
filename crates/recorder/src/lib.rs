//! Service recordings.
//!
//! A recording is a folder (the bundle) plus rows in a local SQLite database:
//! the audio files, every control change with its time, and the channel names
//! at the start. See `docs/RECORDINGS.md` for the full design.
//!
//! - [`store`]: the database and bundle folders.
//! - [`log`]: turns the live control stream into recorded events.
//! - [`replay`]: pure helpers to rebuild mixer state and walk events in time.
//! - [`sync`]: the cloud boundary (local only for now).
//! - [`disk`]: size estimates and free-space guards.

pub mod disk;
pub mod log;
pub mod replay;
pub mod store;
pub mod sync;
mod types;

pub use log::{finalize_bundle, ControlLog};
pub use store::Store;
pub use sync::{LocalOnly, RecordingSync};
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("recording {0} not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, RecorderError>;

/// Current wall-clock time in Unix epoch milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Who a new recording belongs to: `(org_id, created_by)` from the session.
/// Signed out, that is `(None, "local")`.
pub fn owner_from_session(session: &auth::Session) -> (Option<String>, String) {
    (
        session.active_org.as_ref().map(|o| o.id.clone()),
        session.user.id.clone(),
    )
}
