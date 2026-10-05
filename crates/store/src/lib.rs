//! The app's local database.
//!
//! SQLite on the machine is the source of truth (see the approved backend
//! plan): settings, saved auto-mix choices and the auto-mix activity log live
//! here, so the app never needs the internet to mix. Cloud sync comes later.

use std::path::Path;

use automix::{Adjustment, AdjustmentKind};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("couldn't read a saved setting: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Settings keys.
pub mod keys {
    /// The operator's auto-mix choices ([`automix::AutoMixConfig`]).
    pub const AUTOMIX_CONFIG: &str = "automix.config";
    /// Whether the sample account was signed in when the app last closed.
    pub const SIGNED_IN: &str = "auth.signedIn";
}

/// Schema migrations, applied in order. Never edit one that has shipped; add a new one.
const MIGRATIONS: &[&str] = &[
    // 1: settings and the auto-mix log.
    "CREATE TABLE settings (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    CREATE TABLE automix_log (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        at_ms        INTEGER NOT NULL,
        kind         TEXT NOT NULL,
        channel      INTEGER,
        channel_name TEXT,
        from_db      REAL,
        to_db        REAL,
        reason       TEXT NOT NULL
    );
    CREATE INDEX automix_log_at ON automix_log (at_ms);",
];

/// Oldest log entries are trimmed past this many rows.
const LOG_KEEP_ROWS: i64 = 50_000;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version.max(0) as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn append_adjustment(&self, a: &Adjustment) -> Result<()> {
        self.conn.execute(
            "INSERT INTO automix_log (at_ms, kind, channel, channel_name, from_db, to_db, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                a.at_ms as i64,
                serde_json::to_value(a.kind)?.as_str().unwrap_or_default(),
                a.channel,
                a.channel_name,
                a.from_db,
                a.to_db,
                a.reason,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        if id % 1000 == 0 {
            self.conn.execute(
                "DELETE FROM automix_log WHERE id <= ?1",
                [id - LOG_KEEP_ROWS],
            )?;
        }
        Ok(())
    }

    /// Newest first.
    pub fn recent_adjustments(&self, limit: u32) -> Result<Vec<Adjustment>> {
        let mut stmt = self.conn.prepare(
            "SELECT at_ms, kind, channel, channel_name, from_db, to_db, reason
             FROM automix_log ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<u16>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<f32>>(4)?,
                r.get::<_, Option<f32>>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (at_ms, kind, channel, channel_name, from_db, to_db, reason) = row?;
            // Rows from a newer app version with a kind we don't know are skipped.
            let Ok(kind) = serde_json::from_value::<AdjustmentKind>(kind.into()) else {
                continue;
            };
            out.push(Adjustment {
                at_ms: at_ms.max(0) as u64,
                kind,
                channel,
                channel_name,
                from_db,
                to_db,
                reason,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use automix::{AutoMixConfig, ChannelRole, ManagedChannel, RoomFeel};

    #[test]
    fn saves_and_loads_the_automix_config() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            store
                .get_setting::<AutoMixConfig>(keys::AUTOMIX_CONFIG)
                .unwrap(),
            None
        );
        let config = AutoMixConfig {
            feel: RoomFeel::DeepLowEnd,
            channels: vec![ManagedChannel {
                channel: 4,
                role: ChannelRole::Kick,
            }],
            ..AutoMixConfig::default()
        };
        store.set_setting(keys::AUTOMIX_CONFIG, &config).unwrap();
        store.set_setting(keys::AUTOMIX_CONFIG, &config).unwrap();
        assert_eq!(
            store.get_setting(keys::AUTOMIX_CONFIG).unwrap(),
            Some(config)
        );
    }

    #[test]
    fn logs_adjustments_newest_first() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..3u16 {
            store
                .append_adjustment(&Adjustment {
                    at_ms: 1_000 + i as u64,
                    kind: if i == 2 {
                        AdjustmentKind::Frozen
                    } else {
                        AdjustmentKind::Auto
                    },
                    channel: (i < 2).then_some(i),
                    channel_name: Some(format!("Ch {}", i + 1)),
                    from_db: Some(0.0),
                    to_db: Some(0.5),
                    reason: "test".into(),
                })
                .unwrap();
        }
        let got = store.recent_adjustments(2).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].kind, AdjustmentKind::Frozen);
        assert_eq!(got[0].channel, None);
        assert_eq!(got[1].channel, Some(1));
        assert_eq!(got[1].to_db, Some(0.5));
    }

    #[test]
    fn reopening_keeps_data_and_skips_applied_migrations() {
        let dir = std::env::temp_dir().join(format!("sanctuarymix-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        {
            let store = Store::open(&path).unwrap();
            store.set_setting("k", &42).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get_setting::<i32>("k").unwrap(), Some(42));
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
