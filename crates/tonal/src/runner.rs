//! Runs [`AiEq`] against a live console.
//!
//! [`start`] returns a cloneable [`AiEqHandle`] and a task to spawn on the
//! async runtime. The task owns the engine: it takes analyser frames,
//! console events and calls from the handle, runs the engine ten times a
//! second, reads EQ from the desk for picked channels, sends EQ changes
//! through a [`DeskSink`] under the per-second budget, and reports status,
//! the EQ log, ideas and feedback pulls to an [`Observer`].
//!
//! Freeze is an atomic flag the handle sets directly, so queued AI moves are
//! dropped before the next message goes out even if the call queue is busy.

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use mix_core::eq::EqChange;
use mix_core::ConsoleEvent;
use tokio::sync::{mpsc, oneshot};

use crate::analyser::{AnalyseTarget, ToneFrame};
use crate::engine::{AiEq, EqSend, Profile, Pull};
use crate::guardrails::hard;
use crate::types::{AiEqConfig, AiEqStatus, EqIdea, EqLogEntry};

pub const TICK: Duration = Duration::from_millis(100);
const STATUS_EVERY_TICKS: u32 = 3;
/// Reading one channel's EQ costs this many messages.
const READ_COST: u32 = 18;
/// Ask a channel again if its EQ hasn't all arrived by then.
const REREAD_AFTER: Duration = Duration::from_secs(5);

/// Where EQ (and feedback-check fader) messages go.
#[async_trait]
pub trait DeskSink: Send + Sync + 'static {
    async fn set_eq(&self, channel: u16, change: EqChange) -> Result<(), String>;
    /// Asks the desk for every EQ parameter on `channel`.
    async fn request_eq(&self, channel: u16) -> Result<(), String>;
    /// Feedback check only.
    async fn set_fader(&self, channel: u16, db: f32) -> Result<(), String>;
}

/// Receives what AI EQ is doing.
pub trait Observer: Send + Sync + 'static {
    fn status(&self, status: &AiEqStatus);
    fn log(&self, entry: &EqLogEntry);
    /// Ideas collected at the end of a service.
    fn ideas(&self, ideas: &[EqIdea]);
    /// A ringing channel's fader should come down (auto-mix does it).
    fn feedback_pull(&self, pull: Pull);
    /// New feedback ceilings from the feedback check.
    fn ceilings(&self, ceilings: Vec<(u16, f32)>);
    /// Saved mic EQs changed; persist them.
    fn profiles(&self, profiles: &BTreeMap<String, Profile>);
    /// Which channels the analyser should listen to.
    fn analyse(&self, targets: Vec<AnalyseTarget>);
    /// Start these channels' averages over.
    fn reset(&self, channels: Vec<u16>);
}

#[derive(Debug, Clone, Copy)]
pub struct Stopped;

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AI EQ isn't running")
    }
}

impl std::error::Error for Stopped {}

type Call = Box<dyn FnOnce(&mut AiEq, Duration, u64) + Send>;

enum Input {
    Tone(ToneFrame),
    Console(ConsoleEvent),
    Call(Call),
}

#[derive(Clone)]
pub struct AiEqHandle {
    tx: mpsc::Sender<Input>,
    frozen: Arc<AtomicBool>,
}

impl AiEqHandle {
    /// From the analyser thread. Never blocks; a frame with a howl is never dropped quietly.
    pub fn push_tone(&self, frame: ToneFrame) {
        let howl = !frame.howls.is_empty();
        if self.tx.try_send(Input::Tone(frame)).is_err() && howl {
            log::warn!("AI EQ dropped a feedback report");
        }
    }

    /// Never blocks.
    pub fn push_console(&self, event: ConsoleEvent) {
        if self.tx.try_send(Input::Console(event)).is_err() {
            log::warn!("AI EQ dropped a console event");
        }
    }

    /// Stops AI EQ moves immediately (Freeze stops every AI move).
    pub fn freeze(&self) {
        self.frozen.store(true, Ordering::SeqCst);
        let _ = self
            .tx
            .try_send(Input::Call(Box::new(|e, _, _| e.freeze())));
    }

    pub async fn unfreeze(&self) -> Result<(), Stopped> {
        self.frozen.store(false, Ordering::SeqCst);
        self.call(|e, _, _| e.unfreeze()).await
    }

