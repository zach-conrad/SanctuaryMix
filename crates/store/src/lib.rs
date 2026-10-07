//! The app's local database.
//!
//! SQLite on the machine is the source of truth (see the approved backend
//! plan): settings, saved auto-mix choices and the auto-mix activity log live
//! here, so the app never needs the internet to mix. Cloud sync comes later.
//!
//! The EQ log and EQ ideas use the same fields as `eq_audit` and `eq_ideas`
//! in `docs/supabase/eq_audit.sql`, so they can sync up as they are.

use std::path::Path;

use automix::{Adjustment, AdjustmentKind};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tonal::{ActorKind, ActorWhere, EqAction, EqActor, EqIdea, EqLogEntry, IdeaApply, IdeaState};

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
    /// AI EQ's on/off and options ([`tonal::AiEqConfig`]).
    pub const AIEQ_CONFIG: &str = "aieq.config";
    /// Each mic's saved EQ by channel name ([`tonal::engine::Profile`]).
    pub const AIEQ_PROFILES: &str = "aieq.profiles";
    /// Feedback ceilings from the last feedback check, (channel, fader dB).
    pub const AIEQ_CEILINGS: &str = "aieq.ceilings";
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
    // 2: the EQ log (who changed each channel's EQ and why) and EQ ideas.
    "CREATE TABLE eq_log (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        at_ms        INTEGER NOT NULL,
        channel      INTEGER NOT NULL,
        channel_name TEXT NOT NULL,
        action       TEXT NOT NULL,
        changes      TEXT NOT NULL,
        reason       TEXT,
        by_kind      TEXT NOT NULL,
        by_where     TEXT,
        by_role      TEXT,
        by_name      TEXT,
        by_user      TEXT,
        applied_role TEXT,
        applied_name TEXT,
        applied_user TEXT
    );
    CREATE INDEX eq_log_at ON eq_log (at_ms);
    CREATE TABLE eq_ideas (
        id           TEXT PRIMARY KEY NOT NULL,
        recording_id TEXT,
        at_ms        INTEGER NOT NULL,
        channel      INTEGER NOT NULL,
        channel_name TEXT NOT NULL,
        title        TEXT NOT NULL,
        change       TEXT NOT NULL,
        reason       TEXT NOT NULL,
        state        TEXT NOT NULL,
        apply        TEXT
    );
    CREATE INDEX eq_ideas_recording ON eq_ideas (recording_id);",
];

const EQ_COLS: &str = "at_ms, channel, channel_name, action, changes, reason, by_kind, by_where, \
     by_role, by_name, by_user, applied_role, applied_name, applied_user";
const IDEA_COLS: &str =
    "id, recording_id, at_ms, channel, channel_name, title, change, reason, state, apply";

fn where_str(w: Option<ActorWhere>) -> Option<&'static str> {
    w.map(|w| match w {
        ActorWhere::App => "app",
        ActorWhere::Desk => "desk",
    })
}

fn eq_row(r: &rusqlite::Row) -> rusqlite::Result<Option<EqLogEntry>> {
    let action: String = r.get(3)?;
    let Some(action) = EqAction::parse(&action) else {
        return Ok(None);
    };
    let changes: String = r.get(4)?;
    let by_kind: String = r.get(6)?;
    let place = |s: Option<String>| match s.as_deref() {
        Some("app") => Some(ActorWhere::App),
        Some("desk") => Some(ActorWhere::Desk),
        _ => None,
    };
    let by = EqActor {
        kind: if by_kind == "ai" {
            ActorKind::Ai
        } else {
            ActorKind::Person
        },
        where_: place(r.get(7)?),
        role: r.get(8)?,
        name: r.get(9)?,
        user_id: r.get(10)?,
    };
    let applied_role: Option<String> = r.get(11)?;
    let applied_name: Option<String> = r.get(12)?;
    let applied_user: Option<String> = r.get(13)?;
    let applied = applied_role.is_some() || applied_name.is_some() || applied_user.is_some();
    let applied_by = applied.then_some(EqActor {
        kind: ActorKind::Person,
        role: applied_role,
        name: applied_name,
        user_id: applied_user,
        where_: Some(ActorWhere::App),
    });
    Ok(Some(EqLogEntry {
        at_ms: r.get::<_, i64>(0)?.max(0) as u64,
        channel: r.get(1)?,
        channel_name: r.get(2)?,
        action,
        changes: serde_json::from_str(&changes).unwrap_or_default(),
        reason: r.get(5)?,
        by,
        applied_by,
    }))
}

fn idea_row(r: &rusqlite::Row) -> rusqlite::Result<Option<EqIdea>> {
    let state: String = r.get(8)?;
    let Some(state) = IdeaState::parse(&state) else {
        return Ok(None);
    };
    let apply: Option<String> = r.get(9)?;
    Ok(Some(EqIdea {
        id: r.get(0)?,
        recording_id: r.get(1)?,
        at_ms: r.get::<_, i64>(2)?.max(0) as u64,
        channel: r.get(3)?,
        channel_name: r.get(4)?,
        title: r.get(5)?,
        change: r.get(6)?,
        reason: r.get(7)?,
        state,
        apply: apply.and_then(|a| serde_json::from_str::<IdeaApply>(&a).ok()),
    }))
}

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

