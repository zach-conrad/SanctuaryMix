//! Recording types. They serialize to camelCase JSON matching `src/lib/types.ts`.

use mix_core::{ChangeSource, ChannelId, ConsoleEvent};
use serde::{Deserialize, Serialize};

/// What audio a recording has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioMode {
    None,
    Stereo,
    StereoMultitrack,
}

/// Where a recording is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingStatus {
    Recording,
    Complete,
    /// The app quit or crashed while recording.
    Interrupted,
}

/// Cloud sync progress of a row or file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncState {
    LocalOnly,
    Pending,
    Uploading,
    Synced,
    Failed,
}

/// What a file in a bundle holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Mix,
    Track,
    Events,
    Manifest,
}

macro_rules! str_enum {
    ($ty:ty { $($variant:ident => $s:literal),+ $(,)? }) => {
        impl $ty {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

str_enum!(AudioMode { None => "none", Stereo => "stereo", StereoMultitrack => "stereoMultitrack" });
str_enum!(RecordingStatus { Recording => "recording", Complete => "complete", Interrupted => "interrupted" });
str_enum!(SyncState {
    LocalOnly => "localOnly",
    Pending => "pending",
    Uploading => "uploading",
    Synced => "synced",
    Failed => "failed",
});
str_enum!(FileKind { Mix => "mix", Track => "track", Events => "events", Manifest => "manifest" });

/// Everything needed to start a recording row.
#[derive(Debug, Clone, PartialEq)]
pub struct NewRecording {
    /// `None` while signed out.
    pub org_id: Option<String>,
    /// Auth user id, `local` when signed out.
    pub created_by: String,
    pub title: String,
    /// Local calendar date `YYYY-MM-DD`.
    pub service_date: String,
    pub started_at: u64,
    pub console_model: String,
    pub sample_rate: Option<u32>,
    pub audio_mode: AudioMode,
    pub mix_channels: Option<[u16; 2]>,
    pub track_count: u16,
}

/// A full `recordings` row. This is also what `manifest.json` holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub id: String,
    pub org_id: Option<String>,
    pub created_by: String,
    pub title: String,
    pub service_date: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub duration_ms: u64,
    pub status: RecordingStatus,
    pub console_model: String,
    pub sample_rate: Option<u32>,
    pub audio_mode: AudioMode,
    pub mix_channels: Option<[u16; 2]>,
    pub track_count: u16,
    pub bytes_on_disk: u64,
    pub notes: String,
    pub sync_state: SyncState,
    pub remote_key: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub deleted_at: Option<u64>,
}

/// One row in the Mix Manager list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSummary {
    pub id: String,
    pub org_id: Option<String>,
    pub created_by: String,
    pub title: String,
    pub service_date: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub duration_ms: u64,
    pub status: RecordingStatus,
    pub console_model: String,
    pub sample_rate: Option<u32>,
    pub audio_mode: AudioMode,
    pub mix_channels: Option<[u16; 2]>,
    pub track_count: u16,
    pub bytes_on_disk: u64,
    pub notes: String,
    /// Control moves recorded (the start snapshot is not counted).
    pub event_count: u64,
    pub sync_state: SyncState,
}

impl RecordingSummary {
    pub fn from_row(r: Recording, event_count: u64) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            created_by: r.created_by,
            title: r.title,
            service_date: r.service_date,
            started_at: r.started_at,
            ended_at: r.ended_at,
            duration_ms: r.duration_ms,
            status: r.status,
            console_model: r.console_model,
            sample_rate: r.sample_rate,
            audio_mode: r.audio_mode,
            mix_channels: r.mix_channels,
            track_count: r.track_count,
            bytes_on_disk: r.bytes_on_disk,
            notes: r.notes,
            event_count,
            sync_state: r.sync_state,
        }
    }
}

/// A file in a bundle, as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingFile {
    pub kind: FileKind,
    /// Device input (0-based) for tracks; `None` otherwise.
    pub channel: Option<u16>,
    pub rel_path: String,
    pub bytes: u64,
}

/// A full `recording_files` row, for sync and integrity checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredFile {
    pub id: String,
    pub recording_id: String,
    pub kind: FileKind,
    pub channel: Option<u16>,
    pub rel_path: String,
    pub bytes: u64,
    /// Hex SHA-256, filled when the file is finalized.
    pub sha256: Option<String>,
    pub sync_state: SyncState,
}

impl From<StoredFile> for RecordingFile {
    fn from(f: StoredFile) -> Self {
        Self {
            kind: f.kind,
            channel: f.channel,
            rel_path: f.rel_path,
            bytes: f.bytes,
        }
    }
}

/// A channel name captured when the recording started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelName {
    pub id: ChannelId,
    pub name: String,
}

/// One recording with its files and starting channel names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingDetail {
    #[serde(flatten)]
    pub summary: RecordingSummary,
    pub files: Vec<RecordingFile>,
    pub channel_names: Vec<ChannelName>,
}

/// A control change inside a recording; `t_ms` is the offset from its start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    pub seq: u64,
    pub t_ms: u64,
    pub source: ChangeSource,
    pub event: ConsoleEvent,
}
