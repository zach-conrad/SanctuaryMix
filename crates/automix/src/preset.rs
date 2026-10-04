//! Channel roles and room-feel presets.
//!
//! Numbers come from the project's church live-mix research
//! (`research/live-mix/church-auto-mix-research.md`, sections 3 and 7). Most
//! are starting points from common practice, to be tuned in beta.
//!
//! Without a calibrated measurement mic the app can't know how loud the room
//! is, so a preset is a *balance*: how loud each kind of source sits relative
//! to the lead vocal (in dB of post-fader short-term level). Lead vocals and
//! speech mics are the anchors and ride toward a reference level so the
//! pastor and worship leader sound the same every week.

use serde::{Deserialize, Serialize};

/// What a channel carries. Decides its target and which limits apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelRole {
    /// Pastor, host, announcements, readings: lav, handheld and pulpit mics.
    Speech,
    LeadVocal,
    BackingVocal,
    Choir,
    Kick,
    Bass,
    /// Snare, toms, hats and overheads.
    Drums,
    KeysPads,
    PianoOrgan,
    ElectricGuitar,
    AcousticGuitar,
    /// Tracks, video and walk-in music.
    Playback,
    Other,
}

impl ChannelRole {
    pub const ALL: [ChannelRole; 13] = [
        ChannelRole::Speech,
        ChannelRole::LeadVocal,
        ChannelRole::BackingVocal,
        ChannelRole::Choir,
        ChannelRole::Kick,
        ChannelRole::Bass,
        ChannelRole::Drums,
        ChannelRole::KeysPads,
        ChannelRole::PianoOrgan,
        ChannelRole::ElectricGuitar,
        ChannelRole::AcousticGuitar,
        ChannelRole::Playback,
        ChannelRole::Other,
    ];

    /// Speech mics get the speech guardrails and push music down while they talk.
    pub fn is_speech(self) -> bool {
        self == ChannelRole::Speech
    }

    /// Anchors ride toward an absolute reference; everything else follows them.
    pub fn is_anchor(self) -> bool {
        matches!(self, ChannelRole::Speech | ChannelRole::LeadVocal)
    }

    /// Instruments and playback: what steps back while someone talks.
    pub fn is_music_bed(self) -> bool {
        !self.is_vocal()
    }

    fn is_low_end(self) -> bool {
        matches!(self, ChannelRole::Kick | ChannelRole::Bass)
    }

    fn is_vocal(self) -> bool {
        matches!(
            self,
            ChannelRole::Speech
                | ChannelRole::LeadVocal
                | ChannelRole::BackingVocal
                | ChannelRole::Choir
        )
    }
}

/// Best guess at a channel's role from its console name. The operator can change it.
pub fn guess_role(name: &str) -> ChannelRole {
    let n = format!(" {} ", name.to_ascii_lowercase());
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&[
        "pastor", "preach", "speak", "lapel", "lav", "host", "announce", "podium", "pulpit",
        "lectern", "handheld", "sermon",
    ]) {
        ChannelRole::Speech
    } else if has(&["bgv", " bv", "backing", "harmony"]) {
        ChannelRole::BackingVocal
    } else if has(&["choir"]) {
        ChannelRole::Choir
    } else if has(&["worship", "lead", " ld ", "vox", "vocal", "cantor"]) {
        ChannelRole::LeadVocal
    } else if has(&["kick", " kik", " bd "]) {
        ChannelRole::Kick
    } else if has(&["bass"]) {
        ChannelRole::Bass
    } else if has(&[
        "snare", "snr", "tom", " hat", " hh ", " oh ", "overhead", "ride", "cymbal", "drum",
    ]) {
        ChannelRole::Drums
    } else if has(&["piano", "organ", "grand"]) {
        ChannelRole::PianoOrgan
    } else if has(&["key", "pad", "synth", "rhodes", " kb"]) {
        ChannelRole::KeysPads
    } else if has(&["acous", "ac gtr", "agtr", "nylon"]) {
        ChannelRole::AcousticGuitar
    } else if has(&["gtr", "guitar", "elec", "egtr"]) {
        ChannelRole::ElectricGuitar
    } else if has(&["video", "playback", "track", "walk", "media", " pb "]) {
        ChannelRole::Playback
    } else {
        ChannelRole::Other
    }
}

/// How the room should feel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomFeel {
    /// Contemporary worship, full band, vocals on top. The default.
    FullModern,
    /// Concert energy. Admin only, with a hearing-safety warning.
    BigLoud,
    /// Modern sound with more kick and bass, at the same loudness.
    DeepLowEnd,
    /// Acoustic sets and smaller rooms.
    WarmIntimate,
    /// Hymns, choir, piano or organ.
    TraditionalChoral,
    /// Sermon, prayer, announcements. Music sits well under speech.
    SpokenWord,
}

