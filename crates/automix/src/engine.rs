//! The auto-mix controller.
//!
//! [`AutoMix`] is plain, synchronous state with time passed in, so every rule
//! can be tested without audio, a console or a clock. It watches meter frames
//! and console events, and on each [`AutoMix::tick`] proposes fader moves for
//! the channels the operator handed it. Every move goes through
//! [`Guardrails::limit`]. It can only produce fader moves: there is no mute,
//! scene or routing path out of this module.
//!
//! How targets work (research sections 3 and 4):
//! - Anchors (speech mics and lead vocals) ride toward the preset's reference
//!   level, so the pastor and worship leader land in the same place every week.
//! - Everything else rides toward its place relative to the loudest lead
//!   vocal that is singing right now, so builds and quiet verses keep their
//!   shape. With no lead vocal singing, the band is left alone (unless no lead
//!   vocal is handed over at all, then the reference level stands in).
//! - While a speech mic is talking, music steps back to sit well under it.
//!
//! - With listening on, a vocal or speech mic only counts as in use while the
//!   on-device voice detector hears someone at it. A pastor's mic full of
//!   band, or a vocal mic picking up the drums, is bleed: it isn't ridden, it
//!   doesn't make the band step back, and it can't become the lead.
//!
//! Audio input N is assumed to carry console input N (the Dante patch the
//! Setup screen asks for). Levels are estimated post-fader: input RMS plus
//! the fader position.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use mix_core::hearing::{Hearing, HearingFrame, Sound};
use mix_core::level::SILENCE_DB;
use mix_core::{ChannelKind, ConsoleEvent, MeterFrame};
use serde::{Deserialize, Serialize};

use crate::guardrails::{Guardrails, Limit, MoveContext};
use crate::preset::{ChannelRole, Nudges, Preset, RoomFeel};

/// Time constant of the level estimate (the research's 3 s short-term window).
const LEVEL_WINDOW_SECS: f32 = 3.0;
/// A channel stays "in use" this long after its level drops below the gate.
const ACTIVE_HOLD: Duration = Duration::from_millis(1500);
/// How long a channel must be in use before its own fader is ridden.
const WARM_UP: Duration = Duration::from_secs(2);
/// How long a speech mic must be in use before music steps back.
const SPEECH_ONSET: Duration = Duration::from_millis(300);
/// After the operator unmutes, wait this long with signal before riding again.
const UNMUTE_SETTLE: Duration = Duration::from_secs(5);
/// No meter frame for this long means the audio is gone; hold everything.
const STALE_AUDIO: Duration = Duration::from_millis(500);
/// A clip keeps the channel on hold for this long.
const CLIP_HOLD: Duration = Duration::from_secs(3);
/// How fast the remembered peak falls, dB per second.
const PEAK_DECAY_DB_PER_SEC: f32 = 10.0;
/// Once moving, keep going until this close to target (hysteresis with the dead band).
const SETTLE_DB: f32 = 0.5;
/// A console fader report this close to what we sent is our own move coming back.
const ECHO_TOLERANCE_DB: f32 = 0.3;
/// Listening results older than this are ignored (the models stopped or fell behind).
const HEARING_STALE: Duration = Duration::from_millis(1500);
/// After an EQ change on a channel, its level estimate settles for this long first.
const AFTER_EQ_CHANGE: Duration = Duration::from_secs(3);
/// After feedback, no channel is raised for this long.
const FEEDBACK_RAISE_FREEZE: Duration = Duration::from_secs(10);
/// Most input channels a console can have (dLive has 128).
pub const MAX_CHANNELS: u16 = 128;

/// One input the operator handed to auto-mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedChannel {
    /// Console input, 0-based (input 1 is 0).
    pub channel: u16,
    pub role: ChannelRole,
}

/// What the operator chose. Saved between services; auto-mix itself always starts off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoMixConfig {
    pub feel: RoomFeel,
    pub nudges: Nudges,
    pub channels: Vec<ManagedChannel>,
    pub guardrails: Guardrails,
    /// Use the on-device listening models to tell voices from bleed.
    pub listen: bool,
}

impl Default for AutoMixConfig {
    fn default() -> Self {
        Self {
            feel: RoomFeel::FullModern,
            nudges: Nudges::default(),
            channels: Vec::new(),
            guardrails: Guardrails::default(),
            listen: true,
        }
    }
}

impl AutoMixConfig {
    /// Drops duplicates and impossible channels and pulls every number inside its hard limits.
    pub fn sanitized(mut self) -> Self {
        let mut seen = BTreeSet::new();
        self.channels
            .retain(|c| c.channel < MAX_CHANNELS && seen.insert(c.channel));
        self.channels.sort_by_key(|c| c.channel);
        self.nudges = self.nudges.sanitized();
        self.guardrails = self.guardrails.sanitized();
        self
    }

    fn role_of(&self, channel: u16) -> Option<ChannelRole> {
        self.channels
            .iter()
            .find(|c| c.channel == channel)
            .map(|c| c.role)
    }
}

/// Why a managed channel is or isn't moving right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelMode {
    /// Auto-mix is off.
    Off,
    /// The console isn't connected.
    ConsoleOffline,
    /// Everything is frozen; a person has control.
    Frozen,
    /// The operator moved this fader, so auto-mix let go of it.
    HeldByOperator,
    /// The operator undid auto-mix on this channel.
    Undone,
    /// We don't know where the fader is yet.
    WaitingForFader,
    /// Muted on the console, or just unmuted. Auto-mix never mutes or unmutes.
    Muted,
    /// The input is clipping. A fader can't fix that; the preamp gain needs to come down.
    Clipping,
    /// A voice mic with sound on it but no one at it: the band or another
    /// voice bleeding in. Held so bleed is never turned up.
    Bleed,
    /// No audio is arriving from Dante.
    NoAudio,
    /// Nothing is coming through this mic, so it's never raised.
    Idle,
    /// Holding the band where it is until a lead vocal is singing.
    WaitingForLead,
    /// Moving toward its target.
    Riding,
    /// Wants to go further but is at the edge of its allowed range. Often a
    /// sign the preamp gain needs a look.
    AtLimit,
    /// At its target.
    Settled,
}

/// What kind of entry an [`Adjustment`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdjustmentKind {
    /// Auto-mix moved a fader.
    Auto,
    /// The operator undid auto-mix on a channel.
    Undo,
    /// The operator moved a managed fader and auto-mix let go.
    OperatorTookOver,
    Engaged,
    Disengaged,
    Frozen,
    Resumed,
    /// The operator handed a held channel back to auto-mix.
    ChannelResumed,
    /// AI EQ heard feedback and pulled this fader down.
    Feedback,
}

/// One entry in the auto-mix activity log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjustment {
    /// Unix time in milliseconds. Filled in by the runner.
    pub at_ms: u64,
    pub kind: AdjustmentKind,
    pub channel: Option<u16>,
    pub channel_name: Option<String>,
    pub from_db: Option<f32>,
    pub to_db: Option<f32>,
    /// One sentence a volunteer can check by ear.
    pub reason: String,
}

