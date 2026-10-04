//! Local recording storage: `<root>/recordings.db` plus one bundle folder per
//! recording at `<root>/<id>/`.
//!
//! The connection sits behind a mutex so a [`Store`] can be shared (for
//! example in an `Arc`) between the recorder, the UI commands and sync.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use mix_core::{ChannelId, ChannelKind, ConsoleEvent};
use rusqlite::{params, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};

use crate::{
    now_ms, AudioMode, ChannelName, FileKind, NewRecording, RecordedEvent, RecorderError,
    Recording, RecordingDetail, RecordingStatus, RecordingSummary, Result, StoredFile, SyncState,
};

/// Database file name inside the store root.
pub const DB_FILE: &str = "recordings.db";

/// Schema migrations, applied in order. Append only: never edit a shipped one.
/// `PRAGMA user_version` holds how many have run.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema.
    "
    CREATE TABLE recordings (
        id            TEXT PRIMARY KEY,
        org_id        TEXT,
        created_by    TEXT NOT NULL,
        title         TEXT NOT NULL,
        service_date  TEXT NOT NULL,
        started_at    INTEGER NOT NULL,
        ended_at      INTEGER,
        duration_ms   INTEGER NOT NULL DEFAULT 0,
        status        TEXT NOT NULL,
        console_model TEXT NOT NULL,
        sample_rate   INTEGER,
        audio_mode    TEXT NOT NULL,
        mix_channels  TEXT,
        track_count   INTEGER NOT NULL DEFAULT 0,
        bytes_on_disk INTEGER NOT NULL DEFAULT 0,
        notes         TEXT NOT NULL DEFAULT '',
        sync_state    TEXT NOT NULL DEFAULT 'localOnly',
        remote_key    TEXT,
        created_at    INTEGER NOT NULL,
        updated_at    INTEGER NOT NULL,
        deleted_at    INTEGER
    );
    CREATE INDEX recordings_started ON recordings(started_at);

    CREATE TABLE recording_files (
        id           TEXT PRIMARY KEY,
        recording_id TEXT NOT NULL REFERENCES recordings(id),
        kind         TEXT NOT NULL,
        channel      INTEGER,
        rel_path     TEXT NOT NULL,
        bytes        INTEGER NOT NULL DEFAULT 0,
        sha256       TEXT,
        sync_state   TEXT NOT NULL DEFAULT 'localOnly',
        updated_at   INTEGER NOT NULL,
        UNIQUE (recording_id, rel_path)
    );

    CREATE TABLE control_events (
        recording_id  TEXT NOT NULL REFERENCES recordings(id),
        seq           INTEGER NOT NULL,
        t_ms          INTEGER NOT NULL,
        source        TEXT NOT NULL,
        channel_kind  TEXT,
        channel_index INTEGER,
        kind          TEXT NOT NULL,
        value         TEXT NOT NULL,
        PRIMARY KEY (recording_id, seq)
    );
    CREATE INDEX control_events_time ON control_events(recording_id, t_ms);

    CREATE TABLE recording_channels (
        recording_id  TEXT NOT NULL REFERENCES recordings(id),
        channel_kind  TEXT NOT NULL,
        channel_index INTEGER NOT NULL,
        name          TEXT NOT NULL,
        PRIMARY KEY (recording_id, channel_kind, channel_index)
    );
    ",
];

const RECORDING_COLS: &str = "id, org_id, created_by, title, service_date, started_at, ended_at, \
     duration_ms, status, console_model, sample_rate, audio_mode, mix_channels, track_count, \
     bytes_on_disk, notes, sync_state, remote_key, created_at, updated_at, deleted_at";

const FILE_COLS: &str = "id, recording_id, kind, channel, rel_path, bytes, sha256, sync_state";

/// SQL for the number of control moves (not the start snapshot) in recording `r.id`.
const EVENT_COUNT_SQL: &str =
    "(SELECT COUNT(*) FROM control_events e WHERE e.recording_id = r.id AND e.source != 'snapshot')";

