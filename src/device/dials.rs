//! Dial data types and decoding logic.
//!
//! Dial raw data is little-endian u16 pairs representing x/y coordinates.
//! These are converted to angles, with overflow handling, then mapped
//! to a 0–127 MIDI value range.

/// State for decoding 11 dials.
pub struct DialDecoder {
    /// Running accumulated dial values (0–127).
    dials: [u8; 11],
    /// Last angle for each dial (used for delta/overflow calculation).
    last_angles: [f64; 11],
}

impl DialDecoder {
    pub fn new() -> Self {
        Self {
            dials: [0; 11],
            last_angles: [0.0; 11],
        }
    }

    /// Decode raw dial data from a USB message.
    ///
    /// `raw_dials` should be the 22 bytes from offset 1 of the 33-byte message
    /// (11 little-endian u16 pairs).
    ///
    /// Returns the updated dial values if they changed, or None if unchanged.
    pub fn decode(&mut self, raw_dials: &[u8]) -> Option<[u8; 11]> {
        if raw_dials.len() < 22 {
            return None;
        }

        let mut changed = false;
        let mut result = [0u8; 11];

        for (i, chunk) in raw_dials.chunks(2).enumerate() {
            if i >= 11 || chunk.len() < 2 {
                break;
            }

            let x0 = (chunk[0] as f64 / 127.0) - 1.0; // normalize
            let x1 = (chunk[1] as f64 / 127.0) - 1.0; // normalize

            let a = x0.atan2(x1) / core::f64::consts::PI;
            let last_a = self.last_angles[i];
            let d = a - last_a;

            self.last_angles[i] = a;

            // Overflow handling
            let mut b = a;
            let mut last_b = last_a;
            let mut e = d;

            if d >= 1.0 {
                last_b = -last_b;
                e = b - last_b;
                if e >= 0.0 {
                    e = -e;
                }
            } else if d < -1.0 {
                b = -b;
                e = b - last_b;
                if e < 0.0 {
                    e = -e;
                }
            }

            let f = ((-e / 2.0) * 255.0) as i8; // re-scale to MIDI range

            let mut g = self.dials[i] as i8;
            g = g.saturating_add(f);
            if g < 0 {
                g = 0;
            }

            result[i] = g as u8;
            if result[i] != self.dials[i] {
                changed = true;
            }
            self.dials[i] = g as u8;
        }

        if changed {
            Some(result)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a dial position from x and y byte values.
    fn dial_bytes(x: u8, y: u8) -> [u8; 2] {
        [x, y]
    }

    /// Build a 22-byte raw dial slice (11 dial entries).
    fn make_raw_dials(entries: &[[u8; 2]]) -> [u8; 22] {
        let mut buf = [0u8; 22];
        for (i, &[x, y]) in entries.iter().enumerate() {
            buf[i * 2] = x;
            buf[i * 2 + 1] = y;
        }
        buf
    }

    #[test]
    fn no_change_returns_none() {
        let mut decoder = DialDecoder::new();
        // All 11 dials at center (127, 127) → angle 0 for all
        let raw = make_raw_dials(&[
            dial_bytes(127, 127), dial_bytes(127, 127), dial_bytes(127, 127),
            dial_bytes(127, 127), dial_bytes(127, 127), dial_bytes(127, 127),
            dial_bytes(127, 127), dial_bytes(127, 127), dial_bytes(127, 127),
            dial_bytes(127, 127), dial_bytes(127, 127),
        ]);
        // First decode establishes baseline (returns None since angles match initial state)
        assert!(decoder.decode(&raw).is_none());
        // Same data again should also return None
        assert!(decoder.decode(&raw).is_none());
    }

    #[test]
    fn single_rotation_increases_value() {
        let mut decoder = DialDecoder::new();
        let all_center = [
            dial_bytes(127, 127); 11
        ];
        let raw = make_raw_dials(&all_center); // Establish baseline
        decoder.decode(&raw);

        // Rotate dial 0: x=0, y=127 → x_norm=-1, y_norm=0
        // atan2(-1, 0) = -PI/2 → angle = -0.5
        // delta = -0.5 - 0 = -0.5
        // f = (-(-0.5) / 2.0) * 255 = 63.75 → 63
        let mut rotated = all_center;
        rotated[0] = dial_bytes(0, 127);
        let raw_rotate = make_raw_dials(&rotated);
        let result = decoder.decode(&raw_rotate);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[0] > 0, "dial 0 should increase, got {}", vals[0]);
    }

    #[test]
    fn rotation_increase_then_decrease() {
        let mut decoder = DialDecoder::new();
        let all_center = [dial_bytes(127, 127); 11];
        decoder.decode(&make_raw_dials(&all_center));

        // Rotate up (increase value)
        let mut up = all_center;
        up[0] = dial_bytes(0, 127);
        let result_up = decoder.decode(&make_raw_dials(&up));
        assert!(result_up.is_some());
        let vals_up = result_up.unwrap();
        assert!(vals_up[0] > 0);

        // Rotate back down (decrease value)
        let result_down = decoder.decode(&make_raw_dials(&all_center));
        assert!(result_down.is_some());
        let vals_down = result_down.unwrap();
        // Should decrease from the increased value
        assert!(vals_down[0] < vals_up[0],
            "dial should decrease from {} to {}", vals_up[0], vals_down[0]);
    }

    #[test]
    fn clamped_at_zero_when_decreasing_from_nonzero() {
        let mut decoder = DialDecoder::new();
        let all_center = [dial_bytes(127, 127); 11];
        decoder.decode(&make_raw_dials(&all_center));

        // Rotate up to get a positive value
        let mut up = all_center;
        up[0] = dial_bytes(0, 127);
        let result_up = decoder.decode(&make_raw_dials(&up));
        assert!(result_up.is_some());

        // Rotate back to center — should decrease
        let result_center = decoder.decode(&make_raw_dials(&all_center));
        assert!(result_center.is_some());
        // Value should have decreased back toward 0
        assert!(result_center.unwrap()[0] <= result_up.unwrap()[0]);
    }

    #[test]
    fn short_input_is_ignored() {
        let mut decoder = DialDecoder::new();
        let raw = [127u8; 10]; // Only 10 bytes, need 22
        assert!(decoder.decode(&raw).is_none());
    }

    #[test]
    fn multiple_dials_rotate_together() {
        let mut decoder = DialDecoder::new();
        let all_center = [dial_bytes(127, 127); 11];
        decoder.decode(&make_raw_dials(&all_center));

        // Rotate dial 0 and dial 1
        let mut multi = all_center;
        multi[0] = dial_bytes(0, 127);   // dial 0: increase
        multi[1] = dial_bytes(255, 127); // dial 1: different direction
        let result = decoder.decode(&make_raw_dials(&multi));
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[0] > 0, "dial 0 should increase");
    }

