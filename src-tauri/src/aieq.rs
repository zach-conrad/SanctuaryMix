//! AI EQ in the app: the analyser on the audio tap, the AI EQ loop, its link
//! to the console and auto-mix, the EQ log and the commands the screens use.
//! The rules live in the `tonal` crate; these commands only ask, so nothing
//! the UI sends can loosen a guardrail. See docs/AIEQ.md.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use auth::{Feature, Permission, Session};
use automix::{AutoMixConfig, AutoMixHandle};
use console::ConsoleError;
use mix_core::eq::{ChannelEq, EqChange};
use mix_core::{ChangeSource, ChannelId, ConsoleEvent};
use serde::Serialize;
use store::Store;
use tauri::{AppHandle, Emitter, Manager, State};
use tonal::analyser::{AnalyseTarget, Analyser};
use tonal::engine::{Profile, Pull};
use tonal::runner::{AiEqHandle, DeskSink, Observer};
use tonal::{AiEqConfig, AiEqStatus, CompareSide, EqActor, EqIdea, EqLogEntry, IdeaState};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// How often the app checks whether a service has started or ended.
const SERVICE_CHECK: Duration = Duration::from_secs(2);

/// The recording of the service under way (or the last one), so ideas can name it.
pub type ServiceRecording = Arc<Mutex<Option<String>>>;

/// Starts the analyser and the AI EQ loop with the saved settings.
pub fn start(
    app: &AppHandle,
    store: &Arc<Mutex<Store>>,
    automix: &AutoMixHandle,
    automix_config: &AutoMixConfig,
    service: ServiceRecording,
) -> (AiEqHandle, Arc<Analyser>) {
    let (config, profiles, ceilings) = {
        let s = store.lock().unwrap();
        let config = s
            .get_setting::<AiEqConfig>(store::keys::AIEQ_CONFIG)
            .inspect_err(|e| log::warn!("ignoring saved AI EQ settings: {e}"))
            .ok()
            .flatten()
            .unwrap_or_default();
        let profiles = s
            .get_setting::<BTreeMap<String, Profile>>(store::keys::AIEQ_PROFILES)
            .ok()
            .flatten()
            .unwrap_or_default();
        let ceilings = s
            .get_setting::<Vec<(u16, f32)>>(store::keys::AIEQ_CEILINGS)
            .ok()
            .flatten()
            .unwrap_or_default();
        (config, profiles, ceilings)
    };
    let analyser_slot: Arc<Mutex<Option<Arc<Analyser>>>> = Arc::default();
    let (handle, task) = tonal::runner::start(
        config,
        profiles,
        Arc::new(Desk(app.clone())),
        AppObserver {
            app: app.clone(),
            store: store.clone(),
            automix: automix.clone(),
            analyser: analyser_slot.clone(),
            service,
        },
    );
    tauri::async_runtime::spawn(task);
    let tone = handle.clone();
    let analyser = Arc::new(Analyser::start(move |frame| tone.push_tone(frame)));
    *analyser_slot.lock().unwrap() = Some(analyser.clone());

    let automix = automix.clone();
    tauri::async_runtime::spawn(async move {
        if !ceilings.is_empty() {
            let _ = automix.set_feedback_ceilings(ceilings).await;
        }
    });
    set_picks(&handle, automix_config);
    (handle, analyser)
}

/// One pick for both: AI EQ looks after auto-mix's channels.
pub fn set_picks(handle: &AiEqHandle, config: &AutoMixConfig) {
    let picks = config.channels.clone();
    let nudges = config.nudges;
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let _ = handle.call(move |e, _, _| e.set_picks(picks, nudges)).await;
    });
}

/// Tells AI EQ when a service starts and ends, and which recording it is.
pub async fn watch_service(app: AppHandle) {
    let mut ticker = tokio::time::interval(SERVICE_CHECK);
    loop {
        ticker.tick().await;
        let on = crate::commands::in_service(&app).await;
        let state = app.state::<AppState>();
        let recording = match &state.recordings {
            Some(r) => r.active_id().await,
            None => None,
        };
        if let (true, Some(id)) = (on, recording) {
            // Taken (and so cleared) when the service ends and its ideas are saved.
            *state.aieq_service.lock().unwrap() = Some(id);
        }
        let _ = state
            .aieq
            .call(move |e, now, ms| e.set_in_service(on, now, ms))
            .await;
    }
}

/// AI EQ's way to the console: input EQ, plus faders during the feedback check.
struct Desk(AppHandle);