impl Store {
    pub fn append_eq(&self, e: &EqLogEntry) -> Result<()> {
        let applied = e.applied_by.as_ref();
        self.conn.execute(
            &format!("INSERT INTO eq_log ({EQ_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"),
            params![
                e.at_ms as i64,
                e.channel,
                e.channel_name,
                e.action.as_str(),
                serde_json::to_string(&e.changes)?,
                e.reason,
                match e.by.kind {
                    ActorKind::Ai => "ai",
                    ActorKind::Person => "person",
                },
                where_str(e.by.where_),
                e.by.role,
                e.by.name,
                e.by.user_id,
                applied.and_then(|a| a.role.clone()),
                applied.and_then(|a| a.name.clone()),
                applied.and_then(|a| a.user_id.clone()),
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        if id % 1000 == 0 {
            self.conn
                .execute("DELETE FROM eq_log WHERE id <= ?1", [id - LOG_KEEP_ROWS])?;
        }
        Ok(())
    }

    /// Newest first.
    pub fn recent_eq(&self, limit: u32) -> Result<Vec<EqLogEntry>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {EQ_COLS} FROM eq_log ORDER BY id DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit], eq_row)?;
        Ok(rows
            .filter_map(|r| r.transpose())
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Oldest first, `from_ms <= at_ms < to_ms`.
    pub fn eq_between(&self, from_ms: u64, to_ms: u64) -> Result<Vec<EqLogEntry>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {EQ_COLS} FROM eq_log WHERE at_ms >= ?1 AND at_ms < ?2 ORDER BY at_ms, id"
        ))?;
        let rows = stmt.query_map(params![from_ms as i64, to_ms as i64], eq_row)?;
        Ok(rows
            .filter_map(|r| r.transpose())
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_ideas(&self, ideas: &[EqIdea]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for i in ideas {
            tx.execute(
                &format!("INSERT OR REPLACE INTO eq_ideas ({IDEA_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"),
                params![
                    i.id,
                    i.recording_id,
                    i.at_ms as i64,
                    i.channel,
                    i.channel_name,
                    i.title,
                    i.change,
                    i.reason,
                    i.state.as_str(),
                    i.apply.map(|a| serde_json::to_string(&a)).transpose()?,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A recording's ideas, or (with `None`) every idea still waiting. Newest first.
    pub fn ideas(&self, recording_id: Option<&str>) -> Result<Vec<EqIdea>> {
        let (sql, arg) = match recording_id {
            Some(id) => (
                format!("SELECT {IDEA_COLS} FROM eq_ideas WHERE recording_id = ?1 ORDER BY at_ms DESC, id"),
                id.to_string(),
            ),
            None => (
                format!("SELECT {IDEA_COLS} FROM eq_ideas WHERE state = ?1 ORDER BY at_ms DESC, id"),
                IdeaState::Waiting.as_str().to_string(),
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([arg], idea_row)?;
        Ok(rows
            .filter_map(|r| r.transpose())
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Sets an idea's state and returns it, or `None` if there's no such idea.
    pub fn set_idea_state(&self, id: &str, state: IdeaState) -> Result<Option<EqIdea>> {
        self.conn.execute(
            "UPDATE eq_ideas SET state = ?2 WHERE id = ?1",
            params![id, state.as_str()],
        )?;
        let idea = self
            .conn
            .query_row(
                &format!("SELECT {IDEA_COLS} FROM eq_ideas WHERE id = ?1"),
                [id],
                idea_row,
            )
            .optional()?;
        Ok(idea.flatten())
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

    #[test]
    fn keeps_who_changed_eq() {
        let store = Store::open_in_memory().unwrap();
        let entry = EqLogEntry {
            at_ms: 5_000,
            channel: 3,
            channel_name: "Pastor".into(),
            action: EqAction::Soundcheck,
            changes: vec!["320 Hz  0.0 \u{2192} \u{2212}2.0 dB".into()],
            reason: Some("There's a boxy sound around 320 Hz.".into()),
            by: EqActor::ai(),
            applied_by: Some(EqActor::app("volunteer", "Sam", Some("u-1"))),
        };
        let desk = EqLogEntry {
            at_ms: 6_000,
            action: EqAction::Person,
            reason: None,
            by: EqActor::desk(),
            applied_by: None,
            ..entry.clone()
        };
        store.append_eq(&entry).unwrap();
        store.append_eq(&desk).unwrap();
        assert_eq!(
            store.recent_eq(10).unwrap(),
            vec![desk.clone(), entry.clone()]
        );
        assert_eq!(store.eq_between(5_000, 6_000).unwrap(), vec![entry]);
    }

    #[test]
    fn ideas_wait_until_kept_or_dismissed() {
        let store = Store::open_in_memory().unwrap();
        let idea = EqIdea {
            id: "1-3-0".into(),
            recording_id: Some("rec-1".into()),
            at_ms: 9,
            channel: 3,
            channel_name: "Pastor".into(),
            title: "Keep the 2.5 kHz notch on Pastor".into(),
            change: "2.5 kHz  0.0 \u{2192} \u{2212}6.0 dB (band 4)".into(),
            reason: "It rang twice today.".into(),
            state: IdeaState::Waiting,
            apply: Some(IdeaApply::KeepNotch {
                hz: 2_500.0,
                gain_db: -6.0,
            }),
        };
        store.save_ideas(std::slice::from_ref(&idea)).unwrap();
        assert_eq!(store.ideas(None).unwrap(), vec![idea.clone()]);
        assert_eq!(store.ideas(Some("rec-1")).unwrap(), vec![idea.clone()]);
        let kept = store
            .set_idea_state("1-3-0", IdeaState::Kept)
            .unwrap()
            .unwrap();
        assert_eq!(kept.state, IdeaState::Kept);
        assert!(store.ideas(None).unwrap().is_empty());
        assert_eq!(store.set_idea_state("nope", IdeaState::Kept).unwrap(), None);
    }
}