impl RoomFeel {
    pub const ALL: [RoomFeel; 6] = [
        RoomFeel::FullModern,
        RoomFeel::BigLoud,
        RoomFeel::DeepLowEnd,
        RoomFeel::WarmIntimate,
        RoomFeel::TraditionalChoral,
        RoomFeel::SpokenWord,
    ];

    pub fn preset(self) -> Preset {
        // Balance tables in dB relative to the lead vocal (research section 3.2), in
        // the order: backing vocal, choir, kick, bass, drums, keys and pads, piano or
        // organ, electric guitar, acoustic guitar, playback, other.
        #[rustfmt::skip]
        let (name, description, room_level, reference_db, balance, admin_only) = match self {
            RoomFeel::FullModern => (
                "Full and modern",
                "Contemporary worship with a full band and vocals on top.",
                "92 to 95 dBA",
                -20.0,
                [-6.0, -4.0, -4.0, -5.0, -6.0, -8.0, -6.0, -7.0, -8.0, -9.0, -8.0],
                false,
            ),
            RoomFeel::BigLoud => (
                "Big and loud",
                "Concert energy for youth nights and big moments.",
                "95 to 97 dBA",
                -18.0,
                [-5.0, -5.0, -2.0, -3.0, -4.0, -7.0, -7.0, -5.0, -9.0, -7.0, -7.0],
                true,
            ),
            RoomFeel::DeepLowEnd => (
                "Deep low end",
                "The modern sound with more kick and bass, at the same loudness.",
                "92 to 95 dBA",
                -20.0,
                [-6.0, -4.0, -2.0, -3.0, -6.0, -7.0, -6.0, -7.0, -8.0, -8.0, -8.0],
                false,
            ),
            RoomFeel::WarmIntimate => (
                "Warm and intimate",
                "Acoustic sets and smaller rooms. Softer, with voices forward.",
                "82 to 88 dBA",
                -24.0,
                [-7.0, -4.0, -9.0, -8.0, -10.0, -5.0, -4.0, -10.0, -4.0, -12.0, -8.0],
                false,
            ),
            RoomFeel::TraditionalChoral => (
                "Traditional and choral",
                "Hymns, choir, piano or organ. The congregation is the lead.",
                "78 to 86 dBA",
                -26.0,
                [-6.0, -3.0, -12.0, -8.0, -12.0, -8.0, -4.0, -10.0, -8.0, -10.0, -8.0],
                false,
            ),
            RoomFeel::SpokenWord => (
                "Spoken word",
                "Sermon, prayer and announcements. Any music sits well under the voice.",
                "72 to 78 dBA",
                -20.0,
                [-18.0; 11],
                false,
            ),
        };
        Preset {
            feel: self,
            name,
            description,
            room_level,
            reference_db,
            speech_reference_db: -20.0,
            balance,
            speech_over_music_db: if self == RoomFeel::SpokenWord {
                18.0
            } else {
                15.0
            },
            admin_only,
            warning: admin_only.then_some(
                "Loud enough to tire ears over a long set. Keep worship sets under 30 minutes at this level.",
            ),
        }
    }
}

/// The numbers behind a room feel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub feel: RoomFeel,
    pub name: &'static str,
    pub description: &'static str,
    /// Average room level this feel aims for. Only measurable with a calibrated room mic.
    pub room_level: &'static str,
    /// Where a lead vocal sits (estimated post-fader level, dB).
    pub reference_db: f32,
    /// Where a speech mic sits.
    pub speech_reference_db: f32,
    /// dB relative to the lead vocal, in this order: backing vocal, choir,
    /// kick, bass, drums, keys and pads, piano or organ, electric guitar,
    /// acoustic guitar, playback, other.
    pub balance: [f32; 11],
    /// How far music sits under a speech mic while someone talks.
    pub speech_over_music_db: f32,
    /// Only admins may pick it.
    pub admin_only: bool,
    pub warning: Option<&'static str>,
}

/// The three nudges a church can make on top of a preset (research section 3).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Nudges {
    /// Moves the anchors, and so the whole mix, up or down.
    pub loudness_db: f32,
    /// Moves kick and bass.
    pub low_end_db: f32,
    /// Lifts the vocals over the band (positive pulls the band down).
    pub vocal_presence_db: f32,
}

/// How far each nudge can go.
pub const NUDGE_LIMIT_DB: f32 = 3.0;

impl Nudges {
    pub fn sanitized(self) -> Self {
        let c = |v: f32| {
            if v.is_finite() {
                v.clamp(-NUDGE_LIMIT_DB, NUDGE_LIMIT_DB)
            } else {
                0.0
            }
        };
        Self {
            loudness_db: c(self.loudness_db),
            low_end_db: c(self.low_end_db),
            vocal_presence_db: c(self.vocal_presence_db),
        }
    }
}

