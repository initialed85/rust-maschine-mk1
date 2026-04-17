//! Shared MIDI output state and sending logic.

pub mod buttons;
pub mod dials;
pub mod pads;

use anyhow::{Context, Result, anyhow};
use chrono::Local;
use midir::os::unix::VirtualOutput;
use std::{collections::HashMap, sync::{Arc, Mutex}};

const CHAN_1_NOTE_ON: u8 = 0x90;
const CHAN_1_NOTE_OFF: u8 = 0x80;
const CHAN_1_POLYPHONIC_AFTERTOUCH: u8 = 0xA0;

const CC_PREFIX: u8 = 0xB0;
const CC_CHANNEL: u8 = 1;

/// MIDI output abstraction wrapping a virtual MIDI port and shared state.
#[derive(Clone)]
pub struct MidiPort {
    conn: Arc<Mutex<midir::MidiOutputConnection>>,
    pad_is_on_by_note: Arc<Mutex<HashMap<u8, bool>>>,
    velocity_by_note: Arc<Mutex<HashMap<u8, u8>>>,
    value_by_cc: Arc<Mutex<HashMap<u8, u8>>>,
}

impl MidiPort {
    /// Create a new virtual MIDI output and return the MidiPort wrapper.
    pub fn new(label: &str, name: &str) -> Result<Self> {
        let midi_out = midir::MidiOutput::new(label)?;

        let conn = midi_out
            .create_virtual(name)
            .map_err(|e| anyhow!("MIDI virtual port creation error: {:?}", e))
            .with_context(|| "failed to create virtual MIDI port")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            pad_is_on_by_note: Arc::new(Mutex::new(HashMap::new())),
            velocity_by_note: Arc::new(Mutex::new(HashMap::new())),
            value_by_cc: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Send a pad event (note on, note off, or polyphonic aftertouch).
    pub fn send_pad(
        &self,
        note: u8,
        velocity: u8,
        desired_is_on: bool,
    ) -> Result<()> {
        let now = Local::now();

        assert!(
            note < 128,
            "{now} | send_pad; note out of spec- note: {note}, velocity: {velocity}, desired_is_on: {desired_is_on}",
        );

        let mut midi_conn = self
            .conn
            .lock()
            .ok()
            .with_context(|| "failed to lock mutex")?;

        let mut actual_is_on = self
            .pad_is_on_by_note
            .lock()
            .ok()
            .with_context(|| "send_pad: failed to lock pad_is_on_by_note")?
            .get(&note)
            .copied()
            .unwrap_or(false);

        let last_velocity = self
            .velocity_by_note
            .lock()
            .ok()
            .with_context(|| "send_pad: failed to lock velocity_by_note")?
            .get(&note)
            .copied()
            .unwrap_or(255); // 255 = sentinel (not possible)

        if desired_is_on {
            if actual_is_on {
                // Already on — send polyphonic aftertouch if velocity changed
                if last_velocity != velocity {
                    println!(
                        "{now} | send_pad; state: aftertouch, note: {note}, last_velocity: {last_velocity}, velocity: {velocity}, changed: {}",
                        last_velocity != velocity
                    );
                    midi_conn
                        .send(&[CHAN_1_POLYPHONIC_AFTERTOUCH, note, velocity])
                        .with_context(|| "send_pad: polyphonic aftertouch failed")?;
                }
            } else {
                // Turning on
                println!(
                    "{now} | send_pad; state: on, note: {note}, velocity: {velocity}"
                );
                midi_conn
                    .send(&[CHAN_1_NOTE_ON, note, velocity])
                    .with_context(|| "send_pad: note on failed")?;
                actual_is_on = true;
            }
        } else if actual_is_on {
            // Turning off
            println!(
                "{now} | send_pad; state: off, note: {note}, velocity: {velocity}"
            );
            midi_conn
                .send(&[CHAN_1_NOTE_OFF, note, velocity])
                .with_context(|| "send_pad: note off failed")?;
            actual_is_on = false;
        }

        // Update state
        self.pad_is_on_by_note
            .lock()
            .ok()
            .with_context(|| "send_pad: failed to lock pad_is_on_by_note")?
            .insert(note, actual_is_on);

        self.velocity_by_note
            .lock()
            .ok()
            .with_context(|| "send_pad: failed to lock velocity_by_note")?
            .insert(note, velocity);

        Ok(())
    }

    /// Send a dial event (MIDI CC) if the value has changed.
    pub fn send_dial(&self, dial: usize, value: u8, cc_mapping: &[u8; 11]) -> Result<()> {
        let now = Local::now();

        assert!(
            dial < cc_mapping.len(),
            "{now} | send_dial; dial out of spec- dial: {dial}, value: {value}",
        );
        assert!(
            value < 128,
            "{now} | send_dial; value out of spec- cc: {}, value: {value}",
            cc_mapping[dial]
        );

        let cc = cc_mapping[dial];

        let mut midi_conn = self
            .conn
            .lock()
            .ok()
            .with_context(|| "failed to lock mutex")?;

        let last_value = self
            .value_by_cc
            .lock()
            .ok()
            .with_context(|| "send_dial: failed to lock value_by_cc")?
            .get(&cc)
            .copied()
            .unwrap_or(255); // 255 = sentinel

        if last_value != value {
            println!(
                "{now} | send_dial; cc: {cc}, last_value: {last_value}, value: {value}, changed: {}",
                last_value != value
            );
            midi_conn
                .send(&[CC_PREFIX + CC_CHANNEL, cc, value])
                .with_context(|| "send_dial: MIDI CC send failed")?;
        }

        self.value_by_cc
            .lock()
            .ok()
            .with_context(|| "send_dial: failed to lock value_by_cc")?
            .insert(cc, value);

        Ok(())
    }
}
