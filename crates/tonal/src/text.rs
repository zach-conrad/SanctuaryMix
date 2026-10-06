//! Numbers and change lines the way the app shows them: "320 Hz", "3.2 kHz",
//! "−3.0 dB" with a real minus sign, "320 Hz  0.0 → −2.0 dB".

use mix_core::eq::{ChannelEq, EqBandKind};

pub fn hz(hz: f32) -> String {
    if hz >= 1_000.0 {
        let k = hz / 1_000.0;
        if k >= 10.0 {
            format!("{k:.0} kHz")
        } else {
            format!("{k:.1} kHz")
        }
    } else {
        format!("{} Hz", hz.round() as i32)
    }
}

/// A gain with its sign: "−3.0 dB", "+2.0 dB", "0.0 dB".
pub fn db(db: f32) -> String {
    format!("{} dB", num(db))
}

/// A gain number without the unit: "−3.0", "+2.0", "0.0".
pub fn num(db: f32) -> String {
    let v = if db.abs() < 0.05 { 0.0 } else { db };
    let s = format!("{:.1}", v.abs());
    if v < 0.0 {
        format!("\u{2212}{s}")
    } else if v > 0.0 {
        format!("+{s}")
    } else {
        s
    }
}

fn kind(k: EqBandKind) -> &'static str {
    match k {
        EqBandKind::Bell => "bell",
        EqBandKind::LowShelf => "low shelf",
        EqBandKind::HighShelf => "high shelf",
        EqBandKind::LowPass => "high cut",
        EqBandKind::HighPass => "low cut",
    }
}

/// Every visible difference from `from` to `to`, one line each, e.g.
/// "Low cut  off → 100 Hz" or "320 Hz  0.0 → −2.0 dB".
pub fn changes(from: &ChannelEq, to: &ChannelEq) -> Vec<String> {
    let mut out = Vec::new();
    let low_cut = |on: bool, f: f32| if on { hz(f) } else { "off".to_string() };
    let (a, b) = (from.snapped(), to.snapped());
    if a.hpf.on != b.hpf.on || (b.hpf.on && a.hpf.freq_hz != b.hpf.freq_hz) {
        out.push(format!(
            "Low cut  {} \u{2192} {}",
            low_cut(a.hpf.on, a.hpf.freq_hz),
            low_cut(b.hpf.on, b.hpf.freq_hz)
        ));
    }
    for (i, (x, y)) in a.bands.iter().zip(b.bands.iter()).enumerate() {
        if x == y {
            continue;
        }
        let band = if i == 3 { " (band 4)" } else { "" };
        if x.kind != y.kind {
            out.push(format!(
                "Band {}  {} \u{2192} {}",
                i + 1,
                kind(x.kind),
                kind(y.kind)
            ));
        }
        let gain_changed = x.gain_db != y.gain_db;
        let moved = x.freq_hz != y.freq_hz || x.width != y.width;
        if gain_changed || (moved && y.gain_db.abs() >= 0.05) {
            out.push(format!(
                "{}  {} \u{2192} {}{band}",
                hz(y.freq_hz),
                num(if moved { 0.0 } else { x.gain_db }),
                db(y.gain_db)
            ));
        }
        if moved && x.gain_db.abs() >= 0.05 && x.freq_hz != y.freq_hz {
            out.push(format!(
                "{}  {} \u{2192} 0.0 dB{band}",
                hz(x.freq_hz),
                num(x.gain_db)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_design() {
        assert_eq!(hz(320.0), "320 Hz");
        assert_eq!(hz(3_200.0), "3.2 kHz");
        assert_eq!(hz(12_000.0), "12 kHz");
        assert_eq!(db(-3.0), "\u{2212}3.0 dB");
        assert_eq!(db(2.0), "+2.0 dB");
        assert_eq!(db(0.02), "0.0 dB");
    }

    #[test]
    fn lists_each_change() {
        let a = ChannelEq::default();
        let mut b = a;
        b.hpf.on = true;
        b.hpf.freq_hz = 100.0;
        b.bands[1].freq_hz = 320.0;
        b.bands[1].gain_db = -2.0;
        let lines = changes(&a, &b);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].starts_with("Low cut  off \u{2192} "),
            "{}",
            lines[0]
        );
        // The desk's grid: −2.0 dB lands on −1.9.
        assert!(
            lines[1].contains("0.0 \u{2192} \u{2212}1.9 dB"),
            "{}",
            lines[1]
        );
    }
}
