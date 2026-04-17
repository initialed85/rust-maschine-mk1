//! Pad data types and decoding logic.

/// Pad state for 16 pads, tracking current decoded values.
pub struct PadState {
    pads: [u8; 16],
}

impl PadState {
    pub fn new() -> Self {
        Self {
            pads: [0; 16],
        }
    }

    /// Decode a raw USB pad message.
    ///
    /// Raw data is little-endian u16 chunks where:
    /// - top 4 bits = pad index
    /// - bottom 12 bits = raw value (0–4095)
    pub fn decode(&mut self, raw: &[u8]) {
        for chunk in raw.chunks(2) {
            if chunk.len() != 2 {
                continue;
            }

            let raw_n = u16::from_le_bytes([chunk[0], chunk[1]]);
            let i = (raw_n >> 12) as usize;
            let mut n = (raw_n & 4095) as f64;
            n /= 4095.0;       // normalize to 0..1
            n *= 255.0 / 2.0;  // re-scale to MIDI range 0..127

            if i < 16 && n <= 127.0 {
                self.pads[i] = n as u8;
            }
        }
    }

    /// Return a reference to the decoded pad values.
    pub fn values(&self) -> &[u8; 16] {
        &self.pads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a single pad entry as it would appear in a raw USB message.
    fn pad_entry(pad_idx: u16, raw_value: u16) -> [u8; 2] {
        let encoded = (pad_idx << 12) | (raw_value & 4095);
        encoded.to_le_bytes()
    }

    #[test]
    fn zero_value_gives_zero_midi() {
        let mut state = PadState::new();
        let raw = pad_entry(0, 0);
        state.decode(&raw);
        assert_eq!(state.values()[0], 0);
    }

    #[test]
    fn max_valid_raw_value_gives_126() {
        let mut state = PadState::new();
        // 4078 / 4095 * 127.5 = 126.992 → 126
        // 4079 gives 127.002 which exceeds 127.0 and is rejected
        let raw = pad_entry(0, 4078);
        state.decode(&raw);
        assert_eq!(state.values()[0], 126);
    }

    #[test]
    fn half_raw_value_gives_63() {
        let mut state = PadState::new();
        // 2048 / 4095 * 127.5 ≈ 63.7 → truncates to 63
        let raw = pad_entry(0, 2048);
        state.decode(&raw);
        assert_eq!(state.values()[0], 63);
    }

    #[test]
    fn pad_index_15_works() {
        let mut state = PadState::new();
        let raw = pad_entry(15, 2048);
        state.decode(&raw);
        assert_eq!(state.values()[15], 63);
    }

    #[test]
    fn bounds_check_prevents_oob() {
        // Pad index 16 would need (16 << 12) = 65536 which overflows u16.
        // The max pad index representable in 2 bytes is 15 (0xF << 12 = 61440).
        // The bounds check `if i < 16` is a safety measure that never triggers
        // with valid 2-byte chunks, but we verify it doesn't panic.
        let mut state = PadState::new();
        // raw_n = 0xF800 = 63488, i = 15, value = 0x800 = 2048
        let raw: [u8; 2] = [0x00, 0xF8];
        state.decode(&raw);
        // 2048 / 4095 * 127.5 ≈ 63.7 → 63
        assert_eq!(state.values()[15], 63);
    }

    #[test]
    fn odd_length_trailing_chunk_is_skipped() {
        let mut state = PadState::new();
        // 5 bytes: 2 full chunks (4 bytes) + 1 leftover byte
        let raw: [u8; 5] = [
            pad_entry(0, 2048)[0], pad_entry(0, 2048)[1],
            pad_entry(1, 2048)[0], pad_entry(1, 2048)[1],
            0xFF,
        ];
        state.decode(&raw);
        assert_eq!(state.values()[0], 63);
        assert_eq!(state.values()[1], 63);
    }

    #[test]
    fn multiple_pads_decode_independently() {
        let mut state = PadState::new();
        let raw: [u8; 8] = [
            pad_entry(0, 2048)[0], pad_entry(0, 2048)[1],  // pad 0 → 63
            pad_entry(5, 0)[0], pad_entry(5, 0)[1],         // pad 5 → 0
            pad_entry(10, 1024)[0], pad_entry(10, 1024)[1], // pad 10 → 31
            pad_entry(15, 2048)[0], pad_entry(15, 2048)[1], // pad 15 → 63
        ];
        state.decode(&raw);
        assert_eq!(state.values()[0], 63);
        assert_eq!(state.values()[5], 0);
        assert_eq!(state.values()[10], 31);
        assert_eq!(state.values()[15], 63);
        // Unused pads stay at 0
        assert_eq!(state.values()[1], 0);
    }

    #[test]
    fn empty_input_changes_nothing() {
        let mut state = PadState::new();
        state.decode(&[]);
        assert_eq!(state.values(), &[0u8; 16]);
    }

    #[test]
    fn repeated_decode_overwrites_values() {
        let mut state = PadState::new();
        state.decode(&pad_entry(0, 2048));
        assert_eq!(state.values()[0], 63);

        // Decode again with different value
        state.decode(&pad_entry(0, 0));
        assert_eq!(state.values()[0], 0);
    }

    #[test]
    fn value_just_over_127_is_rejected() {
        let mut state = PadState::new();
        // 4095 / 4095 * 127.5 = 127.5 > 127.0 → rejected
        let raw = pad_entry(0, 4095);
        state.decode(&raw);
        assert_eq!(state.values()[0], 0); // unchanged
    }
}
