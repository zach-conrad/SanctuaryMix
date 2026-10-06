//! Byte-level encoding and decoding of the dLive MIDI protocol.
//!
//! Source: Allen & Heath "dLive MIDI Over TCP/IP Protocol" (firmware 1.9x).
//! Kept free of I/O so it can be unit-tested and reused by a future
//! Avantis/SQ adapter, which share most of this protocol.
//!
//! Message shapes (N = MIDI channel base + channel-type offset):
//! - Mute on/off: `9N CH 7F 9N CH 00` / `9N CH 3F 9N CH 00`
//! - Fader level (NRPN): `BN 63 CH  BN 62 17  BN 06 LV`
//! - Get channel name: `F0 00 00 1A 50 10 01 00 0N 01 CH F7`
//! - Name reply: `F0 00 00 1A 50 10 01 00 0N 02 CH <ascii> F7`
//! - Input PEQ and HPF (NRPN, firmware 1.9+): `BN 63 CH  BN 62 PP  BN 06 VV`,
//!   PP = `1A`..`29` (band 0-3 x type, frequency, width, gain), `30` HPF
//!   frequency, `31` HPF on/off. Value grids are in [`mix_core::eq::grid`].
//! - Get a parameter: `F0 00 00 1A 50 10 01 00 0N 05 0B PP CH F7`; the desk
//!   answers with the matching NRPN message (fader `17`, EQ, HPF).

use mix_core::eq::{grid, EqChange};
use mix_core::{ChannelId, ChannelKind, ConsoleEvent};

const SYSEX_HEADER: [u8; 7] = [0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00];
const NRPN_FADER: u8 = 0x17;
const SYSEX_GET_NAME: u8 = 0x01;
const SYSEX_NAME_REPLY: u8 = 0x02;
const SYSEX_GET: [u8; 2] = [0x05, 0x0B];
/// First PEQ parameter (band 0 type); each band has four, in the order type,
/// frequency, width, gain.
const NRPN_PEQ_FIRST: u8 = 0x1A;
const NRPN_PEQ_LAST: u8 = 0x29;
const NRPN_HPF_FREQ: u8 = 0x30;
const NRPN_HPF_ON: u8 = 0x31;

/// Every EQ parameter on an input, in the order a full read asks for them.
pub const EQ_PARAMS: [u8; 18] = [
    0x30, 0x31, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x28, 0x29,
];

/// The console's "MIDI channel" setting. dLive uses it and the next four
/// MIDI channels, so valid bases are 0..=11 (1-12 on the surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiBase(u8);

impl MidiBase {
    pub fn new(channel: u8) -> Self {
        Self(channel.min(11))
    }
}

/// Maps a channel to (MIDI channel offset from base, CH byte).
fn address(id: ChannelId) -> Option<(u8, u8)> {
    let i = id.index;
    let (offset, ch) = match id.kind {
        ChannelKind::Input if i < 128 => (0, i),
        ChannelKind::Group if i < 62 => (1, i),
        ChannelKind::Aux if i < 62 => (2, i),
        ChannelKind::Matrix if i < 62 => (3, i),
        ChannelKind::FxReturn if i < 16 => (4, 0x20 + i),
        ChannelKind::Main if i < 6 => (4, 0x30 + i),
        ChannelKind::Dca if i < 24 => (4, 0x36 + i),
        _ => return None,
    };
    Some((offset, ch as u8))
}

fn channel_from_address(offset: u8, ch: u8) -> Option<ChannelId> {
    let (kind, index) = match (offset, ch) {
        (0, 0x00..=0x7F) => (ChannelKind::Input, ch),
        (1, 0x00..=0x3D) => (ChannelKind::Group, ch),
        (2, 0x00..=0x3D) => (ChannelKind::Aux, ch),
        (3, 0x00..=0x3D) => (ChannelKind::Matrix, ch),
        (4, 0x20..=0x2F) => (ChannelKind::FxReturn, ch - 0x20),
        (4, 0x30..=0x35) => (ChannelKind::Main, ch - 0x30),
        (4, 0x36..=0x4D) => (ChannelKind::Dca, ch - 0x36),
        _ => return None,
    };
    Some(ChannelId {
        kind,
        index: index as u16,
    })
}