pub struct Store {
    root: PathBuf,
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (or creates) the store at `root` and runs any pending migrations.
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let conn = Connection::open(root.join(DB_FILE))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&conn)?;
        Ok(Self {
            root: root.to_path_buf(),
            conn: Mutex::new(conn),
        })
    }

    /// The folder holding the database and every bundle.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The bundle folder for a recording (`<root>/<id>/`).
    pub fn recording_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Schema version (number of migrations applied).
    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn()
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    // ---- recordings ----

    /// Inserts a new row with status `recording` and creates its bundle folder.
    pub fn create_recording(&self, new: NewRecording) -> Result<Recording> {
        let now = now_ms();
        let rec = Recording {
            id: uuid::Uuid::now_v7().to_string(),
            sync_state: if new.org_id.is_some() {
                SyncState::Pending
            } else {
                SyncState::LocalOnly
            },
            org_id: new.org_id,
            created_by: new.created_by,
            title: new.title,
            service_date: new.service_date,
            started_at: new.started_at,
            ended_at: None,
            duration_ms: 0,
            status: RecordingStatus::Recording,
            console_model: new.console_model,
            sample_rate: new.sample_rate,
            audio_mode: new.audio_mode,
            mix_channels: new.mix_channels,
            track_count: new.track_count,
            bytes_on_disk: 0,
            notes: String::new(),
            remote_key: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        fs::create_dir_all(self.recording_dir(&rec.id))?;
        let mix_channels = rec
            .mix_channels
            .map(|c| serde_json::to_string(&c))
            .transpose()?;
        self.conn().execute(
            &format!(
                "INSERT INTO recordings ({RECORDING_COLS}) VALUES \
                 (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)"
            ),
            params![
                rec.id,
                rec.org_id,
                rec.created_by,
                rec.title,
                rec.service_date,
                rec.started_at as i64,
                None::<i64>,
                0i64,
                rec.status.as_str(),
                rec.console_model,
                rec.sample_rate,
                rec.audio_mode.as_str(),
                mix_channels,
                rec.track_count,
                0i64,
                rec.notes,
                rec.sync_state.as_str(),
                None::<String>,
                now as i64,
                now as i64,
                None::<i64>,
            ],
        )?;
        Ok(rec)
    }

    /// The raw row, including soft-deleted ones.
    pub fn recording(&self, id: &str) -> Result<Recording> {
        self.conn()
            .query_row(
                &format!("SELECT {RECORDING_COLS} FROM recordings WHERE id = ?1"),
                [id],
                row_to_recording,
            )
            .optional()?
            .ok_or_else(|| RecorderError::NotFound(id.to_string()))
    }

    /// Every recording that is not deleted, newest first.
    pub fn list_recordings(&self) -> Result<Vec<RecordingSummary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORDING_COLS}, {EVENT_COUNT_SQL} FROM recordings r \
             WHERE deleted_at IS NULL ORDER BY started_at DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], |row| {
            let rec = row_to_recording(row)?;
            let count: i64 = row.get(21)?;
            Ok(RecordingSummary::from_row(rec, count as u64))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One recording with its files and starting channel names.
    pub fn get_recording(&self, id: &str) -> Result<RecordingDetail> {
        let conn = self.conn();
        let summary = conn
            .query_row(
                &format!(
                    "SELECT {RECORDING_COLS}, {EVENT_COUNT_SQL} FROM recordings r \
                     WHERE id = ?1 AND deleted_at IS NULL"
                ),
                [id],
                |row| {
                    let rec = row_to_recording(row)?;
                    let count: i64 = row.get(21)?;
                    Ok(RecordingSummary::from_row(rec, count as u64))
                },
            )
            .optional()?
            .ok_or_else(|| RecorderError::NotFound(id.to_string()))?;
        let files = query_files(&conn, id)?
            .into_iter()
            .map(Into::into)
            .collect();
        let mut stmt = conn.prepare(
            "SELECT channel_kind, channel_index, name FROM recording_channels \
             WHERE recording_id = ?1 ORDER BY rowid",
        )?;
        let channel_names = stmt
            .query_map([id], |row| {
                let kind: String = row.get(0)?;
                Ok(ChannelName {
                    id: ChannelId {
                        kind: parse_kind(0, &kind)?,
                        index: row.get(1)?,
                    },
                    name: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(RecordingDetail {
            summary,
            files,
            channel_names,
        })
    }

    /// Closes a recording: sets `ended_at`, the duration and the final status.
    pub fn finish_recording(
        &self,
        id: &str,
        ended_at_ms: u64,
        status: RecordingStatus,
    ) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE recordings SET ended_at = ?2, duration_ms = MAX(?2 - started_at, 0), \
             status = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, ended_at_ms as i64, status.as_str(), now_ms() as i64],
        )?;
        found(n, id)
    }

    /// After a crash or quit, closes any row still marked `recording` as
    /// `interrupted`. It ends at its last event, or at its start if it has none.
    /// Returns the ids it closed.
    pub fn mark_interrupted_on_startup(&self) -> Result<Vec<String>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let ids: Vec<String> = {
            let mut stmt = tx.prepare("SELECT id FROM recordings WHERE status = 'recording'")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let now = now_ms() as i64;
        for id in &ids {
            tx.execute(
                "UPDATE recordings SET \
                 ended_at = started_at + COALESCE( \
                     (SELECT MAX(t_ms) FROM control_events WHERE recording_id = ?1), 0), \
                 duration_ms = COALESCE( \
                     (SELECT MAX(t_ms) FROM control_events WHERE recording_id = ?1), 0), \
                 status = 'interrupted', updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn update_title_notes(&self, id: &str, title: &str, notes: &str) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE recordings SET title = ?2, notes = ?3, updated_at = ?4 \
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, title, notes, now_ms() as i64],
        )?;
        found(n, id)
    }

    /// Soft delete: keeps a tombstone row so the deletion can sync, removes
    /// the files, events and the bundle folder. A row that was synced becomes
    /// `pending` so the tombstone uploads; otherwise its sync state is kept.
    pub fn delete_recording(&self, id: &str) -> Result<()> {
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let now = now_ms() as i64;
            let n = tx.execute(
                "UPDATE recordings SET deleted_at = ?2, updated_at = ?2, bytes_on_disk = 0, \
                 sync_state = CASE WHEN sync_state = 'synced' THEN 'pending' ELSE sync_state END \
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![id, now],
            )?;
            found(n, id)?;
            tx.execute("DELETE FROM recording_files WHERE recording_id = ?1", [id])?;
            tx.execute("DELETE FROM control_events WHERE recording_id = ?1", [id])?;
            tx.execute(
                "DELETE FROM recording_channels WHERE recording_id = ?1",
                [id],
            )?;
            tx.commit()?;
        }
        match fs::remove_dir_all(self.recording_dir(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    // ---- channel names ----

    /// Replaces the channel names stored for a recording.
    pub fn set_channel_names(&self, id: &str, names: &[ChannelName]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM recording_channels WHERE recording_id = ?1",
            [id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO recording_channels \
                 (recording_id, channel_kind, channel_index, name) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for n in names {
                stmt.execute(params![id, kind_str(n.id.kind), n.id.index, n.name])?;
            }
        }
        touch(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    // ---- files ----

    /// Registers a file in the bundle (or returns the existing row for that path).
    pub fn add_file(
        &self,
        recording_id: &str,
        kind: FileKind,
        channel: Option<u16>,
        rel_path: &str,
    ) -> Result<StoredFile> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO recording_files \
             (id, recording_id, kind, channel, rel_path, bytes, sync_state, updated_at) \
             SELECT ?1, ?2, ?3, ?4, ?5, 0, \
                    CASE WHEN sync_state = 'localOnly' THEN 'localOnly' ELSE 'pending' END, ?6 \
             FROM recordings WHERE id = ?2 \
             ON CONFLICT (recording_id, rel_path) DO NOTHING",
            params![
                uuid::Uuid::now_v7().to_string(),
                recording_id,
                kind.as_str(),
                channel,
                rel_path,
                now_ms() as i64
            ],
        )?;
        let file = tx
            .query_row(
                &format!(
                    "SELECT {FILE_COLS} FROM recording_files WHERE recording_id = ?1 AND rel_path = ?2"
                ),
                params![recording_id, rel_path],
                row_to_file,
            )
            .optional()?
            .ok_or_else(|| RecorderError::NotFound(recording_id.to_string()))?;
        touch(&tx, recording_id)?;
        tx.commit()?;
        Ok(file)
    }

    /// Sets a file's size (and hash once it is final) and recomputes the
    /// recording's `bytes_on_disk`.
    pub fn update_file(&self, file_id: &str, bytes: u64, sha256: Option<&str>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let recording_id: String = tx
            .query_row(
                "SELECT recording_id FROM recording_files WHERE id = ?1",
                [file_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| RecorderError::NotFound(file_id.to_string()))?;
        tx.execute(
            "UPDATE recording_files SET bytes = ?2, sha256 = COALESCE(?3, sha256), updated_at = ?4 \
             WHERE id = ?1",
            params![file_id, bytes as i64, sha256, now_ms() as i64],
        )?;
        recompute_bytes(&tx, &recording_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Reads a finished file from the bundle and stores its size and SHA-256.
    pub fn finalize_file(&self, file: &StoredFile) -> Result<()> {
        let path = self.recording_dir(&file.recording_id).join(&file.rel_path);
        let (bytes, sha) = hash_file(&path)?;
        self.update_file(&file.id, bytes, Some(&sha))
    }

    /// Every file row for a recording, with hashes and sync state.
    pub fn files(&self, recording_id: &str) -> Result<Vec<StoredFile>> {
        query_files(&self.conn(), recording_id)
    }

    // ---- events ----

    /// Appends events in one transaction.
    pub fn append_events(&self, id: &str, events: &[RecordedEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO control_events \
                 (recording_id, seq, t_ms, source, channel_kind, channel_index, kind, value) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for e in events {
                let (channel, kind, value) = encode_event(&e.event)?;
                stmt.execute(params![
                    id,
                    e.seq as i64,
                    e.t_ms as i64,
                    e.source.as_str(),
                    channel.map(|c| kind_str(c.kind)),
                    channel.map(|c| c.index),
                    kind,
                    value
                ])?;
            }
        }
        touch(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    /// Every event of a recording, in order.
    pub fn events(&self, id: &str) -> Result<Vec<RecordedEvent>> {
        self.query_events(id, 0, i64::MAX)
    }

    /// Events with `from_ms <= t_ms < to_ms`, in order.
    pub fn events_between(&self, id: &str, from_ms: u64, to_ms: u64) -> Result<Vec<RecordedEvent>> {
        self.query_events(id, from_ms as i64, to_ms.min(i64::MAX as u64) as i64)
    }

    fn query_events(&self, id: &str, from: i64, to: i64) -> Result<Vec<RecordedEvent>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT seq, t_ms, source, channel_kind, channel_index, kind, value \
             FROM control_events WHERE recording_id = ?1 AND t_ms >= ?2 AND t_ms < ?3 \
             ORDER BY t_ms, seq",
        )?;
        let rows = stmt.query_map(params![id, from, to], |row| {
            let seq: i64 = row.get(0)?;
            let t_ms: i64 = row.get(1)?;
            let source: String = row.get(2)?;
            let ckind: Option<String> = row.get(3)?;
            let cindex: Option<u16> = row.get(4)?;
            let kind: String = row.get(5)?;
            let value: String = row.get(6)?;
            let channel = match (ckind, cindex) {
                (Some(k), Some(index)) => Some(ChannelId {
                    kind: parse_kind(3, &k)?,
                    index,
                }),
                _ => None,
            };
            let event = decode_event(&kind, channel, &value).map_err(|e| conv_err(6, e))?;
            Ok(RecordedEvent {
                seq: seq as u64,
                t_ms: t_ms as u64,
                source: parse_serde(2, &source)?,
                event,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- retention and disk use ----

    /// Deletes multitrack files of recordings that started more than `days`
    /// ago. Never touches the mix, the events or the row itself. Returns the
    /// bytes freed. The cloud copy (if any) is not affected.
    pub fn delete_multitracks_older_than(&self, days: u32, now_ms: u64) -> Result<u64> {
        let cutoff = now_ms.saturating_sub(u64::from(days) * 86_400_000);
        let targets: Vec<StoredFile> = {
            let conn = self.conn();
            let mut stmt = conn.prepare(&format!(
                "SELECT {} FROM recording_files f JOIN recordings r ON r.id = f.recording_id \
                 WHERE f.kind = 'track' AND r.started_at < ?1 AND r.deleted_at IS NULL \
                 AND r.status != 'recording'",
                FILE_COLS
                    .split(", ")
                    .map(|c| format!("f.{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))?;
            let rows = stmt.query_map([cutoff as i64], row_to_file)?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut freed = 0u64;
        let mut touched: Vec<&str> = Vec::new();
        for f in &targets {
            let path = self.recording_dir(&f.recording_id).join(&f.rel_path);
            let on_disk = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(&path) {
                Ok(()) => freed += on_disk.max(f.bytes),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            if let Some(parent) = path.parent() {
                // Only succeeds once the folder is empty.
                let _ = fs::remove_dir(parent);
            }
            if !touched.contains(&f.recording_id.as_str()) {
                touched.push(&f.recording_id);
            }
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for f in &targets {
            tx.execute("DELETE FROM recording_files WHERE id = ?1", [&f.id])?;
        }
        for id in touched {
            tx.execute("UPDATE recordings SET track_count = 0 WHERE id = ?1", [id])?;
            recompute_bytes(&tx, id)?;
        }
        tx.commit()?;
        Ok(freed)
    }

    /// Bytes used by all recordings that are not deleted.
    pub fn recordings_bytes(&self) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            "SELECT COALESCE(SUM(bytes_on_disk), 0) FROM recordings WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Bytes used by multitrack files of recordings that are not deleted.
    pub fn multitrack_bytes(&self) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            "SELECT COALESCE(SUM(f.bytes), 0) FROM recording_files f \
             JOIN recordings r ON r.id = f.recording_id \
             WHERE f.kind = 'track' AND r.deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    // ---- sync ----

    /// After the first sign-in, gives every local row (no `org_id`) to the
    /// church and marks it `pending` so it uploads. `created_by` moves from
    /// `local` to the signed-in user. Deleted rows are left alone. Returns how
    /// many recordings were claimed.
    pub fn claim_local_recordings(&self, org_id: &str, user_id: &str) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = now_ms() as i64;
        tx.execute(
            "UPDATE recording_files SET sync_state = 'pending', updated_at = ?1 \
             WHERE recording_id IN \
               (SELECT id FROM recordings WHERE org_id IS NULL AND deleted_at IS NULL)",
            [now],
        )?;
        let n = tx.execute(
            "UPDATE recordings SET org_id = ?1, sync_state = 'pending', updated_at = ?3, \
             created_by = CASE WHEN created_by = 'local' THEN ?2 ELSE created_by END \
             WHERE org_id IS NULL AND deleted_at IS NULL",
            params![org_id, user_id, now],
        )?;
        tx.commit()?;
        Ok(n)
    }

    /// Rows waiting to upload (`pending` or `failed`), tombstones included.
    /// Recordings still in progress are skipped.
    pub fn pending_recordings(&self) -> Result<Vec<Recording>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORDING_COLS} FROM recordings \
             WHERE sync_state IN ('pending', 'failed') AND status != 'recording' \
             ORDER BY started_at"
        ))?;
        let rows = stmt.query_map([], row_to_recording)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Records sync progress for a recording (and the storage prefix once known).
    pub fn set_sync_state(
        &self,
        id: &str,
        state: SyncState,
        remote_key: Option<&str>,
    ) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE recordings SET sync_state = ?2, remote_key = COALESCE(?3, remote_key), \
             updated_at = ?4 WHERE id = ?1",
            params![id, state.as_str(), remote_key, now_ms() as i64],
        )?;
        found(n, id)
    }

    /// Records sync progress for one file.
    pub fn set_file_sync_state(&self, file_id: &str, state: SyncState) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE recording_files SET sync_state = ?2, updated_at = ?3 WHERE id = ?1",
            params![file_id, state.as_str(), now_ms() as i64],
        )?;
        found(n, file_id)
    }
}

/// Size and hex SHA-256 of a file.
pub fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((total, hex))
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        conn.execute_batch(&format!(
            "BEGIN; {sql}; PRAGMA user_version = {}; COMMIT;",
            i + 1
        ))?;
    }
    Ok(())
}

fn found(n: usize, id: &str) -> Result<()> {
    if n == 0 {
        Err(RecorderError::NotFound(id.to_string()))
    } else {
        Ok(())
    }
}

fn touch(conn: &Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE recordings SET updated_at = ?2 WHERE id = ?1",
        params![id, now_ms() as i64],
    )
}

fn recompute_bytes(conn: &Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE recordings SET bytes_on_disk = \
           (SELECT COALESCE(SUM(bytes), 0) FROM recording_files WHERE recording_id = ?1), \
         updated_at = ?2 WHERE id = ?1",
        params![id, now_ms() as i64],
    )
}

fn query_files(conn: &Connection, recording_id: &str) -> Result<Vec<StoredFile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {FILE_COLS} FROM recording_files WHERE recording_id = ?1 \
         ORDER BY kind, channel, rel_path"
    ))?;
    let rows = stmt.query_map([recording_id], row_to_file)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn row_to_recording(row: &Row<'_>) -> rusqlite::Result<Recording> {
    let status: String = row.get(8)?;
    let audio_mode: String = row.get(11)?;
    let mix_channels: Option<String> = row.get(12)?;
    let sync_state: String = row.get(16)?;
    Ok(Recording {
        id: row.get(0)?,
        org_id: row.get(1)?,
        created_by: row.get(2)?,
        title: row.get(3)?,
        service_date: row.get(4)?,
        started_at: row.get::<_, i64>(5)? as u64,
        ended_at: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
        duration_ms: row.get::<_, i64>(7)? as u64,
        status: parse_with(8, &status, RecordingStatus::parse)?,
        console_model: row.get(9)?,
        sample_rate: row.get(10)?,
        audio_mode: parse_with(11, &audio_mode, AudioMode::parse)?,
        mix_channels: mix_channels
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| conv_err(12, e))?,
        track_count: row.get(13)?,
        bytes_on_disk: row.get::<_, i64>(14)? as u64,
        notes: row.get(15)?,
        sync_state: parse_with(16, &sync_state, SyncState::parse)?,
        remote_key: row.get(17)?,
        created_at: row.get::<_, i64>(18)? as u64,
        updated_at: row.get::<_, i64>(19)? as u64,
        deleted_at: row.get::<_, Option<i64>>(20)?.map(|v| v as u64),
    })
}

fn row_to_file(row: &Row<'_>) -> rusqlite::Result<StoredFile> {
    let kind: String = row.get(2)?;
    let sync_state: String = row.get(7)?;
    Ok(StoredFile {
        id: row.get(0)?,
        recording_id: row.get(1)?,
        kind: parse_with(2, &kind, FileKind::parse)?,
        channel: row.get(3)?,
        rel_path: row.get(4)?,
        bytes: row.get::<_, i64>(5)? as u64,
        sha256: row.get(6)?,
        sync_state: parse_with(7, &sync_state, SyncState::parse)?,
    })
}

fn conv_err<E: std::error::Error + Send + Sync + 'static>(idx: usize, e: E) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
}

fn parse_with<T>(idx: usize, s: &str, f: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    f(s).ok_or_else(|| {
        conv_err(
            idx,
            io::Error::new(io::ErrorKind::InvalidData, format!("unknown value {s:?}")),
        )
    })
}

/// Parses a camelCase serde enum (like `ChangeSource`) from its bare string.
fn parse_serde<T: serde::de::DeserializeOwned>(idx: usize, s: &str) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|e| conv_err(idx, e))
}

fn parse_kind(idx: usize, s: &str) -> rusqlite::Result<ChannelKind> {
    parse_serde(idx, s)
}

fn kind_str(kind: ChannelKind) -> String {
    match serde_json::to_value(kind) {
        Ok(serde_json::Value::String(s)) => s,
        _ => format!("{kind:?}"),
    }
}

/// Splits an event into its columns: channel, kind and JSON value.
fn encode_event(event: &ConsoleEvent) -> Result<(Option<ChannelId>, &'static str, String)> {
    Ok(match event {
        ConsoleEvent::Fader { id, db } => (Some(*id), "fader", serde_json::to_string(db)?),
        ConsoleEvent::Mute { id, muted } => (Some(*id), "mute", serde_json::to_string(muted)?),
        ConsoleEvent::Name { id, name } => (Some(*id), "name", serde_json::to_string(name)?),
        ConsoleEvent::Connected { model } => (None, "connected", serde_json::to_string(model)?),
        ConsoleEvent::Disconnected { reason } => {
            (None, "disconnected", serde_json::to_string(reason)?)
        }
    })
}

fn decode_event(kind: &str, channel: Option<ChannelId>, value: &str) -> Result<ConsoleEvent> {
    let need_channel =
        || channel.ok_or_else(|| RecorderError::Invalid(format!("{kind} event without a channel")));
    Ok(match kind {
        "fader" => ConsoleEvent::Fader {
            id: need_channel()?,
            db: serde_json::from_str(value)?,
        },
        "mute" => ConsoleEvent::Mute {
            id: need_channel()?,
            muted: serde_json::from_str(value)?,
        },
        "name" => ConsoleEvent::Name {
            id: need_channel()?,
            name: serde_json::from_str(value)?,
        },
        "connected" => ConsoleEvent::Connected {
            model: serde_json::from_str(value)?,
        },
        "disconnected" => ConsoleEvent::Disconnected {
            reason: serde_json::from_str(value)?,
        },
        other => {
            return Err(RecorderError::Invalid(format!(
                "unknown event kind {other}"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mix_core::ChangeSource;

    pub(crate) fn new_rec(started_at: u64) -> NewRecording {
        NewRecording {
            org_id: None,
            created_by: "local".into(),
            title: "Sunday 9:00 AM".into(),
            service_date: "2026-10-04".into(),
            started_at,
            console_model: "simulated".into(),
            sample_rate: Some(48_000),
            audio_mode: AudioMode::StereoMultitrack,
            mix_channels: Some([0, 1]),
            track_count: 2,
        }
    }

    fn ev(seq: u64, t_ms: u64, source: ChangeSource, event: ConsoleEvent) -> RecordedEvent {
        RecordedEvent {
            seq,
            t_ms,
            source,
            event,
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as u32);
        store.create_recording(new_rec(1_000)).unwrap();
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as u32);
        assert_eq!(store.list_recordings().unwrap().len(), 1);
    }

    #[test]
    fn create_list_finish() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let a = store.create_recording(new_rec(1_000)).unwrap();
        let b = store.create_recording(new_rec(5_000)).unwrap();
        assert!(store.recording_dir(&a.id).is_dir());
        assert_eq!(a.status, RecordingStatus::Recording);
        assert_eq!(a.sync_state, SyncState::LocalOnly);

        let ch = ChannelId::input(3);
        store
            .append_events(
                &a.id,
                &[
                    ev(
                        0,
                        0,
                        ChangeSource::Snapshot,
                        ConsoleEvent::Fader {
                            id: ch,
                            db: Some(-5.0),
                        },
                    ),
                    ev(
                        1,
                        100,
                        ChangeSource::Console,
                        ConsoleEvent::Fader { id: ch, db: None },
                    ),
                    ev(
                        2,
                        200,
                        ChangeSource::Operator,
                        ConsoleEvent::Mute {
                            id: ch,
                            muted: true,
                        },
                    ),
                    ev(
                        3,
                        300,
                        ChangeSource::Console,
                        ConsoleEvent::Disconnected { reason: None },
                    ),
                ],
            )
            .unwrap();
        store
            .finish_recording(&a.id, 61_000, RecordingStatus::Complete)
            .unwrap();

        let list = store.list_recordings().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id, "newest first");
        assert_eq!(list[1].status, RecordingStatus::Complete);
        assert_eq!(list[1].duration_ms, 60_000);
        assert_eq!(list[1].event_count, 3, "snapshot rows are not counted");

        let events = store.events(&a.id).unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[1].event, ConsoleEvent::Fader { id: ch, db: None });
        assert_eq!(store.events_between(&a.id, 100, 300).unwrap().len(), 2);

        store
            .set_channel_names(
                &a.id,
                &[ChannelName {
                    id: ch,
                    name: "Pastor".into(),
                }],
            )
            .unwrap();
        store.update_title_notes(&a.id, "Easter", "great").unwrap();
        let detail = store.get_recording(&a.id).unwrap();
        assert_eq!(detail.summary.title, "Easter");
        assert_eq!(detail.channel_names[0].name, "Pastor");

        // camelCase JSON matching src/lib/types.ts.
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["audioMode"], "stereoMultitrack");
        assert_eq!(json["syncState"], "localOnly");
        assert_eq!(json["mixChannels"], serde_json::json!([0, 1]));
        assert_eq!(json["channelNames"][0]["id"]["kind"], "input");
        assert!(json.get("eventCount").is_some());
        let e = serde_json::to_value(&events[2]).unwrap();
        assert_eq!(e["tMs"], 200);
        assert_eq!(e["source"], "operator");
        assert_eq!(e["event"]["type"], "mute");
    }

    #[test]
    fn interrupted_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let a = store.create_recording(new_rec(10_000)).unwrap();
        let b = store.create_recording(new_rec(20_000)).unwrap();
        let ch = ChannelId::input(0);
        store
            .append_events(
                &a.id,
                &[ev(
                    0,
                    4_500,
                    ChangeSource::Console,
                    ConsoleEvent::Mute {
                        id: ch,
                        muted: true,
                    },
                )],
            )
            .unwrap();
        drop(store);

        let store = Store::open(dir.path()).unwrap();
        let mut closed = store.mark_interrupted_on_startup().unwrap();
        closed.sort();
        let mut want = vec![a.id.clone(), b.id.clone()];
        want.sort();
        assert_eq!(closed, want);
        let a = store.recording(&a.id).unwrap();
        assert_eq!(a.status, RecordingStatus::Interrupted);
        assert_eq!(a.ended_at, Some(14_500));
        assert_eq!(a.duration_ms, 4_500);
        let b = store.recording(&b.id).unwrap();
        assert_eq!(b.ended_at, Some(20_000));
        assert!(store.mark_interrupted_on_startup().unwrap().is_empty());
    }

    #[test]
    fn files_and_retention_delete_only_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let day = 86_400_000u64;
        let old = store.create_recording(new_rec(day)).unwrap();
        let recent = store.create_recording(new_rec(40 * day)).unwrap();
        for rec in [&old, &recent] {
            let bundle = store.recording_dir(&rec.id);
            fs::create_dir_all(bundle.join("tracks")).unwrap();
            fs::write(bundle.join("mix.wav"), vec![1u8; 100]).unwrap();
            fs::write(bundle.join("tracks/in-01.wav"), vec![2u8; 50]).unwrap();
            fs::write(bundle.join("tracks/in-02.wav"), vec![3u8; 50]).unwrap();
            let mix = store
                .add_file(&rec.id, FileKind::Mix, None, "mix.wav")
                .unwrap();
            store.finalize_file(&mix).unwrap();
            for (i, p) in ["tracks/in-01.wav", "tracks/in-02.wav"].iter().enumerate() {
                let f = store
                    .add_file(&rec.id, FileKind::Track, Some(i as u16), p)
                    .unwrap();
                store.finalize_file(&f).unwrap();
            }
            store
                .append_events(
                    &rec.id,
                    &[ev(
                        0,
                        0,
                        ChangeSource::Console,
                        ConsoleEvent::Connected {
                            model: "dlive".into(),
                        },
                    )],
                )
                .unwrap();
            store
                .finish_recording(&rec.id, rec.started_at + 1_000, RecordingStatus::Complete)
                .unwrap();
        }
        // Adding the same path twice returns the same row.
        let again = store
            .add_file(&old.id, FileKind::Mix, None, "mix.wav")
            .unwrap();
        assert_eq!(again.bytes, 100);
        assert_eq!(again.sha256.as_deref().map(str::len), Some(64));

        assert_eq!(store.recordings_bytes().unwrap(), 400);
        assert_eq!(store.multitrack_bytes().unwrap(), 200);

        let freed = store.delete_multitracks_older_than(30, 45 * day).unwrap();
        assert_eq!(freed, 100);
        let old_bundle = store.recording_dir(&old.id);
        assert!(old_bundle.join("mix.wav").exists());
        assert!(!old_bundle.join("tracks").exists());
        assert!(store
            .recording_dir(&recent.id)
            .join("tracks/in-01.wav")
            .exists());

        let detail = store.get_recording(&old.id).unwrap();
        assert_eq!(detail.files.len(), 1);
        assert_eq!(detail.files[0].kind, FileKind::Mix);
        assert_eq!(detail.summary.bytes_on_disk, 100);
        assert_eq!(detail.summary.track_count, 0);
        assert_eq!(store.events(&old.id).unwrap().len(), 1);
        assert_eq!(store.multitrack_bytes().unwrap(), 100);
        assert_eq!(store.recordings_bytes().unwrap(), 300);
    }

    #[test]
    fn soft_delete_keeps_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let a = store.create_recording(new_rec(1_000)).unwrap();
        let b = store.create_recording(new_rec(2_000)).unwrap();
        store
            .set_sync_state(&b.id, SyncState::Synced, Some("org/x/recordings/b"))
            .unwrap();

        store.delete_recording(&a.id).unwrap();
        store.delete_recording(&b.id).unwrap();
        assert!(store.list_recordings().unwrap().is_empty());
        assert!(!store.recording_dir(&a.id).exists());
        assert!(matches!(
            store.get_recording(&a.id),
            Err(RecorderError::NotFound(_))
        ));

        let a = store.recording(&a.id).unwrap();
        assert!(a.deleted_at.is_some());
        assert_eq!(a.sync_state, SyncState::LocalOnly);
        let b = store.recording(&b.id).unwrap();
        assert_eq!(b.sync_state, SyncState::Pending);
        assert_eq!(b.remote_key.as_deref(), Some("org/x/recordings/b"));
    }

    #[test]
    fn claim_local() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let a = store.create_recording(new_rec(1_000)).unwrap();
        let f = store
            .add_file(&a.id, FileKind::Mix, None, "mix.wav")
            .unwrap();
        assert_eq!(f.sync_state, SyncState::LocalOnly);
        store
            .finish_recording(&a.id, 2_000, RecordingStatus::Complete)
            .unwrap();
        let mut owned = new_rec(3_000);
        owned.org_id = Some("org-b".into());
        owned.created_by = "user-b".into();
        let b = store.create_recording(owned).unwrap();
        assert_eq!(b.sync_state, SyncState::Pending);

        assert_eq!(store.claim_local_recordings("org-a", "user-a").unwrap(), 1);
        let a = store.recording(&a.id).unwrap();
        assert_eq!(a.org_id.as_deref(), Some("org-a"));
        assert_eq!(a.created_by, "user-a");
        assert_eq!(a.sync_state, SyncState::Pending);
        assert_eq!(
            store.files(&a.id).unwrap()[0].sync_state,
            SyncState::Pending
        );
        assert_eq!(
            store.recording(&b.id).unwrap().org_id.as_deref(),
            Some("org-b")
        );
        assert_eq!(store.claim_local_recordings("org-a", "user-a").unwrap(), 0);

        // `b` is still recording, so only `a` is ready to upload.
        let pending = store.pending_recordings().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, a.id);
    }
}