#[async_trait]
impl DeskSink for Desk {
    async fn set_eq(&self, channel: u16, change: EqChange, by_ai: bool) -> Result<(), String> {
        let state = self.0.state::<AppState>();
        let slot = state.console.lock().await;
        let adapter = slot
            .as_ref()
            .ok_or(ConsoleError::NotConnected)
            .map_err(err)?;
        let id = ChannelId::input(channel);
        adapter.set_eq(id, change).await.map_err(err)?;
        let source = if by_ai {
            ChangeSource::Assist
        } else {
            ChangeSource::Operator
        };
        state
            .control
            .publish(source, ConsoleEvent::Eq { id, change });
        Ok(())
    }

    async fn request_eq(&self, channel: u16) -> Result<(), String> {
        let state = self.0.state::<AppState>();
        let slot = state.console.lock().await;
        let adapter = slot
            .as_ref()
            .ok_or(ConsoleError::NotConnected)
            .map_err(err)?;
        adapter
            .request_eq(ChannelId::input(channel))
            .await
            .map_err(err)
    }

    async fn set_fader(&self, channel: u16, db: f32) -> Result<(), String> {
        let state = self.0.state::<AppState>();
        let slot = state.console.lock().await;
        let adapter = slot
            .as_ref()
            .ok_or(ConsoleError::NotConnected)
            .map_err(err)?;
        let id = ChannelId::input(channel);
        adapter.set_fader(id, Some(db)).await.map_err(err)?;
        state.control.publish(
            ChangeSource::Assist,
            ConsoleEvent::Fader { id, db: Some(db) },
        );
        Ok(())
    }
}

struct AppObserver {
    app: AppHandle,
    store: Arc<Mutex<Store>>,
    automix: AutoMixHandle,
    analyser: Arc<Mutex<Option<Arc<Analyser>>>>,
    service: ServiceRecording,
}

impl AppObserver {
    fn analyser(&self) -> Option<Arc<Analyser>> {
        self.analyser.lock().unwrap().clone()
    }
}

impl Observer for AppObserver {
    fn status(&self, status: &AiEqStatus) {
        let _ = self.app.emit("aieq", status);
    }

    fn log(&self, entry: &EqLogEntry) {
        if let Err(e) = self.store.lock().unwrap().append_eq(entry) {
            log::error!("couldn't log an EQ change: {e}");
        }
        let _ = self.app.emit("aieq-log", entry);
    }

    fn ideas(&self, ideas: &[EqIdea]) {
        let recording = self.service.lock().unwrap().take();
        let ideas: Vec<EqIdea> = ideas
            .iter()
            .cloned()
            .map(|mut i| {
                i.recording_id = recording.clone();
                i
            })
            .collect();
        if let Err(e) = self.store.lock().unwrap().save_ideas(&ideas) {
            log::error!("couldn't save EQ ideas: {e}");
        }
    }

    fn feedback_pull(&self, pull: Pull) {
        self.automix
            .feedback_pull(pull.channel, pull.cut_db, pull.hz);
    }

    fn ceilings(&self, ceilings: Vec<(u16, f32)>) {
        if let Err(e) = self
            .store
            .lock()
            .unwrap()
            .set_setting(store::keys::AIEQ_CEILINGS, &ceilings)
        {
            log::error!("couldn't save feedback ceilings: {e}");
        }
        let automix = self.automix.clone();
        tauri::async_runtime::spawn(async move {
            let _ = automix.set_feedback_ceilings(ceilings).await;
        });
    }

    fn profiles(&self, profiles: &BTreeMap<String, Profile>) {
        if let Err(e) = self
            .store
            .lock()
            .unwrap()
            .set_setting(store::keys::AIEQ_PROFILES, profiles)
        {
            log::error!("couldn't save mic EQs: {e}");
        }
    }

    fn analyse(&self, targets: Vec<AnalyseTarget>) {
        if let Some(a) = self.analyser() {
            a.set_targets(targets);
        }
    }

    fn reset(&self, channels: Vec<u16>) {
        if let Some(a) = self.analyser() {
            a.reset(channels);
        }
    }
}

// ---- commands -------------------------------------------------------------

/// Who is tapping, for the EQ log.
fn actor(session: &Session) -> EqActor {
    let role = serde_json::to_value(session.role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "volunteer".into());
    EqActor::app(
        &role,
        &session.user.display_name,
        session.authenticated.then_some(session.user.id.as_str()),
    )
}

async fn session(state: &AppState) -> Session {
    state.auth.current_session().await
}

