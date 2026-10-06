//! The AI EQ controller: plain state, time passed in, no I/O.
//!
//! It keeps a model of every picked input's desk EQ (read from the desk and
//! kept in step with every change, ours or a person's), and decides:
//!
//! - **Soundcheck**: listens to each mic, proposes gentle EQ, and changes the
//!   desk only when a person taps Apply (who tapped is logged).
//! - **Feedback**: a ringing channel gets a narrow notch on band 4 (−3, then
//!   −6, then −9 dB) and its fader pulled through auto-mix. When band 4 is a
//!   person's, only the fader moves.
//! - **Speech tone keeping**: during the service, speech mics are nudged back
//!   toward how they sounded at soundcheck, 0.5 dB at a time.
//! - **Hands off**: any EQ change a person makes (desk or app) makes that
//!   channel theirs until someone hands it back. Feedback notches still work
//!   on it. Undo puts a channel back to its soundcheck EQ and holds it there.
//! - **Feedback check (ring-out)**: before the service, raises one mic at a
//!   time until it rings, records where, and puts the fader back.
//!
//! Every change goes through [`crate::guardrails`]. The engine only queues
//! changes; the runner sends them (under the per-second budget) and owns the
//! clock.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use automix::{ChannelRole, ManagedChannel, Nudges};
use mix_core::eq::{grid, ChannelEq, EqBand, EqBandKind, EqChange, NOTCH_BAND};
use mix_core::{ChannelKind, ConsoleEvent};
use serde::{Deserialize, Serialize};

use crate::analyser::{band_hz, AnalyseTarget, ToneFrame, SPECTRUM_BANDS};
use crate::guardrails::{self, hard, ToneContext};
use crate::solver::{self, SolveInput};
use crate::text;
use crate::types::*;

/// Gated seconds a mic needs before soundcheck proposes anything.
pub const SOUNDCHECK_HEAR_SECS: f32 = 15.0;
/// A soundcheck channel that has heard less than a second by now has no sound.
const NO_SOUND_AFTER: Duration = Duration::from_secs(60);
/// Proposals are refreshed this often while more is heard.
const RESOLVE_EVERY: Duration = Duration::from_secs(5);
/// Our own sends echo back from the desk within this.
const ECHO_WINDOW: Duration = Duration::from_secs(3);
/// The status shows "Keeping" this long after a tone move.
const KEEPING_SHOWN: Duration = Duration::from_secs(15);
/// A ring ends when nothing has rung for this long.
const RING_ENDS_AFTER: Duration = Duration::from_secs(5);
/// Fader pull per ring report, and the most per ring.
const PULL_STEP_DB: f32 = 3.0;
const PULL_MAX_DB: f32 = 6.0;
/// Speech tone keeping: gated seconds heard this service before it acts.
const TONE_HEAR_SECS: f32 = 20.0;
/// ...and how far the tone must drift around a band (dB) before it does.
const TONE_DRIFT_DB: f32 = 1.5;
/// No audio frames for this long means the audio feed has stopped.
const AUDIO_TIMEOUT: Duration = Duration::from_secs(2);

/// Feedback check: how fast a fader rises, how far, and the pause between mics.
pub const RING_RISE_DB_PER_SEC: f32 = 0.5;
pub const RING_MAX_RISE_DB: f32 = 10.0;
pub const RING_FADER_LIMIT_DB: f32 = 6.0;
const RING_MARGIN_DB: f32 = 3.0;
const RING_PAUSE: Duration = Duration::from_secs(2);

/// What AI EQ remembers about a mic between services, keyed by channel name
/// (no voice ID: the name on the desk is the mic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// The EQ it ended last service on (soundcheck EQ plus kept ideas).
    pub eq: ChannelEq,
    /// Frequencies that have rung on it; no boosts near these.
    #[serde(default)]
    pub feedback_hz: Vec<f32>,
}

/// Why a person's request was turned down, in words for the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub &'static str);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Refused {}

type Answer<T = ()> = Result<T, Refused>;

const OFF: Refused = Refused("AI EQ is off.");
const OFFLINE: Refused = Refused("The console isn't connected.");
const UNSUPPORTED: Refused = Refused("This console's EQ can't be set from SanctuaryMix.");
const FROZEN: Refused = Refused("AI is frozen. Resume it first.");
const IN_SERVICE: Refused = Refused("That waits until the service ends.");
const NOT_PICKED: Refused = Refused("That channel isn't picked for AI.");
const NOT_READ: Refused = Refused("Still reading that channel's EQ from the console.");

/// A queued EQ message for the desk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqSend {
    pub channel: u16,
    pub change: EqChange,
    /// AI's own move (feedback, tone), held back while frozen; otherwise a person asked for it.
    pub by_ai: bool,
}

/// A fader pull for auto-mix to make (feedback).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pull {
    pub channel: u16,
    pub cut_db: f32,
    pub hz: f32,
}

#[derive(Debug, Clone, Default)]
struct Track {
    role: Option<ChannelRole>,
    /// The desk's EQ as last read or sent.
    eq: ChannelEq,
    /// Parameters read from the desk so far (bit per [`param_bit`]).
    read: u32,
    /// What we sent recently, to tell our own echoes from a person.
    sent: VecDeque<(EqChange, Duration)>,
    /// This service's reference: soundcheck EQ, or what the desk had when AI EQ adopted it.
    baseline: Option<ChannelEq>,
    /// The desk's EQ before soundcheck changed it (for Before | After and undo).
    before_soundcheck: Option<ChannelEq>,
    /// Soundcheck applied, kept or skipped this service.
    checked: bool,
    /// A person changed it: theirs until handed back.
    held_by: Option<EqActor>,
    undone: bool,
    proposal: Option<EqProposal>,
    soundcheck: Option<SoundcheckState>,
    solved_at: Option<Duration>,
    /// Pre-EQ spectrum (absolute dB per band) and seconds heard.
    measured: Option<Vec<f32>>,
    heard_secs: f32,
    /// Seconds heard since the service started (speech tone keeping).
    heard_in_service: f32,
    /// The pre-EQ spectrum at soundcheck, relative; tone keeping steers back to it.
    tone_ref: Option<Vec<f32>>,
    tone_moved: [Option<Duration>; 3],
    tone_last: Option<Duration>,
    msgs: VecDeque<Duration>,
    fader_db: Option<f32>,
    fader_moved: Option<Duration>,
    notch: Option<Notch>,
    notch_stage: usize,
    /// Band 4 as it was before the notch.
    notch_was: Option<EqBand>,
    ring_last: Option<Duration>,
    ring_pulled_db: f32,
    rings_today: u32,
    feedback_hz: Vec<f32>,
    name: Option<String>,
}

const ALL_PARAMS: u32 = (1 << 18) - 1;

fn param_bit(change: &EqChange) -> u32 {
    let i = match *change {
        EqChange::HpfOn { .. } => 16,
        EqChange::HpfFreq { .. } => 17,
        EqChange::BandKind { band, .. } => band as u32 * 4,
        EqChange::BandFreq { band, .. } => band as u32 * 4 + 1,
        EqChange::BandWidth { band, .. } => band as u32 * 4 + 2,
        EqChange::BandGain { band, .. } => band as u32 * 4 + 3,
    };
    1 << i.min(17)
}

impl Track {
    fn is_read(&self) -> bool {
        self.read == ALL_PARAMS
    }

    fn label(&self, channel: u16) -> String {
        self.name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("Input {}", channel + 1))
    }

    fn relative_spectrum(&self) -> Option<Vec<f32>> {
        let m = self.measured.as_ref()?;
        relative(m)
    }
}

/// A spectrum relative to its own mean over 100 Hz-10 kHz.
fn relative(m: &[f32]) -> Option<Vec<f32>> {
    let (mut sum, mut n) = (0.0, 0);
    for (i, v) in m.iter().enumerate() {
        if (100.0..=10_000.0).contains(&band_hz(i)) {
            sum += v;
            n += 1;
        }
    }
    (n > 0).then(|| m.iter().map(|v| v - sum / n as f32).collect())
}

#[derive(Debug, Clone)]
struct RingOut {
    channels: Vec<u16>,
    index: usize,
    start_db: Option<f32>,
    level_db: Option<f32>,
    next_at: Duration,
    results: Vec<RingResult>,
    running: bool,
}

pub struct AiEq {
    config: AiEqConfig,
    picks: Vec<ManagedChannel>,
    nudges: Nudges,
    tracks: BTreeMap<u16, Track>,
    console_online: bool,
    eq_supported: bool,
    frozen: bool,
    in_service: bool,
    last_audio: Option<Duration>,
    soundcheck_running: bool,
    soundcheck_started: Option<Duration>,
    compare: Option<Compare>,
    ring_out: Option<RingOut>,
    feedback: Option<FeedbackEvent>,
    feedback_seq: u64,
    profiles: BTreeMap<String, Profile>,
    profiles_changed: bool,
    profile_dismissed: bool,
    sends: Vec<EqSend>,
    pulls: Vec<Pull>,
    faders: Vec<(u16, f32)>,
    log: Vec<EqLogEntry>,
    ideas: Vec<EqIdea>,
    ceilings: Option<Vec<(u16, f32)>>,
    analyse: Option<Vec<AnalyseTarget>>,
    reset: Option<Vec<u16>>,
}