impl Adjustment {
    fn note(kind: AdjustmentKind, reason: impl Into<String>) -> Self {
        Self {
            at_ms: 0,
            kind,
            channel: None,
            channel_name: None,
            from_db: None,
            to_db: None,
            reason: reason.into(),
        }
    }
}

/// A fader move for the runner to send. The only output that reaches the console.
#[derive(Debug, Clone, PartialEq)]
pub struct FaderMove {
    pub channel: u16,
    pub to_db: f32,
    pub limited_by: Option<Limit>,
    /// Logged once the console has the move.
    pub record: Adjustment,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub channel: u16,
    pub name: Option<String>,
    pub role: ChannelRole,
    pub mode: ChannelMode,
    /// `None` when unknown or fully down.
    pub fader_db: Option<f32>,
    /// Where the operator left it.
    pub baseline_db: Option<f32>,
    /// Where its level should sit right now, if auto-mix has a target for it.
    pub target_db: Option<f32>,
    /// Estimated post-fader level, once there's enough signal to tell.
    pub level_db: Option<f32>,
    /// What the listening models hear on it, if they're running.
    pub heard: Option<Sound>,
    /// Whether someone is talking or singing into it, if listening.
    pub voice: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMixStatus {
    pub engaged: bool,
    pub frozen: bool,
    pub console_online: bool,
    pub audio_ok: bool,
    /// The listening models are running and reporting on these channels.
    pub listening: bool,
    pub feel: RoomFeel,
    /// The speech mic music is stepping back for, if someone is talking.
    pub speech_channel: Option<u16>,
    /// The lead vocal everything else is balanced against right now.
    pub lead_channel: Option<u16>,
    pub channels: Vec<ChannelStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    Operator,
    Undone,
}

/// Everything known about one input, managed or not.
#[derive(Debug, Clone, Default)]
struct Track {
    name: Option<String>,
    /// `None` = unknown; `Some(None)` = fully down.
    fader: Option<Option<f32>>,
    muted: bool,
    unmuted_at: Option<Duration>,
    /// Smoothed input power (linear), only updated while there's signal.
    level_pow: Option<f32>,
    peak_dbfs: f32,
    clip_until: Duration,
    active_since: Option<Duration>,
    last_active: Duration,
    /// Last time the input was above the gate, voice or not.
    last_signal: Option<Duration>,
    hearing: Option<(Duration, Hearing)>,
    // Ride state, reset when auto-mix is engaged.
    baseline: Option<f32>,
    hold: Option<Hold>,
    converging: bool,
    /// Converging, but the range or unity limit stops it going further.
    at_limit: bool,
    last_move_at: Option<Duration>,
    /// Last value we sent and haven't seen come back yet.
    pending: Option<f32>,
    /// Where the fader was when this mic rang: never raised above it again
    /// this service.
    ring_cap: Option<f32>,
    /// Feedback ceiling from the soundcheck feedback check.
    ceiling: Option<f32>,
    /// Last EQ change on this channel (anyone's).
    eq_changed_at: Option<Duration>,
}

impl Track {
    fn known_fader(&self) -> Option<f32> {
        self.fader.flatten()
    }

    fn label(&self, channel: u16) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Ch {}", channel + 1))
    }

    fn in_use_for(&self, now: Duration, at_least: Duration) -> bool {
        self.active_since
            .is_some_and(|since| now.saturating_sub(since) >= at_least)
            && self.level_pow.is_some()
    }

    /// Estimated post-fader level.
    fn post_fader_db(&self) -> Option<f32> {
        Some(10.0 * self.level_pow?.max(1e-12).log10() + self.known_fader()?)
    }

    fn settling_after_unmute(&self, now: Duration) -> bool {
        self.unmuted_at.is_some_and(|at| {
            let since = self.active_since.map_or(now, |a| a.max(at));
            now.saturating_sub(since) < UNMUTE_SETTLE || self.active_since.is_none()
        })
    }
}

/// Who everyone is balancing against right now.
#[derive(Debug, Clone, Copy, Default)]
struct References {
    speech: Option<(u16, f32)>,
    lead: Option<(u16, f32)>,
    leads_managed: bool,
}

pub struct AutoMix {
    config: AutoMixConfig,
    engaged: bool,
    frozen: bool,
    console_online: bool,
    last_meter_at: Option<Duration>,
    tracks: BTreeMap<u16, Track>,
    log: Vec<Adjustment>,
    /// No raises anywhere until then (just after feedback).
    raise_freeze_until: Duration,
}

impl AutoMix {
    pub fn new(config: AutoMixConfig) -> Self {
        Self {
            config: config.sanitized(),
            engaged: false,
            frozen: false,
            console_online: false,
            last_meter_at: None,
            tracks: BTreeMap::new(),
            log: Vec::new(),
            raise_freeze_until: Duration::ZERO,
        }
    }

    pub fn config(&self) -> &AutoMixConfig {
        &self.config
    }

