//! dB / linear helpers shared by the audio engine and console adapters.

/// The floor we report for silence, so meters never see -inf.
pub const SILENCE_DB: f32 = -120.0;

pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        SILENCE_DB
    } else {
        (20.0 * linear.log10()).max(SILENCE_DB)
    }
}

pub fn db_to_linear(db: f32) -> f32 {
    if db <= SILENCE_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_is_zero_db() {
        assert!(linear_to_db(1.0).abs() < 1e-6);
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn silence_is_clamped() {
        assert_eq!(linear_to_db(0.0), SILENCE_DB);
        assert_eq!(db_to_linear(SILENCE_DB), 0.0);
    }

    #[test]
    fn round_trips() {
        for db in [-60.0, -18.0, -6.0, 0.0, 6.0] {
            assert!((linear_to_db(db_to_linear(db)) - db).abs() < 1e-3);
        }
    }
}
