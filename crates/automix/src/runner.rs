//! Runs [`AutoMix`] against a live console.
//!
//! [`start`] returns a cloneable [`AutoMixHandle`] and a task to spawn on the
//! async runtime. The task owns the engine: it takes meter frames, console
//! events and commands from the handle, runs the control loop ten times a
//! second, sends fader moves through a [`FaderSink`] and reports status and
//! every adjustment to an [`Observer`].
//!
//! Freeze is an atomic flag the handle sets directly, so it takes effect
//! before the next move goes out even if the command queue is busy.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use mix_core::hearing::HearingFrame;
use mix_core::{ConsoleEvent, MeterFrame};
use tokio::sync::{mpsc, oneshot};

use crate::engine::{Adjustment, AdjustmentKind, AutoMix, AutoMixConfig, AutoMixStatus, FaderMove};

/// How often the controller runs.
pub const TICK: Duration = Duration::from_millis(100);
/// Status goes to the UI at most this often (plus right after every command).
const STATUS_EVERY_TICKS: u32 = 3;

/// Where fader moves go. Deliberately fader-only: auto-mix has no way to mute,
/// unmute, recall scenes or touch routing.
#[async_trait]
pub trait FaderSink: Send + Sync + 'static {
    /// Sets input `channel` (0-based) to `db`.
    async fn set_fader(&self, channel: u16, db: f32) -> Result<(), String>;
}

/// Receives what auto-mix is doing.
pub trait Observer: Send + Sync + 'static {
    fn status(&self, status: &AutoMixStatus);
    fn adjustment(&self, adjustment: &Adjustment);
}

#[derive(Debug, Clone, Copy)]
pub struct Stopped;

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("auto-mix isn't running")
    }
}

impl std::error::Error for Stopped {}

enum Command {
    Configure(AutoMixConfig, oneshot::Sender<AutoMixConfig>),
    Engage(bool),
    Freeze,
    Unfreeze,
    ResumeChannel(u16),
    Undo(u16),
    UndoAll,
    Status(oneshot::Sender<AutoMixStatus>),
}

enum Input {
    Meter(MeterFrame),
    Hearing(HearingFrame),
    Console(ConsoleEvent),
    Command(Command),
}

#[derive(Clone)]
pub struct AutoMixHandle {
    tx: mpsc::Sender<Input>,
    frozen: Arc<AtomicBool>,
}

impl AutoMixHandle {
    /// Called from the audio thread; never blocks. Drops the frame if the loop is behind.
    pub fn push_meters(&self, frame: MeterFrame) {
        let _ = self.tx.try_send(Input::Meter(frame));
    }

    /// Latest listening results. Never blocks; dropped if the loop is behind.
    pub fn push_hearing(&self, frame: HearingFrame) {
        let _ = self.tx.try_send(Input::Hearing(frame));
    }

    /// Never blocks.
    pub fn push_console(&self, event: ConsoleEvent) {
        if self.tx.try_send(Input::Console(event)).is_err() {
            log::warn!("auto-mix dropped a console event");
        }
    }

    /// Stops automatic moves immediately.
    pub fn freeze(&self) {
        self.frozen.store(true, Ordering::SeqCst);
        // The flag already holds every move; the command just updates status and the log.
        let _ = self.tx.try_send(Input::Command(Command::Freeze));
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen.load(Ordering::SeqCst)
    }

    async fn send(&self, cmd: Command) -> Result<(), Stopped> {
        self.tx.send(Input::Command(cmd)).await.map_err(|_| Stopped)
    }

    pub async fn unfreeze(&self) -> Result<(), Stopped> {
        self.frozen.store(false, Ordering::SeqCst);
        self.send(Command::Unfreeze).await
    }

    /// Applies new settings and returns them as the engine will use them (sanitized).
    pub async fn configure(&self, config: AutoMixConfig) -> Result<AutoMixConfig, Stopped> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Configure(config, tx)).await?;
        rx.await.map_err(|_| Stopped)
    }

    pub async fn engage(&self, on: bool) -> Result<(), Stopped> {
        self.send(Command::Engage(on)).await
    }

    pub async fn resume_channel(&self, channel: u16) -> Result<(), Stopped> {
        self.send(Command::ResumeChannel(channel)).await
    }

    pub async fn undo(&self, channel: u16) -> Result<(), Stopped> {
        self.send(Command::Undo(channel)).await
    }

    pub async fn undo_all(&self) -> Result<(), Stopped> {
        self.send(Command::UndoAll).await
    }

    pub async fn status(&self) -> Result<AutoMixStatus, Stopped> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Status(tx)).await?;
        rx.await.map_err(|_| Stopped)
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