    pub fn is_engaged(&self) -> bool {
        self.engaged
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    fn preset(&self) -> Preset {
        self.config.feel.preset()
    }

    /// Log entries since the last call (unstamped).
    pub fn take_log(&mut self) -> Vec<Adjustment> {
        std::mem::take(&mut self.log)
    }

    pub fn configure(&mut self, config: AutoMixConfig) -> &AutoMixConfig {
        let config = config.sanitized();
        for c in &config.channels {
            let was_managed = self.config.role_of(c.channel).is_some();
            let t = self.tracks.entry(c.channel).or_default();
            if !was_managed {
                t.baseline = t.known_fader();
                t.hold = None;
                t.last_move_at = None;
            }
            t.converging = false;
        }
        self.config = config;
        &self.config
    }

    /// Turns auto-mix on or off. Turning on takes every fader's current position as the operator's baseline.
    pub fn set_engaged(&mut self, on: bool) {
        if on == self.engaged {
            return;
        }
        self.engaged = on;
        if on {
            for c in self.config.channels.clone() {
                let t = self.tracks.entry(c.channel).or_default();
                t.baseline = t.known_fader();
                t.hold = None;
                t.converging = false;
                t.last_move_at = None;
                // A new service: last service's ring levels no longer apply.
                t.ring_cap = None;
            }
            self.log.push(Adjustment::note(
                AdjustmentKind::Engaged,
                format!(
                    "Auto-mix started on {} channels with the {} room feel.",
                    self.config.channels.len(),
                    self.preset().name.to_lowercase()
                ),
            ));
        } else {
            self.log.push(Adjustment::note(
                AdjustmentKind::Disengaged,
                "Auto-mix stopped. The faders stay where they are.",
            ));
        }
    }

    /// Stops every automatic move at once. Undo still works.
    pub fn freeze(&mut self) {
        if !self.frozen {
            self.frozen = true;
            self.log.push(Adjustment::note(
                AdjustmentKind::Frozen,
                "Frozen. Auto-mix won't move anything until you resume.",
            ));
        }
    }

    pub fn unfreeze(&mut self) {
        if self.frozen {
            self.frozen = false;
            self.log.push(Adjustment::note(
                AdjustmentKind::Resumed,
                "Resumed. Auto-mix is riding faders again.",
            ));
        }
    }

    /// Hands a held channel back to auto-mix, taking its current fader as the new baseline.
    pub fn resume_channel(&mut self, channel: u16) {
        if self.config.role_of(channel).is_none() {
            return;
        }
        let t = self.tracks.entry(channel).or_default();
        if t.hold.take().is_some() {
            t.baseline = t.known_fader();
            t.converging = false;
            t.last_move_at = None;
            let mut a = Adjustment::note(
                AdjustmentKind::ChannelResumed,
                "Handed back to auto-mix from where you left it.",
            );
            a.channel = Some(channel);
            a.channel_name = Some(t.label(channel));
            self.log.push(a);
        }
    }

    /// Puts a channel back where the operator had it and holds it there.
    /// An operator action, so it isn't rate limited and works while frozen.
    pub fn undo(&mut self, channel: u16) -> Option<FaderMove> {
        self.config.role_of(channel)?;
        let t = self.tracks.get_mut(&channel)?;
        let baseline = t.baseline?;
        let current = t.known_fader()?;
        t.hold = Some(Hold::Undone);
        t.converging = false;
        if (current - baseline).abs() < 0.25 {
            return None;
        }
        Some(FaderMove {
            channel,
            to_db: baseline,
            limited_by: None,
            record: Adjustment {
                at_ms: 0,
                kind: AdjustmentKind::Undo,
                channel: Some(channel),
                channel_name: Some(t.label(channel)),
                from_db: Some(current),
                to_db: Some(baseline),
                reason: "Back where you set it. Auto-mix is leaving this channel alone until you hand it back.".into(),
            },
        })
    }

    /// Undo on every managed channel.
    pub fn undo_all(&mut self) -> Vec<FaderMove> {
        let channels: Vec<u16> = self.config.channels.iter().map(|c| c.channel).collect();
        channels.into_iter().filter_map(|c| self.undo(c)).collect()
    }

    /// Feedback ceilings from the soundcheck feedback check (channel, fader dB).
    /// Auto-mix never raises those faders above them.
    pub fn set_feedback_ceilings(&mut self, ceilings: &[(u16, f32)]) {
        for t in self.tracks.values_mut() {
            t.ceiling = None;
        }
        for &(ch, db) in ceilings {
            if ch < MAX_CHANNELS && db.is_finite() {
                self.tracks.entry(ch).or_default().ceiling = Some(db);
            }
        }
    }

    /// AI EQ heard `channel` ringing at `hz`: pulls its fader down `cut_db`
    /// (whether or not auto-mix is on), holds every raise for 10 s and never
    /// raises this fader above where it rang for the rest of the service.
    /// Nothing moves while frozen: Freeze stops every AI move.
    pub fn feedback_pull(
        &mut self,
        channel: u16,
        cut_db: f32,
        hz: f32,
        now: Duration,
    ) -> Option<FaderMove> {
        if self.frozen || !self.console_online || channel >= MAX_CHANNELS {
            return None;
        }
        let cut = if cut_db.is_finite() {
            cut_db.clamp(0.0, crate::guardrails::hard::MAX_CUT_DB)
        } else {
            return None;
        };
        self.raise_freeze_until = self.raise_freeze_until.max(now + FEEDBACK_RAISE_FREEZE);
        let t = self.tracks.get_mut(&channel)?;
        let current = t.known_fader()?;
        t.ring_cap = Some(t.ring_cap.map_or(current, |c| c.min(current)));
        t.converging = false;
        let floor = t
            .baseline
            .map(|b| b - crate::guardrails::hard::MAX_CUT_DB)
            .unwrap_or(f32::MIN)
            .max(crate::guardrails::hard::ABSOLUTE_FLOOR_DB);
        let to_db = (current - cut).max(floor);
        if current - to_db < 0.25 {
            return None;
        }
        let hz_text = if hz >= 1_000.0 {
            format!("{:.1} kHz", hz / 1_000.0)
        } else {
            format!("{} Hz", hz.round() as i32)
        };
        Some(FaderMove {
            channel,
            to_db,
            limited_by: None,
            record: Adjustment {
                at_ms: 0,
                kind: AdjustmentKind::Feedback,
                channel: Some(channel),
                channel_name: Some(t.label(channel)),
                from_db: Some(current),
                to_db: Some(to_db),
                reason: format!(
                    "Feedback at {hz_text}, so it came down {:.1} dB. It won't go back above where it rang.",
                    current - to_db
                ),
            },
        })
    }

    /// Takes the latest listening results.
    pub fn on_hearing(&mut self, frame: &HearingFrame, now: Duration) {
        for h in &frame.channels {
            self.tracks.entry(h.channel).or_default().hearing = Some((now, *h));
        }
    }

    /// The listening result for a track, if listening is on and current.
    fn heard<'a>(&self, t: &'a Track, now: Duration) -> Option<&'a Hearing> {
        if !self.config.listen {
            return None;
        }
        t.hearing
            .as_ref()
            .filter(|(at, _)| now.saturating_sub(*at) <= HEARING_STALE)
            .map(|(_, h)| h)
    }

    /// A voice mic with sound on it but, by ear, nobody at it.
    fn is_bleed(&self, t: &Track, role: ChannelRole, now: Duration) -> bool {
        role.is_vocal()
            && self.heard(t, now).is_some_and(|h| !h.voice)
            && t.last_signal
                .is_some_and(|at| now.saturating_sub(at) <= ACTIVE_HOLD)
    }

    pub fn on_meter(&mut self, frame: &MeterFrame, now: Duration) {
        let dt = self
            .last_meter_at
            .map(|t| now.saturating_sub(t).as_secs_f32())
            .unwrap_or(0.033)
            .clamp(0.0, 0.5);
        self.last_meter_at = Some(now);
        let gate = self.config.guardrails.gate_dbfs;
        let keep = (-dt / LEVEL_WINDOW_SECS).exp();
        for m in &frame.channels {
            let vocal = self
                .config
                .role_of(m.channel)
                .is_some_and(ChannelRole::is_vocal);
            let listen = self.config.listen;
            let t = self.tracks.entry(m.channel).or_default();
            // On a voice mic, sound without a voice is bleed: it doesn't count
            // as the mic being in use and doesn't feed its level.
            let bleed = vocal
                && listen
                && t.hearing
                    .as_ref()
                    .is_some_and(|(at, h)| now.saturating_sub(*at) <= HEARING_STALE && !h.voice);
            t.peak_dbfs = m
                .peak_db
                .max(t.peak_dbfs - PEAK_DECAY_DB_PER_SEC * dt)
                .max(SILENCE_DB);
            if m.clipped {
                t.clip_until = now + CLIP_HOLD;
            }
            if m.rms_db > gate {
                t.last_signal = Some(now);
            }
            if m.rms_db > gate && !bleed {
                t.active_since.get_or_insert(now);
                t.last_active = now;
                let p = 10f32.powf(m.rms_db / 10.0);
                t.level_pow = Some(match t.level_pow {
                    Some(old) => keep * old + (1.0 - keep) * p,
                    None => p,
                });
            } else if now.saturating_sub(t.last_active) > ACTIVE_HOLD {
                t.active_since = None;
            }
        }
    }

    pub fn on_console(&mut self, event: &ConsoleEvent, now: Duration) {
        match event {
            ConsoleEvent::Connected { .. } => self.console_online = true,
            ConsoleEvent::Disconnected { .. } => {
                self.console_online = false;
                // Positions may change while we can't see the desk. On reconnect
                // whatever the console reports becomes the new starting point;
                // auto-mix never snaps faders back to where it last saw them.
                for t in self.tracks.values_mut() {
                    t.fader = None;
                    t.pending = None;
                    t.baseline = None;
                    t.converging = false;
                }
            }
            ConsoleEvent::Fader { id, db } if id.kind == ChannelKind::Input => {
                self.on_fader(id.index, *db)
            }
            ConsoleEvent::Mute { id, muted } if id.kind == ChannelKind::Input => {
                let t = self.tracks.entry(id.index).or_default();
                if t.muted && !*muted {
                    t.unmuted_at = Some(now);
                }
                t.muted = *muted;
                t.converging = false;
            }
            ConsoleEvent::Name { id, name } if id.kind == ChannelKind::Input => {
                self.tracks.entry(id.index).or_default().name =
                    (!name.is_empty()).then(|| name.clone());
            }
            // EQ changes the level a little; let the estimate settle first.
            ConsoleEvent::Eq { id, .. } if id.kind == ChannelKind::Input => {
                let t = self.tracks.entry(id.index).or_default();
                t.eq_changed_at = Some(now);
            }
            _ => {}
        }
    }

    fn on_fader(&mut self, channel: u16, db: Option<f32>) {
        let managed = self.config.role_of(channel).is_some();
        let engaged = self.engaged;
        let t = self.tracks.entry(channel).or_default();
        let previous = t.fader;
        t.fader = Some(db);

        if let (Some(sent), Some(now_db)) = (t.pending.take(), db) {
            if (now_db - sent).abs() <= ECHO_TOLERANCE_DB {
                return;
            }
        }
        if !(managed && engaged) {
            return;
        }
        match previous {
            // First report since engaging or reconnecting: that's where the operator has it.
            None => {
                if t.baseline.is_none() {
                    t.baseline = db;
                }
            }
            Some(prev) => {
                let same = match (prev, db) {
                    (Some(a), Some(b)) => (a - b).abs() <= ECHO_TOLERANCE_DB,
                    (None, None) => true,
                    _ => false,
                };
                if same || t.hold == Some(Hold::Operator) {
                    return;
                }
                // Someone moved a managed fader by hand. They win, and where
                // they put it becomes the baseline for the rest of the service.
                t.hold = Some(Hold::Operator);
                t.baseline = db;
                t.converging = false;
                let name = t.label(channel);
                self.log.push(Adjustment {
                    at_ms: 0,
                    kind: AdjustmentKind::OperatorTookOver,
                    channel: Some(channel),
                    channel_name: Some(name),
                    from_db: prev,
                    to_db: db,
                    reason: "You moved this fader, so auto-mix let go of it. Hand it back from Assist when you're ready.".into(),
                });
            }
        }
    }

    /// Records that the console accepted a move.
    pub fn confirm_sent(&mut self, mv: &FaderMove, now: Duration) {
        let t = self.tracks.entry(mv.channel).or_default();
        t.fader = Some(Some(mv.to_db));
        t.pending = Some(mv.to_db);
        if mv.record.kind == AdjustmentKind::Auto {
            t.last_move_at = Some(now);
        }
    }

    fn audio_ok(&self, now: Duration) -> bool {
        self.last_meter_at
            .is_some_and(|t| now.saturating_sub(t) <= STALE_AUDIO)
    }

    /// Can this track count as a reference voice right now?
    fn usable_reference(&self, t: &Track, now: Duration, at_least: Duration) -> bool {
        !t.muted && t.hold != Some(Hold::Undone) && t.in_use_for(now, at_least)
    }

    fn references(&self, now: Duration) -> References {
        let mut refs = References::default();
        let loudest = |current: Option<(u16, f32)>, ch: u16, level: f32| match current {
            Some((_, l)) if l >= level => current,
            _ => Some((ch, level)),
        };
        for c in &self.config.channels {
            let Some(t) = self.tracks.get(&c.channel) else {
                if c.role == ChannelRole::LeadVocal {
                    refs.leads_managed = true;
                }
                continue;
            };
            match c.role {
                ChannelRole::Speech if self.usable_reference(t, now, SPEECH_ONSET) => {
                    if let Some(level) = t.post_fader_db() {
                        refs.speech = loudest(refs.speech, c.channel, level);
                    }
                }
                ChannelRole::LeadVocal => {
                    refs.leads_managed = true;
                    if self.usable_reference(t, now, WARM_UP) {
                        if let Some(level) = t.post_fader_db() {
                            refs.lead = loudest(refs.lead, c.channel, level);
                        }
                    }
                }
                _ => {}
            }
        }
        refs
    }

    /// Where `role` should sit right now, or `None` to leave it alone.
    fn target_db(&self, role: ChannelRole, refs: &References) -> Option<f32> {
        let preset = self.preset();
        let nudges = &self.config.nudges;
        let normal = if role.is_anchor() {
            Some(preset.anchor_target_db(role, nudges))
        } else if let Some((_, lead)) = refs.lead {
            Some(lead + preset.offset_db(role, nudges))
        } else if !refs.leads_managed {
            Some(preset.reference_db + nudges.loudness_db + preset.offset_db(role, nudges))
        } else {
            None
        };
        match refs.speech {
            Some((_, speech)) if role.is_music_bed() => {
                let under_speech = speech - preset.speech_over_music_db;
                Some(normal.map_or(under_speech, |n| n.min(under_speech)))
            }
            _ => normal,
        }
    }

    fn mode(&self, t: &Track, role: ChannelRole, refs: &References, now: Duration) -> ChannelMode {
        if !self.engaged {
            ChannelMode::Off
        } else if !self.console_online {
            ChannelMode::ConsoleOffline
        } else if t.hold == Some(Hold::Operator) {
            ChannelMode::HeldByOperator
        } else if t.hold == Some(Hold::Undone) {
            ChannelMode::Undone
        } else if self.frozen {
            ChannelMode::Frozen
        } else if t.known_fader().is_none() || t.baseline.is_none() {
            ChannelMode::WaitingForFader
        } else if t.muted || t.settling_after_unmute(now) {
            ChannelMode::Muted
        } else if !self.audio_ok(now) {
            ChannelMode::NoAudio
        } else if now < t.clip_until {
            ChannelMode::Clipping
        } else if self.is_bleed(t, role, now) && !t.in_use_for(now, WARM_UP) {
            ChannelMode::Bleed
        } else if !t.in_use_for(now, WARM_UP) {
            ChannelMode::Idle
        } else if self.target_db(role, refs).is_none() {
            ChannelMode::WaitingForLead
        } else if t.converging && t.at_limit {
            ChannelMode::AtLimit
        } else if t.converging {
            ChannelMode::Riding
        } else {
            ChannelMode::Settled
        }
    }

    /// Proposes this instant's fader moves. Empty unless engaged, unfrozen and hearing audio.
    pub fn tick(&mut self, now: Duration) -> Vec<FaderMove> {
        if !self.engaged || self.frozen || !self.console_online || !self.audio_ok(now) {
            return Vec::new();
        }
        let refs = self.references(now);
        let preset = self.preset();
        let guardrails = self.config.guardrails;
        let label = |tracks: &BTreeMap<u16, Track>, ch: u16| {
            tracks
                .get(&ch)
                .map(|t| t.label(ch))
                .unwrap_or_else(|| format!("Ch {}", ch + 1))
        };
        let mut moves = Vec::new();

        for c in self.config.channels.clone() {
            let Some(t) = self.tracks.get(&c.channel) else {
                continue;
            };
            if t.eq_changed_at
                .is_some_and(|at| now.saturating_sub(at) < AFTER_EQ_CHANGE)
            {
                continue;
            }
            if !matches!(
                self.mode(t, c.role, &refs, now),
                ChannelMode::Riding | ChannelMode::AtLimit | ChannelMode::Settled
            ) {
                continue;
            }
            let (Some(target), Some(fader), Some(baseline), Some(level)) = (
                self.target_db(c.role, &refs),
                t.known_fader(),
                t.baseline,
                t.post_fader_db(),
            ) else {
                continue;
            };
            let error = target - level;
            let limits = guardrails.limits(c.role.is_speech());

            let t = self.tracks.get_mut(&c.channel).expect("checked above");
            if t.converging {
                if error.abs() < SETTLE_DB {
                    t.converging = false;
                    t.at_limit = false;
                    continue;
                }
            } else if error.abs() <= limits.deadband_db {
                continue;
            } else {
                t.converging = true;
            }
            let (lo, hi) = limits.window(baseline);
            t.at_limit = (error > 0.0 && fader >= hi - 1e-3) || (error < 0.0 && fader <= lo + 1e-3);

            let ctx = MoveContext {
                speech: c.role.is_speech(),
                current_db: fader,
                baseline_db: baseline,
                secs_since_last_move: t
                    .last_move_at
                    .map(|at| now.saturating_sub(at).as_secs_f32())
                    .unwrap_or(f32::MAX),
                peak_dbfs: t.peak_dbfs,
            };
            let Some(mut limited) = guardrails.limit(&ctx, fader + error) else {
                continue;
            };
            if limited.to_db > fader {
                // Just after feedback nothing goes up, and a mic that rang (or
                // has a feedback ceiling) never goes back above that level.
                if now < self.raise_freeze_until {
                    continue;
                }
                let cap = [t.ring_cap, t.ceiling]
                    .into_iter()
                    .flatten()
                    .reduce(f32::min);
                if let Some(cap) = cap {
                    if fader >= cap - 1e-3 {
                        continue;
                    }
                    limited.to_db = limited.to_db.min(cap);
                }
            }
            let up = limited.to_db > fader;
            let step = (limited.to_db - fader).abs();
            let ducking = refs
                .speech
                .filter(|_| c.role.is_music_bed() && !up)
                .map(|(s, _)| s);
            let reason = if let Some(speaker) = ducking {
                format!(
                    "Stepping back {step:.1} dB so {} stays clear.",
                    label(&self.tracks, speaker)
                )
            } else if let (false, Some((lead, _))) = (c.role.is_anchor(), refs.lead) {
                format!(
                    "Was {:.1} dB too {} against {}, so it went {} {step:.1} dB.",
                    error.abs(),
                    if up { "quiet" } else { "loud" },
                    label(&self.tracks, lead),
                    if up { "up" } else { "down" },
                )
            } else {
                format!(
                    "Was {:.1} dB too {} for {}, so it went {} {step:.1} dB.",
                    error.abs(),
                    if up { "quiet" } else { "loud" },
                    preset.name.to_lowercase(),
                    if up { "up" } else { "down" },
                )
            };
            moves.push(FaderMove {
                channel: c.channel,
                to_db: limited.to_db,
                limited_by: limited.limited_by,
                record: Adjustment {
                    at_ms: 0,
                    kind: AdjustmentKind::Auto,
                    channel: Some(c.channel),
                    channel_name: Some(label(&self.tracks, c.channel)),
                    from_db: Some(fader),
                    to_db: Some(limited.to_db),
                    reason,
                },
            });
        }
        moves
    }

    pub fn status(&self, now: Duration) -> AutoMixStatus {
        let refs = self.references(now);
        let empty = Track::default();
        let live = self.engaged && !self.frozen;
        AutoMixStatus {
            engaged: self.engaged,
            frozen: self.frozen,
            console_online: self.console_online,
            audio_ok: self.audio_ok(now),
            listening: self.config.channels.iter().any(|c| {
                self.tracks
                    .get(&c.channel)
                    .is_some_and(|t| self.heard(t, now).is_some())
            }),
            feel: self.config.feel,
            speech_channel: refs.speech.filter(|_| live).map(|(c, _)| c),
            lead_channel: refs.lead.filter(|_| live).map(|(c, _)| c),
            channels: self
                .config
                .channels
                .iter()
                .map(|c| {
                    let t = self.tracks.get(&c.channel).unwrap_or(&empty);
                    let heard = self.heard(t, now);
                    ChannelStatus {
                        channel: c.channel,
                        name: t.name.clone(),
                        role: c.role,
                        mode: self.mode(t, c.role, &refs, now),
                        fader_db: t.known_fader(),
                        baseline_db: t.baseline,
                        target_db: self.target_db(c.role, &refs),
                        level_db: t.post_fader_db().filter(|_| t.in_use_for(now, WARM_UP)),
                        heard: heard.and_then(|h| h.sound),
                        voice: heard.map(|h| h.voice),
                    }
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrails::RideLimits;
    use mix_core::{ChannelId, ChannelMeter};

    const MS: Duration = Duration::from_millis(1);

    fn config(channels: &[(u16, ChannelRole)]) -> AutoMixConfig {
        AutoMixConfig {
            channels: channels
                .iter()
                .map(|&(channel, role)| ManagedChannel { channel, role })
                .collect(),
            ..AutoMixConfig::default()
        }
    }

    /// A tiny world: an engine, a console that does what it's told, and a clock.
    struct Rig {
        mix: AutoMix,
        now: Duration,
        faders: BTreeMap<u16, f32>,
        /// Input RMS per channel, dBFS.
        rms: BTreeMap<u16, f32>,
        /// What the listening models report per channel (voice, sound), sent every 100 ms.
        hear: BTreeMap<u16, (bool, Option<Sound>)>,
        sent: Vec<FaderMove>,
    }

    impl Rig {
        /// Every channel starts at `start_db` on the desk.
        fn new(channels: &[(u16, ChannelRole)], start_db: f32) -> Self {
            let mut mix = AutoMix::new(config(channels));
            mix.on_console(
                &ConsoleEvent::Connected {
                    model: "test".into(),
                },
                Duration::ZERO,
            );
            let mut rig = Self {
                mix,
                now: Duration::ZERO,
                faders: BTreeMap::new(),
                rms: BTreeMap::new(),
                hear: BTreeMap::new(),
                sent: Vec::new(),
            };
            for ch in 0..8 {
                rig.operator_sets(ch, start_db);
            }
            rig
        }

        fn console(&mut self, event: ConsoleEvent) {
            self.mix.on_console(&event, self.now);
        }

        fn operator_sets(&mut self, ch: u16, db: f32) {
            self.faders.insert(ch, db);
            self.console(ConsoleEvent::Fader {
                id: ChannelId::input(ch),
                db: Some(db),
            });
        }

        /// Runs the 30 Hz meters and a 10 Hz control loop for `secs`.
        fn run(&mut self, secs: f32) {
            let end = self.now + Duration::from_secs_f32(secs);
            let mut frame_no = 0u32;
            while self.now < end {
                self.now += 33 * MS;
                frame_no += 1;
                let frame = MeterFrame {
                    sample_rate: 48_000,
                    channels: (0..8)
                        .map(|ch| {
                            let rms = *self.rms.get(&ch).unwrap_or(&SILENCE_DB);
                            ChannelMeter {
                                channel: ch,
                                rms_db: rms,
                                peak_db: if rms <= SILENCE_DB { rms } else { rms + 10.0 },
                                clipped: false,
                            }
                        })
                        .collect(),
                };
                self.mix.on_meter(&frame, self.now);
                if frame_no.is_multiple_of(3) && !self.hear.is_empty() {
                    let hearing = HearingFrame {
                        channels: self
                            .hear
                            .iter()
                            .map(|(&channel, &(voice, sound))| Hearing {
                                channel,
                                voice,
                                voice_prob: if voice { 0.9 } else { 0.05 },
                                sound,
                                confidence: 0.8,
                            })
                            .collect(),
                    };
                    self.mix.on_hearing(&hearing, self.now);
                }
                if frame_no.is_multiple_of(3) {
                    for mv in self.mix.tick(self.now) {
                        self.apply(mv);
                    }
                }
            }
        }

        fn apply(&mut self, mv: FaderMove) {
            self.mix.confirm_sent(&mv, self.now);
            self.faders.insert(mv.channel, mv.to_db);
            self.console(ConsoleEvent::Fader {
                id: ChannelId::input(mv.channel),
                db: Some(mv.to_db),
            });
            self.sent.push(mv);
        }

        fn fader(&self, ch: u16) -> f32 {
            self.faders[&ch]
        }

        fn status(&self, ch: u16) -> ChannelStatus {
            self.mix
                .status(self.now)
                .channels
                .into_iter()
                .find(|c| c.channel == ch)
                .unwrap()
        }

        fn mode(&self, ch: u16) -> ChannelMode {
            self.status(ch).mode
        }

        fn moves_on(&self, ch: u16) -> usize {
            self.sent.iter().filter(|m| m.channel == ch).count()
        }
    }

    #[test]
    fn does_nothing_until_engaged() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.rms.insert(0, -40.0);
        rig.run(10.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(0), ChannelMode::Off);
    }

    #[test]
    fn rides_a_quiet_lead_vocal_up_gently_to_the_range_limit() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        // Full and modern puts a lead vocal at −20; it's at −40 (−30 in, −10 fader).
        rig.rms.insert(0, -30.0);
        rig.run(20.0);
        // Never more than +6 dB over where the operator left it.
        assert_eq!(rig.fader(0), -4.0);
        for pair in rig.sent.windows(2) {
            assert_eq!(pair[1].record.from_db, pair[0].record.to_db);
        }
        for mv in &rig.sent {
            assert!(
                (mv.to_db - mv.record.from_db.unwrap()).abs() <= 0.5 + 1e-4,
                "{mv:?}"
            );
        }
        assert_eq!(rig.sent.len(), 12);
    }

    #[test]
    fn never_raises_past_unity() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -2.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -35.0);
        rig.run(20.0);
        assert_eq!(rig.fader(0), 0.0);
        assert_eq!(rig.sent.last().unwrap().limited_by, Some(Limit::Unity));
        assert_eq!(rig.mode(0), ChannelMode::AtLimit);
    }

    #[test]
    fn speech_moves_faster_than_music_but_within_its_rate() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::Speech), (1, ChannelRole::LeadVocal)],
            -15.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -20.0); // speech at −35, target −20
        rig.rms.insert(1, -20.0); // lead at −35, target −20
        rig.run(2.0); // warm-up: nothing yet
        assert!(rig.sent.is_empty());
        rig.run(2.0);
        let speech = rig.fader(0) - -15.0;
        let music = rig.fader(1) - -15.0;
        assert!(speech > music, "speech {speech} music {music}");
        assert!(speech <= 3.0 * 2.0 + 1e-3);
        assert!(music <= 1.5 * 2.0 + 1e-3);
    }

    #[test]
    fn leaves_a_fader_alone_inside_the_dead_band() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -10.8); // −20.8 post-fader: 0.8 dB under, dead band is 1.0
        rig.run(10.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(0), ChannelMode::Settled);
    }

    #[test]
    fn never_raises_an_idle_mic() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -70.0); // below the gate: nobody singing
        rig.run(10.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(0), ChannelMode::Idle);
    }

    #[test]
    fn balances_the_band_against_the_lead_vocal() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::LeadVocal), (1, ChannelRole::KeysPads)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -10.0); // lead at −20: right on target
        rig.rms.insert(1, -6.0); // keys at −16: should be 8 under the lead (−28)
        rig.run(20.0);
        assert_eq!(rig.moves_on(0), 0);
        assert_eq!(rig.fader(1), -22.0);
        assert!(
            rig.sent[0].record.reason.contains("too loud against Ch 1"),
            "{}",
            rig.sent[0].record.reason
        );
    }

    #[test]
    fn holds_the_band_while_no_lead_vocal_is_singing() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::LeadVocal), (1, ChannelRole::KeysPads)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(1, -6.0); // instrumental: keys loud, no vocal
        rig.run(10.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(1), ChannelMode::WaitingForLead);
    }

    #[test]
    fn a_build_where_everything_rises_together_needs_no_correction() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::LeadVocal), (1, ChannelRole::Drums)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -10.0);
        rig.rms.insert(1, -16.0); // 6 under the lead: on its Full and modern spot
        rig.run(6.0);
        // The bridge: both come up 4 dB. The lead vocal's anchor pulls it back a
        // little, but drums stay in their place relative to it.
        rig.rms.insert(0, -6.0);
        rig.rms.insert(1, -12.0);
        rig.run(10.0);
        let drums_moves = rig.moves_on(1);
        let lead_moves = rig.moves_on(0);
        assert!(lead_moves > 0);
        assert!(
            drums_moves <= lead_moves,
            "drums {drums_moves} lead {lead_moves}"
        );
    }

    #[test]
    fn music_steps_back_while_the_pastor_talks() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::Speech), (1, ChannelRole::KeysPads)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        // Pad on its own, no lead vocal handed over: −28 is its spot (−20 − 8).
        rig.rms.insert(1, -18.0);
        rig.run(5.0);
        assert!(rig.sent.is_empty());
        // Pastor talks at −20. Music has to sit 15 under: −35, so 7 dB down.
        rig.rms.insert(0, -10.0);
        rig.run(15.0);
        assert_eq!(rig.fader(1), -17.0);
        assert_eq!(rig.moves_on(0), 0);
        assert!(
            rig.sent[0].record.reason.contains("so Ch 1 stays clear"),
            "{}",
            rig.sent[0].record.reason
        );
        assert_eq!(rig.mix.status(rig.now).speech_channel, Some(0));
    }

    #[test]
    fn band_bleeding_into_the_pastor_mic_does_not_duck_the_band() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::Speech), (1, ChannelRole::KeysPads)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(1, -18.0); // pad on its spot
                                  // The pastor's mic is loud, but all it hears is the band.
        rig.rms.insert(0, -10.0);
        rig.hear.insert(0, (false, Some(Sound::Music)));
        rig.run(15.0);
        assert!(rig.sent.is_empty(), "{:?}", rig.sent);
        assert_eq!(rig.mode(0), ChannelMode::Bleed);
        assert_eq!(rig.mix.status(rig.now).speech_channel, None);
        let status = rig.status(0);
        assert_eq!(status.heard, Some(Sound::Music));
        assert_eq!(status.voice, Some(false));
        assert!(rig.mix.status(rig.now).listening);

        // Now the pastor actually talks: music steps back as usual.
        rig.hear.insert(0, (true, Some(Sound::Speech)));
        rig.run(15.0);
        assert_eq!(rig.fader(1), -17.0);
        assert_eq!(rig.mix.status(rig.now).speech_channel, Some(0));
    }

    #[test]
    fn bleed_on_a_vocal_mic_is_never_turned_up() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        // Drums in the vocal mic between songs: quiet enough that riding
        // on level alone would push the fader up.
        rig.rms.insert(0, -30.0);
        rig.hear.insert(0, (false, Some(Sound::Drums)));
        rig.run(20.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(0), ChannelMode::Bleed);
        assert_eq!(rig.status(0).level_db, None);
    }

    #[test]
    fn bleed_does_not_count_as_the_lead_singing() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::LeadVocal), (1, ChannelRole::KeysPads)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -10.0); // loud with keys bleed
        rig.hear.insert(0, (false, Some(Sound::Keys)));
        rig.rms.insert(1, -6.0);
        rig.run(10.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(1), ChannelMode::WaitingForLead);
    }

    #[test]
    fn falls_back_to_levels_when_listening_stops_or_is_off() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.hear.insert(0, (false, None));
        rig.run(5.0);
        assert!(rig.sent.is_empty());
        // The models stop reporting: after a moment, level-only riding resumes.
        rig.hear.clear();
        rig.run(10.0);
        assert!(!rig.sent.is_empty());
        assert!(!rig.mix.status(rig.now).listening);

        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        let mut c = rig.mix.config().clone();
        c.listen = false;
        rig.mix.configure(c);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.hear.insert(0, (false, Some(Sound::Drums)));
        rig.run(10.0);
        assert!(!rig.sent.is_empty());
        assert_eq!(rig.status(0).heard, None);
    }

    #[test]
    fn listening_only_gates_voice_mics() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::LeadVocal), (1, ChannelRole::KeysPads)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -10.0);
        rig.hear.insert(0, (true, Some(Sound::Singing)));
        rig.rms.insert(1, -6.0);
        // Keys hear no voice, which is just what a keys mic should hear.
        rig.hear.insert(1, (false, Some(Sound::Keys)));
        rig.run(20.0);
        assert_eq!(rig.fader(1), -22.0);
    }

    #[test]
    fn never_touches_unselected_channels() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.rms.insert(1, -45.0);
        rig.run(10.0);
        assert!(!rig.sent.is_empty());
        assert!(rig.sent.iter().all(|m| m.channel == 0));
    }

    #[test]
    fn leaves_muted_and_just_unmuted_channels_alone() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.console(ConsoleEvent::Mute {
            id: ChannelId::input(0),
            muted: true,
        });
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(10.0);
        assert!(rig.sent.is_empty());
        assert_eq!(rig.mode(0), ChannelMode::Muted);

        rig.console(ConsoleEvent::Mute {
            id: ChannelId::input(0),
            muted: false,
        });
        rig.run(4.5);
        assert!(rig.sent.is_empty(), "waits 5 s after an unmute");
        rig.run(2.0);
        assert!(!rig.sent.is_empty());
    }

    #[test]
    fn operator_move_takes_over_the_channel() {
        let mut rig = Rig::new(
            &[(0, ChannelRole::LeadVocal), (1, ChannelRole::Speech)],
            -10.0,
        );
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.rms.insert(1, -30.0);
        rig.run(3.0);
        assert!(rig.moves_on(0) > 0);
        rig.operator_sets(0, -12.0);
        let log = rig.mix.take_log();
        assert!(log
            .iter()
            .any(|a| a.kind == AdjustmentKind::OperatorTookOver && a.channel == Some(0)));
        let before = rig.sent.len();
        rig.run(5.0);
        assert!(rig.sent[before..].iter().all(|m| m.channel != 0));
        assert_eq!(rig.fader(0), -12.0);
        assert_eq!(rig.mode(0), ChannelMode::HeldByOperator);
        assert_eq!(
            rig.status(0).baseline_db,
            Some(-12.0),
            "their spot is the new baseline"
        );
        assert!(
            rig.mix.undo(0).is_none(),
            "undo never reverses a person's move"
        );
        assert!(
            rig.sent[before..].iter().any(|m| m.channel == 1),
            "others keep riding"
        );

        // Handed back, it rides again from the operator's new spot.
        rig.mix.resume_channel(0);
        assert_eq!(rig.status(0).baseline_db, Some(-12.0));
        rig.run(3.0);
        assert!(rig.fader(0) > -12.0);
    }

    #[test]
    fn our_own_echo_is_not_a_takeover() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(5.0);
        assert!(!rig.sent.is_empty());
        assert!(rig
            .mix
            .take_log()
            .iter()
            .all(|a| a.kind != AdjustmentKind::OperatorTookOver));
    }

    #[test]
    fn freeze_stops_everything_and_undo_still_works() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(5.0);
        assert!(rig.fader(0) > -10.0);
        rig.mix.freeze();
        let before = rig.sent.len();
        rig.run(5.0);
        assert_eq!(rig.sent.len(), before);
        assert_eq!(rig.mode(0), ChannelMode::Frozen);

        let undo = rig.mix.undo(0).expect("there's something to undo");
        assert_eq!(undo.to_db, -10.0);
        assert_eq!(undo.record.kind, AdjustmentKind::Undo);
        rig.apply(undo);

        rig.mix.unfreeze();
        rig.run(5.0);
        assert_eq!(rig.fader(0), -10.0, "an undone channel stays put");
        assert_eq!(rig.mode(0), ChannelMode::Undone);
    }

    #[test]
    fn feedback_pulls_the_fader_even_when_auto_mix_is_off() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech)], -5.0);
        let mv = rig
            .mix
            .feedback_pull(0, 3.0, 2_500.0, rig.now)
            .expect("pulls the fader");
        assert_eq!(mv.to_db, -8.0);
        assert_eq!(mv.record.kind, AdjustmentKind::Feedback);
        assert!(mv.record.reason.contains("2.5 kHz"), "{}", mv.record.reason);
        rig.apply(mv);
        rig.mix.freeze();
        assert!(rig.mix.feedback_pull(0, 3.0, 2_500.0, rig.now).is_none());
    }

    #[test]
    fn a_rung_fader_never_goes_back_above_where_it_rang() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(2.0);
        let rang_at = rig.fader(0);
        let mv = rig.mix.feedback_pull(0, 3.0, 800.0, rig.now).unwrap();
        rig.apply(mv);
        // Raises wait 10 s, then stop at where it rang.
        rig.run(9.0);
        assert_eq!(rig.fader(0), rang_at - 3.0);
        rig.run(20.0);
        assert!(rig.fader(0) <= rang_at + 1e-4, "{}", rig.fader(0));
    }

    #[test]
    fn soundcheck_ceilings_cap_raises() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_feedback_ceilings(&[(0, -8.0)]);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(20.0);
        assert!(rig.fader(0) <= -8.0 + 1e-4, "{}", rig.fader(0));
    }

    #[test]
    fn waits_after_an_eq_change() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(1.0);
        rig.console(ConsoleEvent::Eq {
            id: ChannelId::input(0),
            change: mix_core::eq::EqChange::BandGain { band: 1, db: -2.0 },
        });
        let before = rig.moves_on(0);
        rig.run(2.5);
        assert_eq!(rig.moves_on(0), before);
        rig.run(3.0);
        assert!(rig.moves_on(0) > before);
    }

    #[test]
    fn holds_when_audio_stops() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(3.0);
        rig.now += Duration::from_secs(1);
        assert!(rig.mix.tick(rig.now).is_empty());
        assert_eq!(rig.mode(0), ChannelMode::NoAudio);
    }

    #[test]
    fn a_clipping_input_is_left_alone() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -5.0); // too loud: would come down
        rig.run(2.5);
        let clip = MeterFrame {
            sample_rate: 48_000,
            channels: vec![ChannelMeter {
                channel: 0,
                rms_db: -5.0,
                peak_db: 0.0,
                clipped: true,
            }],
        };
        rig.mix.on_meter(&clip, rig.now);
        let before = rig.sent.len();
        rig.run(2.0);
        assert_eq!(rig.sent.len(), before, "a fader can't fix input clipping");
        assert_eq!(rig.mode(0), ChannelMode::Clipping);
    }

    #[test]
    fn waits_to_learn_where_a_fader_is() {
        let mut mix = AutoMix::new(config(&[(3, ChannelRole::KeysPads)]));
        mix.on_console(
            &ConsoleEvent::Connected { model: "t".into() },
            Duration::ZERO,
        );
        mix.set_engaged(true);
        assert_eq!(
            mix.status(Duration::ZERO).channels[0].mode,
            ChannelMode::WaitingForFader
        );
        mix.on_console(
            &ConsoleEvent::Fader {
                id: ChannelId::input(3),
                db: Some(-5.0),
            },
            Duration::ZERO,
        );
        assert_eq!(
            mix.status(Duration::ZERO).channels[0].baseline_db,
            Some(-5.0)
        );
        assert!(mix
            .take_log()
            .iter()
            .all(|a| a.kind != AdjustmentKind::OperatorTookOver));
    }

    #[test]
    fn adopts_the_console_after_a_reconnect() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -30.0);
        rig.run(4.0);
        rig.console(ConsoleEvent::Disconnected { reason: None });
        assert_eq!(rig.mode(0), ChannelMode::ConsoleOffline);
        rig.console(ConsoleEvent::Connected { model: "t".into() });
        // Someone pulled it down while we were away.
        rig.operator_sets(0, -25.0);
        assert!(rig
            .mix
            .take_log()
            .iter()
            .all(|a| a.kind != AdjustmentKind::OperatorTookOver));
        assert_eq!(rig.status(0).baseline_db, Some(-25.0));
        rig.run(4.0);
        // Rides from there, inside the new range; never snaps back.
        assert!(rig.fader(0) > -25.0 && rig.fader(0) <= -19.0);
    }

    #[test]
    fn nudges_shift_the_targets() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal)], -10.0);
        let mut c = rig.mix.config().clone();
        c.nudges.loudness_db = 2.0;
        rig.mix.configure(c);
        rig.mix.set_engaged(true);
        rig.rms.insert(0, -14.0); // −24: the target moves from −20 to −18
        rig.run(10.0);
        assert_eq!(rig.fader(0), -4.0);
        assert_eq!(rig.status(0).target_db, Some(-18.0));
    }

    #[test]
    fn config_is_sanitized() {
        let c = AutoMixConfig {
            channels: vec![
                ManagedChannel {
                    channel: 2,
                    role: ChannelRole::KeysPads,
                },
                ManagedChannel {
                    channel: 2,
                    role: ChannelRole::Bass,
                },
                ManagedChannel {
                    channel: 999,
                    role: ChannelRole::Bass,
                },
            ],
            nudges: Nudges {
                loudness_db: 40.0,
                ..Nudges::default()
            },
            guardrails: Guardrails {
                music: RideLimits {
                    max_step_db: 9.0,
                    ..RideLimits::MUSIC
                },
                ..Guardrails::default()
            },
            ..AutoMixConfig::default()
        }
        .sanitized();
        assert_eq!(c.channels.len(), 1);
        assert_eq!(c.channels[0].role, ChannelRole::KeysPads);
        assert_eq!(c.nudges.loudness_db, 3.0);
        assert_eq!(c.guardrails.music.max_step_db, 1.0);
    }

    #[test]
    fn config_round_trips_as_camel_case_json() {
        let c = config(&[(0, ChannelRole::LeadVocal)]);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"leadVocal\""), "{json}");
        assert!(json.contains("\"maxStepDb\""), "{json}");
        assert!(json.contains("\"fullModern\""), "{json}");
        let back: AutoMixConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
        let partial: AutoMixConfig = serde_json::from_str(r#"{"feel":"deepLowEnd"}"#).unwrap();
        assert_eq!(partial.feel, RoomFeel::DeepLowEnd);
        assert_eq!(partial.guardrails, Guardrails::default());
        assert!(
            partial.listen,
            "listening defaults on for saved configs from before it existed"
        );
    }
}