fn midi_channel(base: MidiBase, offset: u8) -> u8 {
    (base.0 + offset) & 0x0F
}

/// dLive fader values are ~2 steps per dB: 0x6B is 0 dB, 0x7F is +10 dB, 0x00 is -inf.
pub fn db_to_level(db: Option<f32>) -> u8 {
    match db {
        None => 0,
        Some(db) if db < -53.0 => 0,
        Some(db) => (107.0 + 2.0 * db).round().clamp(1.0, 127.0) as u8,
    }
}

pub fn level_to_db(level: u8) -> Option<f32> {
    if level == 0 {
        None
    } else {
        Some((level as f32 - 107.0) / 2.0)
    }
}

pub fn mute(base: MidiBase, id: ChannelId, muted: bool) -> Option<Vec<u8>> {
    let (offset, ch) = address(id)?;
    let status = 0x90 | midi_channel(base, offset);
    let velocity = if muted { 0x7F } else { 0x3F };
    Some(vec![status, ch, velocity, status, ch, 0x00])
}

pub fn fader(base: MidiBase, id: ChannelId, db: Option<f32>) -> Option<Vec<u8>> {
    let (offset, ch) = address(id)?;
    let status = 0xB0 | midi_channel(base, offset);
    Some(vec![
        status,
        0x63,
        ch,
        status,
        0x62,
        NRPN_FADER,
        status,
        0x06,
        db_to_level(db),
    ])
}

/// The NRPN parameter and 7-bit value for one EQ change, or `None` for a
/// band shape the desk can't put on that band.
pub fn eq_param(change: &EqChange) -> Option<(u8, u8)> {
    let peq = |band: u8, field: u8| -> Option<u8> {
        (band < 4).then_some(NRPN_PEQ_FIRST + band * 4 + field)
    };
    Some(match *change {
        EqChange::BandKind { band, kind } => {
            if !kind.allowed_on(band) {
                return None;
            }
            (peq(band, 0)?, grid::kind_value(kind))
        }
        EqChange::BandFreq { band, hz } => (peq(band, 1)?, grid::freq_value(hz)),
        EqChange::BandWidth { band, width } => (peq(band, 2)?, grid::width_value(width)),
        EqChange::BandGain { band, db } => (peq(band, 3)?, grid::gain_value(db)),
        EqChange::HpfFreq { hz } => (NRPN_HPF_FREQ, grid::hpf_value(hz)),
        EqChange::HpfOn { on } => (NRPN_HPF_ON, if on { 0x7F } else { 0x00 }),
    })
}

/// The EQ change an NRPN parameter and value stand for.
pub fn eq_change(param: u8, value: u8) -> Option<EqChange> {
    Some(match param {
        NRPN_PEQ_FIRST..=NRPN_PEQ_LAST => {
            let band = (param - NRPN_PEQ_FIRST) / 4;
            match (param - NRPN_PEQ_FIRST) % 4 {
                0 => EqChange::BandKind {
                    band,
                    kind: grid::kind_from_value(value)?,
                },
                1 => EqChange::BandFreq {
                    band,
                    hz: grid::freq_from_value(value),
                },
                2 => EqChange::BandWidth {
                    band,
                    width: grid::width_from_value(value),
                },
                _ => EqChange::BandGain {
                    band,
                    db: grid::gain_from_value(value),
                },
            }
        }
        NRPN_HPF_FREQ => EqChange::HpfFreq {
            hz: grid::hpf_from_value(value),
        },
        NRPN_HPF_ON => EqChange::HpfOn { on: value >= 0x40 },
        _ => return None,
    })
}

/// Sets one EQ parameter on an input. Bands 1 and 2 are always bells, so a
/// shape change there is skipped (returns an empty message).
pub fn eq(base: MidiBase, id: ChannelId, change: &EqChange) -> Option<Vec<u8>> {
    if id.kind != ChannelKind::Input {
        return None;
    }
    let (offset, ch) = address(id)?;
    let Some((param, value)) = eq_param(change) else {
        // Bells on bands 1 and 2 have no shape message at all.
        return matches!(change, EqChange::BandKind { band: 1 | 2, kind } if *kind == mix_core::eq::EqBandKind::Bell)
            .then(Vec::new);
    };
    let status = 0xB0 | midi_channel(base, offset);
    Some(vec![
        status, 0x63, ch, status, 0x62, param, status, 0x06, value,
    ])
}