impl AiEq {
    pub fn new(config: AiEqConfig) -> Self {
        Self {
            config,
            picks: Vec::new(),
            nudges: Nudges::default(),
            tracks: BTreeMap::new(),
            console_online: false,
            eq_supported: false,
            frozen: false,
            in_service: false,
            last_audio: None,
            soundcheck_running: false,
            soundcheck_started: None,
            compare: None,
            ring_out: None,
            feedback: None,
            feedback_seq: 0,
            profiles: BTreeMap::new(),
            profiles_changed: false,
            profile_dismissed: false,
            sends: Vec::new(),
            pulls: Vec::new(),
            faders: Vec::new(),
            log: Vec::new(),
            ideas: Vec::new(),
            ceilings: None,
            analyse: None,
            reset: None,
        }
    }

    // ---- what the runner drains --------------------------------------------

    pub fn take_sends(&mut self) -> Vec<EqSend> {
        std::mem::take(&mut self.sends)
    }
    pub fn take_pulls(&mut self) -> Vec<Pull> {
        std::mem::take(&mut self.pulls)
    }
    /// Fader moves for the feedback check.
    pub fn take_faders(&mut self) -> Vec<(u16, f32)> {
        std::mem::take(&mut self.faders)
    }
    pub fn take_log(&mut self) -> Vec<EqLogEntry> {
        std::mem::take(&mut self.log)
    }
    /// Ideas collected when a service ends.
    pub fn take_ideas(&mut self) -> Vec<EqIdea> {
        std::mem::take(&mut self.ideas)
    }
    /// New feedback ceilings for auto-mix, after a feedback check.
    pub fn take_ceilings(&mut self) -> Option<Vec<(u16, f32)>> {
        self.ceilings.take()
    }
    /// New analyser targets when the picks change.
    pub fn take_analyse(&mut self) -> Option<Vec<AnalyseTarget>> {
        self.analyse.take()
    }
    /// Channels whose averages should start over (a new soundcheck).
    pub fn take_reset(&mut self) -> Option<Vec<u16>> {
        self.reset.take()
    }
    /// The profiles, when they changed since last asked.
    pub fn take_profiles(&mut self) -> Option<BTreeMap<String, Profile>> {
        std::mem::replace(&mut self.profiles_changed, false).then(|| self.profiles.clone())
    }

    // ---- setup -------------------------------------------------------------

    pub fn config(&self) -> &AiEqConfig {
        &self.config
    }

    pub fn configure(&mut self, config: AiEqConfig) -> &AiEqConfig {
        if !config.enabled {
            self.compare = None;
            self.soundcheck_running = false;
            self.stop_ring_out();
        }
        self.config = config;
        &self.config
    }

    pub fn load_profiles(&mut self, profiles: BTreeMap<String, Profile>) {
        self.profiles = profiles;
    }

    /// The channels and roles auto-mix has picked (one pick for both).
    pub fn set_picks(&mut self, picks: Vec<ManagedChannel>, nudges: Nudges) {
        self.nudges = nudges;
        let keep: BTreeSet<u16> = picks.iter().map(|p| p.channel).collect();
        for (ch, t) in self.tracks.iter_mut() {
            t.role = picks.iter().find(|p| p.channel == *ch).map(|p| p.role);
            if !keep.contains(ch) {
                t.soundcheck = None;
                t.proposal = None;
            }
        }
        for p in &picks {
            self.tracks.entry(p.channel).or_default().role = Some(p.role);
        }
        self.picks = picks;
        self.analyse = Some(
            self.picks
                .iter()
                .map(|p| AnalyseTarget {
                    channel: p.channel,
                    voice_gate: p.role.is_vocal(),
                    feedback: true,
                })
                .collect(),
        );
    }

    /// The channels the runner should read EQ from (picked and not yet read).
    pub fn unread(&self) -> Vec<u16> {
        if !self.console_online || !self.eq_supported {
            return Vec::new();
        }
        self.picks
            .iter()
            .map(|p| p.channel)
            .filter(|ch| self.tracks.get(ch).is_none_or(|t| !t.is_read()))
            .collect()
    }

    pub fn set_eq_supported(&mut self, on: bool) {
        self.eq_supported = on;
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
        self.stop_ring_out();
    }