struct Runner<S: FaderSink, O: Observer> {
    mix: AutoMix,
    sink: Arc<S>,
    observer: O,
    frozen: Arc<AtomicBool>,
    start: tokio::time::Instant,
}

impl<S: FaderSink, O: Observer> Runner<S, O> {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn flush_log(&mut self) {
        for mut a in self.mix.take_log() {
            a.at_ms = unix_ms();
            self.observer.adjustment(&a);
        }
    }

    fn publish(&self) {
        self.observer.status(&self.mix.status(self.now()));
    }

    async fn send_moves(&mut self, moves: Vec<FaderMove>) {
        for mv in moves {
            let automatic = mv.record.kind == AdjustmentKind::Auto;
            // Checked per move: a freeze lands between two moves of the same tick.
            if automatic && (self.frozen.load(Ordering::SeqCst) || self.mix.is_frozen()) {
                continue;
            }
            match self.sink.set_fader(mv.channel, mv.to_db).await {
                Ok(()) => {
                    let now = self.now();
                    self.mix.confirm_sent(&mv, now);
                    let mut record = mv.record;
                    record.at_ms = unix_ms();
                    self.observer.adjustment(&record);
                }
                Err(e) => log::warn!("auto-mix couldn't move input {}: {e}", mv.channel + 1),
            }
        }
    }

    async fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Configure(config, reply) => {
                let applied = self.mix.configure(config).clone();
                let _ = reply.send(applied);
            }
            Command::Engage(on) => self.mix.set_engaged(on),
            Command::Freeze => self.mix.freeze(),
            Command::Unfreeze => {
                if !self.frozen.load(Ordering::SeqCst) {
                    self.mix.unfreeze();
                }
            }
            Command::ResumeChannel(ch) => self.mix.resume_channel(ch),
            Command::Undo(ch) => {
                let moves = self.mix.undo(ch).into_iter().collect();
                self.send_moves(moves).await;
            }
            Command::UndoAll => {
                let moves = self.mix.undo_all();
                self.send_moves(moves).await;
            }
            Command::Status(reply) => {
                let _ = reply.send(self.mix.status(self.now()));
            }
        }
        self.flush_log();
        self.publish();
    }

    async fn run(mut self, mut rx: mpsc::Receiver<Input>) {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut ticks = 0u32;
        loop {
            tokio::select! {
                input = rx.recv() => match input {
                    None => break,
                    Some(Input::Meter(frame)) => {
                        let now = self.now();
                        self.mix.on_meter(&frame, now);
                    }
                    Some(Input::Hearing(frame)) => {
                        let now = self.now();
                        self.mix.on_hearing(&frame, now);
                    }
                    Some(Input::Console(event)) => {
                        let now = self.now();
                        self.mix.on_console(&event, now);
                        self.flush_log();
                    }
                    Some(Input::Command(cmd)) => self.command(cmd).await,
                },
                _ = ticker.tick() => {
                    let now = self.now();
                    let moves = self.mix.tick(now);
                    self.send_moves(moves).await;
                    self.flush_log();
                    ticks = ticks.wrapping_add(1);
                    if ticks.is_multiple_of(STATUS_EVERY_TICKS) {
                        self.publish();
                    }
                }
            }
        }
    }
}

/// Builds the auto-mix loop. Spawn the returned future on the async runtime;
/// it ends when every handle is dropped.
pub fn start<S: FaderSink, O: Observer>(
    config: AutoMixConfig,
    sink: Arc<S>,
    observer: O,
) -> (AutoMixHandle, impl Future<Output = ()> + Send + 'static) {
    let (tx, rx) = mpsc::channel(1024);
    let frozen = Arc::new(AtomicBool::new(false));
    let runner = Runner {
        mix: AutoMix::new(config),
        sink,
        observer,
        frozen: frozen.clone(),
        start: tokio::time::Instant::now(),
    };
    (AutoMixHandle { tx, frozen }, runner.run(rx))
}