/// Asks the desk for one parameter (fader `0x17`, or one of [`EQ_PARAMS`]).
pub fn get_request(base: MidiBase, id: ChannelId, param: u8) -> Option<Vec<u8>> {
    let (offset, ch) = address(id)?;
    let mut msg = vec![0xF0];
    msg.extend_from_slice(&SYSEX_HEADER);
    msg.push(midi_channel(base, offset));
    msg.extend_from_slice(&SYSEX_GET);
    msg.extend_from_slice(&[param, ch, 0xF7]);
    Some(msg)
}

pub fn fader_request(base: MidiBase, id: ChannelId) -> Option<Vec<u8>> {
    get_request(base, id, NRPN_FADER)
}

pub fn name_request(base: MidiBase, id: ChannelId) -> Option<Vec<u8>> {
    let (offset, ch) = address(id)?;
    let mut msg = vec![0xF0];
    msg.extend_from_slice(&SYSEX_HEADER);
    msg.extend_from_slice(&[midi_channel(base, offset), SYSEX_GET_NAME, ch, 0xF7]);
    Some(msg)
}

/// Turns the console's byte stream into [`ConsoleEvent`]s. Handles messages
/// split across TCP reads and MIDI running status.
pub struct Decoder {
    base: MidiBase,
    status: Option<u8>,
    data: Vec<u8>,
    sysex: Option<Vec<u8>>,
    /// Per MIDI channel: (NRPN MSB = CH, NRPN LSB = parameter).
    nrpn: [(Option<u8>, Option<u8>); 16],
}