    pub fn unfreeze(&mut self) {
        self.frozen = false;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// A service started (recording, or auto-mix on) or ended. Ending one
    /// collects ideas for next week and saves each mic's EQ.
    pub fn set_in_service(&mut self, on: bool, now: Duration, at_ms: u64) {
        if on == self.in_service {
            return;
        }
        self.in_service = on;
        if on {
            self.compare = None;
            self.soundcheck_running = false;
            self.stop_ring_out();
            for t in self.tracks.values_mut() {
                t.heard_in_service = 0.0;
                t.rings_today = 0;
            }
        } else {
            self.end_of_service(now, at_ms);
        }
    }

    fn picked(&self, channel: u16) -> Option<ChannelRole> {
        self.picks
            .iter()
            .find(|p| p.channel == channel)
            .map(|p| p.role)
    }

    fn can_send(&self) -> Answer {
        if !self.config.enabled {
            return Err(OFF);
        }
        if !self.console_online {
            return Err(OFFLINE);
        }
        if !self.eq_supported {
            return Err(UNSUPPORTED);
        }
        Ok(())
    }

    /// AI may move things on its own right now.
    fn ai_may_move(&self) -> bool {
        self.can_send().is_ok() && !self.frozen
    }

    // ---- inputs ------------------------------------------------------------

    pub fn on_console(&mut self, event: &ConsoleEvent, now: Duration, at_ms: u64) {
        match event {
            ConsoleEvent::Connected { .. } => self.console_online = true,
            ConsoleEvent::Disconnected { .. } => {
                self.console_online = false;
                // Read everything again on reconnect: the desk may have changed.
                for t in self.tracks.values_mut() {
                    t.read = 0;
                    t.sent.clear();
                }
                self.stop_ring_out();
                self.compare = None;
            }
            ConsoleEvent::Name { id, name } if id.kind == ChannelKind::Input => {
                self.tracks.entry(id.index).or_default().name = Some(name.clone());
            }
            ConsoleEvent::Fader { id, db } if id.kind == ChannelKind::Input => {
                let t = self.tracks.entry(id.index).or_default();
                t.fader_db = *db;
                t.fader_moved = Some(now);
            }
            ConsoleEvent::Eq { id, change } if id.kind == ChannelKind::Input => {
                self.on_eq(id.index, *change, now, at_ms);
            }
            _ => {}
        }
    }

    fn on_eq(&mut self, channel: u16, change: EqChange, now: Duration, at_ms: u64) {
        let picked = self.picked(channel).is_some();
        let t = self.tracks.entry(channel).or_default();
        t.sent
            .retain(|(_, at)| now.saturating_sub(*at) <= ECHO_WINDOW);
        if let Some(i) = t.sent.iter().position(|(c, _)| c.same_value(&change)) {
            // Our own change coming back.
            t.sent.remove(i);
            t.read |= param_bit(&change);
            return;
        }
        let was_read = t.is_read();
        let before = t.eq;
        t.eq.apply(&change);
        t.read |= param_bit(&change);
        if !was_read {
            if t.is_read() && t.baseline.is_none() {
                t.baseline = Some(t.eq);
            }
            return;
        }
        if before.snapped() == t.eq.snapped() || !picked {
            return;
        }
        // A person on the desk changed it: it's theirs now.
        let changes = text::changes(&before, &t.eq);
        if t.notch.is_some() && change.band() == Some(NOTCH_BAND) {
            t.notch = None;
            t.notch_was = None;
        }
        t.held_by = Some(EqActor::desk());
        t.undone = false;
        self.log.push(EqLogEntry {
            at_ms,
            channel,
            channel_name: t.label(channel),
            action: EqAction::Person,
            changes,
            reason: None,
            by: EqActor::desk(),
            applied_by: None,
        });
    }

    /// Latest from the analyser.
    pub fn on_tone(&mut self, frame: &ToneFrame, now: Duration, at_ms: u64) {
        self.last_audio = Some(now);
        let after_eq = self.config.tap == TapPoint::AfterEq;
        for c in &frame.channels {
            let Some(t) = self.tracks.get_mut(&c.channel) else {
                continue;
            };
            let gained = (c.gated_secs - t.heard_secs).max(0.0);
            t.heard_secs = c.gated_secs;
            if self.in_service {
                t.heard_in_service += gained;
            }
            t.measured = c.ltas_db.as_ref().map(|l| {
                if after_eq {
                    // Take the desk's EQ back out so every decision sees the mic itself.
                    l.iter()
                        .enumerate()
                        .map(|(i, v)| v - crate::model::response(&t.eq, band_hz(i)))
                        .collect()
                } else {
                    l.clone()
                }
            });
        }
        for howl in &frame.howls {
            self.on_howl(howl.channel, howl.hz, now, at_ms);
        }
    }

    // ---- soundcheck --------------------------------------------------------

    fn soundcheck_order(&self) -> Vec<u16> {
        let rank = |r: ChannelRole| match r {
            ChannelRole::Speech => 0,
            r if r.is_vocal() => 1,
            _ => 2,
        };
        let mut picks: Vec<&ManagedChannel> = self
            .picks
            .iter()
            .filter(|p| crate::targets::target(p.role, &self.nudges).is_some())
            .collect();
        picks.sort_by_key(|p| (rank(p.role), p.channel));
        picks.iter().map(|p| p.channel).collect()
    }

    pub fn soundcheck_start(&mut self, now: Duration) -> Answer {
        self.can_send()?;
        if self.in_service {
            return Err(IN_SERVICE);
        }
        let channels = self.soundcheck_order();
        for &ch in &channels {
            let t = self.tracks.entry(ch).or_default();
            t.soundcheck = Some(SoundcheckState::UpNext);
            t.proposal = None;
            t.solved_at = None;
            t.measured = None;
            t.heard_secs = 0.0;
        }
        self.soundcheck_running = true;
        self.soundcheck_started = Some(now);
        self.profile_dismissed = false;
        self.reset = Some(channels);
        Ok(())
    }

    pub fn soundcheck_stop(&mut self) {
        self.soundcheck_running = false;
        self.compare = None;
    }

    fn solve(&self, channel: u16) -> Option<EqProposal> {
        let t = self.tracks.get(&channel)?;
        let role = t.role?;
        let measured = t.measured.as_ref()?;
        if !t.is_read() || measured.len() != SPECTRUM_BANDS {
            return None;
        }
        let mut feedback_hz = t.feedback_hz.clone();
        if let Some(p) = self.profiles.get(&t.label(channel)) {
            feedback_hz.extend(&p.feedback_hz);
        }
        let desk = t.before_soundcheck.unwrap_or(t.eq);
        solver::solve(&SolveInput {
            role,
            nudges: &self.nudges,
            measured_db: measured,
            desk,
            feedback_hz: &feedback_hz,
        })
    }

    fn step_soundcheck(&mut self, now: Duration) {
        if !self.soundcheck_running {
            return;
        }
        let started = self.soundcheck_started.unwrap_or(now);
        let channels: Vec<u16> = self
            .tracks
            .iter()
            .filter(|(_, t)| t.soundcheck.is_some())
            .map(|(&c, _)| c)
            .collect();
        for ch in channels {
            let t = &self.tracks[&ch];
            let state = t.soundcheck.unwrap_or(SoundcheckState::UpNext);
            if matches!(state, SoundcheckState::Applied | SoundcheckState::Skipped) {
                continue;
            }
            let heard = t.heard_secs;
            let due = guardrails::since(now, t.solved_at) >= RESOLVE_EVERY;
            let next = if heard >= SOUNDCHECK_HEAR_SECS {
                if !due {
                    continue;
                }
                let proposal = self.solve(ch);
                let t = self.tracks.get_mut(&ch).expect("track");
                t.solved_at = Some(now);
                let state = if proposal.is_some() {
                    SoundcheckState::Done
                } else {
                    SoundcheckState::SoundsGood
                };
                t.proposal = proposal;
                state
            } else if heard > 0.5 {
                SoundcheckState::Listening
            } else if now.saturating_sub(started) >= NO_SOUND_AFTER {
                SoundcheckState::NoSound
            } else {
                SoundcheckState::UpNext
            };
            self.tracks.get_mut(&ch).expect("track").soundcheck = Some(next);
        }
    }

    /// Someone tapped Apply on a channel's soundcheck proposal.
    pub fn apply(&mut self, channel: u16, by: &EqActor, now: Duration, at_ms: u64) -> Answer {
        self.can_send()?;
        if self.frozen {
            return Err(FROZEN);
        }
        if self.in_service {
            return Err(IN_SERVICE);
        }
        let role = self.picked(channel).ok_or(NOT_PICKED)?;
        let t = self.tracks.get(&channel).ok_or(NOT_PICKED)?;
        if !t.is_read() {
            return Err(NOT_READ);
        }
        let proposal = t
            .proposal
            .clone()
            .ok_or(Refused("There's nothing to apply on that channel."))?;
        // Rechecked against the desk as it is now.
        let mut feedback_hz = t.feedback_hz.clone();
        if let Some(p) = self.profiles.get(&t.label(channel)) {
            feedback_hz.extend(&p.feedback_hz);
        }
        let before = t.eq;
        let safe = guardrails::soundcheck(role, &before, &proposal.eq, &feedback_hz);
        let changes = text::changes(&before, &safe);
        self.set_desk(channel, safe, now, false);
        let t = self.tracks.get_mut(&channel).expect("track");
        t.before_soundcheck = Some(before);
        t.baseline = Some(safe);
        t.tone_ref = t.relative_spectrum();
        t.checked = true;
        t.held_by = None;
        t.undone = false;
        t.soundcheck = Some(SoundcheckState::Applied);
        let name = t.label(channel);
        self.log.push(EqLogEntry {
            at_ms,
            channel,
            channel_name: name,
            action: EqAction::Soundcheck,
            changes,
            reason: Some(proposal.reason),
            by: EqActor::ai(),
            applied_by: Some(by.clone()),
        });
        self.compare = Some(Compare {
            channel,
            side: CompareSide::After,
        });
        Ok(())
    }

    pub fn apply_all(&mut self, by: &EqActor, now: Duration, at_ms: u64) -> Answer<usize> {
        let ready: Vec<u16> = self
            .tracks
            .iter()
            .filter(|(_, t)| t.soundcheck == Some(SoundcheckState::Done) && t.proposal.is_some())
            .map(|(&c, _)| c)
            .collect();
        let mut n = 0;
        for ch in ready {
            self.apply(ch, by, now, at_ms)?;
            n += 1;
        }
        self.compare = None;
        Ok(n)
    }

    pub fn skip(&mut self, channel: u16) -> Answer {
        let t = self.tracks.get_mut(&channel).ok_or(NOT_PICKED)?;
        t.soundcheck = Some(SoundcheckState::Skipped);
        t.proposal = None;
        t.checked = true;
        if t.is_read() {
            t.baseline = Some(t.eq);
            t.tone_ref = t.relative_spectrum();
        }
        Ok(())
    }

    /// "Keep my EQ": the desk's EQ stays as it is and becomes this service's reference.
    pub fn keep_my_eq(&mut self) -> Answer {
        for ch in self.soundcheck_order() {
            if let Some(t) = self.tracks.get_mut(&ch) {
                if t.soundcheck != Some(SoundcheckState::Applied) {
                    t.soundcheck = Some(SoundcheckState::Skipped);
                    t.proposal = None;
                }
                t.checked = true;
                if t.is_read() {
                    t.baseline = Some(t.eq);
                    t.tone_ref = t.relative_spectrum();
                }
            }
        }
        self.soundcheck_running = false;
        self.compare = None;
        Ok(())
    }

    /// Before | After after an apply. `None` ends comparing (back to After).
    pub fn compare(&mut self, channel: u16, side: Option<CompareSide>, now: Duration) -> Answer {
        self.can_send()?;
        if self.in_service {
            return Err(IN_SERVICE);
        }
        let t = self.tracks.get(&channel).ok_or(NOT_PICKED)?;
        let (Some(before), Some(after)) = (t.before_soundcheck, t.baseline) else {
            return Err(Refused("Apply a change first, then compare."));
        };
        let to = match side.unwrap_or(CompareSide::After) {
            CompareSide::Before => before,
            CompareSide::After => after,
        };
        self.set_desk(channel, to, now, false);
        self.compare = side.map(|side| Compare { channel, side });
        Ok(())
    }

    // ---- people ------------------------------------------------------------

    /// A person set a channel's EQ in the app.
    pub fn set_by_person(
        &mut self,
        channel: u16,
        eq: ChannelEq,
        by: &EqActor,
        now: Duration,
        at_ms: u64,
    ) -> Answer {
        if !self.console_online {
            return Err(OFFLINE);
        }
        if !self.eq_supported {
            return Err(UNSUPPORTED);
        }
        let t = self.tracks.get(&channel).ok_or(NOT_PICKED)?;
        if !t.is_read() {
            return Err(NOT_READ);
        }
        let mut eq = eq.snapped();
        for (i, b) in eq.bands.iter_mut().enumerate() {
            if !b.kind.allowed_on(i as u8) {
                b.kind = EqBandKind::Bell;
            }
        }
        let before = t.eq;
        let changes = text::changes(&before, &eq);
        if changes.is_empty() {
            return Ok(());
        }
        self.set_desk(channel, eq, now, false);
        let t = self.tracks.get_mut(&channel).expect("track");
        if t.notch.is_some() && before.bands[NOTCH_BAND as usize] != eq.bands[NOTCH_BAND as usize] {
            t.notch = None;
            t.notch_was = None;
        }
        t.held_by = Some(by.clone());
        t.undone = false;
        let name = t.label(channel);
        self.log.push(EqLogEntry {
            at_ms,
            channel,
            channel_name: name,
            action: EqAction::Person,
            changes,
            reason: None,
            by: by.clone(),
            applied_by: None,
        });
        Ok(())
    }

    /// Undo: back to this service's soundcheck EQ and held there. Before the
    /// service, on a channel at its soundcheck EQ, undoes the soundcheck itself.
    pub fn undo(&mut self, channel: u16, by: &EqActor, now: Duration, at_ms: u64) -> Answer {
        if !self.console_online {
            return Err(OFFLINE);
        }
        let t = self.tracks.get(&channel).ok_or(NOT_PICKED)?;
        let Some(baseline) = t.baseline else {
            return Err(Refused("There's nothing to undo on that channel."));
        };
        let at_baseline = t.eq.diff(&baseline).is_empty();
        let (to, undo_soundcheck) = match t.before_soundcheck {
            Some(before) if at_baseline && !self.in_service => (before, true),
            _ => (baseline, false),
        };
        let before = t.eq;
        let changes = text::changes(&before, &to);
        if changes.is_empty() {
            return Err(Refused("There's nothing to undo on that channel."));
        }
        self.set_desk(channel, to, now, false);
        let t = self.tracks.get_mut(&channel).expect("track");
        if undo_soundcheck {
            t.baseline = Some(to);
            t.before_soundcheck = None;
            t.soundcheck = t
                .proposal
                .as_ref()
                .map(|_| SoundcheckState::Done)
                .or(t.soundcheck);
            self.compare = None;
        } else {
            t.undone = true;
        }
        t.notch = None;
        t.notch_was = None;
        t.held_by = None;
        let name = t.label(channel);
        self.log.push(EqLogEntry {
            at_ms,
            channel,
            channel_name: name,
            action: EqAction::Undo,
            changes,
            reason: None,
            by: by.clone(),
            applied_by: None,
        });
        Ok(())
    }

    pub fn undo_all(&mut self, by: &EqActor, now: Duration, at_ms: u64) -> usize {
        let channels: Vec<u16> = self.picks.iter().map(|p| p.channel).collect();
        channels
            .into_iter()
            .filter(|&c| self.undo(c, by, now, at_ms).is_ok())
            .count()
    }

    /// Gives a held (or undone) channel back to AI EQ; its EQ now becomes the reference.
    pub fn hand_back(&mut self, channel: u16, by: &EqActor, at_ms: u64) -> Answer {
        let t = self.tracks.get_mut(&channel).ok_or(NOT_PICKED)?;
        if t.held_by.is_none() && !t.undone {
            return Ok(());
        }
        t.held_by = None;
        t.undone = false;
        if t.is_read() {
            t.baseline = Some(t.eq);
        }
        t.tone_moved = [None; 3];
        let name = t.label(channel);
        self.log.push(EqLogEntry {
            at_ms,
            channel,
            channel_name: name,
            action: EqAction::HandBack,
            changes: Vec::new(),
            reason: Some("Handed back to AI EQ.".into()),
            by: by.clone(),
            applied_by: None,
        });
        Ok(())
    }

    // ---- profiles ----------------------------------------------------------

    fn profile_differs(&self) -> Vec<u16> {
        if self.profile_dismissed || self.in_service {
            return Vec::new();
        }
        self.picks
            .iter()
            .filter_map(|p| {
                let t = self.tracks.get(&p.channel)?;
                let prof = self.profiles.get(&t.name.clone()?)?;
                (t.is_read() && !t.checked && !t.eq.diff(&prof.eq).is_empty()).then_some(p.channel)
            })
            .collect()
    }

    /// Puts last Sunday's EQ back on every channel that differs.
    pub fn restore_profiles(&mut self, by: &EqActor, now: Duration, at_ms: u64) -> Answer<usize> {
        self.can_send()?;
        if self.frozen {
            return Err(FROZEN);
        }
        let channels = self.profile_differs();
        for &ch in &channels {
            let t = &self.tracks[&ch];
            let prof = self.profiles[t.name.as_deref().unwrap_or_default()].eq;
            let changes = text::changes(&t.eq, &prof);
            self.set_desk(ch, prof, now, false);
            let t = self.tracks.get_mut(&ch).expect("track");
            t.baseline = Some(prof);
            let name = t.label(ch);
            self.log.push(EqLogEntry {
                at_ms,
                channel: ch,
                channel_name: name,
                action: EqAction::Person,
                changes,
                reason: Some("Back to last Sunday's EQ.".into()),
                by: by.clone(),
                applied_by: None,
            });
        }
        self.profile_dismissed = true;
        Ok(channels.len())
    }

    pub fn dismiss_profiles(&mut self) {
        self.profile_dismissed = true;
    }

    /// A kept idea changes that mic's saved EQ for next week.
    pub fn keep_idea(&mut self, idea: &EqIdea) {
        let Some(apply) = idea.apply else { return };
        let Some(prof) = self.profiles.get_mut(&idea.channel_name) else {
            return;
        };
        let band4 = &mut prof.eq.bands[NOTCH_BAND as usize];
        match apply {
            IdeaApply::KeepNotch { hz, gain_db } => {
                *band4 = EqBand::bell(hz, grid::NARROWEST, gain_db).snapped();
                band4.gain_db = grid::gain_within(gain_db, hard::NOTCH_MAX_CUT_DB, 0.0);
            }
            IdeaApply::DropNotch => {
                *band4 = ChannelEq::default().bands[NOTCH_BAND as usize];
            }
            IdeaApply::BandGain { band, gain_db } if band < NOTCH_BAND => {
                let b = &mut prof.eq.bands[band as usize];
                b.gain_db = grid::gain_within(gain_db, hard::TONE_MAX_CUT_DB, hard::MAX_BOOST_DB);
            }
            IdeaApply::BandGain { .. } => return,
        }
        self.profiles_changed = true;
    }

    fn end_of_service(&mut self, now: Duration, at_ms: u64) {
        let mut ideas = Vec::new();
        for p in &self.picks {
            let Some(t) = self.tracks.get(&p.channel) else {
                continue;
            };
            let name = t.label(p.channel);
            let mut idea = |n: usize, title: String, change: String, reason: String, apply| {
                ideas.push(EqIdea {
                    id: format!("{at_ms}-{}-{n}", p.channel),
                    recording_id: None,
                    at_ms,
                    channel: p.channel,
                    channel_name: name.clone(),
                    title,
                    change,
                    reason,
                    state: IdeaState::Waiting,
                    apply: Some(apply),
                })
            };
            let saved = self.profiles.get(&name);
            if let Some(n) = t.notch {
                idea(
                    0,
                    format!("Keep the {} notch on {name}", text::hz(n.hz)),
                    format!(
                        "{}  0.0 \u{2192} {} (band 4)",
                        text::hz(n.hz),
                        text::db(n.gain_db)
                    ),
                    format!(
                        "It rang at {} {} today. Keeping the notch stops it before it starts.",
                        text::hz(n.hz),
                        times(t.rings_today)
                    ),
                    IdeaApply::KeepNotch {
                        hz: n.hz,
                        gain_db: n.gain_db,
                    },
                );
            } else if let Some(prof) = saved {
                let b = prof.eq.bands[NOTCH_BAND as usize];
                if b.gain_db < -0.3 && t.rings_today == 0 {
                    idea(
                        1,
                        format!("Remove the {} notch on {name}", text::hz(b.freq_hz)),
                        format!(
                            "{}  {} \u{2192} 0.0 dB (band 4)",
                            text::hz(b.freq_hz),
                            text::num(b.gain_db)
                        ),
                        "It didn't ring today, so the notch may not be needed.".into(),
                        IdeaApply::DropNotch,
                    );
                }
            }
            // Speech tone drift: carry 1 dB of it into next week's EQ.
            if let (Some(base), Some(ChannelRole::Speech)) = (t.baseline, t.role) {
                for band in 0..NOTCH_BAND as usize {
                    let drift = t.eq.bands[band].gain_db - base.bands[band].gain_db;
                    if drift.abs() < 1.0 || t.held_by.is_some() || t.undone {
                        continue;
                    }
                    let to = base.bands[band].gain_db + drift.signum();
                    let hz = base.bands[band].freq_hz;
                    let what = if drift < 0.0 {
                        if hz < 400.0 {
                            "Less boomy"
                        } else {
                            "Less harsh"
                        }
                    } else {
                        "Clearer"
                    };
                    idea(
                        2 + band,
                        format!("{what} on {name} next week"),
                        format!(
                            "{}  {} \u{2192} {}",
                            text::hz(hz),
                            text::num(base.bands[band].gain_db),
                            text::db(to)
                        ),
                        format!(
                            "Tone keeping moved {} {} during the service.",
                            text::hz(hz),
                            text::db(drift)
                        ),
                        IdeaApply::BandGain {
                            band: band as u8,
                            gain_db: to,
                        },
                    );
                }
            }
        }
        self.ideas = ideas;
        // Save each mic's EQ for next week: its reference without today's notch.
        for p in &self.picks {
            let Some(t) = self.tracks.get_mut(&p.channel) else {
                continue;
            };
            let (Some(name), Some(mut eq)) = (t.name.clone(), t.baseline) else {
                continue;
            };
            if let Some(was) = t.notch_was {
                eq.bands[NOTCH_BAND as usize] = was;
            }
            let prof = self.profiles.entry(name).or_insert(Profile {
                eq,
                feedback_hz: Vec::new(),
            });
            prof.eq = eq;
            for &hz in &t.feedback_hz {
                if !prof
                    .feedback_hz
                    .iter()
                    .any(|f| (f / hz).log2().abs() < 1.0 / 12.0)
                {
                    prof.feedback_hz.push(hz);
                }
            }
            // Next service starts fresh.
            t.checked = false;
            t.held_by = None;
            t.undone = false;
            t.before_soundcheck = None;
            t.soundcheck = None;
            t.proposal = None;
            t.tone_moved = [None; 3];
        }
        self.profiles_changed = true;
        self.profile_dismissed = false;
        let _ = now;
    }

    // ---- feedback ----------------------------------------------------------

    fn on_howl(&mut self, channel: u16, hz: f32, now: Duration, at_ms: u64) {
        if self.picked(channel).is_none() {
            return;
        }
        if let Some(r) = &self.ring_out {
            if r.running {
                self.ring_out_howl(channel, hz, now);
                return;
            }
        }
        if !self.ai_may_move() {
            return;
        }
        let t = self.tracks.get_mut(&channel).expect("picked has a track");
        if !t.is_read() {
            return;
        }
        let new_ring = guardrails::since(now, t.ring_last) >= RING_ENDS_AFTER;
        t.ring_last = Some(now);
        if new_ring {
            t.ring_pulled_db = 0.0;
            t.rings_today += 1;
        }
        if !t
            .feedback_hz
            .iter()
            .any(|f| (f / hz).log2().abs() < 1.0 / 12.0)
        {
            t.feedback_hz.push(hz);
        }
        // Band 4: a new notch, a deeper one on the same ring, or fader only.
        let before = t.eq;
        let notch_band = if guardrails::notch_matches(&t.eq, hz) && t.notch.is_some() {
            (t.notch_stage + 1 < guardrails::NOTCH_STAGES_DB.len()).then(|| {
                t.notch_stage += 1;
                guardrails::notch(t.notch.map_or(hz, |n| n.hz), t.notch_stage)
            })
        } else if guardrails::notch_band_free(&t.eq) && t.notch.is_none() {
            t.notch_was = Some(t.eq.bands[NOTCH_BAND as usize]);
            t.notch_stage = 0;
            Some(guardrails::notch(hz, 0))
        } else {
            None
        };
        let mut eq = t.eq;
        if let Some(b) = notch_band {
            eq.bands[NOTCH_BAND as usize] = b;
            t.notch = Some(Notch {
                hz: b.freq_hz,
                gain_db: b.gain_db,
                at_ms,
            });
        }
        let cut = (PULL_MAX_DB - t.ring_pulled_db).clamp(0.0, PULL_STEP_DB);
        t.ring_pulled_db += cut;
        t.rings_today = t.rings_today.max(1);
        let rings = t.rings_today;
        let name = t.label(channel);
        let notch_db = notch_band.map(|b| b.gain_db).or(t
            .notch
            .map(|n| n.gain_db)
            .filter(|_| guardrails::notch_matches(&eq, hz)));
        if notch_band.is_some() {
            self.set_desk(channel, eq, now, true);
        }
        if cut > 0.0 {
            self.pulls.push(Pull {
                channel,
                cut_db: cut,
                hz,
            });
        }
        if notch_band.is_none() && cut <= 0.0 {
            return;
        }
        self.feedback_seq += 1;
        self.feedback = Some(FeedbackEvent {
            id: self.feedback_seq,
            at_ms,
            channel,
            channel_name: name.clone(),
            hz,
            notch_db,
            fader_cut_db: cut,
            count_today: rings,
        });
        let mut reason = format!("It rang at {}", text::hz(hz));
        match (notch_band, cut > 0.0) {
            (Some(b), true) => reason.push_str(&format!(
                ", so band 4 cut it {} dB and the fader came down {cut:.0} dB.",
                text::num(-b.gain_db).trim_start_matches('+')
            )),
            (Some(b), false) => reason.push_str(&format!(
                ", so band 4 cut it {} dB.",
                text::num(-b.gain_db).trim_start_matches('+')
            )),
            (None, _) => reason.push_str(&format!(
                ". Band 4 is in use, so only the fader came down {cut:.0} dB."
            )),
        }
        self.log.push(EqLogEntry {
            at_ms,
            channel,
            channel_name: name,
            action: EqAction::Feedback,
            changes: text::changes(&before, &eq),
            reason: Some(reason),
            by: EqActor::ai(),
            applied_by: None,
        });
    }

    pub fn dismiss_feedback(&mut self) {
        self.feedback = None;
    }

    // ---- feedback check (ring-out) -----------------------------------------

    pub fn ring_out_start(&mut self, now: Duration) -> Answer {
        self.can_send()?;
        if self.frozen {
            return Err(FROZEN);
        }
        if self.in_service {
            return Err(IN_SERVICE);
        }
        let channels: Vec<u16> = self
            .soundcheck_order()
            .into_iter()
            .filter(|ch| {
                self.picked(*ch)
                    .is_some_and(|r| r.is_vocal() || matches!(r, ChannelRole::AcousticGuitar))
            })
            .collect();
        if channels.is_empty() {
            return Err(Refused("Pick at least one mic for AI first."));
        }
        self.ring_out = Some(RingOut {
            channels,
            index: 0,
            start_db: None,
            level_db: None,
            next_at: now,
            results: Vec::new(),
            running: true,
        });
        Ok(())
    }

    pub fn ring_out_stop(&mut self) {
        self.stop_ring_out();
    }

    fn stop_ring_out(&mut self) {
        let Some(r) = self.ring_out.as_mut() else {
            return;
        };
        if !r.running {
            return;
        }
        if let (Some(&ch), Some(start)) = (r.channels.get(r.index), r.start_db) {
            self.faders.push((ch, start));
        }
        r.running = false;
        r.start_db = None;
        let ceilings: Vec<(u16, f32)> = r
            .results
            .iter()
            .filter_map(|x| x.ceiling_db.map(|c| (x.channel, c)))
            .collect();
        self.ceilings = Some(ceilings);
    }

    fn ring_out_howl(&mut self, channel: u16, hz: f32, now: Duration) {
        let Some(r) = self.ring_out.as_mut() else {
            return;
        };
        if r.channels.get(r.index) != Some(&channel) {
            return;
        }
        let (Some(start), Some(level)) = (r.start_db, r.level_db) else {
            return;
        };
        r.results.push(RingResult {
            channel,
            hz: Some(hz),
            ceiling_db: Some(level - RING_MARGIN_DB),
        });
        self.faders.push((channel, start));
        r.index += 1;
        r.start_db = None;
        r.level_db = None;
        r.next_at = now + RING_PAUSE;
        if let Some(t) = self.tracks.get_mut(&channel) {
            if !t
                .feedback_hz
                .iter()
                .any(|f| (f / hz).log2().abs() < 1.0 / 12.0)
            {
                t.feedback_hz.push(hz);
            }
        }
    }

    fn step_ring_out(&mut self, now: Duration, dt: Duration) {
        let Some(r) = self.ring_out.as_mut() else {
            return;
        };
        if !r.running || now < r.next_at {
            return;
        }
        let Some(&ch) = r.channels.get(r.index) else {
            self.stop_ring_out();
            return;
        };
        let fader = self.tracks.get(&ch).and_then(|t| t.fader_db);
        match (r.start_db, r.level_db) {
            (None, _) => {
                let Some(f) = fader else {
                    // Fully down or unknown: nothing to raise.
                    r.results.push(RingResult {
                        channel: ch,
                        hz: None,
                        ceiling_db: None,
                    });
                    r.index += 1;
                    return;
                };
                r.start_db = Some(f);
                r.level_db = Some(f);
            }
            (Some(start), Some(level)) => {
                let limit = (start + RING_MAX_RISE_DB).min(RING_FADER_LIMIT_DB);
                if level >= limit - 1e-3 {
                    r.results.push(RingResult {
                        channel: ch,
                        hz: None,
                        ceiling_db: None,
                    });
                    self.faders.push((ch, start));
                    r.index += 1;
                    r.start_db = None;
                    r.level_db = None;
                    r.next_at = now + RING_PAUSE;
                    return;
                }
                let to = (level + RING_RISE_DB_PER_SEC * dt.as_secs_f32()).min(limit);
                // The desk's fader works in small steps; send at most every 0.5 dB.
                if to - fader.unwrap_or(level) >= 0.5 || to >= limit {
                    self.faders.push((ch, to));
                }
                r.level_db = Some(to);
            }
            (Some(_), None) => r.level_db = fader,
        }
    }

    // ---- speech tone keeping -----------------------------------------------

    fn step_tone(&mut self, now: Duration, at_ms: u64) {
        if !self.config.tone_keeping || !self.in_service || !self.ai_may_move() {
            return;
        }
        let channels: Vec<u16> = self
            .picks
            .iter()
            .filter(|p| p.role == ChannelRole::Speech)
            .map(|p| p.channel)
            .collect();
        for ch in channels {
            let Some(t) = self.tracks.get(&ch) else {
                continue;
            };
            if !t.is_read()
                || !t.checked
                || t.held_by.is_some()
                || t.undone
                || t.heard_in_service < TONE_HEAR_SECS
                || guardrails::since(now, t.ring_last) < RING_ENDS_AFTER
            {
                continue;
            }
            let (Some(base), Some(reference), Some(current)) =
                (t.baseline, t.tone_ref.as_ref(), t.relative_spectrum())
            else {
                continue;
            };
            let msgs = t
                .msgs
                .iter()
                .filter(|&&m| now.saturating_sub(m) < Duration::from_secs(60))
                .count();
            // The band with the largest drift around it moves, one step.
            let mut best: Option<(usize, f32, f32)> = None;
            for band in 0..NOTCH_BAND as usize {
                let b = t.eq.bands[band];
                if b.kind != EqBandKind::Bell {
                    continue;
                }
                let drift = drift_around(&current, reference, base.bands[band].freq_hz);
                if drift.abs() < TONE_DRIFT_DB {
                    continue;
                }
                let wanted = base.bands[band].gain_db - (drift - drift.signum());
                let ctx = ToneContext {
                    role: ChannelRole::Speech,
                    baseline_db: base.bands[band].gain_db,
                    current_db: b.gain_db,
                    since_last_move: guardrails::since(now, t.tone_moved[band]),
                    msgs_last_minute: msgs,
                    since_fader_step: guardrails::since(now, t.fader_moved),
                };
                if let Some(to) = guardrails::live_tone(&ctx, wanted) {
                    if best.is_none_or(|(_, d, _)| drift.abs() > d.abs()) {
                        best = Some((band, drift, to));
                    }
                }
            }
            let Some((band, drift, to)) = best else {
                continue;
            };
            let before = t.eq;
            let mut eq = t.eq;
            eq.bands[band].gain_db = to;
            let changes = text::changes(&before, &eq);
            self.set_desk(ch, eq, now, true);
            let t = self.tracks.get_mut(&ch).expect("track");
            t.tone_moved[band] = Some(now);
            t.tone_last = Some(now);
            t.msgs.push_back(now);
            while t.msgs.len() > 32 {
                t.msgs.pop_front();
            }
            let hz = text::hz(base.bands[band].freq_hz);
            let reason = if drift > 0.0 {
                format!("More {hz} than at soundcheck, so it came down a little.")
            } else {
                format!("Less {hz} than at soundcheck, so it came up a little.")
            };
            let name = t.label(ch);
            self.log.push(EqLogEntry {
                at_ms,
                channel: ch,
                channel_name: name,
                action: EqAction::Tone,
                changes,
                reason: Some(reason),
                by: EqActor::ai(),
                applied_by: None,
            });
        }
    }

    // ---- the loop ----------------------------------------------------------

    /// Runs the timed parts. Call about ten times a second with the time since the last call.
    pub fn tick(&mut self, now: Duration, dt: Duration, at_ms: u64) {
        self.step_soundcheck(now);
        self.step_ring_out(now, dt);
        self.step_tone(now, at_ms);
    }

    /// Queues every message that takes the desk from its EQ to `to`.
    fn set_desk(&mut self, channel: u16, to: ChannelEq, now: Duration, by_ai: bool) {
        let t = self.tracks.entry(channel).or_default();
        for change in t.eq.diff(&to.snapped()) {
            t.eq.apply(&change);
            t.sent.push_back((change, now));
            self.sends.push(EqSend {
                channel,
                change,
                by_ai,
            });
        }
        while t.sent.len() > 64 {
            t.sent.pop_front();
        }
    }

    fn mode(&self, t: &Track, now: Duration) -> EqMode {
        if !self.config.enabled {
            EqMode::Off
        } else if !self.console_online {
            EqMode::ConsoleOffline
        } else if !self.eq_supported {
            EqMode::NotSupported
        } else if !t.is_read() {
            EqMode::Reading
        } else if self.frozen {
            EqMode::Frozen
        } else if t.held_by.is_some() {
            EqMode::Yours
        } else if t.undone {
            EqMode::Undone
        } else if t.notch.is_some() {
            EqMode::Notch
        } else if guardrails::since(now, t.tone_last) < KEEPING_SHOWN {
            EqMode::Keeping
        } else if !t.checked {
            EqMode::NotChecked
        } else {
            EqMode::Set
        }
    }

    pub fn status(&self, now: Duration) -> AiEqStatus {
        let audio_ok = guardrails::since(now, self.last_audio) < AUDIO_TIMEOUT;
        let channels = self
            .picks
            .iter()
            .map(|p| {
                let empty = Track::default();
                let t = self.tracks.get(&p.channel).unwrap_or(&empty);
                let tone_offset_db = (p.role == ChannelRole::Speech)
                    .then_some(t.baseline)
                    .flatten()
                    .map(|b| {
                        (0..NOTCH_BAND as usize)
                            .map(|i| t.eq.bands[i].gain_db - b.bands[i].gain_db)
                            .fold(0.0f32, |m, d| if d.abs() > m.abs() { d } else { m })
                    });
                EqChannelStatus {
                    channel: p.channel,
                    name: t.name.clone(),
                    role: p.role,
                    mode: self.mode(t, now),
                    eq: t.is_read().then_some(t.eq),
                    baseline: t.baseline,
                    proposal: t.proposal.clone(),
                    notch: t.notch,
                    tone_offset_db,
                    spectrum: (t.heard_secs >= 3.0)
                        .then(|| t.relative_spectrum())
                        .flatten(),
                    heard_secs: t.heard_secs,
                    differs_from_profile: false,
                }
            })
            .collect::<Vec<_>>();
        let differs = self.profile_differs();
        let channels = channels
            .into_iter()
            .map(|mut c| {
                c.differs_from_profile = differs.contains(&c.channel);
                c
            })
            .collect();
        let soundcheck = self
            .tracks
            .values()
            .any(|t| t.soundcheck.is_some())
            .then(|| SoundcheckStatus {
                running: self.soundcheck_running,
                channels: self
                    .soundcheck_order()
                    .into_iter()
                    .filter_map(|ch| {
                        let t = self.tracks.get(&ch)?;
                        Some(SoundcheckChannel {
                            channel: ch,
                            state: t.soundcheck?,
                            heard_secs: t.heard_secs,
                            changes: t
                                .proposal
                                .as_ref()
                                .map_or(0, |p| p.changes.len().min(255) as u8),
                        })
                    })
                    .collect(),
            });
        AiEqStatus {
            enabled: self.config.enabled,
            frozen: self.frozen,
            console_online: self.console_online,
            eq_supported: self.eq_supported,
            audio_ok,
            tone_keeping: self.config.tone_keeping,
            in_service: self.in_service,
            soundcheck,
            compare: self.compare,
            ring_out: self.ring_out.as_ref().map(|r| RingOutStatus {
                running: r.running,
                channel: r
                    .running
                    .then(|| r.channels.get(r.index).copied())
                    .flatten(),
                results: r.results.clone(),
            }),
            feedback: self.feedback.clone(),
            profile_differs: differs,
            channels,
        }
    }
}

fn times(n: u32) -> String {
    match n {
        0 | 1 => "once".into(),
        2 => "twice".into(),
        n => format!("{n} times"),
    }
}

/// How much louder (+) or quieter (−) the mic is now than at soundcheck
/// within about a third of an octave of `hz`.
fn drift_around(now: &[f32], reference: &[f32], hz: f32) -> f32 {
    let (mut sum, mut n) = (0.0, 0);
    for i in 0..now.len().min(reference.len()) {
        if (band_hz(i) / hz).log2().abs() <= 1.0 / 3.0 {
            sum += now[i] - reference[i];
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyser::ChannelTone;
    use mix_core::ChannelId;

    const S: Duration = Duration::from_secs(1);

    fn person() -> EqActor {
        EqActor::app("volunteer", "Sam", Some("user-1"))
    }

    struct Rig {
        eq: AiEq,
        now: Duration,
        desk: BTreeMap<u16, ChannelEq>,
    }

    impl Rig {
        fn new(picks: &[(u16, ChannelRole, &str)]) -> Self {
            let mut eq = AiEq::new(AiEqConfig {
                enabled: true,
                ..AiEqConfig::default()
            });
            eq.set_eq_supported(true);
            eq.set_picks(
                picks
                    .iter()
                    .map(|&(channel, role, _)| ManagedChannel { channel, role })
                    .collect(),
                Nudges::default(),
            );
            let mut rig = Self {
                eq,
                now: Duration::ZERO,
                desk: BTreeMap::new(),
            };
            rig.event(ConsoleEvent::Connected {
                model: "test".into(),
            });
            for &(ch, _, name) in picks {
                rig.event(ConsoleEvent::Name {
                    id: ChannelId::input(ch),
                    name: name.into(),
                });
                rig.event(ConsoleEvent::Fader {
                    id: ChannelId::input(ch),
                    db: Some(-10.0),
                });
                rig.read(ch, ChannelEq::default());
            }
            rig.now += 10 * S;
            rig
        }

        fn event(&mut self, e: ConsoleEvent) {
            self.eq.on_console(&e, self.now, 1);
        }

        fn read(&mut self, ch: u16, eq: ChannelEq) {
            self.desk.insert(ch, eq);
            let mut changes = ChannelEq {
                hpf: mix_core::eq::Hpf {
                    on: !eq.hpf.on,
                    freq_hz: 1.0,
                },
                ..ChannelEq::default()
            }
            .diff(&eq);
            // Make sure all 18 params arrive.
            for band in 0..4u8 {
                let b = eq.bands[band as usize];
                changes.push(EqChange::BandKind { band, kind: b.kind });
                changes.push(EqChange::BandFreq {
                    band,
                    hz: b.freq_hz,
                });
                changes.push(EqChange::BandWidth {
                    band,
                    width: b.width,
                });
                changes.push(EqChange::BandGain {
                    band,
                    db: b.gain_db,
                });
            }
            changes.push(EqChange::HpfOn { on: eq.hpf.on });
            changes.push(EqChange::HpfFreq { hz: eq.hpf.freq_hz });
            for change in changes {
                self.event(ConsoleEvent::Eq {
                    id: ChannelId::input(ch),
                    change,
                });
            }
        }

        /// Sends what the engine queued to the "desk", echoing it back.
        fn flush(&mut self) -> Vec<EqSend> {
            let sends = self.eq.take_sends();
            for s in &sends {
                self.desk.entry(s.channel).or_default().apply(&s.change);
                self.event(ConsoleEvent::Eq {
                    id: ChannelId::input(s.channel),
                    change: s.change,
                });
            }
            for (ch, db) in self.eq.take_faders() {
                self.event(ConsoleEvent::Fader {
                    id: ChannelId::input(ch),
                    db: Some(db),
                });
            }
            sends
        }

        fn hear(&mut self, ch: u16, spectrum: &[f32], secs: f32) {
            let frame = ToneFrame {
                channels: vec![ChannelTone {
                    channel: ch,
                    ltas_db: Some(spectrum.to_vec()),
                    gated_secs: secs,
                    active: true,
                }],
                howls: Vec::new(),
            };
            self.eq.on_tone(&frame, self.now, 1);
        }

        fn howl(&mut self, ch: u16, hz: f32) {
            let frame = ToneFrame {
                channels: Vec::new(),
                howls: vec![crate::feedback::Howl {
                    channel: ch,
                    hz,
                    level_db: -10.0,
                    secs: 0.4,
                }],
            };
            self.eq.on_tone(&frame, self.now, 1);
        }

        fn tick(&mut self, secs: f32) {
            let end = self.now + Duration::from_secs_f32(secs);
            while self.now < end {
                self.now += Duration::from_millis(100);
                self.eq.tick(self.now, Duration::from_millis(100), 1);
                self.flush();
            }
        }

        fn mode(&self, ch: u16) -> EqMode {
            self.eq
                .status(self.now)
                .channels
                .into_iter()
                .find(|c| c.channel == ch)
                .unwrap()
                .mode
        }
    }

    /// A boxy voice: the speech target with 6 dB too much around 400 Hz.
    fn boxy() -> Vec<f32> {
        let t = crate::targets::target(ChannelRole::Speech, &Nudges::default()).unwrap();
        t.iter()
            .enumerate()
            .map(|(i, v)| {
                let x = (band_hz(i) / 400.0).log2();
                v - 30.0 + 6.0 * (-x * x * 4.0).exp()
            })
            .collect()
    }

    fn flat_speech() -> Vec<f32> {
        crate::targets::target(ChannelRole::Speech, &Nudges::default())
            .unwrap()
            .iter()
            .map(|v| v - 30.0)
            .collect()
    }

    #[test]
    fn reads_the_desk_then_adopts_it() {
        let rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        assert_eq!(rig.mode(0), EqMode::NotChecked);
        let st = rig.eq.status(rig.now);
        assert_eq!(st.channels[0].baseline, Some(ChannelEq::default()));
        assert!(rig.eq.unread().is_empty());
    }

    #[test]
    fn soundcheck_proposes_and_only_a_person_applies() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.soundcheck_start(rig.now).unwrap();
        assert_eq!(rig.eq.take_reset(), Some(vec![0]));
        rig.hear(0, &boxy(), 5.0);
        rig.tick(0.5);
        let st = rig.eq.status(rig.now).soundcheck.unwrap();
        assert_eq!(st.channels[0].state, SoundcheckState::Listening);
        rig.hear(0, &boxy(), 16.0);
        rig.tick(0.5);
        let st = rig.eq.status(rig.now);
        assert_eq!(
            st.soundcheck.unwrap().channels[0].state,
            SoundcheckState::Done
        );
        let p = st.channels[0].proposal.clone().unwrap();
        assert!(p.eq.bands[..3].iter().any(|b| b.gain_db < -1.0));
        // Nothing reached the desk yet.
        assert!(rig.eq.take_sends().is_empty());

        rig.eq.apply(0, &person(), rig.now, 42).unwrap();
        let sent = rig.flush();
        assert!(!sent.is_empty());
        assert_eq!(rig.desk[&0].snapped(), p.eq.snapped());
        let log = rig.eq.take_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].action, EqAction::Soundcheck);
        assert_eq!(log[0].by.kind, ActorKind::Ai);
        assert_eq!(log[0].applied_by, Some(person()));
        assert!(!log[0].changes.is_empty());
        assert_eq!(rig.mode(0), EqMode::Set);
        // Our own echoes are not a person's change.
        assert!(rig.eq.take_log().is_empty());
    }

