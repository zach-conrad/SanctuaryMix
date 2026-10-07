//! What AI EQ tells the UI and the log. Serializes to camelCase JSON matching
//! `src/lib/types.ts`; the log entries match `eq_audit` in
//! `docs/supabase/eq_audit.sql` so they can sync up as they are.

use automix::ChannelRole;
use mix_core::eq::ChannelEq;
use serde::{Deserialize, Serialize};

/// Where the Dante feed is taken on the desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TapPoint {
    /// Preamp or input socket: the app hears the channel before the desk's EQ
    /// and models that EQ in software (the normal dLive patch).
    #[default]
    BeforeEq,
    /// Post-EQ direct outs: the app already hears the desk's EQ.
    AfterEq,
}

/// The operator's AI EQ choices. Channels and roles are auto-mix's (one pick for both).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiEqConfig {
    /// AI EQ's own on/off, separate from auto-mix.
    pub enabled: bool,
    /// Small, slow tone moves on speech mics during the service (Engineers and Admins switch it).
    pub tone_keeping: bool,
    pub tap: TapPoint,
}

impl Default for AiEqConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tone_keeping: true,
            tap: TapPoint::BeforeEq,
        }
    }
}

/// Why a picked channel's EQ is or isn't being looked after right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EqMode {
    /// AI EQ is off.
    Off,
    ConsoleOffline,
    /// This console's EQ can't be set from SanctuaryMix.
    NotSupported,
    /// Still reading the channel's EQ from the desk.
    Reading,
    Frozen,
    /// No soundcheck yet this service.
    NotChecked,
    /// Soundcheck EQ applied (or kept); guarding for feedback.
    Set,
    /// Speech tone keeping is nudging it right now.
    Keeping,
    /// Holding a feedback notch on band 4.
    Notch,
    /// A person changed the EQ, so it's theirs until handed back.
    Yours,
    /// Undone; held at its soundcheck EQ.
    Undone,
}

/// A soundcheck (or report) proposal for one channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqProposal {
    /// Plain title, e.g. "Less boxy, less harsh on loud notes".
    pub title: String,
    /// One sentence naming what you'd hear and where.
    pub reason: String,
    /// The whole EQ as it would be on the desk.
    pub eq: ChannelEq,
    /// Each change as shown, e.g. "320 Hz  0.0 → −2.0 dB".
    pub changes: Vec<String>,
}

