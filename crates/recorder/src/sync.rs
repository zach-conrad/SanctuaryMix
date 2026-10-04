//! The cloud boundary for recordings, like `auth::AuthProvider` is for sign-in.
//!
//! Today the app uses [`LocalOnly`], which does nothing. The Supabase plan
//! (draft schema in `docs/supabase/recordings.sql`):
//!
//! 1. On first sign-in, [`RecordingSync::claim_local`] calls
//!    [`Store::claim_local_recordings`] so rows made while signed out belong to
//!    the church and are marked `pending`.
//! 2. [`RecordingSync::push_pending`] walks [`Store::pending_recordings`]
//!    (finished recordings and tombstones). For each one it uploads every
//!    bundle file to the `recordings` Storage bucket under
//!    `org/<org_id>/recordings/<id>/<rel_path>`, checking the stored SHA-256
//!    so a partial or changed upload is caught, then upserts the
//!    `recordings`, `recording_files` and `control_events` rows by id
//!    (last `updated_at` wins). A tombstone upserts `deleted_at` and removes
//!    the Storage objects.
//! 3. Progress is written back with [`Store::set_sync_state`] and
//!    [`Store::set_file_sync_state`]: `uploading`, then `synced` with the
//!    storage prefix as `remote_key`, or `failed` to retry later.
//!
//! Row-level security limits every row and object to members of its church.

use async_trait::async_trait;

use crate::{Result, Store};

/// Storage key for one bundle file.
pub fn storage_key(org_id: &str, recording_id: &str, rel_path: &str) -> String {
    format!("{}/{rel_path}", storage_prefix(org_id, recording_id))
}

/// Storage prefix of a whole bundle (kept as `remote_key` once synced).
pub fn storage_prefix(org_id: &str, recording_id: &str) -> String {
    format!("org/{org_id}/recordings/{recording_id}")
}

#[async_trait]
pub trait RecordingSync: Send + Sync {
    /// False when there is nowhere to sync to.
    fn is_enabled(&self) -> bool;

    /// Gives recordings made while signed out to `org_id` and `user_id`.
    /// Returns how many were claimed.
    async fn claim_local(&self, store: &Store, org_id: &str, user_id: &str) -> Result<usize>;

    /// Uploads pending recordings. Returns how many finished syncing.
    async fn push_pending(&self, store: &Store) -> Result<usize>;
}

/// No cloud: recordings stay on this computer and nothing changes.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalOnly;

#[async_trait]
impl RecordingSync for LocalOnly {
    fn is_enabled(&self) -> bool {
        false
    }

    async fn claim_local(&self, _store: &Store, _org_id: &str, _user_id: &str) -> Result<usize> {
        Ok(0)
    }

    async fn push_pending(&self, _store: &Store) -> Result<usize> {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_only_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let sync: Box<dyn RecordingSync> = Box::new(LocalOnly);
        assert!(!sync.is_enabled());
        assert_eq!(sync.claim_local(&store, "org", "user").await.unwrap(), 0);
        assert_eq!(sync.push_pending(&store).await.unwrap(), 0);
        assert_eq!(
            storage_key("o1", "r1", "tracks/in-01.wav"),
            "org/o1/recordings/r1/tracks/in-01.wav"
        );
    }
}