const ENGINEER_ONLY: &str = "Ask an engineer or admin to do that.";

fn require_engineer(session: &Session) -> CmdResult<()> {
    session
        .require(Permission::ChangeAutoMixSetup)
        .map_err(|_| ENGINEER_ONLY.to_string())
}

fn refused<T>(r: Result<T, tonal::engine::Refused>) -> CmdResult<T> {
    r.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn aieq_get_config(state: State<'_, AppState>) -> CmdResult<AiEqConfig> {
    state
        .aieq
        .call(|e, _, _| e.config().clone())
        .await
        .map_err(err)
}

/// On/off is anyone's (turning on needs the plan); tone keeping and the tap
/// point are Engineers' and Admins'.
#[tauri::command]
pub async fn aieq_set_config(app: AppHandle, config: AiEqConfig) -> CmdResult<AiEqConfig> {
    let state = app.state::<AppState>();
    let s = session(&state).await;
    let current = aieq_get_config(state.clone()).await?;
    if (config.tone_keeping != current.tone_keeping || config.tap != current.tap)
        && require_engineer(&s).is_err()
    {
        return Err("Ask an engineer or admin to change speech tone keeping.".into());
    }
    if config.enabled && !current.enabled {
        s.access.require(Feature::AiEq)?;
    }
    let applied = state.aieq.configure(config).await.map_err(err)?;
    let saved = applied.clone();
    state
        .store
        .lock()
        .unwrap()
        .set_setting(store::keys::AIEQ_CONFIG, &saved)
        .map_err(err)?;
    Ok(applied)
}

#[tauri::command]
pub async fn aieq_status(state: State<'_, AppState>) -> CmdResult<AiEqStatus> {
    state.aieq.status().await.map_err(err)
}

/// Newest first.
#[tauri::command]
pub async fn aieq_log(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> CmdResult<Vec<EqLogEntry>> {
    state
        .store
        .lock()
        .unwrap()
        .recent_eq(limit.unwrap_or(200).min(5_000))
        .map_err(err)
}

#[tauri::command]
pub async fn aieq_soundcheck_start(state: State<'_, AppState>) -> CmdResult<()> {
    session(&state).await.access.require(Feature::AiEq)?;
    refused(
        state
            .aieq
            .call(|e, now, _| e.soundcheck_start(now))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_soundcheck_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .aieq
        .call(|e, _, _| e.soundcheck_stop())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn aieq_apply(state: State<'_, AppState>, channel: u16) -> CmdResult<()> {
    let s = session(&state).await;
    s.access.require(Feature::AiEq)?;
    let by = actor(&s);
    refused(
        state
            .aieq
            .call(move |e, now, ms| e.apply(channel, &by, now, ms))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_apply_all(state: State<'_, AppState>) -> CmdResult<()> {
    let s = session(&state).await;
    s.access.require(Feature::AiEq)?;
    let by = actor(&s);
    refused(
        state
            .aieq
            .call(move |e, now, ms| e.apply_all(&by, now, ms).map(|_| ()))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_skip(state: State<'_, AppState>, channel: u16) -> CmdResult<()> {
    refused(
        state
            .aieq
            .call(move |e, _, _| e.skip(channel))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_keep_my_eq(state: State<'_, AppState>) -> CmdResult<()> {
    require_engineer(&session(&state).await)?;
    refused(
        state
            .aieq
            .call(|e, _, _| e.keep_my_eq())
            .await
            .map_err(err)?,
    )
}

/// Anyone at the desk can always undo.
#[tauri::command]
pub async fn aieq_undo(state: State<'_, AppState>, channel: u16) -> CmdResult<()> {
    let by = actor(&session(&state).await);
    refused(
        state
            .aieq
            .call(move |e, now, ms| e.undo(channel, &by, now, ms))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_undo_all(state: State<'_, AppState>) -> CmdResult<()> {
    let by = actor(&session(&state).await);
    state
        .aieq
        .call(move |e, now, ms| {
            e.undo_all(&by, now, ms);
        })
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn aieq_hand_back(state: State<'_, AppState>, channel: u16) -> CmdResult<()> {
    let by = actor(&session(&state).await);
    refused(
        state
            .aieq
            .call(move |e, _, ms| e.hand_back(channel, &by, ms))
            .await
            .map_err(err)?,
    )
}

/// A person's own EQ edit. Mixing by hand is never locked.
#[tauri::command]
pub async fn aieq_set_eq(state: State<'_, AppState>, channel: u16, eq: ChannelEq) -> CmdResult<()> {
    let by = actor(&session(&state).await);
    refused(
        state
            .aieq
            .call(move |e, now, ms| e.set_by_person(channel, eq, &by, now, ms))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_compare(
    state: State<'_, AppState>,
    channel: u16,
    side: Option<CompareSide>,
) -> CmdResult<()> {
    refused(
        state
            .aieq
            .call(move |e, now, _| e.compare(channel, side, now))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_restore_profile(state: State<'_, AppState>) -> CmdResult<()> {
    let by = actor(&session(&state).await);
    refused(
        state
            .aieq
            .call(move |e, now, ms| e.restore_profiles(&by, now, ms).map(|_| ()))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_dismiss_profile(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .aieq
        .call(|e, _, _| e.dismiss_profiles())
        .await
        .map_err(err)
}

/// The feedback check. Engineers and Admins; the screen asks first, every time.
#[tauri::command]
pub async fn aieq_ring_out_start(state: State<'_, AppState>) -> CmdResult<()> {
    let s = session(&state).await;
    require_engineer(&s)?;
    s.access.require(Feature::AiEq)?;
    refused(
        state
            .aieq
            .call(|e, now, _| e.ring_out_start(now))
            .await
            .map_err(err)?,
    )
}

#[tauri::command]
pub async fn aieq_ring_out_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .aieq
        .call(|e, _, _| e.ring_out_stop())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn aieq_dismiss_feedback(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .aieq
        .call(|e, _, _| e.dismiss_feedback())
        .await
        .map_err(err)
}

/// A recording's ideas, or every idea still waiting.
#[tauri::command]
pub async fn aieq_ideas(
    state: State<'_, AppState>,
    recording_id: Option<String>,
) -> CmdResult<Vec<EqIdea>> {
    state
        .store
        .lock()
        .unwrap()
        .ideas(recording_id.as_deref())
        .map_err(err)
}

/// Keep or dismiss an idea; keeping one changes that mic's saved EQ for
/// next week. Returns the ideas from the same service.
#[tauri::command]
pub async fn aieq_set_idea(
    app_state: State<'_, AppState>,
    id: String,
    state: IdeaState,
) -> CmdResult<Vec<EqIdea>> {
    let app = app_state;
    require_engineer(&session(&app).await)?;
    let idea = app
        .store
        .lock()
        .unwrap()
        .set_idea_state(&id, state)
        .map_err(err)?
        .ok_or("That idea is gone. Reopen the service.")?;
    if idea.state == IdeaState::Kept {
        let kept = idea.clone();
        app.aieq
            .call(move |e, _, _| e.keep_idea(&kept))
            .await
            .map_err(err)?;
    }
    let list = app
        .store
        .lock()
        .unwrap()
        .ideas(idea.recording_id.as_deref());
    list.map_err(err)
}

/// One EQ log entry in a recorded service, with its time from the recording's
/// start (`None` for soundcheck before it began).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    #[serde(flatten)]
    entry: EqLogEntry,
    t_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqAudit {
    entries: Vec<AuditEntry>,
    ideas: Vec<EqIdea>,
}

/// Every EQ change from the morning of a recorded service to its end, and
/// the ideas it left. Same fields as the website's EQ history.
#[tauri::command]
pub async fn aieq_audit(state: State<'_, AppState>, recording_id: String) -> CmdResult<EqAudit> {
    let recs = state
        .recordings
        .as_ref()
        .ok_or("Recordings aren't available on this computer.")?;
    let rec = recs.store.recording(&recording_id).map_err(err)?;
    let from =
        day_start_ms(&rec.service_date).unwrap_or(rec.started_at.saturating_sub(6 * 3_600_000));
    let to = rec
        .ended_at
        .unwrap_or_else(recorder::now_ms)
        .saturating_add(1);
    let store = state.store.lock().unwrap();
    let entries = store
        .eq_between(from, to)
        .map_err(err)?
        .into_iter()
        .map(|entry| AuditEntry {
            t_ms: entry
                .at_ms
                .checked_sub(rec.started_at)
                .filter(|_| entry.at_ms >= rec.started_at),
            entry,
        })
        .collect();
    let ideas = store.ideas(Some(&recording_id)).map_err(err)?;
    Ok(EqAudit { entries, ideas })
}

/// Local midnight at the start of `YYYY-MM-DD`, in Unix ms.
fn day_start_ms(date: &str) -> Option<u64> {
    use chrono::{Local, NaiveDate, TimeZone};
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let midnight = Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    u64::try_from(midnight.timestamp_millis()).ok()
}