/// A feedback notch AI EQ is holding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notch {
    pub hz: f32,
    pub gain_db: f32,
    pub at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqChannelStatus {
    pub channel: u16,
    pub name: Option<String>,
    pub role: ChannelRole,
    pub mode: EqMode,
    /// What is on the desk now, once read.
    pub eq: Option<ChannelEq>,
    /// What soundcheck set (or the desk had when AI EQ adopted it).
    pub baseline: Option<ChannelEq>,
    pub proposal: Option<EqProposal>,
    pub notch: Option<Notch>,
    /// Speech tone keeping: largest band offset from the baseline right now.
    pub tone_offset_db: Option<f32>,
    /// What the mic hears (1/6 octave, dB relative to its own average), once there's enough.
    pub spectrum: Option<Vec<f32>>,
    /// Seconds of clean signal heard (gated to the channel's own source).
    pub heard_secs: f32,
    /// Differs from the EQ saved for this mic last Sunday.
    pub differs_from_profile: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SoundcheckState {
    UpNext,
    Listening,
    /// Has a proposal waiting.
    Done,
    /// Heard enough; nothing to change.
    SoundsGood,
    /// Listened a while and heard nothing on it.
    NoSound,
    Applied,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundcheckChannel {
    pub channel: u16,
    pub state: SoundcheckState,
    pub heard_secs: f32,
    /// How many changes its proposal makes.
    pub changes: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundcheckStatus {
    pub running: bool,
    /// Picked channels in role order (speech, vocals, band).
    pub channels: Vec<SoundcheckChannel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareSide {
    Before,
    After,
}

/// Before | After listening after a soundcheck apply.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compare {
    pub channel: u16,
    pub side: CompareSide,
}

/// The latest feedback AI EQ caught.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackEvent {
    pub id: u64,
    pub at_ms: u64,
    pub channel: u16,
    pub channel_name: String,
    pub hz: f32,
    /// Notch depth on band 4, or `None` when band 4 was busy (fader only).
    pub notch_db: Option<f32>,
    pub fader_cut_db: f32,
    pub count_today: u32,
}

/// One result from the feedback check (ring-out).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RingResult {
    pub channel: u16,
    /// First frequency that rang, or `None` if it reached its limit without ringing.
    pub hz: Option<f32>,
    /// Fader level where it rang (its feedback ceiling for auto-mix).
    pub ceiling_db: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RingOutStatus {
    pub running: bool,
    pub channel: Option<u16>,
    pub results: Vec<RingResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEqStatus {
    pub enabled: bool,
    pub frozen: bool,
    pub console_online: bool,
    /// The connected desk takes EQ from SanctuaryMix.
    pub eq_supported: bool,
    pub audio_ok: bool,
    pub tone_keeping: bool,
    /// A service is under way (recording, or auto-mix on): soundcheck changes
    /// and Before | After wait for it to end.
    pub in_service: bool,
    pub soundcheck: Option<SoundcheckStatus>,
    pub compare: Option<Compare>,
    pub ring_out: Option<RingOutStatus>,
    pub feedback: Option<FeedbackEvent>,
    /// Picked channels whose desk EQ differs from last Sunday's.
    pub profile_differs: Vec<u16>,
    pub channels: Vec<EqChannelStatus>,
}

/// What happened to one channel's EQ. Matches `eq_audit.action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EqAction {
    /// AI EQ proposal applied at soundcheck.
    Soundcheck,
    /// AI EQ caught ringing and cut it.
    Feedback,
    /// AI EQ kept a speech mic's tone.
    Tone,
    /// Someone undid an AI EQ change.
    Undo,
    /// Someone changed EQ by hand (desk or app); the channel's EQ is theirs.
    Person,
    /// Someone gave the channel back to AI EQ.
    HandBack,
}

impl EqAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Soundcheck => "soundcheck",
            Self::Feedback => "feedback",
            Self::Tone => "tone",
            Self::Undo => "undo",
            Self::Person => "person",
            Self::HandBack => "handBack",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "soundcheck" => Self::Soundcheck,
            "feedback" => Self::Feedback,
            "tone" => Self::Tone,
            "undo" => Self::Undo,
            "person" => Self::Person,
            "handBack" => Self::HandBack,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorKind {
    Ai,
    Person,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorWhere {
    App,
    Desk,
}

/// Who made an EQ change (or tapped Apply on one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqActor {
    pub kind: ActorKind,
    /// `admin`, `engineer` or `volunteer`; `None` for AI and for the desk.
    pub role: Option<String>,
    /// Display name, for the church's own members only.
    pub name: Option<String>,
    /// Account id; `None` when signed out, for AI and for the desk.
    pub user_id: Option<String>,
    #[serde(rename = "where")]
    pub where_: Option<ActorWhere>,
}

impl EqActor {
    pub fn ai() -> Self {
        Self {
            kind: ActorKind::Ai,
            role: None,
            name: None,
            user_id: None,
            where_: None,
        }
    }

    /// Someone on the desk itself; the desk never says who.
    pub fn desk() -> Self {
        Self {
            kind: ActorKind::Person,
            role: None,
            name: None,
            user_id: None,
            where_: Some(ActorWhere::Desk),
        }
    }

    /// A signed-in (or local) person using the app.
    pub fn app(role: &str, name: &str, user_id: Option<&str>) -> Self {
        Self {
            kind: ActorKind::Person,
            role: Some(role.to_string()),
            name: Some(name.to_string()),
            user_id: user_id.map(str::to_string),
            where_: Some(ActorWhere::App),
        }
    }
}

/// One entry in the EQ log: every EQ change, with why and who.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqLogEntry {
    /// Unix time in milliseconds.
    pub at_ms: u64,
    pub channel: u16,
    pub channel_name: String,
    pub action: EqAction,
    /// Exact changes as shown, e.g. "Low cut  off → 100 Hz", "320 Hz  0.0 → −2.0 dB".
    pub changes: Vec<String>,
    /// One sentence a volunteer can check by ear (AI changes).
    pub reason: Option<String>,
    pub by: EqActor,
    /// For AI changes a person applied (soundcheck), who tapped Apply.
    pub applied_by: Option<EqActor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdeaState {
    Waiting,
    Kept,
    Dismissed,
}

impl IdeaState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Kept => "kept",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "waiting" => Self::Waiting,
            "kept" => Self::Kept,
            "dismissed" => Self::Dismissed,
            _ => return None,
        })
    }
}

/// An idea AI EQ collected during a service, for next Sunday. Matches `eq_ideas`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqIdea {
    pub id: String,
    /// The service it came from (the recording, when one was made).
    pub recording_id: Option<String>,
    pub at_ms: u64,
    pub channel: u16,
    pub channel_name: String,
    pub title: String,
    /// The exact change, e.g. "2.5 kHz  0.0 → −6.0 dB (band 4)".
    pub change: String,
    pub reason: String,
    pub state: IdeaState,
    /// What keeping it does to the channel's saved EQ (internal; not shown).
    #[serde(default)]
    pub apply: Option<IdeaApply>,
}

/// What a kept idea changes in the channel's saved profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IdeaApply {
    /// Keep a feedback notch on band 4.
    KeepNotch { hz: f32, gain_db: f32 },
    /// Remove a notch that hasn't been needed.
    DropNotch,
    /// Move a band's saved gain (speech tone drift).
    BandGain { band: u8, gain_db: f32 },
}
