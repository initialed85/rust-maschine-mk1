//! Device constants, USB setup, and data types.

pub mod buttons;
pub mod dials;
pub mod pads;

pub const MASCHINE_MK1_VID: u16 = 0x17cc;
pub const MASCHINE_MK1_PID: u16 = 0x0808;

/// MIDI CC numbers mapped to dial positions (index 0–10).
pub const CC_MAPPING: [u8; 11] = [
    77, // 0 = right display, dial 4
    73, // 1 = left display, dial 4
    79, // 2 = swing
    76, // 3 = right display, dial 3
    72, // 4 = left display, dial 3
    78, // 5 = tempo
    75, // 6 = right display, dial 2
    71, // 7 = left display, dial 2
    7,  // 8 = volume
    74, // 9 = right display, dial 1
    70, // 10 = left display, dial 1
];

/// Pad MIDI note numbers (A minor starting at A0).
pub const PAD_NOTES: [u8; 16] = [
    53, //  0 = 13 : = 0
    55, //  1 = 14 : + 2
    57, //  2 = 15 : + 2
    59, //  3 = 16 : + 2
    47, //  4 = 9  : - 12
    48, //  5 = 10 : + 2
    50, //  6 = 11 : + 2
    52, //  7 = 12 : + 2
    40, //  8 = 5  : - 12
    41, //  9 = 6  : + 2
    43, // 10 = 7  : + 2
    45, // 11 = 8  : + 2
    33, // 12 = 1  : - 12
    35, // 13 = 2  : + 2
    36, // 14 = 3  : + 2
    38, // 15 = 4  : + 2
];

/// USB endpoint numbers for the Maschine Mk1.
#[allow(dead_code)]
pub struct Endpoints {
    pub heartbeat_out: u8,
    pub buttons_and_dials_in: u8,
    pub pads_in: u8,
    pub display_out: u8,
}

impl Endpoints {
    pub const fn default_maschine_mk1() -> Self {
        Self {
            heartbeat_out: 0x01,
            buttons_and_dials_in: 0x81,
            pads_in: 0x84,
            display_out: 0x08,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vid_pid_match_maschine_mk1() {
        assert_eq!(MASCHINE_MK1_VID, 0x17cc);
        assert_eq!(MASCHINE_MK1_PID, 0x0808);
    }

    #[test]
    fn cc_mapping_has_11_entries() {
        assert_eq!(CC_MAPPING.len(), 11);
    }

    #[test]
    fn cc_mapping_values_are_valid_midi_cc() {
        for &cc in &CC_MAPPING {
            assert!(cc <= 127, "CC value {} is out of MIDI range", cc);
        }
    }

    #[test]
    fn pad_notes_has_16_entries() {
        assert_eq!(PAD_NOTES.len(), 16);
    }

    #[test]
    fn pad_notes_are_valid_midi_notes() {
        for &note in &PAD_NOTES {
            assert!(note < 128, "Pad note {} is out of MIDI range", note);
        }
    }

    #[test]
    fn endpoints_default_maschine_mk1() {
        let eps = Endpoints::default_maschine_mk1();
        assert_eq!(eps.heartbeat_out, 0x01);
        assert_eq!(eps.buttons_and_dials_in, 0x81);
        assert_eq!(eps.pads_in, 0x84);
        assert_eq!(eps.display_out, 0x08);
    }
}