    /// Runs `f` on the engine with the loop's clock (`now`, Unix ms) and
    /// returns its result. Status goes out right after.
    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut AiEq, Duration, u64) -> T + Send + 'static,
    ) -> Result<T, Stopped> {
        let (tx, rx) = oneshot::channel();
        let call: Call = Box::new(move |e, now, ms| {
            let _ = tx.send(f(e, now, ms));
        });
        self.tx.send(Input::Call(call)).await.map_err(|_| Stopped)?;
        rx.await.map_err(|_| Stopped)
    }

    pub async fn configure(&self, config: AiEqConfig) -> Result<AiEqConfig, Stopped> {
        self.call(move |e, _, _| e.configure(config).clone()).await
    }

    pub async fn status(&self) -> Result<AiEqStatus, Stopped> {
        self.call(|e, now, _| e.status(now)).await
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

struct Runner<S: DeskSink, O: Observer> {
    eq: AiEq,
    sink: Arc<S>,
    observer: O,
    frozen: Arc<AtomicBool>,
    start: tokio::time::Instant,
    queue: VecDeque<EqSend>,
    budget: u32,
    asked: BTreeMap<u16, Duration>,
}

impl<S: DeskSink, O: Observer> Runner<S, O> {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn drain(&mut self) {
        self.queue.extend(self.eq.take_sends());
        for entry in self.eq.take_log() {
            self.observer.log(&entry);
        }
        for pull in self.eq.take_pulls() {
            self.observer.feedback_pull(pull);
        }
        let ideas = self.eq.take_ideas();
        if !ideas.is_empty() {
            self.observer.ideas(&ideas);
        }
        if let Some(c) = self.eq.take_ceilings() {
            self.observer.ceilings(c);
        }
        if let Some(t) = self.eq.take_analyse() {
            self.observer.analyse(t);
        }
        if let Some(r) = self.eq.take_reset() {
            self.observer.reset(r);
        }
        if let Some(p) = self.eq.take_profiles() {
            self.observer.profiles(&p);
        }
    }

    fn publish(&self) {
        self.observer.status(&self.eq.status(self.now()));
    }

    /// Sends what fits in this tick's share of the per-second budget.
    async fn send(&mut self) {
        let frozen = self.frozen.load(Ordering::SeqCst) || self.eq.is_frozen();
        if frozen {
            self.queue.retain(|s| !s.by_ai);
        }
        for (ch, db) in self.eq.take_faders() {
            if let Err(e) = self.sink.set_fader(ch, db).await {
                log::warn!("AI EQ couldn't move input {}: {e}", ch + 1);
            }
        }
        while self.budget > 0 {
            let Some(s) = self.queue.pop_front() else {
                break;
            };
            if s.by_ai && self.frozen.load(Ordering::SeqCst) {
                continue;
            }
            self.budget -= 1;
            if let Err(e) = self.sink.set_eq(s.channel, s.change).await {
                log::warn!("AI EQ couldn't set input {}: {e}", s.channel + 1);
            }
        }
        // Read picked channels' EQ, one channel at a time, when there's room.
        if self.queue.is_empty() && self.budget >= READ_COST {
            let now = self.now();
            let next = self.eq.unread().into_iter().find(|ch| {
                self.asked
                    .get(ch)
                    .is_none_or(|&at| now.saturating_sub(at) >= REREAD_AFTER)
            });
            if let Some(ch) = next {
                self.asked.insert(ch, now);
                self.budget -= READ_COST;
                if let Err(e) = self.sink.request_eq(ch).await {
                    log::warn!("AI EQ couldn't read input {}: {e}", ch + 1);
                }
            }
        }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<Input>) {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ticks = 0u32;
        let mut last = self.now();
        let per_tick = (hard::MSGS_PER_SEC as u128 * TICK.as_millis() / 1000) as u32;
        loop {
            tokio::select! {
                input = rx.recv() => match input {
                    None => break,
                    Some(Input::Tone(frame)) => {
                        let urgent = !frame.howls.is_empty();
                        let now = self.now();
                        self.eq.on_tone(&frame, now, unix_ms());
                        if urgent {
                            // Feedback doesn't wait for the next tick.
                            self.drain();
                            self.send().await;
                            self.publish();
                        }
                    }
                    Some(Input::Console(event)) => {
                        let connected = matches!(event, ConsoleEvent::Connected { .. });
                        let now = self.now();
                        self.eq.on_console(&event, now, unix_ms());
                        if connected {
                            self.asked.clear();
                        }
                        self.drain();
                    }
                    Some(Input::Call(call)) => {
                        let now = self.now();
                        call(&mut self.eq, now, unix_ms());
                        self.drain();
                        self.send().await;
                        self.publish();
                    }
                },
                _ = ticker.tick() => {
                    let now = self.now();
                    let dt = now.saturating_sub(last);
                    last = now;
                    self.budget = (self.budget + per_tick).min(hard::MSGS_PER_SEC);
                    self.eq.tick(now, dt, unix_ms());
                    self.drain();
                    self.send().await;
                    ticks = ticks.wrapping_add(1);
                    if ticks.is_multiple_of(STATUS_EVERY_TICKS) {
                        self.publish();
                    }
                }
            }
        }
    }
}

/// Builds the AI EQ loop. Spawn the returned future on the async runtime;
/// it ends when every handle is dropped.
pub fn start<S: DeskSink, O: Observer>(
    config: AiEqConfig,
    profiles: BTreeMap<String, Profile>,
    sink: Arc<S>,
    observer: O,
) -> (AiEqHandle, impl Future<Output = ()> + Send + 'static) {
    let (tx, rx) = mpsc::channel(1024);
    let frozen = Arc::new(AtomicBool::new(false));
    let mut eq = AiEq::new(config);
    eq.load_profiles(profiles);
    let runner = Runner {
        eq,
        sink,
        observer,
        frozen: frozen.clone(),
        start: tokio::time::Instant::now(),
        queue: VecDeque::new(),
        budget: hard::MSGS_PER_SEC,
        asked: BTreeMap::new(),
    };
    (AiEqHandle { tx, frozen }, runner.run(rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AiEqConfig, EqActor};
    use automix::{ChannelRole, ManagedChannel, Nudges};
    use mix_core::ChannelId;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Desk {
        eq_msgs: Mutex<Vec<(u16, EqChange)>>,
        reads: Mutex<Vec<u16>>,
    }

    #[async_trait]
    impl DeskSink for Desk {
        async fn set_eq(&self, channel: u16, change: EqChange) -> Result<(), String> {
            self.eq_msgs.lock().unwrap().push((channel, change));
            Ok(())
        }
        async fn request_eq(&self, channel: u16) -> Result<(), String> {
            self.reads.lock().unwrap().push(channel);
            Ok(())
        }
        async fn set_fader(&self, _: u16, _: f32) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Default, Clone)]
    struct Seen {
        log: Arc<Mutex<Vec<EqLogEntry>>>,
        pulls: Arc<Mutex<Vec<Pull>>>,
    }

    impl Observer for Seen {
        fn status(&self, _: &AiEqStatus) {}
        fn log(&self, e: &EqLogEntry) {
            self.log.lock().unwrap().push(e.clone());
        }
        fn ideas(&self, _: &[EqIdea]) {}
        fn feedback_pull(&self, p: Pull) {
            self.pulls.lock().unwrap().push(p);
        }
        fn ceilings(&self, _: Vec<(u16, f32)>) {}
        fn profiles(&self, _: &BTreeMap<String, Profile>) {}
        fn analyse(&self, _: Vec<AnalyseTarget>) {}
        fn reset(&self, _: Vec<u16>) {}
    }

    #[tokio::test(start_paused = true)]
    async fn reads_the_desk_and_answers_feedback_within_budget() {
        let desk = Arc::new(Desk::default());
        let seen = Seen::default();
        let (h, task) = start(
            AiEqConfig {
                enabled: true,
                ..AiEqConfig::default()
            },
            BTreeMap::new(),
            desk.clone(),
            seen.clone(),
        );
        tokio::spawn(task);
        h.call(|e, _, _| {
            e.set_eq_supported(true);
            e.set_picks(
                vec![ManagedChannel {
                    channel: 2,
                    role: ChannelRole::Speech,
                }],
                Nudges::default(),
            );
        })
        .await
        .unwrap();
        h.push_console(ConsoleEvent::Connected {
            model: "test".into(),
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(desk.reads.lock().unwrap().as_slice(), &[2]);
        // The desk answers with its EQ.
        let eq = mix_core::eq::ChannelEq::default();
        for band in 0..4u8 {
            let b = eq.bands[band as usize];
            for change in [
                EqChange::BandKind { band, kind: b.kind },
                EqChange::BandFreq {
                    band,
                    hz: b.freq_hz,
                },
                EqChange::BandWidth {
                    band,
                    width: b.width,
                },
                EqChange::BandGain {
                    band,
                    db: b.gain_db,
                },
            ] {
                h.push_console(ConsoleEvent::Eq {
                    id: ChannelId::input(2),
                    change,
                });
            }
        }
        h.push_console(ConsoleEvent::Eq {
            id: ChannelId::input(2),
            change: EqChange::HpfOn { on: false },
        });
        h.push_console(ConsoleEvent::Eq {
            id: ChannelId::input(2),
            change: EqChange::HpfFreq { hz: 80.0 },
        });
        h.push_tone(ToneFrame {
            channels: Vec::new(),
            howls: vec![crate::feedback::Howl {
                channel: 2,
                hz: 2_500.0,
                level_db: -10.0,
                secs: 0.4,
            }],
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(!desk.eq_msgs.lock().unwrap().is_empty());
        assert_eq!(seen.pulls.lock().unwrap().len(), 1);
        assert_eq!(seen.log.lock().unwrap().len(), 1);

        // Frozen: the next ring changes nothing.
        h.freeze();
        let before = desk.eq_msgs.lock().unwrap().len();
        h.push_tone(ToneFrame {
            channels: Vec::new(),
            howls: vec![crate::feedback::Howl {
                channel: 2,
                hz: 2_500.0,
                level_db: -10.0,
                secs: 1.4,
            }],
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(desk.eq_msgs.lock().unwrap().len(), before);
        let st = h.status().await.unwrap();
        assert!(st.frozen);
        // People still get their way while frozen.
        let r = h
            .call(|e, now, ms| e.undo(2, &EqActor::app("admin", "Ann", None), now, ms))
            .await
            .unwrap();
        assert!(r.is_ok());
    }
}
