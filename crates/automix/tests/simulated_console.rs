//! Auto-mix driving the simulated console end to end, on a paused clock.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use automix::{
    Adjustment, AdjustmentKind, AutoMixConfig, AutoMixHandle, AutoMixStatus, ChannelMode,
    ChannelRole, FaderSink, ManagedChannel, Observer,
};
use console::simulated::SimulatedConsole;
use console::ConsoleAdapter;
use mix_core::{ChannelId, ChannelMeter, ConsoleEvent, MeterFrame};

struct DeskSink(Arc<SimulatedConsole>);

#[async_trait]
impl FaderSink for DeskSink {
    async fn set_fader(&self, channel: u16, db: f32) -> Result<(), String> {
        self.0
            .set_fader(ChannelId::input(channel), Some(db))
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(Default)]
struct Recorder {
    status: Mutex<Option<AutoMixStatus>>,
    log: Mutex<Vec<Adjustment>>,
}

struct Rec(Arc<Recorder>);

impl Observer for Rec {
    fn status(&self, status: &AutoMixStatus) {
        *self.0.status.lock().unwrap() = Some(status.clone());
    }
    fn adjustment(&self, adjustment: &Adjustment) {
        self.0.log.lock().unwrap().push(adjustment.clone());
    }
}

/// Watches the desk for every fader and mute change.
#[derive(Default)]
struct DeskState {
    faders: Mutex<[Option<f32>; 8]>,
    mute_events: Mutex<usize>,
}

fn frame(rms: [f32; 3]) -> MeterFrame {
    MeterFrame {
        sample_rate: 48_000,
        channels: rms
            .iter()
            .enumerate()
            .map(|(ch, &rms_db)| ChannelMeter {
                channel: ch as u16,
                rms_db,
                peak_db: rms_db + 10.0,
                clipped: false,
            })
            .collect(),
    }
}

async fn feed(handle: &AutoMixHandle, rms: [f32; 3], secs: u64) {
    for _ in 0..secs * 30 {
        handle.push_meters(frame(rms));
        tokio::time::sleep(Duration::from_millis(33)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn rides_selected_faders_on_the_simulated_console() {
    let mut desk = SimulatedConsole::new(8);
    desk.connect().await.unwrap();
    let desk = Arc::new(desk);
    let recorder = Arc::new(Recorder::default());
    let (handle, task) = automix::start(
        AutoMixConfig::default(),
        Arc::new(DeskSink(desk.clone())),
        Rec(recorder.clone()),
    );
    tokio::spawn(task);

    // Forward desk events to auto-mix, and keep our own picture of the desk.
    let state = Arc::new(DeskState::default());
    let mut events = desk.subscribe();
    {
        let handle = handle.clone();
        let state = state.clone();
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                match &event {
                    ConsoleEvent::Fader { id, db } => {
                        state.faders.lock().unwrap()[id.index as usize] = *db
                    }
                    ConsoleEvent::Mute { .. } => *state.mute_events.lock().unwrap() += 1,
                    _ => {}
                }
                handle.push_console(event);
            }
        });
    }
    handle.push_console(ConsoleEvent::Connected {
        model: "Practice console".into(),
    });
    // The operator's starting positions.
    for ch in 0..3 {
        desk.set_fader(ChannelId::input(ch), Some(-10.0))
            .await
            .unwrap();
    }

    // Lead vocal and a speech mic are handed over; input 3 is not.
    let applied = handle
        .configure(AutoMixConfig {
            channels: vec![
                ManagedChannel {
                    channel: 0,
                    role: ChannelRole::LeadVocal,
                },
                ManagedChannel {
                    channel: 1,
                    role: ChannelRole::Speech,
                },
            ],
            ..AutoMixConfig::default()
        })
        .await
        .unwrap();
    assert_eq!(applied.channels.len(), 2);
    handle.engage(true).await.unwrap();

    // Both sit 20 dB under their −20 target; input 3 is just as quiet.
    feed(&handle, [-30.0, -30.0, -30.0], 15).await;
    let faders = *state.faders.lock().unwrap();
    assert_eq!(
        faders[0],
        Some(-4.0),
        "music rides up to +6 dB over the operator"
    );
    assert_eq!(faders[1], Some(-2.0), "speech may go +8 dB");
    assert_eq!(faders[2], Some(-10.0), "unselected channel untouched");
    assert_eq!(*state.mute_events.lock().unwrap(), 0, "never touches mutes");

    let status = handle.status().await.unwrap();
    assert!(status.engaged);
    assert_eq!(status.channels.len(), 2);

    // A person grabs the keys fader on the desk.
    desk.set_fader(ChannelId::input(1), Some(-3.0))
        .await
        .unwrap();
    feed(&handle, [-30.0, -40.0, -30.0], 3).await;
    assert_eq!(state.faders.lock().unwrap()[1], Some(-3.0));
    let status = handle.status().await.unwrap();
    assert_eq!(status.channels[1].mode, ChannelMode::HeldByOperator);

    // Freeze, then pull the vocal level way down: nothing moves.
    handle.freeze();
    feed(&handle, [-5.0, -30.0, -30.0], 5).await;
    assert_eq!(state.faders.lock().unwrap()[0], Some(-4.0));

    // Undo all puts the vocal back where the operator had it.
    handle.undo_all().await.unwrap();
    feed(&handle, [-5.0, -30.0, -30.0], 1).await;
    assert_eq!(state.faders.lock().unwrap()[0], Some(-10.0));

    let log = recorder.log.lock().unwrap().clone();
    let kinds: Vec<_> = log.iter().map(|a| a.kind).collect();
    for kind in [
        AdjustmentKind::Engaged,
        AdjustmentKind::Auto,
        AdjustmentKind::OperatorTookOver,
        AdjustmentKind::Frozen,
        AdjustmentKind::Undo,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} missing from {kinds:?}");
    }
    assert!(log.iter().all(|a| a.at_ms > 0 && !a.reason.is_empty()));
    let auto = log.iter().find(|a| a.kind == AdjustmentKind::Auto).unwrap();
    assert!(auto
        .channel_name
        .as_deref()
        .is_some_and(|n| n.starts_with("Ch ")));
    assert!(recorder.status.lock().unwrap().is_some());
}
