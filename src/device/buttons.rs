//! Button data types and decoding logic.
//!
//! Buttons are decoded from 8-byte raw messages. Each byte is a bitmask
//! where each bit represents one button. Indices 8–49 of the 50-button
//! space map to indices 0–41 in the returned slice (indices 0–7 are
//! framing bytes, 50+ are reserved).

/// State for 42 usable buttons.
pub struct ButtonState {
    buttons: [bool; 42],
}

impl ButtonState {
    pub fn new() -> Self {
        Self {
            buttons: [false; 42],
        }
    }

    /// Decode a raw USB button message.
    ///
    /// `raw` should be exactly 8 bytes with the last byte non-zero.
    /// Returns the updated button states if they changed, or None if unchanged.
    pub fn decode(&mut self, raw: &[u8]) -> Option<[bool; 42]> {
        if raw.len() != 8 || raw.last() == Some(&0) {
            return None;
        }

        let mut changed = false;
        let mut result = [false; 42];

        for (i, byte) in raw.iter().enumerate() {
            for j in 0..8 {
                let mut b = j + (i * 8);

                let pressed = ((byte >> j) & 1) > 0;

                // Skip framing bytes (0–7) and reserved area (50+)
                if b <= 7 || b >= 50 {
                    continue;
                }

                b -= 8;

                if self.buttons[b] != pressed {
                    changed = true;
                }
                result[b] = pressed;
            }
        }

        if changed {
            self.buttons = result;
            Some(result)
        } else {
            None
        }
    }

    /// Return a reference to the current button states.
    #[allow(dead_code)]
    pub fn values(&self) -> &[bool; 42] {
        &self.buttons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a single raw button message (8 bytes, each byte is a bitmask).
    /// The last byte must be non-zero for the message to be accepted.
    fn encode_buttons(byte0: u8, byte1: u8, byte2: u8, byte3: u8,
                      byte4: u8, byte5: u8, byte6: u8, byte7: u8) -> [u8; 8] {
        [byte0, byte1, byte2, byte3, byte4, byte5, byte6, byte7]
    }

    #[test]
    fn zero_byte_returns_none() {
        let mut state = ButtonState::new();
        let raw = encode_buttons(0, 0, 0, 0, 0, 0, 0, 0);
        assert!(state.decode(&raw).is_none());
    }

    #[test]
    fn short_message_returns_none() {
        let mut state = ButtonState::new();
        let raw = [0xFF; 7];
        assert!(state.decode(&raw).is_none());
    }

    #[test]
    fn button_in_byte_1_bit_0_maps_to_index_0() {
        let mut state = ButtonState::new();
        // Byte 1, bit 0 → button index 8 → 8 - 8 = 0
        let raw = encode_buttons(0, 1, 0, 0, 0, 0, 0, 1); // byte7 = 1 (non-zero)
        let result = state.decode(&raw);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[0], "button index 0 should be pressed");
    }

    #[test]
    fn button_in_byte_5_bit_0_maps_to_index_32() {
        let mut state = ButtonState::new();
        // Byte 5, bit 0 → button index 40 → 40 - 8 = 32
        let raw = encode_buttons(0, 0, 0, 0, 0, 1, 0, 1);
        let result = state.decode(&raw);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[32], "button index 32 should be pressed");
    }

    #[test]
    fn button_in_byte_6_bit_0_maps_to_index_40() {
        let mut state = ButtonState::new();
        // Byte 6, bit 0 → button index 48 → 48 - 8 = 40
        let raw = encode_buttons(0, 0, 0, 0, 0, 0, 1, 1);
        let result = state.decode(&raw);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[40], "button index 40 should be pressed");
    }

    #[test]
    fn button_in_byte_6_bit_1_maps_to_index_41() {
        let mut state = ButtonState::new();
        // Byte 6, bit 1 → button index 49 → 49 - 8 = 41
        let raw = encode_buttons(0, 0, 0, 0, 0, 0, 2, 1); // 2 = bit 1
        let result = state.decode(&raw);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[41], "button index 41 should be pressed");
    }

    #[test]
    fn buttons_in_byte_0_are_skipped() {
        let mut state = ButtonState::new();
        // Byte 0 has bits set but those map to indices 0-7 which are skipped
        let raw = encode_buttons(0xFF, 0, 0, 0, 0, 0, 0, 1);
        let result = state.decode(&raw);
        assert!(result.is_none()); // no change since all skipped indices
    }

    #[test]
    fn buttons_in_byte_7_are_skipped() {
        let mut state = ButtonState::new();
        // Byte 7, bits 0-7 → button indices 56-63, all >= 50, skipped
        let raw = encode_buttons(0, 0, 0, 0, 0, 0, 0, 0xFF);
        // The message has last() = 0xFF != 0, so it passes the initial check
        // But all button indices are >= 50, so no actual button changes
        let result = state.decode(&raw);
        // Since no buttons actually changed (all skipped), returns None
        assert!(result.is_none());
    }

    #[test]
    fn multiple_buttons_in_same_byte() {
        let mut state = ButtonState::new();
        // Byte 2: bits 0 and 3 set → buttons 16, 19 → indices 8, 11
        let raw = encode_buttons(0, 0, 0b1001, 0, 0, 0, 0, 1);
        let result = state.decode(&raw);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[8], "button index 8 should be pressed");
        assert!(vals[11], "button index 11 should be pressed");
        assert!(!vals[9], "button index 9 should not be pressed");
    }

    #[test]
    fn multiple_bytes_with_buttons() {
        let mut state = ButtonState::new();
        // Byte 1 bit 0 → button 0
        // Byte 2 bit 7 → button 15
        // Byte 4 bit 5 → button 29
        let raw = encode_buttons(0, 1, 0x80, 0, 0x20, 0, 0, 1);
        let result = state.decode(&raw);
        assert!(result.is_some());
        let vals = result.unwrap();
        assert!(vals[0]);
        assert!(vals[15]);
        assert!(vals[29]);
    }

    #[test]
    fn unchanged_state_returns_none() {
        let mut state = ButtonState::new();
        let raw = encode_buttons(0, 0x01, 0, 0, 0, 0, 0, 1);
        state.decode(&raw); // First decode sets button 0 to true
        assert!(state.values()[0]);

        // Same data again
        let result = state.decode(&raw);
        assert!(result.is_none()); // No change
    }

    #[test]
    fn press_and_release() {
        let mut state = ButtonState::new();
        let raw_press = encode_buttons(0, 1, 0, 0, 0, 0, 0, 1); // button 0 pressed
        let result = state.decode(&raw_press);
        assert!(result.is_some());
        assert!(state.values()[0]);

        let raw_release = encode_buttons(0, 0, 0, 0, 0, 0, 0, 1); // all released
        let result = state.decode(&raw_release);
        assert!(result.is_some());
        assert!(!state.values()[0]);
    }
}
