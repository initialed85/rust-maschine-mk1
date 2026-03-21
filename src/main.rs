use anyhow::{Context, Result, anyhow};
use midir::{MidiOutput, os::unix::VirtualOutput};
use nusb::{
    MaybeFuture, list_devices,
    transfer::{Bulk, In, Out},
};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

const MASCHINE_MK1_VID: u16 = 0x17cc;
const MASCHINE_MK1_PID: u16 = 0x0808;

const CHAN_1_NOTE_ON: u8 = 0x90;
const CHAN_1_NOTE_OFF: u8 = 0x80;
const CHAN_1_POLYPHONIC_AFTERTOUCH: u8 = 0xA0;

const NOTE_DEBOUNCE: Duration = Duration::from_millis(10);

#[derive(Clone)]
struct PadState {
    is_on: bool,
    changed_at: SystemTime,
    expires_at: SystemTime,
}

fn main() -> Result<()> {
    // a minor starting at A0
    let pad_notes: Vec<u8> = vec![
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

    // compatible with bespoke synth drumplayer
    // let pad_notes: Vec<u8> = vec![
    //     36, //  0 = 13 : = 0
    //     37, //  1 = 14 : + 2
    //     38, //  2 = 15 : + 2
    //     39, //  3 = 16 : + 2
    //     32, //  4 = 9  : - 12
    //     33, //  5 = 10 : + 2
    //     34, //  6 = 11 : + 2
    //     35, //  7 = 12 : + 2
    //     28, //  8 = 5  : - 12
    //     29, //  9 = 6  : + 2
    //     30, // 10 = 7  : + 2
    //     31, // 11 = 8  : + 2
    //     24, // 12 = 1  : - 12
    //     25, // 13 = 2  : + 2
    //     26, // 14 = 3  : + 2
    //     27, // 15 = 4  : + 2
    // ];

    let pad_state_by_note: Arc<Mutex<HashMap<u8, PadState>>> = Arc::new(Mutex::new(HashMap::new()));

    //
    // setup midi stuff
    //

    let midi_out = MidiOutput::new("rust-maschine-mk1-midi-out")?;

    let mut midi_output_conn = midi_out.create_virtual("rust-maschine-mk1-midi-out-virtual")?;

    //
    // try to find the device and dump out some info for it
    //

    let device_info = list_devices()
        .wait()?
        .find(|dev| dev.vendor_id() == MASCHINE_MK1_VID && dev.product_id() == MASCHINE_MK1_PID)
        .context("failed to find device")?;
    println!("device_info: {:?}", device_info);
    println!();

    let device = device_info.open().wait().context("failed to open device")?;

    for configuration in device.configurations() {
        println!("{:?}", configuration);
        println!();
    }

    //
    // now try to talk to it
    //

    device
        .set_configuration(1)
        .wait()
        .context("failed to set configuration")?;

    let claimed_interface = device
        .claim_interface(0)
        .wait()
        .context("failed to claim interface")?;

    claimed_interface
        .set_alt_setting(1)
        .wait()
        .context("failed to set alt setting")?;

    let mut control_writer = claimed_interface
        .endpoint::<Bulk, Out>(0x01)
        .context("failed to get control writer")?
        .writer(512)
        .with_write_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let _display_writer = claimed_interface
        .endpoint::<Bulk, Out>(0x08)
        .context("failed to get display writer")?
        .writer(512)
        .with_write_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let mut buttons_and_dials_reader = claimed_interface
        .endpoint::<Bulk, In>(0x81)
        .context("failed to get buttons and dials reader")?
        .reader(512)
        .with_read_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let mut pads_reader = claimed_interface
        .endpoint::<Bulk, In>(0x84)
        .context("failed to get pad sreader")?
        .reader(512)
        .with_read_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    //
    // seems like you have to regularly send this or nothing works, idk
    //

    let control_writer_thread = thread::spawn(move || -> Result<()> {
        loop {
            let buf = [0x0B, 0xFF, 0x02, 0x05];

            match control_writer.write(&buf) {
                Ok(_) => {}
                Err(_err) => {
                    println!("control_writer; err: {:?}", _err)
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    });

    //
    // buttons and dials (WIP)
    //

    let buttons_and_dials_reader_thread = thread::spawn(move || -> Result<()> {
        loop {
            thread::sleep(Duration::from_millis(1));

            let mut buttons_and_dials_read_buf: Vec<u8> = vec![0; 512];

            let mut buttons: Vec<u16> = vec![];
            let mut dials: Vec<u16> = vec![];

            match buttons_and_dials_reader.read(&mut buttons_and_dials_read_buf) {
                Ok(n) => {
                    let raw_msg = buttons_and_dials_read_buf[0..n].to_vec();

                    if raw_msg.len() <= 1 {
                        continue;
                    }

                    if raw_msg.len() == 8 {
                        let mask = u64::from_be_bytes([
                            raw_msg[0], raw_msg[1], raw_msg[2], raw_msg[3], raw_msg[4], raw_msg[5],
                            raw_msg[6], raw_msg[7],
                        ]);

                        buttons.clear();

                        let mut i = 1;

                        while i < u64::MAX / 2 {
                            let pressed = mask & i != 0;

                            buttons.push(match pressed {
                                true => 1,
                                false => 0,
                            });

                            i *= 2;
                        }
                    } else if raw_msg.len() == 33 {
                        let raw_msg = &raw_msg[1..];

                        dials.clear();

                        for x in raw_msg.chunks(2) {
                            assert!(x.len() == 2);

                            let n = u16::from_le_bytes([x[0], x[1]]);

                            dials.push(n);
                        }
                    }
                }
                Err(_err) => {}
            }
        }
    });

    //
    // handle midi for pads
    //

    let mut send_pad = move |note: u8, velocity: u8, note_on: bool| -> Result<()> {
        assert!(note < 128);

        let now = SystemTime::now();

        let mut pad_state = {
            let locked_pad_state_by_note = pad_state_by_note
                .lock()
                .ok()
                .context("send_pad: failed to lock mutex")?;

            match locked_pad_state_by_note.get(&note) {
                Some(note_state) => note_state.clone(),
                None => PadState {
                    is_on: false,
                    changed_at: now - NOTE_DEBOUNCE,
                    expires_at: now,
                },
            }
        };

        if note_on {
            if pad_state.is_on {
                midi_output_conn
                    .send(&[CHAN_1_POLYPHONIC_AFTERTOUCH, note, (velocity / 2)])
                    .context("send_pad: note aftertouch failed")?;

                return Ok(());
            };

            midi_output_conn
                .send(&[CHAN_1_NOTE_ON, note, velocity])
                .context("send_pad: note on failed")?;

            pad_state.is_on = true;
            pad_state.changed_at = now;
            pad_state.expires_at = now + NOTE_DEBOUNCE;
        } else {
            if !pad_state.is_on {
                return Ok(());
            };

            midi_output_conn
                .send(&[CHAN_1_NOTE_OFF, note, velocity])
                .context("send_pad: note off failed")?;

            pad_state.is_on = false;
            pad_state.changed_at = now;
            pad_state.expires_at = now + NOTE_DEBOUNCE;
        }

        {
            let mut locked_note_state_by_note = pad_state_by_note
                .lock()
                .ok()
                .context("failed to lock mutex")?;

            locked_note_state_by_note.insert(note, pad_state);
        };

        Ok(())
    };

    //
    // handle pads input
    //

    let pads_reader_thread = thread::spawn(move || -> Result<()> {
        loop {
            thread::sleep(Duration::from_millis(1));

            let mut pads: Vec<u16> = vec![];
            for _ in 0..16 {
                pads.push(0);
            }

            let mut pads_read_buf: Vec<u8> = vec![0; 512];
            match pads_reader.read(&mut pads_read_buf) {
                Ok(n) => {
                    let raw_msg = pads_read_buf[0..n].to_vec();

                    let mut any_not_zero = false;

                    for x in raw_msg.chunks(2) {
                        assert!(x.len() == 2);

                        let raw_n = u16::from_le_bytes([x[0], x[1]]);

                        let i = raw_n >> 12;
                        let mut n = raw_n & (4096 - 1);
                        n /= 4096 / 256;

                        let old_n = pads[i as usize];

                        pads[i as usize] = (old_n + n) / 2;
                    }

                    let mut line = String::new();
                    for (i, n) in pads.iter().enumerate() {
                        if *n > 0 {
                            any_not_zero = true;
                        }

                        if *n > 0 {
                            line.push_str(&format!("{i: <2}:{n: <5}"));
                        } else {
                            line.push_str(&format!("{i: <2}:     "));
                        }
                    }

                    if any_not_zero {
                        println!("{}", line);
                    }

                    for (i, pad) in pads.iter().enumerate() {
                        let n = *pad as f64;

                        let res = send_pad(pad_notes[i], (n + 32.0).min(255.0) as u8, n > 0.0);
                        if res.is_err() {
                            println!("{:?}", res);
                        }
                    }
                }
                Err(_err) => {}
            }
        }
    });

    //
    // just wait for something to explode
    //

    loop {
        thread::sleep(Duration::from_millis(1000));

        if control_writer_thread.is_finished() {
            return Err(anyhow!(
                "control writer thread finished: {:?}",
                control_writer_thread.join()
            ));
        }

        if buttons_and_dials_reader_thread.is_finished() {
            return Err(anyhow!(
                "buttons and dials reader thread finished: {:?}",
                buttons_and_dials_reader_thread.join()
            ));
        }

        if pads_reader_thread.is_finished() {
            return Err(anyhow!(
                "pads reader thread finished: {:?}",
                pads_reader_thread.join()
            ));
        }
    }
}