impl Decoder {
    pub fn new(base: MidiBase) -> Self {
        Self {
            base,
            status: None,
            data: Vec::with_capacity(2),
            sysex: None,
            nrpn: [(None, None); 16],
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ConsoleEvent> {
        let mut out = Vec::new();
        for &b in bytes {
            if let Some(sysex) = self.sysex.as_mut() {
                if b == 0xF7 {
                    let body = self.sysex.take().unwrap();
                    out.extend(self.decode_sysex(&body));
                } else if b & 0x80 == 0 {
                    sysex.push(b);
                } else {
                    // A status byte aborts an unterminated SysEx.
                    self.sysex = None;
                    self.start_status(b);
                }
                continue;
            }
            if b == 0xF0 {
                self.sysex = Some(Vec::new());
            } else if b & 0x80 != 0 {
                self.start_status(b);
            } else if let Some(status) = self.status {
                self.data.push(b);
                if self.data.len() == 2 {
                    let (d1, d2) = (self.data[0], self.data[1]);
                    self.data.clear();
                    out.extend(self.decode_channel_message(status, d1, d2));
                }
            }
        }
        out
    }

    fn start_status(&mut self, b: u8) {
        self.data.clear();
        // Only note-on and control change carry anything we use; ignore the rest.
        self.status = matches!(b & 0xF0, 0x90 | 0xB0).then_some(b);
    }

    fn offset_of(&self, midi_ch: u8) -> Option<u8> {
        let offset = midi_ch.wrapping_sub(self.base.0) & 0x0F;
        (offset <= 4).then_some(offset)
    }

    fn decode_channel_message(&mut self, status: u8, d1: u8, d2: u8) -> Option<ConsoleEvent> {
        let midi_ch = status & 0x0F;
        let offset = self.offset_of(midi_ch)?;
        match status & 0xF0 {
            0x90 => {
                // Velocity 0 is the "note off" half of every mute message.
                if d2 == 0 {
                    return None;
                }
                let id = channel_from_address(offset, d1)?;
                Some(ConsoleEvent::Mute {
                    id,
                    muted: d2 >= 0x40,
                })
            }
            0xB0 => {
                let state = &mut self.nrpn[midi_ch as usize];
                match d1 {
                    0x63 => state.0 = Some(d2),
                    0x62 => state.1 = Some(d2),
                    0x06 => {
                        let (Some(ch), Some(param)) = *state else {
                            return None;
                        };
                        let id = channel_from_address(offset, ch)?;
                        if param == NRPN_FADER {
                            return Some(ConsoleEvent::Fader {
                                id,
                                db: level_to_db(d2),
                            });
                        }
                        if id.kind == ChannelKind::Input {
                            let change = eq_change(param, d2)?;
                            return Some(ConsoleEvent::Eq { id, change });
                        }
                    }
                    _ => {}
                }
                None
            }
            _ => None,
        }
    }

    fn decode_sysex(&self, body: &[u8]) -> Option<ConsoleEvent> {
        let rest = body.strip_prefix(&SYSEX_HEADER[..])?;
        let [midi_ch, SYSEX_NAME_REPLY, ch, name @ ..] = rest else {
            return None;
        };
        let id = channel_from_address(self.offset_of(*midi_ch)?, *ch)?;
        let name = String::from_utf8_lossy(name)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        Some(ConsoleEvent::Name { id, name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: MidiBase = MidiBase(0);

    #[test]
    fn encodes_mute() {
        assert_eq!(
            mute(BASE, ChannelId::input(0), true).unwrap(),
            [0x90, 0x00, 0x7F, 0x90, 0x00, 0x00]
        );
        assert_eq!(
            mute(BASE, ChannelId::input(4), false).unwrap(),
            [0x90, 0x04, 0x3F, 0x90, 0x04, 0x00]
        );
    }

    #[test]
    fn respects_midi_base_and_channel_type() {
        let aux = ChannelId {
            kind: ChannelKind::Aux,
            index: 1,
        };
        assert_eq!(mute(MidiBase::new(3), aux, true).unwrap()[0], 0x95);
        let dca = ChannelId {
            kind: ChannelKind::Dca,
            index: 0,
        };
        assert_eq!(mute(BASE, dca, true).unwrap()[..2], [0x94, 0x36]);
    }

    #[test]
    fn encodes_eq() {
        let id = ChannelId::input(9);
        // Band 1 gain 0 dB: param 0x21, value 0x3F.
        assert_eq!(
            eq(BASE, id, &EqChange::BandGain { band: 1, db: 0.0 }).unwrap(),
            [0xB0, 0x63, 0x09, 0xB0, 0x62, 0x21, 0xB0, 0x06, 0x3F]
        );
        // Band 3 frequency 1 kHz: param 0x27, value 0x47.
        assert_eq!(
            eq(
                BASE,
                id,
                &EqChange::BandFreq {
                    band: 3,
                    hz: 1_000.0
                }
            )
            .unwrap()[5..],
            [0x27, 0xB0, 0x06, 0x47]
        );
        assert_eq!(
            eq(BASE, id, &EqChange::HpfOn { on: true }).unwrap()[5..],
            [0x31, 0xB0, 0x06, 0x7F]
        );
        // A high-pass shape only exists on band 0.
        let hp = mix_core::eq::EqBandKind::HighPass;
        assert!(eq(BASE, id, &EqChange::BandKind { band: 3, kind: hp }).is_none());
        assert_eq!(
            eq(BASE, id, &EqChange::BandKind { band: 0, kind: hp }).unwrap()[5..],
            [0x1A, 0xB0, 0x06, 0x04]
        );
        // EQ is for inputs only.
        let aux = ChannelId {
            kind: ChannelKind::Aux,
            index: 0,
        };
        assert!(eq(BASE, aux, &EqChange::HpfOn { on: true }).is_none());
    }

    #[test]
    fn encodes_get_requests() {
        assert_eq!(
            fader_request(BASE, ChannelId::input(4)).unwrap(),
            [0xF0, 0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00, 0x00, 0x05, 0x0B, 0x17, 0x04, 0xF7]
        );
        assert_eq!(
            get_request(BASE, ChannelId::input(0), 0x30).unwrap()[11],
            0x30
        );
    }

    #[test]
    fn decodes_eq_from_the_desk() {
        let mut d = Decoder::new(BASE);
        let events = d.feed(&[0xB0, 0x63, 0x02, 0xB0, 0x62, 0x29, 0xB0, 0x06, 0x3F]);
        assert_eq!(
            events,
            [ConsoleEvent::Eq {
                id: ChannelId::input(2),
                change: EqChange::BandGain { band: 3, db: 0.0 }
            }]
        );
        // Running status: the next parameter without repeating the status byte.
        let events = d.feed(&[0x62, 0x31, 0x06, 0x00]);
        assert_eq!(
            events,
            [ConsoleEvent::Eq {
                id: ChannelId::input(2),
                change: EqChange::HpfOn { on: false }
            }]
        );
    }

    #[test]
    fn every_eq_param_round_trips() {
        for &param in &EQ_PARAMS {
            for value in [0u8, 1, 4, 24, 63, 100, 126] {
                let Some(change) = eq_change(param, value) else {
                    continue;
                };
                // A shape that band can't take (a high-pass on band 1).
                let Some((p, v)) = eq_param(&change) else {
                    continue;
                };
                assert_eq!(p, param);
                if param != NRPN_HPF_ON {
                    assert_eq!(
                        v,
                        value.min(if param <= 0x29 && (param - 0x1A) % 4 == 2 {
                            24
                        } else {
                            126
                        })
                    );
                }
            }
        }
    }

    #[test]
    fn encodes_fader() {
        assert_eq!(
            fader(BASE, ChannelId::input(9), Some(0.0)).unwrap(),
            [0xB0, 0x63, 0x09, 0xB0, 0x62, 0x17, 0xB0, 0x06, 0x6B]
        );
    }

    #[test]
    fn fader_law_endpoints() {
        assert_eq!(db_to_level(None), 0x00);
        assert_eq!(db_to_level(Some(-90.0)), 0x00);
        assert_eq!(db_to_level(Some(10.0)), 0x7F);
        assert_eq!(db_to_level(Some(-10.0)), 0x57);
        assert_eq!(level_to_db(0x6B), Some(0.0));
        assert_eq!(level_to_db(0), None);
    }

    #[test]
    fn rejects_out_of_range_channels() {
        assert!(mute(BASE, ChannelId::input(128), true).is_none());
        assert!(mute(
            BASE,
            ChannelId {
                kind: ChannelKind::Main,
                index: 6
            },
            true
        )
        .is_none());
    }

    #[test]
    fn encodes_name_request() {
        assert_eq!(
            name_request(BASE, ChannelId::input(2)).unwrap(),
            [0xF0, 0x00, 0x00, 0x1A, 0x50, 0x10, 0x01, 0x00, 0x00, 0x01, 0x02, 0xF7]
        );
    }

    #[test]
    fn round_trips_through_decoder() {
        let mut d = Decoder::new(BASE);
        let id = ChannelId::input(5);
        let mut bytes = mute(BASE, id, true).unwrap();
        bytes.extend(fader(BASE, id, Some(-10.0)).unwrap());
        assert_eq!(
            d.feed(&bytes),
            [
                ConsoleEvent::Mute { id, muted: true },
                ConsoleEvent::Fader {
                    id,
                    db: Some(-10.0)
                }
            ]
        );
    }

    #[test]
    fn decodes_messages_split_across_reads_and_running_status() {
        let mut d = Decoder::new(BASE);
        assert!(d.feed(&[0xB0, 0x63]).is_empty());
        assert!(d.feed(&[0x01, 0x62, 0x17]).is_empty()); // running status
        assert_eq!(
            d.feed(&[0x06, 0x6B]),
            [ConsoleEvent::Fader {
                id: ChannelId::input(1),
                db: Some(0.0)
            }]
        );
    }

    #[test]
    fn decodes_name_reply() {
        let mut d = Decoder::new(BASE);
        let mut bytes = vec![0xF0];
        bytes.extend_from_slice(&SYSEX_HEADER);
        bytes.extend_from_slice(&[0x00, 0x02, 0x00]);
        bytes.extend_from_slice(b"Pastor");
        bytes.push(0xF7);
        assert_eq!(
            d.feed(&bytes),
            [ConsoleEvent::Name {
                id: ChannelId::input(0),
                name: "Pastor".into()
            }]
        );
    }

    #[test]
    fn ignores_other_midi_channels() {
        let mut d = Decoder::new(MidiBase::new(2));
        assert!(d.feed(&[0x90, 0x00, 0x7F]).is_empty());
        assert_eq!(d.feed(&[0x92, 0x00, 0x7F]).len(), 1);
    }
}