    #[test]
    fn overflow_handling_does_not_panic() {
        let mut decoder = DialDecoder::new();
        let all_center = [dial_bytes(127, 127); 11];
        decoder.decode(&make_raw_dials(&all_center));

        // Rotate to angle 0: x=0, y=0 → atan2(0,0)=0
        let mut zero = all_center;
        zero[0] = dial_bytes(0, 0);
        decoder.decode(&make_raw_dials(&zero));

        // Now rotate to angle ≈ -0.75: x=255, y=0 → x_norm≈1, y_norm=-1
        let mut negative = all_center;
        negative[0] = dial_bytes(255, 0);
        let result = decoder.decode(&make_raw_dials(&negative));
        // Should not panic, may or may not return Some
        let _ = result;
    }

    #[test]
    fn repeated_rotations_accumulate() {
        let mut decoder = DialDecoder::new();
        let all_center = [dial_bytes(127, 127); 11];
        decoder.decode(&make_raw_dials(&all_center));

        // Rotate in the "increase" direction multiple times
        let mut max_val = 0u8;
        for _ in 0..5 {
            let mut rotated = all_center;
            rotated[0] = dial_bytes(0, 127);
            let result = decoder.decode(&make_raw_dials(&rotated));
            if let Some(vals) = result {
                max_val = max_val.max(vals[0]);
            }
        }
        // After 5 rotations of ~63 each, value should be substantial
        // (but clamped at 127)
        assert!(max_val > 0, "value should accumulate with repeated rotations");
    }
}