impl Preset {
    /// Absolute target for an anchor channel (speech or lead vocal).
    pub fn anchor_target_db(&self, role: ChannelRole, nudges: &Nudges) -> f32 {
        let base = if role.is_speech() {
            self.speech_reference_db
        } else {
            self.reference_db
        };
        base + nudges.loudness_db
    }

    /// Where `role` sits relative to the lead vocal. Anchors are 0.
    pub fn offset_db(&self, role: ChannelRole, nudges: &Nudges) -> f32 {
        let i = match role {
            ChannelRole::Speech | ChannelRole::LeadVocal => return 0.0,
            ChannelRole::BackingVocal => 0,
            ChannelRole::Choir => 1,
            ChannelRole::Kick => 2,
            ChannelRole::Bass => 3,
            ChannelRole::Drums => 4,
            ChannelRole::KeysPads => 5,
            ChannelRole::PianoOrgan => 6,
            ChannelRole::ElectricGuitar => 7,
            ChannelRole::AcousticGuitar => 8,
            ChannelRole::Playback => 9,
            ChannelRole::Other => 10,
        };
        let mut offset = self.balance[i];
        if role.is_low_end() {
            offset += nudges.low_end_db;
        }
        if !role.is_vocal() {
            offset -= nudges.vocal_presence_db;
        }
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_roles_from_demo_names() {
        for (name, role) in [
            ("Pastor", ChannelRole::Speech),
            ("Worship Ld", ChannelRole::LeadVocal),
            ("BGV 1", ChannelRole::BackingVocal),
            ("Kick", ChannelRole::Kick),
            ("Snare", ChannelRole::Drums),
            ("Hat", ChannelRole::Drums),
            ("OH L", ChannelRole::Drums),
            ("Bass DI", ChannelRole::Bass),
            ("Elec Gtr", ChannelRole::ElectricGuitar),
            ("Acous Gtr", ChannelRole::AcousticGuitar),
            ("Keys L", ChannelRole::KeysPads),
            ("Pad R", ChannelRole::KeysPads),
            ("Choir L", ChannelRole::Choir),
            ("Handheld 1", ChannelRole::Speech),
            ("Video L", ChannelRole::Playback),
            ("Playback R", ChannelRole::Playback),
            ("Lapel 2", ChannelRole::Speech),
            ("Grand Piano", ChannelRole::PianoOrgan),
            ("Ambient L", ChannelRole::Other),
            ("Spare 1", ChannelRole::Other),
        ] {
            assert_eq!(guess_role(name), role, "{name}");
        }
    }

    #[test]
    fn deep_low_end_raises_kick_and_bass_only() {
        let modern = RoomFeel::FullModern.preset();
        let deep = RoomFeel::DeepLowEnd.preset();
        let n = Nudges::default();
        assert!(deep.offset_db(ChannelRole::Kick, &n) > modern.offset_db(ChannelRole::Kick, &n));
        assert!(deep.offset_db(ChannelRole::Bass, &n) > modern.offset_db(ChannelRole::Bass, &n));
        assert_eq!(deep.reference_db, modern.reference_db, "same loudness");
    }

    #[test]
    fn lead_vocal_sits_on_top_in_every_feel() {
        let n = Nudges::default();
        for feel in RoomFeel::ALL {
            let p = feel.preset();
            for role in ChannelRole::ALL {
                if !role.is_anchor() {
                    assert!(p.offset_db(role, &n) < 0.0, "{feel:?} {role:?}");
                }
            }
        }
    }

    #[test]
    fn nudges_move_the_right_things_and_are_capped() {
        let p = RoomFeel::FullModern.preset();
        let n = Nudges {
            loudness_db: 9.0,
            low_end_db: 2.0,
            vocal_presence_db: 1.0,
        }
        .sanitized();
        assert_eq!(n.loudness_db, NUDGE_LIMIT_DB);
        assert_eq!(p.anchor_target_db(ChannelRole::LeadVocal, &n), -17.0);
        assert_eq!(p.offset_db(ChannelRole::Kick, &n), -4.0 + 2.0 - 1.0);
        assert_eq!(p.offset_db(ChannelRole::KeysPads, &n), -8.0 - 1.0);
        assert_eq!(p.offset_db(ChannelRole::BackingVocal, &n), -6.0);
    }

    #[test]
    fn only_big_and_loud_is_admin_only() {
        for feel in RoomFeel::ALL {
            let p = feel.preset();
            assert_eq!(p.admin_only, feel == RoomFeel::BigLoud);
            assert_eq!(p.warning.is_some(), p.admin_only);
        }
    }
}