    #[test]
    fn a_flat_mic_sounds_good() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        let desk = ChannelEq {
            hpf: mix_core::eq::Hpf {
                on: true,
                freq_hz: 100.0,
            },
            ..ChannelEq::default()
        };
        rig.read(0, desk.snapped());
        rig.eq.hand_back(0, &person(), 1).unwrap();
        rig.eq.soundcheck_start(rig.now).unwrap();
        rig.hear(0, &flat_speech(), 20.0);
        rig.tick(0.5);
        let st = rig.eq.status(rig.now);
        assert_eq!(
            st.soundcheck.unwrap().channels[0].state,
            SoundcheckState::SoundsGood,
            "{:?}",
            st.channels[0].proposal
        );
    }

    #[test]
    fn a_silent_mic_reports_no_sound() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.soundcheck_start(rig.now).unwrap();
        rig.tick(61.0);
        let st = rig.eq.status(rig.now).soundcheck.unwrap();
        assert_eq!(st.channels[0].state, SoundcheckState::NoSound);
    }

    #[test]
    fn soundcheck_changes_wait_for_the_service_to_end() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.soundcheck_start(rig.now).unwrap();
        rig.hear(0, &boxy(), 20.0);
        rig.tick(0.5);
        rig.eq.set_in_service(true, rig.now, 1);
        assert_eq!(rig.eq.apply(0, &person(), rig.now, 1), Err(IN_SERVICE));
        assert_eq!(rig.eq.soundcheck_start(rig.now), Err(IN_SERVICE));
    }

    #[test]
    fn before_and_after_swap_the_desk() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.soundcheck_start(rig.now).unwrap();
        rig.hear(0, &boxy(), 20.0);
        rig.tick(0.5);
        rig.eq.apply(0, &person(), rig.now, 1).unwrap();
        rig.flush();
        let after = rig.desk[&0];
        rig.eq
            .compare(0, Some(CompareSide::Before), rig.now)
            .unwrap();
        rig.flush();
        assert_eq!(rig.desk[&0].snapped(), ChannelEq::default().snapped());
        rig.eq.compare(0, None, rig.now).unwrap();
        rig.flush();
        assert_eq!(rig.desk[&0], after);
        assert!(rig
            .eq
            .take_log()
            .iter()
            .all(|e| e.action != EqAction::Person));
    }

    #[test]
    fn a_desk_change_makes_the_channel_theirs_until_handed_back() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.event(ConsoleEvent::Eq {
            id: ChannelId::input(0),
            change: EqChange::BandGain { band: 1, db: 3.0 },
        });
        assert_eq!(rig.mode(0), EqMode::Yours);
        let log = rig.eq.take_log();
        assert_eq!(log[0].action, EqAction::Person);
        assert_eq!(log[0].by, EqActor::desk());
        rig.eq.hand_back(0, &person(), 2).unwrap();
        assert_eq!(rig.mode(0), EqMode::NotChecked);
        let log = rig.eq.take_log();
        assert_eq!(log[0].action, EqAction::HandBack);
        assert_eq!(log[0].by, person());
    }

    #[test]
    fn feedback_notches_band_4_and_pulls_the_fader() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.set_in_service(true, rig.now, 1);
        rig.howl(0, 2_500.0);
        let pulls = rig.eq.take_pulls();
        assert_eq!(pulls.len(), 1);
        assert_eq!(pulls[0].cut_db, 3.0);
        rig.flush();
        let b4 = rig.desk[&0].bands[3];
        assert!((b4.gain_db + 3.0).abs() < 0.2, "{b4:?}");
        assert_eq!(b4.width, grid::NARROWEST);
        assert_eq!(rig.mode(0), EqMode::Notch);
        // Keeps ringing: deeper, and the fader comes down once more.
        rig.now += S;
        rig.howl(0, 2_500.0);
        rig.flush();
        assert!((rig.desk[&0].bands[3].gain_db + 6.0).abs() < 0.2);
        assert_eq!(rig.eq.take_pulls()[0].cut_db, 3.0);
        rig.now += S;
        rig.howl(0, 2_500.0);
        rig.flush();
        assert!(rig.desk[&0].bands[3].gain_db >= hard::NOTCH_MAX_CUT_DB);
        assert!(rig.eq.take_pulls().is_empty(), "6 dB is the most per ring");
        let log = rig.eq.take_log();
        assert!(log.iter().all(|e| e.action == EqAction::Feedback));
        assert!(log[0].reason.as_ref().unwrap().contains("2.5 kHz"));
        let fb = rig.eq.status(rig.now).feedback.unwrap();
        assert_eq!(fb.count_today, 1);
    }

    #[test]
    fn a_persons_band_4_is_left_alone_and_only_the_fader_moves() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.event(ConsoleEvent::Eq {
            id: ChannelId::input(0),
            change: EqChange::BandGain { band: 3, db: 2.0 },
        });
        rig.howl(0, 2_500.0);
        assert!(rig.eq.take_sends().is_empty());
        assert_eq!(rig.eq.take_pulls().len(), 1);
        assert_eq!(rig.eq.status(rig.now).feedback.unwrap().notch_db, None);
    }

    #[test]
    fn frozen_means_nothing_moves() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.freeze();
        rig.howl(0, 2_500.0);
        assert!(rig.eq.take_sends().is_empty());
        assert!(rig.eq.take_pulls().is_empty());
        assert_eq!(rig.mode(0), EqMode::Frozen);
    }

    #[test]
    fn undo_goes_back_to_soundcheck_eq_and_holds() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.set_in_service(true, rig.now, 1);
        rig.howl(0, 2_500.0);
        rig.flush();
        rig.eq.take_log();
        rig.eq.undo(0, &person(), rig.now, 5).unwrap();
        rig.flush();
        assert_eq!(rig.desk[&0].snapped(), ChannelEq::default().snapped());
        assert_eq!(rig.mode(0), EqMode::Undone);
        let log = rig.eq.take_log();
        assert_eq!(log[0].action, EqAction::Undo);
        assert_eq!(log[0].by, person());
    }

    #[test]
    fn speech_tone_keeping_moves_slowly_within_range() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        let mut desk = ChannelEq::default();
        desk.bands[0] = EqBand::bell(250.0, 1.0, 0.0).snapped();
        rig.read(0, desk);
        // Reading a new EQ after the first read counts as a person; give it back.
        rig.eq.hand_back(0, &person(), 1).unwrap();
        rig.eq.soundcheck_start(rig.now).unwrap();
        rig.hear(0, &flat_speech(), 20.0);
        rig.tick(0.5);
        rig.eq.keep_my_eq().unwrap();
        rig.eq.set_in_service(true, rig.now, 1);
        // The pastor leans in: 4 dB more around 250 Hz than at soundcheck.
        let boomy: Vec<f32> = flat_speech()
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = (band_hz(i) / 250.0).log2();
                v + 4.0 * (-x * x * 6.0).exp()
            })
            .collect();
        rig.hear(0, &boomy, 45.0);
        rig.tick(31.0);
        let log = rig.eq.take_log();
        let tones: Vec<_> = log.iter().filter(|e| e.action == EqAction::Tone).collect();
        assert!(!tones.is_empty());
        assert!(tones.len() <= 4, "0.5 dB per 10 s: {}", tones.len());
        let g = rig.desk[&0].bands[0].gain_db;
        assert!((-hard::LIVE_RANGE_DB - 0.01..0.0).contains(&g), "{g}");
    }

    #[test]
    fn no_tone_keeping_on_music_or_before_the_service() {
        let mut rig = Rig::new(&[(0, ChannelRole::LeadVocal, "Lead")]);
        rig.eq.keep_my_eq().unwrap();
        rig.eq.set_in_service(true, rig.now, 1);
        rig.hear(0, &boxy(), 60.0);
        rig.tick(30.0);
        assert!(rig.eq.take_log().is_empty());
    }

    #[test]
    fn end_of_service_collects_ideas_and_saves_the_mic() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.set_in_service(true, rig.now, 1);
        rig.howl(0, 2_500.0);
        rig.flush();
        rig.eq.set_in_service(false, rig.now, 99);
        let ideas = rig.eq.take_ideas();
        assert_eq!(ideas.len(), 1);
        assert_eq!(ideas[0].state, IdeaState::Waiting);
        assert!(ideas[0].title.contains("2.5 kHz"), "{}", ideas[0].title);
        let profiles = rig.eq.take_profiles().unwrap();
        let prof = &profiles["Pastor"];
        // Saved without today's notch; the idea can add it.
        assert!(prof.eq.bands[3].gain_db.abs() < 0.1);
        assert_eq!(prof.feedback_hz.len(), 1);
        rig.eq.keep_idea(&ideas[0]);
        let profiles = rig.eq.take_profiles().unwrap();
        assert!(profiles["Pastor"].eq.bands[3].gain_db < -2.0);
    }

    #[test]
    fn a_desk_that_differs_from_last_week_is_flagged_and_can_be_restored() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        let mut last = ChannelEq::default();
        last.bands[1].gain_db = -2.0;
        let last = last.snapped();
        rig.eq.load_profiles(BTreeMap::from([(
            "Pastor".to_string(),
            Profile {
                eq: last,
                feedback_hz: vec![],
            },
        )]));
        assert_eq!(rig.eq.status(rig.now).profile_differs, vec![0]);
        assert_eq!(rig.eq.restore_profiles(&person(), rig.now, 1), Ok(1));
        rig.flush();
        assert_eq!(rig.desk[&0].snapped(), last);
        assert!(rig.eq.status(rig.now).profile_differs.is_empty());
    }

    #[test]
    fn feedback_check_raises_until_it_rings_then_puts_it_back() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.ring_out_start(rig.now).unwrap();
        rig.tick(4.0);
        let st = rig.eq.status(rig.now).ring_out.unwrap();
        assert_eq!(st.channel, Some(0));
        rig.howl(0, 1_600.0);
        rig.tick(3.0);
        let ceilings = rig.eq.take_ceilings().unwrap();
        assert_eq!(ceilings.len(), 1);
        assert!(ceilings[0].1 < -10.0 + 2.0 - 2.9, "{ceilings:?}");
        let st = rig.eq.status(rig.now).ring_out.unwrap();
        assert!(!st.running);
        assert_eq!(st.results[0].hz, Some(1_600.0));
        // The fader went back to where it was.
        let back = rig.eq.tracks[&0].fader_db;
        assert_eq!(back, Some(-10.0));
        // No notch during the check.
        assert_eq!(rig.desk[&0].bands[3].gain_db, 0.0);
    }

    #[test]
    fn feedback_check_waits_for_the_service_to_end() {
        let mut rig = Rig::new(&[(0, ChannelRole::Speech, "Pastor")]);
        rig.eq.set_in_service(true, rig.now, 1);
        assert_eq!(rig.eq.ring_out_start(rig.now), Err(IN_SERVICE));
    }
}
